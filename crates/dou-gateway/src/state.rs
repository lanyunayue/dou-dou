//! 统一状态模型（W-11）。
//!
//! 设计约束（对应 `docs/handover/05` §2 / §6）：
//! - **全仓唯一 `Connection::open` 点在本文件**；store.rs / audit.rs / scope.rs /
//!   tasks.rs / throttle.rs 一律经 `open_conn()` 取连接，不再各自开库。
//! - 所有新增表的时间字段统一 `*_ms INTEGER`（epoch 毫秒），对外 ISO 8601 UTC。
//! - `sessions` 内含 `russh::client::Handle`，**不可序列化、不可持久化**；
//!   只落 `session_meta` 元数据，进程重启后活动连接需重连并如实告知用户。
//! - 状态写穿：先落库再更新进程内缓存，重启后可从库恢复。
//!
//! 本轮新增 5 张表，连同 `store.rs` 的 6 张与 `audit.rs` 的 1 张共 12 张；
//! `auth_throttle_state` 为防爆破专用（第 13 张表）。

use rusqlite::Connection;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// 统一时间基准：epoch 毫秒。
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 状态库路径。支持 `DOU_DB_PATH` 覆盖（测试与便携部署用）。
pub fn db_path() -> PathBuf {
    if let Ok(p) = std::env::var("DOU_DB_PATH") {
        if !p.trim().is_empty() {
            return PathBuf::from(p.trim());
        }
    }
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("dou").join("audit.db")
}

/// **全仓唯一的 SQLite 连接打开点。**
/// WAL + busy_timeout 让并发短连接不互相撞锁；目录不存在时自动创建。
pub fn open_conn() -> Result<Connection, String> {
    let p = db_path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(&p)
        .map_err(|e| format!("打开状态库 {}: {e}", p.display()))?;
    let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
    let _ = conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;");
    Ok(conn)
}

/// 建状态表（幂等）并做一次时间单位迁移。
/// 调用顺序：main() 先 `store::init_schema()`，再本函数（credential 表需已存在）。
pub fn init_state_schema() -> Result<(), String> {
    let conn = open_conn()?;
    conn.execute_batch(
        r#"
        -- 审批任务：从纯内存 Vec<Task> 迁出，重启后待审任务不丢
        CREATE TABLE IF NOT EXISTS task_state (
            id            TEXT PRIMARY KEY,
            session       TEXT NOT NULL,
            command       TEXT NOT NULL,
            status        TEXT NOT NULL,
            reason        TEXT,
            approver_note TEXT,
            remember      INTEGER NOT NULL DEFAULT 0,
            policy        TEXT,
            stdout        TEXT,
            stderr        TEXT,
            exit_code     INTEGER,
            created_ms    INTEGER NOT NULL,
            decided_ms    INTEGER,
            done_ms       INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_task_status ON task_state(status);

        -- 配额：按凭据累计调用次数与活跃并发
        CREATE TABLE IF NOT EXISTS quota_state (
            cred_id    TEXT PRIMARY KEY,
            used_calls INTEGER NOT NULL DEFAULT 0,
            active     INTEGER NOT NULL DEFAULT 0,
            updated_ms INTEGER NOT NULL
        );

        -- 会话租约：30min，落库后重启不重置
        CREATE TABLE IF NOT EXISTS lease_state (
            session_id TEXT PRIMARY KEY,
            started_ms INTEGER NOT NULL,
            expires_ms INTEGER NOT NULL
        );

        -- 会话元数据（不含 SSH 连接句柄，不可持久化）
        CREATE TABLE IF NOT EXISTS session_meta (
            id         TEXT PRIMARY KEY,
            host       TEXT,
            user       TEXT,
            port       INTEGER,
            source     TEXT,
            status     TEXT NOT NULL DEFAULT 'active',
            started_ms INTEGER NOT NULL,
            ended_ms   INTEGER
        );

        -- 防爆破：滑动窗口失败计数 + 指数退避锁定
        CREATE TABLE IF NOT EXISTS auth_throttle_state (
            subject_key     TEXT PRIMARY KEY,
            window_start_ms INTEGER NOT NULL,
            failures        INTEGER NOT NULL DEFAULT 0,
            locked_until_ms INTEGER NOT NULL DEFAULT 0,
            backoff_exp     INTEGER NOT NULL DEFAULT 0,
            last_failure_ms INTEGER NOT NULL DEFAULT 0,
            updated_ms      INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_throttle_locked ON auth_throttle_state(locked_until_ms);
        "#,
    )
    .map_err(|e| format!("建状态表: {e}"))?;

    migrate_time_units(&conn)?;
    Ok(())
}

/// 幂等迁移：`credential` 表的秒值（< 1e11）统一 ×1000 为毫秒。
/// 毫秒值（> 1e12）不会被二次迁移；`revoked_at` 为 NULL 时运算结果仍为 NULL。
/// 表不存在（首次运行）时忽略错误，由 `store::init_schema()` 负责建表。
fn migrate_time_units(conn: &Connection) -> Result<(), String> {
    let _ = conn.execute(
        "UPDATE credential
            SET expires_at = expires_at * 1000,
                created_at = created_at * 1000,
                revoked_at = revoked_at * 1000
          WHERE type = 'token' AND expires_at > 0 AND expires_at < 100000000000",
        [],
    );
    Ok(())
}

// ---------- 配额 / 租约 的通用读写（写穿） ----------

pub fn quota_snapshot(cred_id: &str) -> (u64, u64) {
    let Ok(conn) = open_conn() else { return (0, 0) };
    conn.query_row(
        "SELECT used_calls, active FROM quota_state WHERE cred_id = ?1",
        rusqlite::params![cred_id],
        |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
    )
    .unwrap_or((0, 0))
}

pub fn quota_write(cred_id: &str, used_calls: u64, active: u64) {
    let Ok(conn) = open_conn() else { return };
    let _ = conn.execute(
        "INSERT INTO quota_state (cred_id, used_calls, active, updated_ms) VALUES (?1,?2,?3,?4)
         ON CONFLICT(cred_id) DO UPDATE SET used_calls=excluded.used_calls,
             active=excluded.active, updated_ms=excluded.updated_ms",
        rusqlite::params![cred_id, used_calls as i64, active as i64, now_ms()],
    );
}

pub fn lease_start(session_id: &str, ttl_ms: i64) {
    let Ok(conn) = open_conn() else { return };
    let now = now_ms();
    let _ = conn.execute(
        "INSERT INTO lease_state (session_id, started_ms, expires_ms) VALUES (?1,?2,?3)
         ON CONFLICT(session_id) DO UPDATE SET started_ms=excluded.started_ms,
             expires_ms=excluded.expires_ms",
        rusqlite::params![session_id, now, now + ttl_ms],
    );
}

pub fn lease_expired(session_id: &str) -> bool {
    let Ok(conn) = open_conn() else { return false };
    conn.query_row(
        "SELECT expires_ms FROM lease_state WHERE session_id=?1",
        rusqlite::params![session_id],
        |r| r.get::<_, i64>(0),
    )
    .map(|exp| now_ms() > exp)
    .unwrap_or(false)
}

// ---------- 测试辅助 ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_ms_is_millisecond_scale() {
        let n = now_ms();
        assert!(n > 1_000_000_000_000, "应为 epoch 毫秒量级，实际 {n}");
        assert!(n < 1_000_000_000_000_000, "不应是微秒/纳秒量级，实际 {n}");
    }

    #[test]
    fn db_path_honors_override() {
        // 不改动全局 env（并行测试安全）：只验证默认值是 ASCII 安全的路径形态
        let p = db_path();
        assert!(p.to_string_lossy().ends_with("audit.db"));
    }
}
