//! 审计：只追加的 JSONL 时间线，带链式哈希。
//!
//! 每条记录包含 prev_hash 与 hash，hash = sha256(prev_hash || 序列化的记录体)。
//! 任何一条被改动或插入，后续所有 hash 都会失配 —— 这就是"不可抵赖"的最低成本实现。

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 全局最后一条 hash（进程内），启动时从文件末尾恢复。
static LAST_HASH: Mutex<Option<String>> = Mutex::new(None);

pub fn default_path() -> PathBuf {
    super::auth::data_dir().join("audit.jsonl")
}

pub fn now_ts() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{ms}")
}

pub fn trace_id() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("t{:x}", n)
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    let d = h.finalize();
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// 进程启动时调用：从已有 audit 文件末尾恢复最后一条 hash，保证链式连续。
pub fn init_chain(path: &Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let last = text.lines().rfind(|l| !l.is_empty()).unwrap_or("");
    let mut guard = LAST_HASH.lock().unwrap_or_else(|e| e.into_inner());
    *guard = serde_json::from_str::<Value>(last).ok().and_then(|v| {
        v.get("hash")
            .and_then(|h| h.as_str().map(|s| s.to_string()))
    });
}

/// 校验整条链：返回 (ok, broken_index)。给 /audit/verify 用。
pub fn verify_chain(path: &Path) -> (bool, usize) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return (true, 0);
    };
    let mut prev = "genesis".to_string();
    for (i, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return (false, i);
        };
        let stored_prev = v.get("prev_hash").and_then(|x| x.as_str()).unwrap_or("");
        let stored_hash = v.get("hash").and_then(|x| x.as_str()).unwrap_or("");
        if stored_prev != prev {
            return (false, i);
        }
        // 重新算 hash：把 hash 字段剥掉再算
        let mut body = v.clone();
        body.as_object_mut().map(|m| m.remove("hash"));
        let body_str = serde_json::to_string(&body).unwrap_or_default();
        let recomputed = sha256_hex(body_str.as_bytes());
        if recomputed != stored_hash {
            return (false, i);
        }
        prev = stored_hash.to_string();
    }
    (true, 0)
}

pub fn write(path: &Path, mut entry: Value) {
    // 取 prev_hash
    let prev_hash = {
        let g = LAST_HASH.lock().unwrap_or_else(|e| e.into_inner());
        g.clone().unwrap_or_else(|| "genesis".to_string())
    };
    entry["prev_hash"] = json!(prev_hash);

    // 算自己 hash
    let body_str = match serde_json::to_string(&entry) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[audit] serialize failed: {e}");
            return;
        }
    };
    let hash = sha256_hex(body_str.as_bytes());
    entry["hash"] = json!(hash);

    let line = match serde_json::to_string(&entry) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[audit] serialize failed: {e}");
            return;
        }
    };

    let res = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut f| f.write_all(format!("{line}\n").as_bytes()));
    if let Err(e) = res {
        eprintln!("[audit] append failed: {e}");
    } else {
        let mut g = LAST_HASH.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            *g = v
                .get("hash")
                .and_then(|h| h.as_str())
                .map(|s| s.to_string());
        }
    }
    // 并行写 SQLite
    write_sqlite(&entry);
}

pub fn event(source: &str, action: &str, target: &str, verdict: &str, detail: Value) -> Value {
    json!({
        "ts_ms": now_ts(),
        "trace_id": trace_id(),
        "source": source,
        "action": action,
        "target": target,
        "verdict": verdict,
        "detail": detail,
    })
}

/// 读最近 n 条审计（给 /audit 查询用）
pub fn tail(path: &Path, n: usize) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return vec![];
    };
    text.lines()
        .filter(|l| !l.is_empty())
        .rev()
        .take(n)
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// 按时间范围、来源与判定检索（计划书 F-27：按项目与时间检索；S3 审计中心补判定维度）。
pub fn query(
    path: &Path,
    since_ms: Option<u64>,
    source: Option<&str>,
    verdict: Option<&str>,
    limit: usize,
) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return vec![];
    };
    text.lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| {
            if let (Some(since), Some(ts)) = (since_ms, v.get("ts_ms").and_then(|x| x.as_str())) {
                if let Ok(t) = ts.parse::<u64>() {
                    if t < since {
                        return false;
                    }
                }
            }
            if let Some(src) = source {
                if v.get("source").and_then(|x| x.as_str()) != Some(src) {
                    return false;
                }
            }
            if let Some(vd) = verdict {
                if v.get("verdict").and_then(|x| x.as_str()) != Some(vd) {
                    return false;
                }
            }
            true
        })
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// SQLite 路径：统一走 W-11 的 DbPool 归口，支持 DOU_DB_PATH 覆盖。
pub fn sqlite_path() -> std::path::PathBuf {
    crate::state::db_path()
}

/// 审计条目数。空库时 `verify_chain` 会返回 `(true, 0)`，
/// 必须配合本函数区分「空链未验证」与「链已校验通过」，避免 M2 出口假通过。
pub fn entry_count(path: &Path) -> usize {
    let Ok(text) = std::fs::read_to_string(path) else {
        return 0;
    };
    text.lines().filter(|l| !l.trim().is_empty()).count()
}

/// 初始化 SQLite 表（只建一次）
pub fn init_sqlite() {
    let Ok(conn) = crate::state::open_conn() else {
        return;
    };
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS audit (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts_ms TEXT,
            trace_id TEXT,
            source TEXT,
            action TEXT,
            target TEXT,
            verdict TEXT,
            detail TEXT,
            prev_hash TEXT,
            hash TEXT
        )",
        [],
    );
}

/// 写 SQLite（和 JSONL 并行）
pub fn write_sqlite(entry: &Value) {
    let Ok(conn) = crate::state::open_conn() else {
        return;
    };
    let _ = conn.execute(
        "INSERT INTO audit (ts_ms, trace_id, source, action, target, verdict, detail, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            entry.get("ts_ms").and_then(|v| v.as_str()).unwrap_or(""),
            entry.get("trace_id").and_then(|v| v.as_str()).unwrap_or(""),
            entry.get("source").and_then(|v| v.as_str()).unwrap_or(""),
            entry.get("action").and_then(|v| v.as_str()).unwrap_or(""),
            entry.get("target").and_then(|v| v.as_str()).unwrap_or(""),
            entry.get("verdict").and_then(|v| v.as_str()).unwrap_or(""),
            entry.get("detail").map(|v| v.to_string()).unwrap_or_default(),
            entry.get("prev_hash").and_then(|v| v.as_str()).unwrap_or(""),
            entry.get("hash").and_then(|v| v.as_str()).unwrap_or(""),
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 直接写原始 JSONL（不走 write()，避免测试期触碰真实 SQLite 审计表）。
    fn seed(path: &Path, rows: &[(&str, &str, &str)]) {
        let mut f = std::fs::File::create(path).unwrap();
        for (i, (src, vd, ts)) in rows.iter().enumerate() {
            writeln!(
                f,
                r#"{{"ts_ms":"{ts}","trace_id":"t{i}","source":"{src}","action":"exec","target":"/tmp/x","verdict":"{vd}","detail":{{}},"prev_hash":"genesis","hash":"h{i}"}}"#
            ).unwrap();
        }
    }

    fn temp_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "dou_audit_q_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn query_filters_by_source_verdict_and_time() {
        let p = temp_path();
        seed(
            &p,
            &[
                ("mcp", "allowed", "1000"),
                ("mcp", "denied", "2000"),
                ("system", "denied", "3000"),
            ],
        );
        assert_eq!(query(&p, None, None, None, 100).len(), 3);
        assert_eq!(query(&p, None, Some("mcp"), None, 100).len(), 2);
        assert_eq!(query(&p, None, None, Some("denied"), 100).len(), 2);
        assert_eq!(query(&p, None, Some("mcp"), Some("allowed"), 100).len(), 1);
        // since_ms 过滤：≥2000 的两条
        assert_eq!(query(&p, Some(2000), None, None, 100).len(), 2);
        // limit 生效（取最新的 N 条）
        assert_eq!(query(&p, None, None, None, 1).len(), 1);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn query_missing_file_returns_empty() {
        assert!(query(&temp_path(), None, None, None, 10).is_empty());
    }
}
