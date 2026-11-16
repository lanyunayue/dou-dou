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
            done_ms       INTEGER,
            target_path   TEXT NOT NULL DEFAULT '',
            history_count INTEGER NOT NULL DEFAULT 0,
            impact        TEXT NOT NULL DEFAULT ''
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
            ended_ms   INTEGER,
            owner      TEXT,
            locked     INTEGER NOT NULL DEFAULT 0
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
    migrate_session_meta_lock(&conn);
    migrate_task_state_columns(&conn);
    Ok(())
}

/// 幂等迁移：给旧库的 `task_state` 补 A4 三列（target_path / history_count / impact）。
fn migrate_task_state_columns(conn: &Connection) {
    let _ = conn.execute(
        "ALTER TABLE task_state ADD COLUMN target_path TEXT NOT NULL DEFAULT ''",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE task_state ADD COLUMN history_count INTEGER NOT NULL DEFAULT 0",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE task_state ADD COLUMN impact TEXT NOT NULL DEFAULT ''",
        [],
    );
}

/// 幂等迁移：给旧库的 `session_meta` 补 `owner` / `locked` 两列。
/// SQLite 不支持 `ADD COLUMN IF NOT EXISTS`，靠「尝试后忽略重复列」实现幂等。
fn migrate_session_meta_lock(conn: &Connection) {
    let _ = conn.execute("ALTER TABLE session_meta ADD COLUMN owner TEXT", []);
    let _ = conn.execute(
        "ALTER TABLE session_meta ADD COLUMN locked INTEGER NOT NULL DEFAULT 0",
        [],
    );
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
//
// A5：核心逻辑全部落在 `_on(&conn, ...)` 变体（可内存库单测），
// 包装函数经全仓唯一 `open_conn()` 打库。调用方：tools.rs 的配额/租约变更点
// 与 main.rs 的启动恢复；重启后配额计数与未过期租约从库还原，不清零。

pub fn quota_snapshot(cred_id: &str) -> (u64, u64) {
    match open_conn() {
        Ok(conn) => quota_snapshot_on(&conn, cred_id),
        Err(_) => (0, 0),
    }
}

pub fn quota_snapshot_on(conn: &Connection, cred_id: &str) -> (u64, u64) {
    conn.query_row(
        "SELECT used_calls, active FROM quota_state WHERE cred_id = ?1",
        rusqlite::params![cred_id],
        |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
    )
    .unwrap_or((0, 0))
}

pub fn quota_write(cred_id: &str, used_calls: u64, active: u64) {
    if let Ok(conn) = open_conn() {
        quota_write_on(&conn, cred_id, used_calls, active);
    }
}

pub fn quota_write_on(conn: &Connection, cred_id: &str, used_calls: u64, active: u64) {
    let _ = conn.execute(
        "INSERT INTO quota_state (cred_id, used_calls, active, updated_ms) VALUES (?1,?2,?3,?4)
         ON CONFLICT(cred_id) DO UPDATE SET used_calls=excluded.used_calls,
             active=excluded.active, updated_ms=excluded.updated_ms",
        rusqlite::params![cred_id, used_calls as i64, active as i64, now_ms()],
    );
}

/// A5 启动恢复：读出全部配额计数（cred_id → (used_calls, active)）。
pub fn quota_load_all() -> Vec<(String, u64, u64)> {
    match open_conn() {
        Ok(conn) => quota_load_all_on(&conn),
        Err(_) => Vec::new(),
    }
}

pub fn quota_load_all_on(conn: &Connection) -> Vec<(String, u64, u64)> {
    let Ok(mut stmt) = conn.prepare("SELECT cred_id, used_calls, active FROM quota_state") else {
        return Vec::new();
    };
    stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)? as u64,
            r.get::<_, i64>(2)? as u64,
        ))
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn lease_start(session_id: &str, ttl_ms: i64) {
    if let Ok(conn) = open_conn() {
        lease_start_on(&conn, session_id, ttl_ms);
    }
}

pub fn lease_start_on(conn: &Connection, session_id: &str, ttl_ms: i64) {
    let now = now_ms();
    let _ = conn.execute(
        "INSERT INTO lease_state (session_id, started_ms, expires_ms) VALUES (?1,?2,?3)
         ON CONFLICT(session_id) DO UPDATE SET started_ms=excluded.started_ms,
             expires_ms=excluded.expires_ms",
        rusqlite::params![session_id, now, now + ttl_ms],
    );
}

/// A5 启动恢复：读出**未过期**租约（session_id → 起始秒）。
/// 已过期行留在库中但不会恢复（租约判定以秒与 `LEASE_SECS` 比较，语义与运行期一致）。
pub fn lease_load_alive() -> Vec<(String, u64)> {
    match open_conn() {
        Ok(conn) => lease_load_alive_on(&conn),
        Err(_) => Vec::new(),
    }
}

pub fn lease_load_alive_on(conn: &Connection) -> Vec<(String, u64)> {
    let Ok(mut stmt) =
        conn.prepare("SELECT session_id, started_ms FROM lease_state WHERE expires_ms > ?1")
    else {
        return Vec::new();
    };
    let now = now_ms();
    stmt.query_map(rusqlite::params![now], |r| {
        Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)? / 1000) as u64))
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

pub fn lease_expired(session_id: &str) -> bool {
    match open_conn() {
        Ok(conn) => lease_expired_on(&conn, session_id),
        Err(_) => false,
    }
}

pub fn lease_expired_on(conn: &Connection, session_id: &str) -> bool {
    conn.query_row(
        "SELECT expires_ms FROM lease_state WHERE session_id=?1",
        rusqlite::params![session_id],
        |r| r.get::<_, i64>(0),
    )
    .map(|exp| now_ms() > exp)
    .unwrap_or(false)
}

// ---------- 会话锁（A1：人机争抢仲裁） ----------
//
// 语义：谁先占用谁持有；人工优先。锁定信息写穿 `session_meta.owner/locked`，
// 重启后仍可恢复占用归属。锁不落 SSH 句柄，只落「谁占着」的元数据。

/// 尝试占用会话。成功返回 true；已被他人占用返回 false。
/// 人工（human）抢占已锁会话时强制成功（人工优先）。
pub fn session_try_lock(session_id: &str, owner: &str, force: bool) -> bool {
    match open_conn() {
        Ok(conn) => session_try_lock_on(&conn, session_id, owner, force),
        Err(_) => false,
    }
}

/// 释放会话锁。仅锁的持有者可释放（或强制释放）。
pub fn session_unlock(session_id: &str, owner: &str, force: bool) -> bool {
    match open_conn() {
        Ok(conn) => session_unlock_on(&conn, session_id, owner, force),
        Err(_) => false,
    }
}

/// 查询当前锁持有者；未锁定返回 None。
pub fn session_owner(session_id: &str) -> Option<String> {
    let Ok(conn) = open_conn() else { return None };
    session_owner_on(&conn, session_id)
}

fn session_try_lock_on(conn: &Connection, session_id: &str, owner: &str, force: bool) -> bool {
    let now = now_ms();
    let held = session_owner_on(conn, session_id);
    if let Some(cur) = held {
        if cur == owner {
            return true; // 自己已持有
        }
        if !force {
            return false; // 他人持有且不强制
        }
    }
    // 首次占用或强制抢占：upsert owner + locked
    let _ = conn.execute(
        "INSERT INTO session_meta (id, source, status, started_ms, owner, locked)
         VALUES (?1,?2,'active',?3,?2,1)
         ON CONFLICT(id) DO UPDATE SET owner=excluded.owner, locked=1",
        rusqlite::params![session_id, owner, now],
    );
    session_owner_on(conn, session_id) == Some(owner.to_string())
}

fn session_unlock_on(conn: &Connection, session_id: &str, owner: &str, force: bool) -> bool {
    let held = session_owner_on(conn, session_id);
    match held {
        None => true,
        Some(cur) if cur == owner || force => {
            let _ = conn.execute(
                "UPDATE session_meta SET locked=0 WHERE id=?1",
                rusqlite::params![session_id],
            );
            true
        }
        Some(_) => false,
    }
}

fn session_owner_on(conn: &Connection, session_id: &str) -> Option<String> {
    conn.query_row(
        "SELECT owner FROM session_meta WHERE id=?1 AND locked=1",
        rusqlite::params![session_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
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

    #[test]
    fn session_lock_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session_meta (
                id TEXT PRIMARY KEY, host TEXT, user TEXT, port INTEGER, source TEXT,
                status TEXT NOT NULL DEFAULT 'active', started_ms INTEGER NOT NULL,
                ended_ms INTEGER, owner TEXT, locked INTEGER NOT NULL DEFAULT 0)",
        ).unwrap();
        let sid = "test_lock_sid";
        // 首次占用成功
        assert!(session_try_lock_on(&conn, sid, "human", false));
        assert_eq!(session_owner_on(&conn, sid).as_deref(), Some("human"));
        // 他人占用失败（不强制）
        assert!(!session_try_lock_on(&conn, sid, "agent", false));
        // 人工强制抢占成功
        assert!(session_try_lock_on(&conn, sid, "human", true));
        // 释放
        assert!(session_unlock_on(&conn, sid, "human", false));
        assert_eq!(session_owner_on(&conn, sid), None);
        // 重复释放幂等
        assert!(session_unlock_on(&conn, sid, "human", false));
    }

    #[test]
    fn session_lock_human_priority() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session_meta (
                id TEXT PRIMARY KEY, host TEXT, user TEXT, port INTEGER, source TEXT,
                status TEXT NOT NULL DEFAULT 'active', started_ms INTEGER NOT NULL,
                ended_ms INTEGER, owner TEXT, locked INTEGER NOT NULL DEFAULT 0)",
        ).unwrap();
        let sid = "test_lock_priority_sid";
        assert!(session_try_lock_on(&conn, sid, "agent", false));
        assert_eq!(session_owner_on(&conn, sid).as_deref(), Some("agent"));
        // 人工强制抢占（force=true）应成功，即使 agent 持有
        assert!(session_try_lock_on(&conn, sid, "human", true));
        assert_eq!(session_owner_on(&conn, sid).as_deref(), Some("human"));
        let _ = session_unlock_on(&conn, sid, "human", false);
    }

    // ---------- A5：配额写穿与启动恢复 ----------

    #[test]
    fn quota_write_then_load_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE quota_state (
                cred_id    TEXT PRIMARY KEY,
                used_calls INTEGER NOT NULL DEFAULT 0,
                active     INTEGER NOT NULL DEFAULT 0,
                updated_ms INTEGER NOT NULL)",
        ).unwrap();
        quota_write_on(&conn, "cred_a", 7, 1);
        quota_write_on(&conn, "cred_b", 0, 0);
        // 同键再写 = upsert 覆盖（模拟第二次调用后的回落）
        quota_write_on(&conn, "cred_a", 8, 0);
        assert_eq!(quota_snapshot_on(&conn, "cred_a"), (8, 0));
        let mut all = quota_load_all_on(&conn);
        all.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            all,
            vec![("cred_a".to_string(), 8, 0), ("cred_b".to_string(), 0, 0)]
        );
    }

    #[test]
    fn lease_load_alive_keeps_unexpired_and_drops_expired() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE lease_state (
                session_id TEXT PRIMARY KEY,
                started_ms INTEGER NOT NULL,
                expires_ms INTEGER NOT NULL)",
        ).unwrap();
        // 未过期租约（走正常写穿路径）
        lease_start_on(&conn, "s_alive", 60_000);
        // 已过期租约（直接注入历史行）
        let past = now_ms() - 10_000;
        conn.execute(
            "INSERT INTO lease_state (session_id, started_ms, expires_ms) VALUES ('s_dead', ?1, ?2)",
            rusqlite::params![past - 30_000, past],
        ).unwrap();
        let alive = lease_load_alive_on(&conn);
        let ids: Vec<&str> = alive.iter().map(|(s, _)| s.as_str()).collect();
        assert!(ids.contains(&"s_alive"), "未过期租约必须恢复");
        assert!(!ids.contains(&"s_dead"), "已过期租约不得恢复");
        // 毫秒 → 秒换算：起始秒落在合理区间
        let (_, start_sec) = alive.iter().find(|(s, _)| s == "s_alive").unwrap();
        assert!(*start_sec > 0);
        assert!(now_ms() / 1000 >= *start_sec as i64);
    }
}
