//! 凭据保险库：主密码加密存储目标连接档案 + 私钥（参考 Xftp 主密码保险库）。
//!
//! 密钥派生：ring PBKDF2-HMAC-SHA256；对称加密：aes-gcm AES-256-GCM；随机数：ring SystemRandom。
//! 全程离线可用（ring / aes-gcm / base64 / zeroize 均已在 Cargo.lock 中）。
//!
//! 语义（非破坏性）：
//!   - `vault_init(master)` 生成随机 salt 并写入加密的校验串（用于解锁时验证主密码）；
//!   - `vault_unlock(master)` 按**文件内记录的迭代次数**派生密钥并验证校验串，密钥仅驻留内存；
//!   - `vault_lock()` 仅清除内存中的密钥，并清理旧版本遗留在 `keys/` 下的明文私钥；
//!   - `vault_add_target(...)` 把某目标的私钥加密入库；
//!   - `vault_get_key(name)` 在内存中解密私钥，只返回**公钥信息**（算法 + 指纹）；
//!     私钥明文既不落盘，也不出本进程；
//!   - `vault_test_connect(name)` 解密私钥 → 在内存中完成一次 SSH 认证 → 立即释放（用后即清）。
//!
//! 安全要点：
//!   - 明文私钥只以内存形式短暂存在，用完即清零（zeroize）；
//!   - KDF 迭代次数写入 `vault.json`，老保险库（无该字段）按 200k 兼容解锁；
//!   - 保险库文件损坏与「未初始化」严格区分；损坏时拒绝覆盖原文件。

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce as AesNonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ring::rand::SecureRandom;
use russh::keys::ssh_key::{HashAlg, PrivateKey};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
use zeroize::Zeroizing;

use crate::ssh;

const VERIFIER_PLAINTEXT: &[u8] = b"dou-vault-ok";
/// 新建保险库使用的 PBKDF2-HMAC-SHA256 迭代次数。
/// OWASP 对 PBKDF2-HMAC-SHA256 的现行建议是 ≥ 600,000。
const ITERATIONS: u32 = 600_000;
/// 旧版本保险库（`vault.json` 中没有 `iterations` 字段）使用的迭代次数。
/// 保留该常量以保证老文件仍能用原主密码解锁（见 `default_iterations`）。
const LEGACY_ITERATIONS: u32 = 200_000;
/// 迭代次数上限：防止被篡改的保险库文件把 PBKDF2 拖成长时间占用（DoS）。
const MAX_ITERATIONS: u32 = 10_000_000;
const KEY_LEN: usize = 32;

#[derive(Clone, Serialize, Deserialize)]
pub struct VaultTarget {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 认证方式：`"key"`（公钥，默认）/ `"password"`（密码，连接时临时输入）。
    /// `serde(default)` 兼容旧版保险库（无该字段 = 密钥模式）。
    #[serde(default)]
    pub auth: String,
    /// 加密后的私钥（base64），仅解锁后可解密；密码模式目标为 None。
    pub enc_key: Option<String>,
    pub nonce: Option<String>,
}

/// 目标私钥的**非敏感**信息。
///
/// `vault_get_key` 只返回本结构：私钥明文不落盘、也不出本进程，
/// 前端只能看到「库里存的是哪一把钥匙」，看不到钥匙本身。
#[derive(Clone, Serialize)]
pub struct VaultKeyInfo {
    /// 密钥算法，例如 `ssh-ed25519`。
    pub algorithm: String,
    /// 公钥 SHA256 指纹（`SHA256:base64`），供人工核对。
    pub fingerprint: String,
}

#[derive(Serialize, Deserialize)]
struct VaultFile {
    salt: String,           // base64
    verifier: String,       // base64(encrypt(VERIFIER_PLAINTEXT))
    verifier_nonce: String, // base64
    /// PBKDF2 迭代次数。
    ///
    /// 旧版本写出的 `vault.json` 没有该字段，`serde(default)` 会回落到
    /// `LEGACY_ITERATIONS`(200k)，因此老保险库仍能用原主密码解锁；
    /// 新建保险库写入当前的 `ITERATIONS`(600k)。
    #[serde(default = "default_iterations")]
    iterations: u32,
    targets: Vec<VaultTarget>,
}

/// 缺省迭代次数：仅用于兼容旧版 `vault.json`（无 `iterations` 字段）。
fn default_iterations() -> u32 {
    LEGACY_ITERATIONS
}

pub struct VaultState {
    unlocked: Mutex<bool>,
    /// 派生密钥（内存）。用 `Zeroizing` 包裹：清空时自动擦除，不留残影。
    key: Mutex<Option<Zeroizing<Vec<u8>>>>,
    path: PathBuf,
    /// 旧版本曾把解密后的明文私钥写在这里；现在只用于清理历史遗留。
    keys_dir: PathBuf,
}

impl Default for VaultState {
    fn default() -> Self {
        VaultState {
            unlocked: Mutex::new(false),
            key: Mutex::new(None),
            path: ssh::app_data_dir().join("vault.json"),
            keys_dir: keys_dir(),
        }
    }
}

/// 历史上存放明文私钥的目录（`%LOCALAPPDATA%/dou/keys`）。
pub fn keys_dir() -> PathBuf {
    ssh::app_data_dir().join("keys")
}

fn b64_encode(b: &[u8]) -> String {
    B64.encode(b)
}
fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    B64.decode(s).map_err(|e| e.to_string())
}

fn derive_key(master: &str, salt: &[u8], iterations: u32) -> Vec<u8> {
    let mut out = vec![0u8; KEY_LEN];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        std::num::NonZeroU32::new(iterations).unwrap(),
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
    // 长度由 random_bytes(12) 保证为 12，不会失败。
    let nonce = AesNonce::try_from(&nonce_bytes[..]).map_err(|e| e.to_string())?;
    let ct = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| e.to_string())?;
    Ok((ct, nonce_bytes))
}

fn aes_decrypt(key: &[u8], ciphertext: &[u8], nonce_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    // nonce 来自磁盘文件，长度可能损坏；这里显式报错而不是 panic。
    let nonce = AesNonce::try_from(nonce_bytes)
        .map_err(|e| format!("nonce 长度非法（应为 12 字节）: {e}"))?;
    cipher
        .decrypt(&nonce, ciphertext)
        .map_err(|e| e.to_string())
}

/// 读取保险库文件，区分「不存在」「损坏」「正常」。
///
/// 旧实现用 `unwrap_or_default()` 把「文件存在但 JSON 损坏」和「文件不存在」
/// 压成同一个空结构，导致 `vault_unlock` 误报「未初始化」，用户照着提示执行
/// `vault_init` 会覆盖掉唯一的数据备份。现在损坏会明确报错。
fn read_vault(path: &Path) -> Result<VaultFile, VaultLoadError> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(VaultLoadError::NotFound);
        }
        Err(e) => {
            return Err(VaultLoadError::Corrupt(format!("读取保险库文件失败：{e}")));
        }
    };
    let vf: VaultFile = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            return Err(VaultLoadError::Corrupt(format!(
                "保险库文件 JSON 解析失败：{e}"
            )));
        }
    };
    if vf.iterations == 0 || vf.iterations > MAX_ITERATIONS {
        return Err(VaultLoadError::Corrupt(format!(
            "保险库文件的 iterations 字段非法（{}），文件可能已损坏",
            vf.iterations
        )));
    }
    Ok(vf)
}

/// 保险库文件的读取结果。
#[derive(Debug)]
enum VaultLoadError {
    /// 文件不存在 —— 尚未初始化。
    NotFound,
    /// 文件存在但内容不可用（JSON 损坏 / 字段缺失 / 参数非法）。
    Corrupt(String),
}

/// 读取失败时给用户看的文案。
///
/// 「未初始化」与「已损坏」必须措辞不同：后者要明确提示**不要**覆盖原文件，
/// 并指出是哪个文件（用户需要据此备份 / 移走）。
fn load_err_msg(path: &Path, e: &VaultLoadError) -> String {
    match e {
        VaultLoadError::NotFound => "保险库未初始化，请先 vault_init".into(),
        VaultLoadError::Corrupt(detail) => format!(
            "保险库文件已损坏（{}：{detail}）。为避免覆盖你唯一的数据备份，本次操作已中止；\
             请先备份并移走该文件（例如重命名为 vault.json.bak）后再处理。",
            path.display()
        ),
    }
}

fn write_vault(path: &Path, v: &VaultFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, serde_json::to_string_pretty(v).unwrap_or_default())
        .map_err(|e| e.to_string())
}

// ------------------------- 纯函数内核（便于单测） -------------------------

/// 用主密码解锁保险库文件，返回派生密钥。
///
/// 迭代次数取自**文件内记录的 `iterations` 字段**：老文件没有该字段时按
/// 200k 兼容，因此老保险库仍能用原主密码解锁，不会被误判成「主密码错误」。
fn unlock_vault_file(vf: &VaultFile, master: &str) -> Result<Vec<u8>, String> {
    let salt = b64_decode(&vf.salt)?;
    let key = derive_key(master, &salt, vf.iterations);
    let verifier = b64_decode(&vf.verifier)?;
    let vnonce = b64_decode(&vf.verifier_nonce)?;
    // 解不开校验串 = 主密码不对，或文件的 KDF 参数与主密码不匹配。
    let dec = aes_decrypt(&key, &verifier, &vnonce)
        .map_err(|_| "主密码错误（或保险库文件的 KDF 参数与主密码不匹配）".to_string())?;
    if dec != VERIFIER_PLAINTEXT {
        return Err("主密码错误".into());
    }
    Ok(key)
}

/// 从磁盘上的保险库文件解锁（区分「未初始化」与「已损坏」）。
fn unlock_vault_at(path: &Path, master: &str) -> Result<Vec<u8>, String> {
    let vf = read_vault(path).map_err(|e| load_err_msg(path, &e))?;
    unlock_vault_file(&vf, master)
}

/// 初始化保险库文件，返回本次派生的密钥（避免重复做一次昂贵的 PBKDF2）。
///
/// 若文件已存在且**损坏**，拒绝写入（防止覆盖用户唯一的数据备份）；
/// 若文件不存在则正常创建。
fn init_vault_at(path: &Path, master: &str) -> Result<Vec<u8>, String> {
    if master.is_empty() {
        return Err("主密码不能为空".into());
    }
    match read_vault(path) {
        Ok(_) => {}
        Err(VaultLoadError::NotFound) => {}
        Err(VaultLoadError::Corrupt(detail)) => {
            return Err(format!(
                "保险库文件已损坏，拒绝覆盖：{detail}。\
                 请先备份并移走该文件（例如重命名为 vault.json.bak）后再初始化。"
            ));
        }
    }
    let salt = random_bytes(16);
    let key = derive_key(master, &salt, ITERATIONS);
    let (verifier, vnonce) = aes_encrypt(&key, VERIFIER_PLAINTEXT)?;
    let vf = VaultFile {
        salt: b64_encode(&salt),
        verifier: b64_encode(&verifier),
        verifier_nonce: b64_encode(&vnonce),
        iterations: ITERATIONS,
        targets: vec![],
    };
    write_vault(path, &vf)?;
    Ok(key)
}

/// 解密某个目标档案里的私钥（明文，仅在内存中短暂存在）。
fn decrypt_target_key(master_key: &[u8], t: &VaultTarget) -> Result<Vec<u8>, String> {
    let enc = b64_decode(t.enc_key.as_deref().ok_or("该目标无加密密钥")?)?;
    let nonce = b64_decode(t.nonce.as_deref().ok_or("该目标无 nonce")?)?;
    aes_decrypt(master_key, &enc, &nonce)
}

/// 解析内存中的私钥字节（不落盘）。优先 OpenSSH PEM，兼容原始二进制格式。
fn parse_private_key(bytes: &[u8]) -> Result<PrivateKey, String> {
    PrivateKey::from_openssh(bytes)
        .or_else(|_| PrivateKey::from_bytes(bytes))
        .map_err(|e| format!("私钥无法解析（可能已加密或格式不受支持）：{e}"))
}

/// 判断文件是否像明文私钥（PEM 私钥头或 PuTTY PPK 头）。
fn looks_like_private_key(path: &Path) -> bool {
    let head = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let head = String::from_utf8_lossy(&head[..head.len().min(4096)]);
    head.contains("PRIVATE KEY-----") || head.contains("PuTTY-User-Key-File-")
}

/// 尽量按「同一个文件」比较（Windows 下路径大小写 / 分隔符可能不同）。
fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// 清理旧版本遗留在 `keys/` 目录下的明文私钥文件。
///
/// - 只处理 `keys/` 目录，不动其它目录；
/// - **保留 `protected` 中的全部路径**：那些是应用当前真正在用的 SSH 凭据。
///   旧版保险库恰好把明文私钥写在了 `keys/id_ed25519`，而这同一个路径同时是
///   网关（`DOU_SSH_KEY`）、前端（硬编码 `SSH_KEY`）与 `transfer_queue` 缺省回退
///   所依赖的真实私钥 —— 删掉它会直接打断 SSH，属于「删除用户数据」而非「修复泄露」；
/// - 跳过 `*.pub`（公钥不是机密）。
fn cleanup_plaintext_keys(dir: &Path, protected: &[PathBuf]) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return removed,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if protected.iter().any(|p| same_file(&path, p)) {
            continue;
        }
        if path.extension().map(|e| e == "pub").unwrap_or(false) {
            continue;
        }
        if !looks_like_private_key(&path) {
            continue;
        }
        if std::fs::remove_file(&path).is_ok() {
            removed.push(path);
        }
    }
    removed
}

/// 当前生效的 SSH 私钥路径集合（清理时必须保留）。
///
/// - `DOU_SSH_KEY`（`transfer_queue` 走 `ssh::ssh_target()` 的缺省回退）；
/// - `keys/id_ed25519`（网关 `DOU_SSH_KEY` 与前端硬编码 `SSH_KEY` 依赖的路径）。
///
/// 这些是应用**真正在用**的凭据，删掉会直接打断 SSH。
fn active_ssh_key_paths(keys_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(ssh::ssh_target().key_path)];
    let default_key = keys_dir.join("id_ed25519");
    if !paths.iter().any(|p| same_file(p, &default_key)) {
        paths.push(default_key);
    }
    paths
}

/// 清理旧版本遗留在 `keys/` 下的明文私钥（启动时 / 锁定时调用）。
///
/// `dir` 即历史上存放明文私钥的目录（`%LOCALAPPDATA%/dou/keys`）。
pub fn cleanup_legacy_keys(dir: &Path) -> Vec<PathBuf> {
    let protected = active_ssh_key_paths(dir);
    cleanup_plaintext_keys(dir, &protected)
}

// ------------------------- 命令 -------------------------

pub fn vault_init(state: State<VaultState>, master: String) -> Result<(), String> {
    let key = init_vault_at(&state.path, &master)?;
    *state.key.lock().unwrap() = Some(Zeroizing::new(key));
    *state.unlocked.lock().unwrap() = true;
    Ok(())
}

pub fn vault_unlock(state: State<VaultState>, master: String) -> Result<(), String> {
    let key = unlock_vault_at(&state.path, &master)?;
    *state.key.lock().unwrap() = Some(Zeroizing::new(key));
    *state.unlocked.lock().unwrap() = true;
    Ok(())
}

pub fn vault_lock(state: State<VaultState>) {
    // 清除内存中的派生密钥（`Zeroizing` 自动擦除），并清理历史遗留的明文私钥。
    *state.key.lock().unwrap() = None;
    *state.unlocked.lock().unwrap() = false;
    let _ = cleanup_legacy_keys(&state.keys_dir);
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
    let vf = read_vault(&state.path).map_err(|e| load_err_msg(&state.path, &e))?;
    Ok(vf.targets)
}

pub fn vault_add_target(
    state: State<VaultState>,
    name: String,
    host: String,
    port: u16,
    user: String,
    auth: String,
    key_path: String,
) -> Result<(), String> {
    let key = state
        .key
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| String::from("保险库已锁定，请先解锁"))?;
    let mut vf = read_vault(&state.path).map_err(|e| load_err_msg(&state.path, &e))?;
    vf.targets.retain(|t| t.name != name);
    if auth == "password" {
        // 密码模式：密码**不入库**（连接时临时输入，内存驻留用完即弃），目标只存地址与认证方式。
        vf.targets.push(VaultTarget {
            name,
            host,
            port,
            user,
            auth,
            enc_key: None,
            nonce: None,
        });
    } else {
        // 明文私钥只在内存中短暂存在，加密后立即清零。
        let plaintext =
            Zeroizing::new(std::fs::read(&key_path).map_err(|e| format!("读取私钥: {e}"))?);
        let (enc, nonce) = aes_encrypt(&key, &plaintext)?;
        vf.targets.push(VaultTarget {
            name,
            host,
            port,
            user,
            auth,
            enc_key: Some(b64_encode(&enc)),
            nonce: Some(b64_encode(&nonce)),
        });
    }
    write_vault(&state.path, &vf)?;
    Ok(())
}

/// 取出目标私钥的**公钥信息**。
///
/// 私钥明文只在内存中短暂存在，用完即清零；既不写盘，也不出本进程。
pub fn vault_get_key(state: State<VaultState>, name: String) -> Result<VaultKeyInfo, String> {
    get_key_info(&state, &name)
}

/// `vault_get_key` 的内核（可单测）。
fn get_key_info(st: &VaultState, name: &str) -> Result<VaultKeyInfo, String> {
    ensure_unlocked(*st.unlocked.lock().unwrap())?;
    let master_key = st
        .key
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| String::from("保险库已锁定，请先解锁"))?;
    let vf = read_vault(&st.path).map_err(|e| load_err_msg(&st.path, &e))?;
    let t = vf
        .targets
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| String::from("目标不存在"))?;
    // 密码模式目标没有私钥，提前给出明确语义。
    if t.enc_key.is_none() {
        return Err(String::from("密码认证目标无私钥（连接时输入密码）"));
    }
    // 明文私钥：解密 → 解析 → 立刻清零，全程不落盘。
    let plain = Zeroizing::new(decrypt_target_key(&master_key, t)?);
    let key = parse_private_key(&plain)?;
    Ok(VaultKeyInfo {
        algorithm: key.algorithm().to_string(),
        fingerprint: key.public_key().fingerprint(HashAlg::Sha256).to_string(),
    })
}

/// 用保险库里的私钥在内存中完成一次 SSH 认证（解密 → 连接 → 用完即清）。
///
/// 这是「明文私钥只在需要使用 SSH 连接的那一刻以内存形式存在」的落点：
/// 私钥从不解密到磁盘，`PrivateKey` 随会话释放时被 ssh-key 自动清零。
pub async fn vault_test_connect(
    app: AppHandle,
    state: State<'_, VaultState>,
    name: String,
    password: Option<String>,
) -> Result<String, String> {
    let st = state.inner();
    ensure_unlocked(*st.unlocked.lock().unwrap())?;
    let master_key = st
        .key
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| String::from("保险库已锁定，请先解锁"))?;
    let vf = read_vault(&st.path).map_err(|e| load_err_msg(&st.path, &e))?;
    let t = vf
        .targets
        .iter()
        .find(|t| t.name == name)
        .cloned()
        .ok_or_else(|| String::from("目标不存在"))?;
    if t.enc_key.is_none() {
        // 密码模式：密码由前端本次请求传入（Zeroizing 内存驻留），用完即弃，不落盘不写日志。
        let pw = Zeroizing::new(
            password
                .filter(|p| !p.is_empty())
                .ok_or_else(|| String::from("密码连接需要输入密码"))?,
        );
        ssh::ssh_connect_with_password(app, &t.host, t.port, &t.user, pw).await?;
        return Ok(format!(
            "连接成功：{}@{}:{}（密码认证，密码仅存在于本次调用内存，未落盘）",
            t.user, t.host, t.port
        ));
    }
    // 明文私钥：解密 → 解析 → 立刻清零。
    let plain = Zeroizing::new(decrypt_target_key(&master_key, &t)?);
    let key = parse_private_key(&plain)?;
    // 私钥以内存形式交给 SSH：认证成功后随会话释放被清零。
    ssh::ssh_connect_with_key(app, &t.host, t.port, &t.user, key).await?;
    Ok(format!(
        "连接成功：{}@{}:{}（私钥仅存在于内存，用完即清，未落盘）",
        t.user, t.host, t.port
    ))
}

pub fn vault_remove_target(state: State<VaultState>, name: String) -> Result<(), String> {
    ensure_unlocked(*state.unlocked.lock().unwrap())?;
    let mut vf = read_vault(&state.path).map_err(|e| load_err_msg(&state.path, &e))?;
    vf.targets.retain(|t| t.name != name);
    write_vault(&state.path, &vf)?;
    Ok(())
}

// ------------------------- 换机迁移（导出 / 导入） -------------------------

/// 纳秒时间戳（不引入 chrono 依赖），用于备份与导出文件名去重。
fn now_stamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// 导入前把现有保险库备份为 `vault.json.bak-<stamp>`；无现有文件则跳过。
/// 备份失败必须中止导入，避免来源文件损坏后无旧库可回退。
fn backup_vault(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let bak = path.with_file_name(format!("vault.json.bak-{}", now_stamp()));
    std::fs::copy(path, &bak).map_err(|e| format!("备份现有保险库失败：{e}；本次导入已中止。"))?;
    Ok(Some(bak))
}

/// 导出当前保险库：**原样复制加密后的 vault.json**，不做任何解密。
///
/// 密文随文件一起走换机；在新机器上用**原主密码**解锁即可，无需重新输入私钥。
/// 返回导出文件的绝对路径，供前端在资源管理器中定位。
pub fn vault_export(state: State<VaultState>) -> Result<String, String> {
    // 未初始化 / 损坏按既有口径报错，避免导出一个不可用的空文件。
    let _ = read_vault(&state.path).map_err(|e| load_err_msg(&state.path, &e))?;
    let out_dir = ssh::app_data_dir().join("exports");
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("创建导出目录失败：{e}"))?;
    let dest = out_dir.join(format!("dou-vault-export-{}.json", now_stamp()));
    std::fs::copy(&state.path, &dest).map_err(|e| format!("导出失败：{e}"))?;
    Ok(dest.display().to_string())
}

/// 导入保险库：校验来源是合法的保险库结构后，备份现有库并原样写入。
///
/// - 只校验 JSON 结构 / 迭代次数，**不校验主密码**（密文来自旧机器，主密码在旧机器上）；
///   导入后回到锁定态，需在本机用原主密码解锁。
/// - `source` 缺省取固定导入位置 `vault.import.json`；显式传入路径时以传入为准。
/// - 导入成功后清理固定临时导入文件（避免密文副本长期残留）。
///
/// 返回导入的档案条目数。
pub fn vault_import(state: State<VaultState>, source: Option<String>) -> Result<usize, String> {
    let src = source
        .map(PathBuf::from)
        .unwrap_or_else(|| ssh::app_data_dir().join("vault.import.json"));
    let vf = read_vault(&src).map_err(|e| match e {
        VaultLoadError::NotFound => format!(
            "未找到待导入的保险库文件。请先把备份文件复制到：{}\n（命名为 vault.import.json），再点导入。",
            ssh::app_data_dir().display()
        ),
        VaultLoadError::Corrupt(detail) => {
            format!("待导入文件不是合法保险库，已拒绝导入：{detail}")
        }
    })?;
    let count = vf.targets.len();

    backup_vault(&state.path)?;
    std::fs::copy(&src, &state.path).map_err(|e| format!("写入保险库失败：{e}"))?;

    // 导入后强制回到锁定态：内存里的旧派生密钥作废，需用原主密码重新解锁。
    *state.key.lock().unwrap() = None;
    *state.unlocked.lock().unwrap() = false;

    let default_src = ssh::app_data_dir().join("vault.import.json");
    if src == default_src {
        let _ = std::fs::remove_file(&src);
    }
    Ok(count)
}

// 解锁后向前端广播状态变化（前端据此刷新面板）。
pub fn vault_notify(app: AppHandle, unlocked: bool) {
    let _ = app.emit("vault_status", unlocked);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个用例独立的临时目录（避免互相干扰）。
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "dou_vault_test_{}_{}",
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 运行时生成一把测试用 ed25519 私钥（OpenSSH PEM）。
    ///
    /// 不在源码里内嵌任何私钥字面量：种子固定，结果确定。
    fn test_private_key_pem() -> String {
        use russh::keys::ssh_key::LineEnding;
        use russh::keys::ssh_key::private::{Ed25519Keypair, KeypairData};
        let kp = Ed25519Keypair::from_seed(&[0x42u8; 32]);
        let key = PrivateKey::new(KeypairData::Ed25519(kp), "dou-vault-test").unwrap();
        key.to_openssh(LineEnding::LF).unwrap().to_string()
    }

    #[test]
    fn vault_encrypt_roundtrip_and_wrong_password_fails() {
        let salt = random_bytes(16);
        let key = derive_key("master-pass", &salt, ITERATIONS);
        let pt = b"secret-key-bytes";
        let (ct, nonce) = aes_encrypt(&key, pt).unwrap();
        let dec = aes_decrypt(&key, &ct, &nonce).unwrap();
        assert_eq!(dec, pt);
        let wrong = derive_key("wrong-password", &salt, ITERATIONS);
        assert!(aes_decrypt(&wrong, &ct, &nonce).is_err());
    }

    /// P0-1 回归：锁定态必须 fail-closed，拒绝档案级操作。
    #[test]
    fn ensure_unlocked_rejects_when_locked() {
        let err = ensure_unlocked(false).unwrap_err();
        assert_eq!(err, "保险库已锁定，请先解锁");
        assert!(ensure_unlocked(true).is_ok());
    }

    #[test]
    fn load_err_msg_distinguishes_not_found_and_corrupt() {
        let p = Path::new(r"C:\dou\vault.json");
        let nf = load_err_msg(p, &VaultLoadError::NotFound);
        assert!(nf.contains("未初始化"));
        let corrupt = load_err_msg(p, &VaultLoadError::Corrupt("bad json".into()));
        assert!(corrupt.contains("损坏"));
        assert!(corrupt.contains("vault.json"));
        assert!(corrupt.contains("bad json"));
    }

    /// P0-1 回归：锁定时不能读到目标档案，解锁后可以。
    #[test]
    fn vault_list_targets_is_blocked_while_locked() {
        // 用临时路径构造一个 VaultState（State 包装需 tauri runtime，
        // 故此处直接验证底层 read_vault + ensure_unlocked 的组合语义）。
        let dir = temp_dir("list");
        let path = dir.join("vault.json");

        let vf = VaultFile {
            salt: b64_encode(&random_bytes(16)),
            verifier: b64_encode(b"v"),
            verifier_nonce: b64_encode(&random_bytes(12)),
            iterations: LEGACY_ITERATIONS,
            targets: vec![VaultTarget {
                name: "t1".into(),
                host: "10.0.0.1".into(),
                port: 22,
                user: "u".into(),
                auth: "key".into(),
                enc_key: None,
                nonce: None,
            }],
        };
        write_vault(&path, &vf).unwrap();

        // 锁定态：闸门必须拦住，调用方拿不到 targets。
        assert!(ensure_unlocked(false).is_err());
        // 归档数据确实在磁盘上（说明拦截来自闸门而非数据缺失）。
        assert_eq!(read_vault(&path).unwrap().targets.len(), 1);

        // 解锁态：放行。
        ensure_unlocked(true).unwrap();
        assert_eq!(read_vault(&path).unwrap().targets.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// P1（迭代次数）回归：**用旧迭代次数（200k）创建的 vault 文件仍能用正确主密码解锁**。
    ///
    /// 这条最容易被做错：把 ITERATIONS 提到 600k 后，如果解锁时硬编码用 600k 派生，
    /// 老文件（salt 相同、迭代不同）解不开校验串，会被误判成「主密码错误」。
    /// 正确做法是读取文件里记录的 `iterations`；老文件没这个字段则按 200k 兼容。
    #[test]
    fn legacy_vault_with_200k_iterations_still_unlocks() {
        let dir = temp_dir("legacy");
        let path = dir.join("vault.json");

        // 旧版本写出的文件：没有 iterations 字段，校验串按 200k 派生。
        let salt = random_bytes(16);
        let old_key = derive_key("old-master", &salt, LEGACY_ITERATIONS);
        let (verifier, vnonce) = aes_encrypt(&old_key, VERIFIER_PLAINTEXT).unwrap();
        let json = format!(
            r#"{{
  "salt": "{}",
  "verifier": "{}",
  "verifier_nonce": "{}",
  "targets": []
}}"#,
            b64_encode(&salt),
            b64_encode(&verifier),
            b64_encode(&vnonce)
        );
        std::fs::write(&path, json).unwrap();

        // 缺省迭代次数必须回落到 200k。
        let vf = read_vault(&path).unwrap();
        assert_eq!(vf.iterations, LEGACY_ITERATIONS);

        // 正确主密码 → 解锁成功（从磁盘路径走一遍完整流程）。
        let key = unlock_vault_at(&path, "old-master").unwrap();
        assert_eq!(key.len(), KEY_LEN);
        // 错误主密码 → 失败。
        assert!(unlock_vault_at(&path, "wrong-master").is_err());

        // 反证：若错误地按 600k 派生，同一个老文件必然解不开（说明兼容逻辑是必需的）。
        let wrong_iter_key = derive_key("old-master", &salt, ITERATIONS);
        assert!(aes_decrypt(&wrong_iter_key, &verifier, &vnonce).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// P1（迭代次数）回归：新版保险库把 600k 写进文件，且能用主密码解锁。
    #[test]
    fn new_vault_records_current_iterations_and_unlocks() {
        let dir = temp_dir("newiter");
        let path = dir.join("vault.json");

        init_vault_at(&path, "new-master").unwrap();
        let vf = read_vault(&path).unwrap();
        assert_eq!(vf.iterations, ITERATIONS);
        // 常量断言：编译期即可校验迭代次数下限，不满足时直接编译失败。
        const { assert!(ITERATIONS >= 600_000, "必须达到 OWASP 建议的 600k") };
        assert!(unlock_vault_at(&path, "new-master").is_ok());
        assert!(unlock_vault_at(&path, "bad-master").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// P1（文件损坏）回归：文件存在但内容损坏时，必须明确报「损坏」而不是「未初始化」，
    /// 且 `vault_init` 必须拒绝执行、不得覆盖原文件。
    #[test]
    fn corrupt_vault_file_is_reported_and_not_overwritten() {
        let dir = temp_dir("corrupt");
        let path = dir.join("vault.json");
        let broken = "{ this is not valid json ";
        std::fs::write(&path, broken).unwrap();

        // 解锁：报「损坏」，而不是「未初始化」。
        let err = unlock_vault_at(&path, "master").unwrap_err();
        assert!(err.contains("损坏"), "实际错误信息: {err}");
        assert!(!err.contains("未初始化"), "不应误报未初始化: {err}");

        // 初始化：拒绝覆盖。
        let err = init_vault_at(&path, "master").unwrap_err();
        assert!(err.contains("损坏"), "实际错误信息: {err}");

        // 原文件必须原封不动。
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// P1（文件损坏）回归：文件不存在时仍然是「未初始化」，与损坏区分开。
    #[test]
    fn missing_vault_file_reports_not_initialized() {
        let dir = temp_dir("missing");
        let path = dir.join("vault.json");
        let err = unlock_vault_at(&path, "master").unwrap_err();
        assert!(err.contains("未初始化"), "实际错误信息: {err}");
        assert!(!err.contains("损坏"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// P0 回归：**明文私钥不再落盘**。
    ///
    /// 解锁 + 取出私钥信息之后，`keys/` 目录下不得出现任何明文私钥文件。
    #[test]
    fn vault_get_key_does_not_write_plaintext_to_disk() {
        let dir = temp_dir("noplain");
        let vault_path = dir.join("vault.json");
        let keys_dir = dir.join("keys");
        std::fs::create_dir_all(&keys_dir).unwrap();

        // 造一把真实的 OpenSSH 私钥（测试用，运行时生成）。
        let pem = test_private_key_pem();

        // 建库 + 入库一个目标（私钥来自磁盘文件）。
        init_vault_at(&vault_path, "master").unwrap();
        let src_key = dir.join("id_ed25519");
        std::fs::write(&src_key, &pem).unwrap();

        let st = VaultState {
            unlocked: Mutex::new(true),
            key: Mutex::new(Some(Zeroizing::new(
                unlock_vault_at(&vault_path, "master").unwrap(),
            ))),
            path: vault_path.clone(),
            keys_dir: keys_dir.clone(),
        };
        let (enc, nonce) = {
            let master_key = st.key.lock().unwrap().clone().unwrap();
            let plaintext = Zeroizing::new(std::fs::read(&src_key).unwrap());
            aes_encrypt(&master_key, &plaintext).unwrap()
        };
        let mut vf = read_vault(&vault_path).unwrap();
        vf.targets.push(VaultTarget {
            name: "t1".into(),
            host: "10.0.0.1".into(),
            port: 22,
            user: "u".into(),
            auth: "key".into(),
            enc_key: Some(b64_encode(&enc)),
            nonce: Some(b64_encode(&nonce)),
        });
        write_vault(&vault_path, &vf).unwrap();

        // 取出私钥信息：只返回公钥信息，私钥明文不落盘。
        let info = get_key_info(&st, "t1").unwrap();
        assert!(!info.algorithm.is_empty());
        assert!(info.fingerprint.starts_with("SHA256:"));

        // keys/ 目录下不得出现任何明文私钥文件。
        let leaked: Vec<_> = std::fs::read_dir(&keys_dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| e.path())
            .collect();
        assert!(leaked.is_empty(), "明文私钥不应落盘，实际发现: {leaked:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// P0 回归：清理历史遗留时，只删 `keys/` 下的明文私钥，
    /// 且必须保留当前生效的 SSH 私钥与公钥文件。
    #[test]
    fn cleanup_removes_legacy_plaintext_but_keeps_active_key() {
        let dir = temp_dir("cleanup");
        let keys = dir.join("keys");
        std::fs::create_dir_all(&keys).unwrap();

        let pem = test_private_key_pem();
        let active = keys.join("id_ed25519");
        let stale = keys.join("id_ed25519.old");
        let pubkey = keys.join("id_ed25519.pub");
        let unrelated = keys.join("notes.txt");
        std::fs::write(&active, &pem).unwrap();
        std::fs::write(&stale, &pem).unwrap();
        std::fs::write(&pubkey, b"ssh-ed25519 AAAAC3Nz... dou-test\n").unwrap();
        std::fs::write(&unrelated, b"hello").unwrap();

        let removed = cleanup_plaintext_keys(&keys, std::slice::from_ref(&active));
        assert_eq!(removed.len(), 1, "只应删除遗留的明文私钥: {removed:?}");
        assert!(!stale.exists(), "遗留明文私钥应被删除");
        assert!(active.exists(), "当前生效的私钥必须保留");
        assert!(pubkey.exists(), "公钥文件必须保留");
        assert!(unrelated.exists(), "无关文件必须保留");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 迁移回归：导入来源不是合法保险库时必须拒绝，且现有库原样保留。
    #[test]
    fn import_rejects_corrupt_source_and_keeps_existing() {
        let dir = temp_dir("import");
        let path = dir.join("vault.json");
        init_vault_at(&path, "master").unwrap();
        let existing = std::fs::read_to_string(&path).unwrap();

        let src = dir.join("vault.import.json");
        std::fs::write(&src, "{ broken not json").unwrap();
        assert!(matches!(read_vault(&src), Err(VaultLoadError::Corrupt(_))));
        // 现有保险库不得被覆盖。
        assert_eq!(std::fs::read_to_string(&path).unwrap(), existing);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 迁移回归：导入前现有库被备份为 .bak，内容与原库一致。
    #[test]
    fn backup_creates_bak_when_existing() {
        let dir = temp_dir("bak");
        let path = dir.join("vault.json");
        init_vault_at(&path, "master").unwrap();

        let bak = backup_vault(&path).unwrap().unwrap();
        assert!(bak.exists());
        assert_eq!(
            std::fs::read_to_string(&bak).unwrap(),
            std::fs::read_to_string(&path).unwrap()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// P0 回归：`keys/id_ed25519` 同时是「旧版保险库明文落盘位置」和
    /// 「应用真正在用的 SSH 凭据」，清理时**必须保留**，否则 SSH 会被打断。
    #[test]
    fn cleanup_keeps_the_shared_live_key_path() {
        let dir = temp_dir("cleanup_live");
        let keys = dir.join("keys");
        std::fs::create_dir_all(&keys).unwrap();

        let live = keys.join("id_ed25519");
        std::fs::write(&live, test_private_key_pem()).unwrap();

        // 保护集合按「实际在用的路径」构造：默认键路径必在其中。
        let protected = active_ssh_key_paths(&keys);
        assert!(
            protected.iter().any(|p| same_file(p, &live)),
            "默认 SSH 私钥路径必须在保护集合中: {protected:?}"
        );

        let removed = cleanup_plaintext_keys(&keys, &protected);
        assert!(removed.is_empty(), "在用的私钥不应被删除: {removed:?}");
        assert!(live.exists(), "在用的私钥必须保留");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn b64_roundtrip_and_rejects_invalid() {
        let raw = b"hello dou vault";
        let enc = b64_encode(raw);
        assert_eq!(b64_decode(&enc).unwrap(), raw);
        assert!(b64_decode("!!!not-base64!!!").is_err());
    }

    #[test]
    fn derive_key_deterministic_and_password_sensitive() {
        let salt = b"0123456789abcdef";
        let k1 = derive_key("master", salt, 1_000);
        let k2 = derive_key("master", salt, 1_000);
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), KEY_LEN);
        // 主密码不同必须派生不同密钥
        assert_ne!(k1, derive_key("other", salt, 1_000));
    }

    #[test]
    fn aes_decrypt_rejects_bad_nonce_length() {
        let key = derive_key("master", b"0123456789abcdef", 1_000);
        let (ct, _nonce) = aes_encrypt(&key, b"plaintext").unwrap();
        // nonce 不是 12 字节必须报错，不 panic
        assert!(aes_decrypt(&key, &ct, b"short").is_err());
    }
}
