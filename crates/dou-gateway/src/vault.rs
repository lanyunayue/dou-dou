//! B1 私钥本地加密库。
//!
//! 判据（PLAN-V2 R2 B1，依赖 A5 已满足）：
//! - **落库只有密文 + 盐**：credential(type='key') 行 ciphertext =
//!   hex(12B nonce ‖ AES-256-GCM(PEM)‖tag)，salt = KEK 派生用的 vault 盐（hex）；
//!   私钥明文不落库、不进日志、不进审计、不进错误信息。
//! - **无读取明文私钥的代理接口**：HTTP 只开放 unlock / lock / status / 列表 /
//!   导入 / 删除；没有任何端点回读 PEM。`load_key_material` 仅网关内部
//!   （建连瞬间在内存解密）可调，刻意不暴露为 HTTP 路由。
//! - **落库文件检索不出明文片段**：单测以标记串断言（tests::db_never_contains_plaintext）。
//!
//! 口令派生：PBKDF2-HMAC-SHA256，210,000 迭代（OWASP 建议量级）。
//! KEK 只驻留 AppState 内存，重启即锁（重启后需重新输入解锁口令）。
//! vault_meta 表存 kek_salt 与 check_blob：check_blob 是用 KEK 加密的已知常量，
//! 解锁时解出来比对以验证口令——库里不存口令本身、也不存口令哈希。

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use pbkdf2::pbkdf2_hmac;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::Sha256;

/// PBKDF2 迭代次数：210k ≈ OWASP 2023 对 PBKDF2-HMAC-SHA256 的建议量级。
/// 解锁一次约 0.1~0.2s，属可接受的一次性代价。
pub const PBKDF2_ITERS: u32 = 210_000;

/// 解锁校验常量（加密进 check_blob；解锁时解出来比对）。
const CHECK_PLAIN: &[u8] = b"dou::vault::check::v1";

/// AES-GCM nonce 字节数（标准 96bit）。
const NONCE_LEN: usize = 12;

// ---------- 编码 ----------

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_val(c: u8) -> Result<u8, String> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(format!("非法 hex 字符: {:?}", c as char)),
    }
}

fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(2) {
        return Err("hex 长度为奇数".into());
    }
    let mut out = Vec::with_capacity(b.len() / 2);
    for i in (0..b.len()).step_by(2) {
        out.push((hex_val(b[i])? << 4) | hex_val(b[i + 1])?);
    }
    Ok(out)
}

// ---------- 密码学原语 ----------

/// PBKDF2-HMAC-SHA256 → 32B KEK。
pub fn derive_kek(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    let mut dk = [0u8; 32];
    pbkdf2_hmac::<Sha256>(passphrase.as_bytes(), salt, PBKDF2_ITERS, &mut dk);
    dk
}

/// AES-256-GCM 加密，返回 hex(nonce ‖ ciphertext‖tag)。nonce 每次 12B 随机。
pub fn seal(plaintext: &[u8], kek: &[u8; 32]) -> Result<String, String> {
    let cipher = Aes256Gcm::new_from_slice(kek).map_err(|e| format!("KEK 长度异常: {e}"))?;
    let mut nb = [0u8; NONCE_LEN];
    getrandom::fill(&mut nb).map_err(|e| format!("CSPRNG 不可用: {e}"))?;
    let nonce = Nonce::try_from(nb.as_slice()).map_err(|_| "加密失败".to_string())?;
    let ct = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|_| "加密失败".to_string())?;
    let mut blob = nb.to_vec();
    blob.extend_from_slice(&ct);
    Ok(to_hex(&blob))
}

/// AES-256-GCM 解密。失败 = 密文损坏或 KEK 不对（认证标签不匹配）。
pub fn unseal(blob_hex: &str, kek: &[u8; 32]) -> Result<Vec<u8>, String> {
    let blob = from_hex(blob_hex)?;
    if blob.len() < NONCE_LEN + 16 {
        return Err("密文长度异常".into());
    }
    let cipher = Aes256Gcm::new_from_slice(kek).map_err(|e| format!("KEK 长度异常: {e}"))?;
    let (nb, ct) = blob.split_at(NONCE_LEN);
    let nonce = Nonce::try_from(nb).map_err(|_| "解密失败：密文损坏或口令不正确".to_string())?;
    cipher
        .decrypt(&nonce, ct)
        .map_err(|_| "解密失败：密文损坏或口令不正确".to_string())
}

// ---------- vault_meta：解锁口令验证 ----------

/// 建表（幂等）。main() 启动时调用一次。
pub fn init_tables() -> Result<(), String> {
    let conn = crate::state::open_conn()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS vault_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
    )
    .map_err(|e| format!("建 vault_meta: {e}"))
}

/// 解锁（全局库形态，供 main.rs 端点调用）。
pub fn unlock(passphrase: &str) -> Result<[u8; 32], String> {
    unlock_on(&crate::state::open_conn()?, passphrase)
}

/// 解锁核心（conn 注入形态，可测）。首次解锁生成盐与校验块；
/// 之后解锁派生 KEK 并以 check_blob 验口令。口令错误 → Err。
pub fn unlock_on(conn: &Connection, passphrase: &str) -> Result<[u8; 32], String> {
    if passphrase.chars().count() < 8 {
        return Err("解锁口令至少 8 个字符".into());
    }
    let salt_row: Option<String> = conn
        .query_row(
            "SELECT value FROM vault_meta WHERE key='kek_salt'",
            [],
            |r| r.get(0),
        )
        .ok();
    match salt_row {
        None => {
            let mut salt = [0u8; 32];
            getrandom::fill(&mut salt).map_err(|e| format!("CSPRNG 不可用: {e}"))?;
            let kek = derive_kek(passphrase, &salt);
            let check = seal(CHECK_PLAIN, &kek)?;
            conn.execute(
                "INSERT OR REPLACE INTO vault_meta (key, value) VALUES ('kek_salt', ?1)",
                params![to_hex(&salt)],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT OR REPLACE INTO vault_meta (key, value) VALUES ('check_blob', ?1)",
                params![check],
            )
            .map_err(|e| e.to_string())?;
            Ok(kek)
        }
        Some(salt_hex) => {
            let salt = from_hex(&salt_hex)?;
            let kek = derive_kek(passphrase, &salt);
            let check_blob: String = conn
                .query_row(
                    "SELECT value FROM vault_meta WHERE key='check_blob'",
                    [],
                    |r| r.get(0),
                )
                .map_err(|e| format!("vault 校验块缺失（库损坏，拒绝解锁）: {e}"))?;
            let plain = unseal(&check_blob, &kek).map_err(|_| "解锁口令不正确".to_string())?;
            if plain != CHECK_PLAIN {
                return Err("解锁口令不正确".into());
            }
            Ok(kek)
        }
    }
}

// ---------- 私钥记录存取（credential 表 type='key'） ----------

/// 形状校验：必须带 PEM 私钥标记。格式深校验交给建连时的 russh 解码。
fn looks_like_private_key(pem: &str) -> bool {
    pem.contains("-----BEGIN") && pem.contains("PRIVATE KEY-----")
}

/// 导入：加密落库，返回记录 id。明文只在本函数栈内存在，不入任何日志/审计。
pub fn import_key(
    conn: &Connection,
    kek: &[u8; 32],
    host_id: Option<&str>,
    label: &str,
    pem: &str,
) -> Result<String, String> {
    if !looks_like_private_key(pem) {
        return Err("不像私钥：缺少 -----BEGIN ... PRIVATE KEY----- 标记".into());
    }
    let ct = seal(pem.as_bytes(), kek)?;
    let salt_hex: String = conn
        .query_row(
            "SELECT value FROM vault_meta WHERE key='kek_salt'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("vault 盐缺失（库未初始化）: {e}"))?;
    let id = format!("key_{}", crate::auth::rand_hex(12));
    conn.execute(
        "INSERT INTO credential (id, type, ciphertext, salt, host_id, created_at, label)
         VALUES (?1,'key',?2,?3,?4,?5,?6)",
        params![id, ct, salt_hex, host_id, crate::state::now_ms(), label],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

/// 列表：只有 id / label / host_id / created_at，**绝不回传 ciphertext / salt**。
pub fn list_keys(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT id, label, host_id, created_at FROM credential WHERE type='key'
             ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "label": r.get::<_, Option<String>>(1)?,
                "host_id": r.get::<_, Option<String>>(2)?,
                "created_at": r.get::<_, i64>(3)?,
            }))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// 删除（硬删：吊销后密文仍是无可依附的风险面，直接移除）。
/// 返回是否确有删除。删除动作本身写审计（调用方负责）。
pub fn delete_key(conn: &Connection, id: &str) -> Result<bool, String> {
    let n = conn
        .execute(
            "DELETE FROM credential WHERE id=?1 AND type='key'",
            params![id],
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

/// 解密单条记录（**网关内部专用**：仅建连瞬间在内存使用，无对应 HTTP 端点）。
pub fn load_key_material(conn: &Connection, kek: &[u8; 32], id: &str) -> Result<String, String> {
    let ct: String = conn
        .query_row(
            "SELECT ciphertext FROM credential WHERE id=?1 AND type='key'",
            params![id],
            |r| r.get(0),
        )
        .map_err(|e| format!("私钥记录不存在: {e}"))?;
    let plain = unseal(&ct, kek)?;
    String::from_utf8(plain).map_err(|_| "解密后的私钥不是合法 UTF-8".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 打开带 credential/vault_meta 表的内存库（不碰全局 DOU_DB_PATH）。
    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().expect("内存库");
        conn.execute_batch(
            "CREATE TABLE credential (
                id         TEXT PRIMARY KEY,
                type       TEXT NOT NULL,
                ciphertext TEXT NOT NULL,
                salt       TEXT NOT NULL,
                host_id    TEXT,
                created_at INTEGER NOT NULL,
                label      TEXT
            );
            CREATE TABLE vault_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .expect("建表");
        conn
    }

    const SAMPLE_PEM: &str = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAAMARKERFRAG0000000000\n-----END OPENSSH PRIVATE KEY-----\n";

    #[test]
    fn pbkdf2_matches_official_vectors() {
        // hashlib.pbkdf2_hmac('sha256', b'password', b'salt', c, 32) 基准（本机 python 生成），
        // 同时是广泛引用的标准 PBKDF2-HMAC-SHA256 测试向量。
        let cases: [(&str, u32, &str); 3] = [
            (
                "password",
                1,
                "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b",
            ),
            (
                "password",
                2,
                "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43",
            ),
            (
                "password",
                4096,
                "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a",
            ),
        ];
        for (pass, rounds, expect) in cases {
            let mut dk = [0u8; 32];
            pbkdf2_hmac::<Sha256>(pass.as_bytes(), b"salt", rounds, &mut dk);
            assert_eq!(to_hex(&dk), expect, "rounds={rounds}");
        }
    }

    #[test]
    fn seal_open_roundtrip_and_tamper_detection() {
        let kek = [7u8; 32];
        let blob = seal(SAMPLE_PEM.as_bytes(), &kek).unwrap();
        assert_ne!(blob, to_hex(SAMPLE_PEM.as_bytes()), "密文不得等于明文 hex");
        assert_eq!(unseal(&blob, &kek).unwrap(), SAMPLE_PEM.as_bytes());

        // 篡改密文区最后一个 hex 字符 → 认证标签必败
        let mut bad = blob.clone();
        let last = bad.pop().unwrap();
        bad.push(if last == '0' { '1' } else { '0' });
        assert!(unseal(&bad, &kek).is_err(), "篡改必须被检测到");

        // 错 KEK 必败
        let kek2 = [8u8; 32];
        assert!(unseal(&blob, &kek2).is_err());
    }

    #[test]
    fn nonce_is_random_per_seal() {
        let kek = [1u8; 32];
        let a = seal(b"same plaintext", &kek).unwrap();
        let b = seal(b"same plaintext", &kek).unwrap();
        assert_ne!(a, b, "同明文两次加密不得得到相同密文（nonce 随机）");
    }

    #[test]
    fn db_never_contains_plaintext_fragments() {
        let conn = mem_db();
        conn.execute("INSERT INTO vault_meta VALUES ('kek_salt','aabbccdd')", [])
            .unwrap();
        let kek = [9u8; 32];
        let id = import_key(&conn, &kek, None, "自测钥", SAMPLE_PEM).unwrap();

        // 全表文本字段扫描：明文标记串与 PEM 头都不得出现
        let mut stmt = conn
            .prepare("SELECT id, type, ciphertext, salt, IFNULL(host_id,''), IFNULL(label,'') FROM credential")
            .unwrap();
        let rows = stmt
            .query_map([], |r| {
                Ok(format!(
                    "{}|{}|{}|{}|{}|{}",
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?
                ))
            })
            .unwrap();
        let mut all = String::new();
        for row in rows {
            all.push_str(&row.unwrap());
        }
        assert!(!all.contains("MARKERFRAG"), "明文片段出现在库中");
        assert!(!all.contains("PRIVATE KEY"), "PEM 头出现在库中");
        assert!(all.len() > SAMPLE_PEM.len() / 2, "扫描范围应覆盖全部行");

        // 列表输出不含任何材料字段
        let items = list_keys(&conn).unwrap();
        assert_eq!(items.len(), 1);
        let j = serde_json::to_string(&items[0]).unwrap();
        assert!(!j.contains("ciphertext") && !j.contains("salt"));
        assert!(j.contains("自测钥"));

        // 内部取材料 roundtrip + 删除闭环
        assert_eq!(load_key_material(&conn, &kek, &id).unwrap(), SAMPLE_PEM);
        assert!(delete_key(&conn, &id).unwrap());
        assert!(list_keys(&conn).unwrap().is_empty());
        assert!(load_key_material(&conn, &kek, &id).is_err());
    }

    #[test]
    fn import_rejects_non_pem_and_locked_salt() {
        let conn = mem_db();
        let kek = [3u8; 32];
        assert!(import_key(&conn, &kek, None, "", "hello world").is_err());
        // vault 盐缺失（未初始化）也必须拒绝
        let pem = "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n";
        assert!(import_key(&conn, &kek, None, "", pem).is_err());
    }

    #[test]
    fn unlock_verifies_passphrase_via_check_blob() {
        let conn = mem_db();
        assert!(unlock_on(&conn, "short").is_err(), "过短口令必须拒绝");

        let kek = unlock_on(&conn, "correct-passphrase").unwrap();
        // 错口令 → Err（PBKDF2 派生不同 → check_blob 解密失败）
        assert!(unlock_on(&conn, "wrong-passphrase").is_err());
        // 正口令 → 再次解锁成功
        let kek2 = unlock_on(&conn, "correct-passphrase").unwrap();
        assert_eq!(kek, kek2);

        // 盐存在但校验块缺失 → 拒绝解锁（fail-closed）
        let conn2 = mem_db();
        conn2
            .execute("INSERT INTO vault_meta VALUES ('kek_salt','00112233')", [])
            .unwrap();
        assert!(unlock_on(&conn2, "correct-passphrase").is_err());
    }

    #[test]
    fn from_hex_rejects_odd_len_and_bad_chars() {
        assert!(from_hex("abc").is_err(), "奇数长度必须报错");
        assert!(from_hex("zz").is_err(), "非法 hex 字符必须报错");
        assert_eq!(from_hex("0fa9").unwrap(), vec![0x0f, 0xa9]);
        assert_eq!(from_hex("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn unseal_rejects_too_short_blob_fail_closed() {
        let kek = [2u8; 32];
        // 远小于 nonce(12)+tag(16) 的密文，必须 fail-closed 不 panic
        assert!(unseal("00", &kek).is_err());
        assert!(unseal("", &kek).is_err());
    }
}
