//! 凭据保险库：主密码加密存储目标连接档案 + 私钥（参考 Xftp 主密码保险库）。
//!
//! 密钥派生：ring PBKDF2-HMAC-SHA256；对称加密：aes-gcm AES-256-GCM；随机数：ring SystemRandom。
//! 全程离线可用（ring / aes-gcm / base64 已在 Cargo.lock 中）。
//!
//! 语义（非破坏性）：
//!   - `vault_init(master)` 生成随机 salt 并写入加密的校验串（用于解锁时验证主密码）；
//!   - `vault_unlock(master)` 派生密钥并验证校验串，密钥仅驻留内存；
//!   - `vault_lock()` 仅清除内存中的密钥（不删除磁盘上的任何文件）；
//!   - `vault_add_target(...)` 把某目标的私钥加密入库；
//!   - `vault_get_key(name)` 解密并把明文私钥落到标准路径，供现有 SSH 命令直接使用。

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce as AesNonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ring::rand::SecureRandom;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

use crate::ssh;

const VERIFIER_PLAINTEXT: &[u8] = b"dou-vault-ok";
const ITERATIONS: u32 = 200_000;
const KEY_LEN: usize = 32;

#[derive(Clone, Serialize, Deserialize)]
pub struct VaultTarget {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 加密后的私钥（base64），仅解锁后可解密
    pub enc_key: Option<String>,
    pub nonce: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct VaultFile {
    salt: String, // base64
    verifier: String, // base64(encrypt(VERIFIER_PLAINTEXT))
    verifier_nonce: String, // base64
    targets: Vec<VaultTarget>,
}

pub struct VaultState {
    unlocked: Mutex<bool>,
    key: Mutex<Option<Vec<u8>>>,
    path: PathBuf,
}

impl Default for VaultState {
    fn default() -> Self {
        VaultState {
            unlocked: Mutex::new(false),
            key: Mutex::new(None),
            path: ssh::app_data_dir().join("vault.json"),
        }
    }
}

fn b64_encode(b: &[u8]) -> String {
    B64.encode(b)
}
fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    B64.decode(s).map_err(|e| e.to_string())
}

fn derive_key(master: &str, salt: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; KEY_LEN];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(ITERATIONS).unwrap(),
        salt,
        master.as_bytes(),
        &mut out,
    );
    out
}

fn random_bytes(n: usize) -> Vec<u8> {
    let rng = ring::rand::SystemRandom::new();
    let mut buf = vec![0u8; n];
    rng.fill(&mut buf).unwrap();
    buf
}

fn aes_encrypt(key: &[u8], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce_bytes = random_bytes(12);
    let nonce = AesNonce::from_slice(&nonce_bytes);
    let ct = cipher.encrypt(nonce, plaintext).map_err(|e| e.to_string())?;
    Ok((ct, nonce_bytes))
}

fn aes_decrypt(key: &[u8], ciphertext: &[u8], nonce_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = AesNonce::from_slice(nonce_bytes);
    cipher.decrypt(nonce, ciphertext).map_err(|e| e.to_string())
}

fn read_vault(path: &PathBuf) -> VaultFile {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
fn write_vault(path: &PathBuf, v: &VaultFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, serde_json::to_string_pretty(v).unwrap_or_default())
        .map_err(|e| e.to_string())
}

// ------------------------- 命令 -------------------------

pub fn vault_init(state: State<VaultState>, master: String) -> Result<(), String> {
    if master.is_empty() {
        return Err("主密码不能为空".into());
    }
    let salt = random_bytes(16);
    let key = derive_key(&master, &salt);
    let (verifier, vnonce) = aes_encrypt(&key, VERIFIER_PLAINTEXT)?;
    let vf = VaultFile {
        salt: b64_encode(&salt),
        verifier: b64_encode(&verifier),
        verifier_nonce: b64_encode(&vnonce),
        targets: vec![],
    };
    write_vault(&state.path, &vf)?;
    *state.key.lock().unwrap() = Some(key);
    *state.unlocked.lock().unwrap() = true;
    Ok(())
}

pub fn vault_unlock(state: State<VaultState>, master: String) -> Result<(), String> {
    let vf = read_vault(&state.path);
    if vf.salt.is_empty() {
        return Err("保险库未初始化，请先 vault_init".into());
    }
    let salt = b64_decode(&vf.salt)?;
    let key = derive_key(&master, &salt);
    let verifier = b64_decode(&vf.verifier)?;
    let vnonce = b64_decode(&vf.verifier_nonce)?;
    let dec = aes_decrypt(&key, &verifier, &vnonce)?;
    if dec != VERIFIER_PLAINTEXT {
        return Err("主密码错误".into());
    }
    *state.key.lock().unwrap() = Some(key);
    *state.unlocked.lock().unwrap() = true;
    Ok(())
}

pub fn vault_lock(state: State<VaultState>) {
    *state.key.lock().unwrap() = None;
    *state.unlocked.lock().unwrap() = false;
    // 仅清除内存密钥；不删除磁盘上任何文件（非破坏性）。
}

pub fn vault_status(state: State<VaultState>) -> bool {
    *state.unlocked.lock().unwrap()
}

/// 保险库锁定态的 fail-closed 闸门（纯函数，便于单测）。
///
/// 档案级操作（列举 / 删除 / 读取私钥）在锁定态必须一律拒绝，
/// 否则锁定只是"形式上的锁"：目标清单（host/port/user 明文）仍会暴露，
/// 且条目仍可被删除。
fn ensure_unlocked(unlocked: bool) -> Result<(), String> {
    if unlocked {
        Ok(())
    } else {
        Err(String::from("保险库已锁定，请先解锁"))
    }
}

pub fn vault_list_targets(state: State<VaultState>) -> Result<Vec<VaultTarget>, String> {
    ensure_unlocked(*state.unlocked.lock().unwrap())?;
    Ok(read_vault(&state.path).targets)
}

pub fn vault_add_target(
    state: State<VaultState>,
    name: String,
    host: String,
    port: u16,
    user: String,
    key_path: String,
) -> Result<(), String> {
    let key = state
        .key
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| String::from("保险库已锁定，请先解锁"))?;
    let plaintext = std::fs::read(&key_path).map_err(|e| format!("读取私钥: {e}"))?;
    let (enc, nonce) = aes_encrypt(&key, &plaintext)?;
    let mut vf = read_vault(&state.path);
    vf.targets.retain(|t| t.name != name);
    vf.targets.push(VaultTarget {
        name,
        host,
        port,
        user,
        enc_key: Some(b64_encode(&enc)),
        nonce: Some(b64_encode(&nonce)),
    });
    write_vault(&state.path, &vf)?;
    Ok(())
}

pub fn vault_get_key(state: State<VaultState>, name: String) -> Result<String, String> {
    let key = state
        .key
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| String::from("保险库已锁定，请先解锁"))?;
    let vf = read_vault(&state.path);
    let t = vf
        .targets
        .into_iter()
        .find(|t| t.name == name)
        .ok_or_else(|| String::from("目标不存在"))?;
    let enc = b64_decode(t.enc_key.ok_or_else(|| String::from("该目标无加密密钥"))?.as_str())?;
    let nonce = b64_decode(t.nonce.ok_or_else(|| String::from("该目标无 nonce"))?.as_str())?;
    let dec = aes_decrypt(&key, &enc, &nonce)?;
    // 解锁后把明文私钥落到标准路径，供现有 SSH 命令直接使用（锁定不删除）。
    let dir = ssh::app_data_dir().join("keys");
    let _ = std::fs::create_dir_all(&dir);
    let kp = dir.join("id_ed25519");
    std::fs::write(&kp, &dec).map_err(|e| e.to_string())?;
    Ok(kp.to_string_lossy().to_string())
}

pub fn vault_remove_target(state: State<VaultState>, name: String) -> Result<(), String> {
    ensure_unlocked(*state.unlocked.lock().unwrap())?;
    let mut vf = read_vault(&state.path);
    vf.targets.retain(|t| t.name != name);
    write_vault(&state.path, &vf)?;
    Ok(())
}

// 解锁后向前端广播状态变化（前端据此刷新面板）。
pub fn vault_notify(app: AppHandle, unlocked: bool) {
    let _ = app.emit("vault_status", unlocked);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_encrypt_roundtrip_and_wrong_password_fails() {
        let salt = random_bytes(16);
        let key = derive_key("master-pass", &salt);
        let pt = b"secret-key-bytes";
        let (ct, nonce) = aes_encrypt(&key, pt).unwrap();
        let dec = aes_decrypt(&key, &ct, &nonce).unwrap();
        assert_eq!(dec, pt);
        let wrong = derive_key("wrong-password", &salt);
        assert!(aes_decrypt(&wrong, &ct, &nonce).is_err());
    }

    /// P0-1 回归：锁定态必须 fail-closed，拒绝档案级操作。
    #[test]
    fn ensure_unlocked_rejects_when_locked() {
        let err = ensure_unlocked(false).unwrap_err();
        assert_eq!(err, "保险库已锁定，请先解锁");
        assert!(ensure_unlocked(true).is_ok());
    }

    /// P0-1 回归：锁定时不能读到目标档案，解锁后可以。
    #[test]
    fn vault_list_targets_is_blocked_while_locked() {
        // 用临时路径构造一个 VaultState（State 包装需 tauri runtime，
        // 故此处直接验证底层 read_vault + ensure_unlocked 的组合语义）。
        let dir = std::env::temp_dir().join("dou_vault_lock_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("vault.json");

        let vf = VaultFile {
            salt: b64_encode(&random_bytes(16)),
            verifier: b64_encode(b"v"),
            verifier_nonce: b64_encode(&random_bytes(12)),
            targets: vec![VaultTarget {
                name: "t1".into(),
                host: "10.0.0.1".into(),
                port: 22,
                user: "u".into(),
                enc_key: None,
                nonce: None,
            }],
        };
        write_vault(&path, &vf).unwrap();

        // 锁定态：闸门必须拦住，调用方拿不到 targets。
        assert!(ensure_unlocked(false).is_err());
        // 归档数据确实在磁盘上（说明拦截来自闸门而非数据缺失）。
        assert_eq!(read_vault(&path).targets.len(), 1);

        // 解锁态：放行。
        ensure_unlocked(true).unwrap();
        assert_eq!(read_vault(&path).targets.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
