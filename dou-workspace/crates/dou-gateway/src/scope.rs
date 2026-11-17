//! 接入凭据与七维作用域（计划书 4.8）。
//!
//! 本轮安全升级：
//! - `hash_token` 由 `DefaultHasher`（SipHash，非密码学）改为
//!   **sha256(固定盐 ‖ 明文)**；高熵令牌无需慢哈希（对应 05 §2 D7）。
//! - 令牌生成统一走 `auth::rand_hex`（OS CSPRNG），删除本模块 xorshift 副本。
//! - 时间单位统一为 **epoch 毫秒 INTEGER**；由 `state::migrate_time_units()`
//!   对旧库做幂等 ×1000 迁移。
//! - 数据库只存哈希；明文仅签发时返回一次；吊销写时间戳而非删记录。

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// 七维作用域。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Scope {
    pub client_id: String,
    pub hosts: Vec<String>,
    pub tools: Vec<String>,
    pub paths: Vec<String>,
    pub command_patterns: Vec<String>,
    pub approval_policy: String,
    pub quota: Quota,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Quota {
    pub ttl_sec: u64,
    pub max_calls: u64,
    pub concurrency: u32,
}

/// 凭据记录（库内形态，不含明文）。`expires_at` / `created_at` / `revoked_at`
/// 单位均为 **epoch 毫秒**。
#[derive(Debug, Clone)]
pub struct Credential {
    pub id: String,
    pub token_hash: String,
    pub tier: String,
    pub scope: Scope,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
}

/// 密码学哈希：sha256(盐 ‖ 明文)，十六进制。
pub fn hash_token(plain: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"dou::scope::v2::");
    h.update(plain.as_bytes());
    let d = h.finalize();
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// 按档位给默认作用域（计划书表 17）。
pub fn default_scope(tier: &str, client_id: &str) -> Scope {
    match tier {
        "execute" => Scope {
            client_id: client_id.to_string(),
            hosts: vec![],
            tools: vec![
                "session_list".into(),
                "read_file".into(),
                "exec".into(),
                "upload".into(),
            ],
            paths: vec!["~/projects".into()],
            command_patterns: vec![
                "ls".into(),
                "cat".into(),
                "git status".into(),
                "tail".into(),
                "df".into(),
            ],
            approval_policy: "execute".into(),
            quota: Quota {
                ttl_sec: 86400,
                max_calls: 200,
                concurrency: 1,
            },
        },
        "change" => Scope {
            client_id: client_id.to_string(),
            hosts: vec![],
            tools: vec![
                "session_list".into(),
                "read_file".into(),
                "exec".into(),
                "upload".into(),
                "write".into(),
            ],
            paths: vec!["~/projects".into()],
            command_patterns: vec![],
            approval_policy: "change".into(),
            quota: Quota {
                ttl_sec: 86400,
                max_calls: 200,
                concurrency: 1,
            },
        },
        _ => Scope {
            client_id: client_id.to_string(),
            hosts: vec![],
            tools: vec!["session_list".into(), "read_file".into()],
            paths: vec!["~/projects".into()],
            command_patterns: vec![],
            approval_policy: "observe".into(),
            quota: Quota {
                ttl_sec: 86400,
                max_calls: 200,
                concurrency: 1,
            },
        },
    }
}

/// 签发设备令牌：返回明文（仅本次可见）与记录。
pub fn issue_token(tier: &str, scope: Scope) -> Result<(String, Credential), String> {
    let plain = format!("dou_ak_{}", crate::auth::rand_hex(24));
    let token_hash = hash_token(&plain);
    let id = format!("cred_{}", crate::auth::rand_hex(12));
    let now = crate::state::now_ms();
    let expires_at = now.saturating_add((scope.quota.ttl_sec as i64).saturating_mul(1000));
    let record = Credential {
        id: id.clone(),
        token_hash: token_hash.clone(),
        tier: tier.to_string(),
        scope: scope.clone(),
        expires_at,
        revoked_at: None,
        created_at: now,
    };
    let conn = crate::store::open()?;
    let scope_json = serde_json::to_string(&scope).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO credential (id, type, ciphertext, salt, host_id, created_at, scope_json, tier, client_id, expires_at, revoked_at)
         VALUES (?1,'token',?2,'',NULL,?3,?4,?5,?6,?7,NULL)",
        rusqlite::params![id, token_hash, record.created_at, scope_json, tier, scope.client_id, expires_at],
    ).map_err(|e| e.to_string())?;
    Ok((plain, record))
}

/// 校验令牌：未吊销、未过期才返回记录。时间基准为 epoch 毫秒。
pub fn verify_token(plain: &str) -> Option<Credential> {
    let token_hash = hash_token(plain);
    let conn = crate::store::open().ok()?;
    let mut stmt = conn
        .prepare("SELECT id, ciphertext, tier, scope_json, expires_at, revoked_at, created_at FROM credential WHERE ciphertext=?1 AND type='token'")
        .ok()?;
    let row = stmt
        .query_row(rusqlite::params![token_hash], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })
        .ok()?;
    let (id, token_hash, tier, scope_json, expires_at, revoked_at, created_at) = row;
    if revoked_at.is_some() {
        return None;
    }
    if crate::state::now_ms() > expires_at {
        return None;
    }
    let scope: Scope = serde_json::from_str(&scope_json).ok()?;
    Some(Credential {
        id,
        token_hash,
        tier,
        scope,
        expires_at,
        revoked_at,
        created_at,
    })
}

/// 吊销：写时间戳（epoch 毫秒），不删记录。
pub fn revoke_token(id: &str) -> Result<(), String> {
    let conn = crate::store::open()?;
    conn.execute(
        "UPDATE credential SET revoked_at=?1 WHERE id=?2 AND revoked_at IS NULL",
        rusqlite::params![crate::state::now_ms(), id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn tool_allowed(scope: &Scope, tool: &str) -> bool {
    scope.tools.iter().any(|t| t == "*" || t == tool)
}

pub fn path_allowed(scope: &Scope, path: &str) -> bool {
    scope
        .paths
        .iter()
        .any(|p| p == "*" || path.starts_with(p.trim_end_matches('/')))
}

/// 列出当前有效凭据（不含明文）。
pub fn list_active() -> Result<Vec<Value>, String> {
    let conn = crate::store::open()?;
    let mut stmt = conn.prepare(
        "SELECT id, tier, scope_json, client_id, expires_at, revoked_at, created_at FROM credential WHERE type='token' ORDER BY created_at DESC",
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| {
        Ok(json!({
            "id": r.get::<_, String>(0)?,
            "tier": r.get::<_, String>(1)?,
            "scope": serde_json::from_str::<Value>(&r.get::<_, String>(2)?).unwrap_or(Value::Null),
            "client_id": r.get::<_, String>(3)?,
            "expires_at": r.get::<_, i64>(4)?,
            "revoked_at": r.get::<_, Option<i64>>(5)?,
            "created_at": r.get::<_, i64>(6)?,
        }))
    }).map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_cryptographic() {
        let a = hash_token("dou_ak_test");
        let b = hash_token("dou_ak_test");
        let c = hash_token("dou_ak_tesu");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64, "应为 sha256 十六进制");
    }

    #[test]
    fn default_scope_is_least_privilege() {
        let s = default_scope("observe", "c1");
        assert_eq!(s.approval_policy, "observe");
        assert!(!s.tools.iter().any(|t| t == "exec"));
    }
}
