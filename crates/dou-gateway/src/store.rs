//! 本地 SQLite 存储：按计划书 4.4 的七张表建模。
//!
//! audit 表由 audit.rs 并行写入；本模块负责 host / credential / session /
//! project / snapshot / policy_rule 六张表的建表与基础读写。
//! 凭据只存密文与盐，不存明文；令牌与私钥明文只在签发/使用时短暂出现。

use rusqlite::{params, Connection};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn db_path() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("dou").join("audit.db"))
        .unwrap_or_else(|_| PathBuf::from("audit.db"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 打开连接（每次调用独立连接，SQLite 单文件并发由 WAL 与短事务保证）。
pub fn open() -> Result<Connection, String> {
    let conn = Connection::open(db_path()).map_err(|e| format!("打开库: {e}"))?;
    Ok(conn)
}

/// 建表（幂等）。
pub fn init_schema() -> Result<(), String> {
    let conn = open()?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;

        CREATE TABLE IF NOT EXISTS project (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            note        TEXT,
            default_cwd TEXT,
            created_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS host (
            id        TEXT PRIMARY KEY,
            alias     TEXT,
            address   TEXT NOT NULL,
            port      INTEGER NOT NULL DEFAULT 22,
            user      TEXT NOT NULL,
            fingerprint TEXT,
            project_id TEXT REFERENCES project(id),
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS credential (
            id         TEXT PRIMARY KEY,
            type       TEXT NOT NULL,           -- key | password | token
            ciphertext TEXT NOT NULL,
            salt       TEXT NOT NULL,
            host_id    TEXT REFERENCES host(id),
            created_at INTEGER NOT NULL,
            revoked_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS session (
            id         TEXT PRIMARY KEY,
            host_id    TEXT REFERENCES host(id),
            source     TEXT NOT NULL,           -- human | agent
            status     TEXT NOT NULL DEFAULT 'active',
            started_at INTEGER NOT NULL,
            ended_at   INTEGER
        );

        CREATE TABLE IF NOT EXISTS snapshot (
            id          TEXT PRIMARY KEY,
            project_id  TEXT REFERENCES project(id),
            session_id  TEXT REFERENCES session(id),
            command     TEXT,
            arg_norm    TEXT,
            dataset_path TEXT,
            fingerprint TEXT,
            result_dir  TEXT,
            collected_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS policy_rule (
            id         TEXT PRIMARY KEY,
            tier       TEXT NOT NULL,           -- observe | execute | change
            pattern    TEXT NOT NULL,
            action     TEXT NOT NULL,           -- allow | alert | approve | deny
            enabled    INTEGER NOT NULL DEFAULT 1,
            builtin    INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_session_host ON session(host_id);
        CREATE INDEX IF NOT EXISTS idx_snapshot_project ON snapshot(project_id);
        CREATE INDEX IF NOT EXISTS idx_snapshot_fp ON snapshot(fingerprint);
        "#,
    )
    .map_err(|e| format!("建表: {e}"))?;

    // 凭据表补齐作用域/有效期/吊销列（老库平滑迁移）
    let conn = open()?;
    let cols: Vec<String> = conn
        .prepare("PRAGMA table_info(credential)")
        .map_err(|e| e.to_string())?
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    let alter = [
        ("scope_json", "TEXT"),
        ("tier", "TEXT"),
        ("client_id", "TEXT"),
        ("expires_at", "INTEGER"),
        ("revoked_at", "INTEGER"),
    ];
    for (col, ty) in alter {
        if !cols.iter().any(|c| c == col) {
            conn.execute(&format!("ALTER TABLE credential ADD COLUMN {col} {ty}"), [])
                .map_err(|e| format!("补列 {col}: {e}"))?;
        }
    }
    Ok(())
}

// ---------- project ----------

pub fn upsert_project(id: &str, name: &str, note: &str, default_cwd: &str) -> Result<(), String> {
    let conn = open()?;
    conn.execute(
        "INSERT INTO project (id, name, note, default_cwd, created_at)
         VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, note=excluded.note, default_cwd=excluded.default_cwd",
        params![id, name, note, default_cwd, now_ms()],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- host ----------

pub fn upsert_host(
    id: &str,
    alias: &str,
    address: &str,
    port: u16,
    user: &str,
    fingerprint: &str,
    project_id: Option<&str>,
) -> Result<(), String> {
    let conn = open()?;
    conn.execute(
        "INSERT INTO host (id, alias, address, port, user, fingerprint, project_id, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(id) DO UPDATE SET alias=excluded.alias, fingerprint=excluded.fingerprint, project_id=excluded.project_id",
        params![id, alias, address, port, user, fingerprint, project_id, now_ms()],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- session ----------

pub fn record_session(id: &str, host_id: Option<&str>, source: &str) -> Result<(), String> {
    let conn = open()?;
    conn.execute(
        "INSERT OR IGNORE INTO session (id, host_id, source, status, started_at) VALUES (?1,?2,?3,'active',?4)",
        params![id, host_id, source, now_ms()],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn close_session(id: &str) -> Result<(), String> {
    let conn = open()?;
    conn.execute(
        "UPDATE session SET status='closed', ended_at=?1 WHERE id=?2",
        params![now_ms(), id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- snapshot ----------

#[allow(clippy::too_many_arguments)]
pub fn upsert_snapshot(
    id: &str,
    project_id: Option<&str>,
    session_id: Option<&str>,
    command: &str,
    arg_norm: &str,
    dataset_path: &str,
    fingerprint: &str,
    result_dir: &str,
) -> Result<(), String> {
    let conn = open()?;
    conn.execute(
        "INSERT INTO snapshot (id, project_id, session_id, command, arg_norm, dataset_path, fingerprint, result_dir, collected_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![id, project_id, session_id, command, arg_norm, dataset_path, fingerprint, result_dir, now_ms()],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

/// 按三段指纹查历史快照（重复实验提示用）。
pub fn find_snapshot_by_fp(fingerprint: &str) -> Result<Option<serde_json::Value>, String> {
    let conn = open()?;
    let mut stmt = conn
        .prepare("SELECT id, command, dataset_path, result_dir, collected_at FROM snapshot WHERE fingerprint=?1 ORDER BY collected_at DESC LIMIT 1")
        .map_err(|e| e.to_string())?;
    let row = stmt
        .query_row(params![fingerprint], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_,String>(0)?,
                "command": r.get::<_,String>(1)?,
                "dataset_path": r.get::<_,String>(2)?,
                "result_dir": r.get::<_,String>(3)?,
                "collected_at": r.get::<_,i64>(4)?,
            }))
        });
    match row {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

// ---------- policy_rule ----------

pub fn list_rules(tier: Option<&str>) -> Result<Vec<serde_json::Value>, String> {
    let conn = open()?;
    let mut stmt = match tier {
        Some(_) => conn.prepare("SELECT id, tier, pattern, action, enabled, builtin FROM policy_rule WHERE tier=?1 ORDER BY builtin DESC").map_err(|e| e.to_string())?,
        None => conn.prepare("SELECT id, tier, pattern, action, enabled, builtin FROM policy_rule ORDER BY builtin DESC").map_err(|e| e.to_string())?,
    };
    let rows = stmt
        .query_map(params![tier.unwrap_or("")], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_,String>(0)?,
                "tier": r.get::<_,String>(1)?,
                "pattern": r.get::<_,String>(2)?,
                "action": r.get::<_,String>(3)?,
                "enabled": r.get::<_,i64>(4)? != 0,
                "builtin": r.get::<_,i64>(5)? != 0,
            }))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn upsert_rule(id: &str, tier: &str, pattern: &str, action: &str, enabled: bool) -> Result<(), String> {
    let conn = open()?;
    conn.execute(
        "INSERT INTO policy_rule (id, tier, pattern, action, enabled, builtin)
         VALUES (?1,?2,?3,?4,?5,0)
         ON CONFLICT(id) DO UPDATE SET tier=excluded.tier, pattern=excluded.pattern, action=excluded.action, enabled=excluded.enabled",
        params![id, tier, pattern, action, enabled as i64],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

/// 内置规则初始化（只插一次）。
pub fn seed_builtin_rules() -> Result<(), String> {
    let builtin = [
        ("ro_ls", "observe", r"^(ls|pwd|df|du|git status|tail|cat|head|whoami|id|env)\b", "allow"),
        ("write_chmod", "execute", r"^(chmod|chown|mv|cp|rm|mkdir|touch|apt|pip|npm|git push|git checkout)\b", "approve"),
        ("delete_risk", "change", r"^(rm -rf|sudo|su|systemctl|iptables|mount|dd\b)", "deny"),
    ];
    for (id, tier, pattern, action) in builtin {
        let exists: i64 = open()?
            .query_row("SELECT COUNT(*) FROM policy_rule WHERE id=?1", params![id], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if exists == 0 {
            let conn = open()?;
            conn.execute(
                "INSERT INTO policy_rule (id, tier, pattern, action, enabled, builtin) VALUES (?1,?2,?3,?4,1,1)",
                params![id, tier, pattern, action],
            ).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
