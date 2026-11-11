//! 接入凭据与七维作用域（计划书 4.8）。
//!
//! 三级凭据：配对码（60s 一次性，auth.rs）、设备令牌（dou_ak_，默认 24h）、
//! 会话租约（内部对象，30min）。本模块负责设备令牌的签发、哈希落库、
//! 吊销时间戳与七维作用域校验。
//!
//! 硬约束：数据库只存秘密值的哈希；明文只在签发时返回一次；
//! 吊销写时间戳而不是删记录，校验时一看吊销时间即拒。

use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

/// 七维作用域。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Scope {
    /// 绑定客户端：签发时绑定，不可改。
    pub client_id: String,
    /// 主机范围：空 = 全部禁止。
    pub hosts: Vec<String>,
    /// 工具清单：默认仅会话列表与读文件。
    pub tools: Vec<String>,
    /// 路径范围：默认用户主目录下项目目录。
    pub paths: Vec<String>,
    /// 命令模式：只读命令白名单。
    pub command_patterns: Vec<String>,
    /// 审批策略：observe | execute | change。
    pub approval_policy: String,
    /// 配额：时长(秒)、次数、并发。
    pub quota: Quota,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Quota {
    pub ttl_sec: u64,
    pub max_calls: u64,
    pub concurrency: u32,
}

/// 凭据记录（库内形态，不含明文）。
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

fn now_sec() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn hash_token(plain: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // 叠加固定盐，避免裸哈希被彩虹表反推。
    let mut h = DefaultHasher::new();
    "dou::scope::v1::".hash(&mut h);
    plain.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn rand_hex(len: usize) -> String {
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9)
        ^ (std::process::id() as u64).wrapping_mul(0x2545_F491_4F6C_DD1D);
    let mut out = String::with_capacity(len);
    while out.len() < len {
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        let v = seed.wrapping_mul(0x2545_F491_4F6C_DD1D);
        for shift in [0u32, 8, 16, 24] {
            if out.len() >= len { break; }
            out.push(char::from_digit(((v >> shift) & 0x0F) as u32, 16).unwrap_or('0'));
        }
    }
    out
}

/// 按档位给默认作用域（计划书表 17）。
pub fn default_scope(tier: &str, client_id: &str) -> Scope {
    match tier {
        "execute" => Scope {
            client_id: client_id.to_string(),
            hosts: vec![],
            tools: vec!["session_list".into(), "read_file".into(), "exec".into(), "upload".into()],
            paths: vec!["~/projects".into()],
            command_patterns: vec!["ls".into(), "cat".into(), "git status".into(), "tail".into(), "df".into()],
            approval_policy: "execute".into(),
            quota: Quota { ttl_sec: 86400, max_calls: 200, concurrency: 1 },
        },
        "change" => Scope {
            client_id: client_id.to_string(),
            hosts: vec![],
            tools: vec!["session_list".into(), "read_file".into(), "exec".into(), "upload".into(), "write".into()],
            paths: vec!["~/projects".into()],
            command_patterns: vec![],
            approval_policy: "change".into(),
            quota: Quota { ttl_sec: 86400, max_calls: 200, concurrency: 1 },
        },
        _ => Scope {
            client_id: client_id.to_string(),
            hosts: vec![],
            tools: vec!["session_list".into(), "read_file".into()],
            paths: vec!["~/projects".into()],
            command_patterns: vec![],
            approval_policy: "observe".into(),
            quota: Quota { ttl_sec: 86400, max_calls: 200, concurrency: 1 },
        },
    }
}

/// 签发设备令牌：返回明文（仅本次可见）与记录。
pub fn issue_token(tier: &str, scope: Scope) -> Result<(String, Credential), String> {
    let plain = format!("dou_ak_{}", rand_hex(24));
    let token_hash = hash_token(&plain);
    let id = format!("cred_{}", rand_hex(12));
    let expires_at = now_sec() + scope.quota.ttl_sec as i64;
    let record = Credential {
        id: id.clone(),
        token_hash: token_hash.clone(),
        tier: tier.to_string(),
        scope: scope.clone(),
        expires_at,
        revoked_at: None,
        created_at: now_sec(),
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

/// 校验令牌：未吊销、未过期才返回记录。
pub fn verify_token(plain: &str) -> Option<Credential> {
    let token_hash = hash_token(plain);
    let conn = crate::store::open().ok()?;
    let mut stmt = conn
        .prepare("SELECT id, ciphertext, tier, scope_json, expires_at, revoked_at, created_at FROM credential WHERE ciphertext=?1 AND type='token'")
        .ok()?;
    let row = stmt.query_row(rusqlite::params![token_hash], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, Option<i64>>(5)?,
            r.get::<_, i64>(6)?,
        ))
    }).ok()?;
    let (id, token_hash, tier, scope_json, expires_at, revoked_at, created_at) = row;
    if revoked_at.is_some() { return None; }
    if now_sec() > expires_at { return None; }
    let scope: Scope = serde_json::from_str(&scope_json).ok()?;
    Some(Credential { id, token_hash, tier, scope, expires_at, revoked_at, created_at })
}

/// 吊销：写时间戳，不删记录。
pub fn revoke_token(id: &str) -> Result<(), String> {
    let conn = crate::store::open()?;
    conn.execute("UPDATE credential SET revoked_at=?1 WHERE id=?2 AND revoked_at IS NULL", rusqlite::params![now_sec(), id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 工具是否在作用域内。
pub fn tool_allowed(scope: &Scope, tool: &str) -> bool {
    scope.tools.iter().any(|t| t == "*" || t == tool)
}

/// 路径是否在作用域内（前缀匹配）。
pub fn path_allowed(scope: &Scope, path: &str) -> bool {
    scope.paths.iter().any(|p| {
        p == "*" || path.starts_with(p.trim_end_matches('/'))
    })
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
