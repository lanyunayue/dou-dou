//! 防爆破 / 风控：滑动窗口失败计数 + 指数退避临时锁定。
//!
//! 安全设计（对应 `docs/handover/05` §2 / §5）：
//! - 状态落在 `auth_throttle_state`（W-11 统一状态模型），**重启后锁定不丢**；
//!   禁止另起进程内 HashMap。
//! - **反 DoS 结构性保证**：失败尝试的主体键 = `sha256(salt ‖ kind ‖ 出示值)`
//!   的自指哈希。攻击者用错令牌轰炸只能锁住「该错令牌自己」，
//!   无法指定合法主体的 `cred_id` 去冻结它。
//! - `/health` 探活不计入限流；成功认证立即清零该主体计数。
//! - 状态库不可用时 `check` 返回 `Unavailable`，中间件按 fail-closed 返回 503。
//! - 所有阈值可由 `DOU_THROTTLE_*` 环境变量配置，内部一律毫秒。

use crate::state;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

pub const DEFAULT_WINDOW_MS: i64 = 60_000;
pub const DEFAULT_MAX_FAILURES: i64 = 5;
pub const DEFAULT_BASE_LOCK_MS: i64 = 1_000;
pub const DEFAULT_MAX_LOCK_MS: i64 = 300_000;
const MAX_BACKOFF_EXP: i64 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectKind {
    Master,
    Device,
    Anonymous,
}

impl SubjectKind {
    pub fn tag(self) -> &'static str {
        match self {
            SubjectKind::Master => "m",
            SubjectKind::Device => "d",
            SubjectKind::Anonymous => "a",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub window_ms: i64,
    pub max_failures: i64,
    pub base_lock_ms: i64,
    pub max_lock_ms: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window_ms: DEFAULT_WINDOW_MS,
            max_failures: DEFAULT_MAX_FAILURES,
            base_lock_ms: DEFAULT_BASE_LOCK_MS,
            max_lock_ms: DEFAULT_MAX_LOCK_MS,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        let get = |key: &str, default: i64| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(default)
        };
        Self {
            window_ms: get("DOU_THROTTLE_WINDOW_MS", DEFAULT_WINDOW_MS),
            max_failures: get("DOU_THROTTLE_MAX_FAILURES", DEFAULT_MAX_FAILURES),
            base_lock_ms: get("DOU_THROTTLE_BASE_LOCK_MS", DEFAULT_BASE_LOCK_MS),
            max_lock_ms: get("DOU_THROTTLE_MAX_LOCK_MS", DEFAULT_MAX_LOCK_MS),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 主体键：按「出示值自身」哈希，互不污染、不可被他人指定。
pub fn subject_key(kind: SubjectKind, presented: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"dou::throttle::v1::");
    h.update(kind.tag().as_bytes());
    h.update(presented.as_bytes());
    let d = h.finalize();
    format!("{}:{}", kind.tag(), hex(&d[..12]))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Locked { retry_after_ms: i64 },
    Unavailable,
}

const DDL: &str = "CREATE TABLE IF NOT EXISTS auth_throttle_state (
    subject_key TEXT PRIMARY KEY,
    window_start_ms INTEGER NOT NULL,
    failures INTEGER NOT NULL DEFAULT 0,
    locked_until_ms INTEGER NOT NULL DEFAULT 0,
    backoff_exp INTEGER NOT NULL DEFAULT 0,
    last_failure_ms INTEGER NOT NULL DEFAULT 0,
    updated_ms INTEGER NOT NULL
)";

/// 认证前调用：已锁定则拒绝；窗口过期则读时自动解锁。
pub fn check(key: &str, cfg: &Config) -> Verdict {
    match state::open_conn() {
        Ok(conn) => check_on(&conn, key, cfg),
        Err(_) => Verdict::Unavailable,
    }
}

fn check_on(conn: &Connection, key: &str, cfg: &Config) -> Verdict {
    let now = state::now_ms();
    let row: Option<(i64, i64)> = conn
        .query_row(
            "SELECT window_start_ms, locked_until_ms FROM auth_throttle_state WHERE subject_key=?1",
            params![key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .unwrap_or(None);
    let Some((start, locked_until)) = row else {
        return Verdict::Allow;
    };
    if locked_until > now {
        return Verdict::Locked {
            retry_after_ms: locked_until - now,
        };
    }
    if now - start > cfg.window_ms {
        let _ = conn.execute(
            "UPDATE auth_throttle_state
                SET window_start_ms=?1, failures=0, backoff_exp=0,
                    locked_until_ms=0, updated_ms=?1
              WHERE subject_key=?2",
            params![now, key],
        );
    }
    Verdict::Allow
}

/// 记一次认证失败；返回 `Some(锁定时长 ms)` 表示本次刚触发锁定。
pub fn record_failure(key: &str, cfg: &Config) -> Result<Option<i64>, String> {
    let conn = state::open_conn()?;
    failure_on(&conn, key, cfg)
}

fn failure_on(conn: &Connection, key: &str, cfg: &Config) -> Result<Option<i64>, String> {
    let now = state::now_ms();
    let row: Option<(i64, i64, i64)> = conn
        .query_row(
            "SELECT window_start_ms, failures, backoff_exp FROM auth_throttle_state WHERE subject_key=?1",
            params![key],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let (start, failures, exp) = row.unwrap_or((now, 0, 0));
    let (start, failures) = if now - start > cfg.window_ms {
        (now, 1)
    } else {
        (start, failures + 1)
    };
    let (locked_until, new_exp) = if failures >= cfg.max_failures {
        let shift = exp.clamp(0, MAX_BACKOFF_EXP) as u32;
        let factor = 1i64.checked_shl(shift).unwrap_or(i64::MAX);
        let lock = cfg.base_lock_ms.saturating_mul(factor).min(cfg.max_lock_ms);
        (now.saturating_add(lock), (exp + 1).min(MAX_BACKOFF_EXP))
    } else {
        (0, exp)
    };
    conn.execute(
        "INSERT INTO auth_throttle_state
            (subject_key, window_start_ms, failures, locked_until_ms, backoff_exp, last_failure_ms, updated_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?6)
         ON CONFLICT(subject_key) DO UPDATE SET
            window_start_ms=excluded.window_start_ms,
            failures=excluded.failures,
            locked_until_ms=excluded.locked_until_ms,
            backoff_exp=excluded.backoff_exp,
            last_failure_ms=excluded.last_failure_ms,
            updated_ms=excluded.updated_ms",
        params![key, start, failures, locked_until, new_exp, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(if locked_until > 0 {
        Some(locked_until - now)
    } else {
        None
    })
}

/// 认证成功：清零该主体计数与退避（不影响其他主体）。
pub fn record_success(key: &str) {
    if let Ok(conn) = state::open_conn() {
        success_on(&conn, key);
    }
}

fn success_on(conn: &Connection, key: &str) {
    let _ = conn.execute(
        "DELETE FROM auth_throttle_state WHERE subject_key=?1",
        params![key],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(DDL).unwrap();
        c
    }

    fn cfg() -> Config {
        Config {
            window_ms: 60_000,
            max_failures: 3,
            base_lock_ms: 1_000,
            max_lock_ms: 30_000,
        }
    }

    #[test]
    fn subject_keys_are_deterministic_and_isolated() {
        let a = subject_key(SubjectKind::Anonymous, "dou_ak_deadbeef");
        let b = subject_key(SubjectKind::Anonymous, "dou_ak_deadbeef");
        let c = subject_key(SubjectKind::Anonymous, "dou_ak_other");
        let d = subject_key(SubjectKind::Device, "dou_ak_deadbeef");
        assert_eq!(a, b);
        assert_ne!(a, c, "不同出示值必须落在不同主体");
        assert_ne!(a, d, "不同主体类型也必须隔离");
        assert!(!a.contains("deadbeef"), "主体键不得含明文令牌");
    }

    #[test]
    fn locks_after_threshold_and_unlocks_on_success() {
        let c = mem();
        let k = subject_key(SubjectKind::Anonymous, "bad");
        assert_eq!(check_on(&c, &k, &cfg()), Verdict::Allow);
        assert_eq!(failure_on(&c, &k, &cfg()).unwrap(), None);
        assert_eq!(failure_on(&c, &k, &cfg()).unwrap(), None);
        let locked = failure_on(&c, &k, &cfg()).unwrap();
        assert!(locked.is_some(), "第 3 次失败应触发锁定");
        assert!(matches!(check_on(&c, &k, &cfg()), Verdict::Locked { .. }));
        success_on(&c, &k);
        assert_eq!(check_on(&c, &k, &cfg()), Verdict::Allow, "成功后应清零");
    }

    #[test]
    fn backoff_is_capped() {
        let c = mem();
        let k = subject_key(SubjectKind::Anonymous, "bad2");
        for _ in 0..12 {
            failure_on(&c, &k, &cfg()).unwrap();
            // 把窗口起点拉回现在，避免窗口过期干扰
            c.execute(
                "UPDATE auth_throttle_state SET window_start_ms=?1 WHERE subject_key=?2",
                params![state::now_ms(), k],
            )
            .unwrap();
        }
        let lock: i64 = c
            .query_row(
                "SELECT locked_until_ms - last_failure_ms FROM auth_throttle_state WHERE subject_key=?1",
                params![k],
                |r| r.get(0),
            )
            .unwrap();
        assert!(lock <= 30_000, "退避必须封顶，实际 {lock}ms");
        assert!(lock >= 1_000, "至少应有基础锁定，实际 {lock}ms");
    }

    #[test]
    fn different_subjects_do_not_contaminate_each_other() {
        let c = mem();
        let attacker = subject_key(SubjectKind::Anonymous, "attacker-guess");
        let victim = subject_key(SubjectKind::Master, "victim-token");
        for _ in 0..5 {
            failure_on(&c, &attacker, &cfg()).unwrap();
        }
        assert_eq!(
            check_on(&c, &victim, &cfg()),
            Verdict::Allow,
            "错令牌轰炸不得冻结合法主体（反 DoS）"
        );
    }

    #[test]
    fn config_defaults_are_sane() {
        let d = Config::default();
        assert!(d.window_ms > 0 && d.max_failures > 0);
        assert!(d.base_lock_ms <= d.max_lock_ms);
    }
}
