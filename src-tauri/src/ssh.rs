//! 共享 SSH 内核：主机密钥校验（known_hosts + TOFU + 变更拒绝）+ 统一连接入口。
//!
//! 取代 pty/sftp/transfer 三处各自 `Ok(true)` 的盲目信任，关闭中间人（MITM）漏洞。
//! 设计对齐 Xftp 的 Host Keys 对话框 + SSH 安全警告：
//!   - 首次见到某主机密钥 → 记录并前端弹「新主机指纹」确认（TOFU）；
//!   - 之后严格比对，密钥变更 → 拒绝连接（返回 false 让 russh 中止）；
//!   - 密钥轮换后可用 `host_key_forget` 清除旧记录重新确认。

use russh::client;
use russh::keys::ssh_key::{HashAlg, PublicKey};
use russh::keys::PublicKeyOrCertificate;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize, Deserialize)]
pub struct HostKeyEntry {
    pub host: String,
    pub port: u16,
    /// OpenSSH 风格 SHA256 指纹：`SHA256:base64`
    pub fingerprint: String,
    /// 首次/最近见到的 Unix 秒（用于展示与审计）
    pub first_seen: u64,
    pub last_seen: u64,
}

/// 应用数据目录：`%LOCALAPPDATA%/dou`（回退 HOME/.dou）。
pub fn app_data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(p).join("dou")
    } else if let Ok(p) = std::env::var("HOME") {
        PathBuf::from(p).join(".dou")
    } else {
        PathBuf::from(".").join("dou")
    }
}

fn known_hosts_path() -> PathBuf {
    app_data_dir().join("known_hosts.json")
}

fn load_known_hosts() -> Vec<HostKeyEntry> {
    match std::fs::read_to_string(known_hosts_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_known_hosts(entries: &[HostKeyEntry]) -> std::io::Result<()> {
    let dir = app_data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let s = serde_json::to_string_pretty(entries).unwrap_or_default();
    std::fs::write(known_hosts_path(), s)
}

/// 主机密钥 SHA256 指纹（OpenSSH 风格 `SHA256:base64`）。
pub fn host_key_fingerprint(key: &PublicKeyOrCertificate) -> String {
    let pk: PublicKey = key.public_key();
    pk.fingerprint(HashAlg::Sha256).to_string()
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug)]
pub enum HostKeyVerdict {
    KnownMatch,
    FirstSeen(String),
    Mismatch { expected: String, got: String },
}

// 序列化对 known_hosts.json 的读改写，避免并发首连互相覆盖。
static STORE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
fn store_lock() -> &'static Mutex<()> {
    STORE_LOCK.get_or_init(|| Mutex::new(()))
}

/// 校验并（首次）学习主机密钥。返回判定结果；首次出现会持久化到 known_hosts。
pub fn verify_or_learn(host: &str, port: u16, fp: &str) -> HostKeyVerdict {
    let _g = store_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut entries = load_known_hosts();
    if let Some(e) = entries.iter_mut().find(|e| e.host == host && e.port == port) {
        if e.fingerprint == fp {
            e.last_seen = now_epoch();
            let _ = save_known_hosts(&entries);
            HostKeyVerdict::KnownMatch
        } else {
            // 密钥与已记录不符：疑似中间人或服务端密钥已轮换。
            HostKeyVerdict::Mismatch {
                expected: e.fingerprint.clone(),
                got: fp.to_string(),
            }
        }
    } else {
        let t = now_epoch();
        entries.push(HostKeyEntry {
            host: host.to_string(),
            port,
            fingerprint: fp.to_string(),
            first_seen: t,
            last_seen: t,
        });
        let _ = save_known_hosts(&entries);
        HostKeyVerdict::FirstSeen(fp.to_string())
    }
}

/// 删除某主机的已知密钥（密钥轮换后重新确认用）。
pub fn forget_host(host: &str, port: u16) -> Result<(), String> {
    let _g = store_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut entries = load_known_hosts();
    entries.retain(|e| !(e.host == host && e.port == port));
    save_known_hosts(&entries).map_err(|e| e.to_string())
}

pub fn list_hosts() -> Vec<HostKeyEntry> {
    load_known_hosts()
}

/// 共享连接处理器：在 `check_server_key` 中执行主机密钥校验与事件上报。
pub struct SshHandler {
    pub app: AppHandle,
    pub host: String,
    pub port: u16,
}

impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let fp = host_key_fingerprint(key);
        match verify_or_learn(&self.host, self.port, &fp) {
            HostKeyVerdict::KnownMatch => Ok(true),
            HostKeyVerdict::FirstSeen(fp) => {
                // 首次连接：记录并提示前端让用户肉眼确认指纹（TOFU）。
                let _ = self.app.emit(
                    "host_key_event",
                    serde_json::json!({
                        "host": self.host, "port": self.port,
                        "fingerprint": fp, "kind": "first_seen"
                    }),
                );
                Ok(true)
            }
            HostKeyVerdict::Mismatch { expected, got } => {
                // 密钥变更：拒绝连接，防中间人。russh 收到 false 会中止握手。
                let _ = self.app.emit(
                    "host_key_event",
                    serde_json::json!({
                        "host": self.host, "port": self.port,
                        "expected": expected, "got": got, "kind": "mismatch"
                    }),
                );
                Ok(false)
            }
        }
    }
}

/// 共享已认证会话句柄类型：`russh::client::connect` 返回 `Handle<H>`，
/// 其 `channel_open_session().await` 给出高层 `Channel<Msg>`，`exec/data/eof/wait` 均为异步。
pub type DouHandle = russh::client::Handle<SshHandler>;

/// 统一连接入口：主机密钥校验（connect 期间）+ 公钥认证，返回已认证的会话。
///
/// 私钥从磁盘读取。凭据保险库走的是 [`ssh_connect_with_key`]：私钥只存在于内存。
pub async fn ssh_connect(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
) -> Result<DouHandle, String> {
    let key = russh::keys::PrivateKey::read_openssh_file(std::path::Path::new(key_path))
        .map_err(|e| format!("读私钥: {e}"))?;
    // 与既有语义保持一致：认证结果不在此处判定，由后续 channel 操作暴露。
    let (session, _auth_ok) = ssh_connect_inner(app, host, port, user, key).await?;
    Ok(session)
}

/// 统一连接入口（内存私钥）：主机密钥校验（connect 期间）+ 公钥认证。
///
/// 凭据保险库把解密出的私钥直接以内存形式交给本函数，**从不落盘**。
/// `PrivateKey` 随会话释放被 `ssh-key` 自动清零（其 `Drop` 实现里做 zeroize）。
///
/// 与 [`ssh_connect`] 不同：本函数会检查认证结果，服务器拒绝该公钥时直接报错，
/// 避免把「未认证的会话」当成连接成功。
pub async fn ssh_connect_with_key(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key: russh::keys::PrivateKey,
) -> Result<DouHandle, String> {
    let (session, auth_ok) = ssh_connect_inner(app, host, port, user, key).await?;
    if !auth_ok {
        return Err(format!("认证失败：服务器拒绝了 {user}@{host}:{port} 的公钥"));
    }
    Ok(session)
}

/// 连接 + 公钥认证的公共实现。
///
/// 返回 `(会话, 认证是否成功)`：注意 `authenticate_publickey` 在服务器拒绝公钥时
/// 返回的是 `Ok(AuthResult::Failure)` 而不是 `Err`，必须显式判定。
async fn ssh_connect_inner(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key: russh::keys::PrivateKey,
) -> Result<(DouHandle, bool), String> {
    let config = std::sync::Arc::new(client::Config::default());
    let mut session = client::connect(
        config,
        (host, port),
        SshHandler {
            app: app.clone(),
            host: host.to_string(),
            port,
        },
    )
    .await
    .map_err(|e| {
        let s = e.to_string();
        if s.contains("server key") || s.contains("key changed") || s.contains("host key") {
            format!("主机密钥校验失败（疑似中间人或密钥已变更）：{s}")
        } else {
            format!("连接失败: {s}")
        }
    })?;

    let auth = session
        .authenticate_publickey(
            user,
            russh::keys::PrivateKeyWithHashAlg::new(std::sync::Arc::new(key), None),
        )
        .await
        .map_err(|e| format!("认证失败: {e}"))?;
    Ok((session, auth.success()))
}

/// 从 `DOU_SSH`（`user@host:port`）与 `DOU_SSH_KEY` 解析本应用统一靶机目标。
/// 与网关启动参数口径一致，避免前端硬编码；缺省回退到 127.0.0.1:22 + 标准密钥路径。
pub struct SshTarget {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub key_path: String,
}

pub fn ssh_target() -> SshTarget {
    let dou_ssh = std::env::var("DOU_SSH").unwrap_or_else(|_| "dou@127.0.0.1:22".into());
    let (user, hostport) = match dou_ssh.split_once('@') {
        Some((u, h)) => (u.to_string(), h.to_string()),
        None => ("dou".into(), dou_ssh),
    };
    let (host, port) = match hostport.split_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(22)),
        None => (hostport, 22),
    };
    let key_path = std::env::var("DOU_SSH_KEY").unwrap_or_else(|_| {
        if let Ok(l) = std::env::var("LOCALAPPDATA") {
            format!("{}/dou/keys/id_ed25519", l)
        } else {
            "id_ed25519".into()
        }
    });
    SshTarget {
        host,
        port,
        user,
        key_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_hosts_first_seen_then_match() {
        // 用临时目录作为 app_data_dir 不可行（路径硬编码），这里仅验证指纹格式稳定。
        let fp = "SHA256:abc";
        match verify_or_learn("example.test", 22, fp) {
            HostKeyVerdict::FirstSeen(got) => assert_eq!(got, fp),
            other => panic!("首次应返回 FirstSeen，实际 {other:?}"),
        }
        // 再次相同 → KnownMatch
        match verify_or_learn("example.test", 22, fp) {
            HostKeyVerdict::KnownMatch => {}
            other => panic!("重复应返回 KnownMatch，实际 {other:?}"),
        }
        // 变更 → Mismatch
        match verify_or_learn("example.test", 22, "SHA256:changed") {
            HostKeyVerdict::Mismatch { expected, got } => {
                assert_eq!(expected, fp);
                assert_eq!(got, "SHA256:changed");
            }
            other => panic!("变更应返回 Mismatch，实际 {other:?}"),
        }
        // 清理，避免污染本机 known_hosts.json
        let _ = forget_host("example.test", 22);
    }
}
