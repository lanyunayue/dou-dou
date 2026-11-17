//! SSH 适配器（russh）。
//!
//! 这一步是尖刀验证的核心：证明"代理不接触私钥、由网关代连代签"在技术上成立。
//! 私钥/口令只存在于网关进程内存与本机目录，MCP 侧只能提交"会话标识 + 命令"，
//! 拿不到任何可用于自建连接的材料。
//!
//! 主机密钥：首次记录、之后严格比对，不匹配即拒连（无旁路）。

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::ChannelMsg;

use crate::hostkey::{self, KnownHosts};

#[derive(Default)]
pub struct HostKeyState {
    pub line: Option<String>,
    /// 已记录的旧公钥（仅在 Mismatch 时填）
    pub stored: Option<String>,
    pub first_seen: bool,
    pub rejected: bool,
    /// known_hosts 损坏导致的拒绝
    pub corrupted: bool,
}

pub struct SshHandler {
    addr: String,
    known: Arc<Mutex<KnownHosts>>,
    state: Arc<Mutex<HostKeyState>>,
}

impl client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = server_public_key.public_key();
        let line = match key.to_openssh() {
            Ok(s) => s,
            Err(_) => return Ok(false),
        };

        let verdict = {
            let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
            known.check_and_record(&self.addr, &line)
        };

        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.line = Some(line);
        match verdict {
            hostkey::Check::Match => {
                st.first_seen = false;
                Ok(true)
            }
            hostkey::Check::FirstSeen => {
                st.first_seen = true;
                Ok(true)
            }
            hostkey::Check::Mismatch { stored } => {
                st.stored = Some(stored);
                st.rejected = true;
                Ok(false)
            }
            hostkey::Check::StoreCorrupted => {
                st.rejected = true;
                st.corrupted = true;
                Ok(false)
            }
        }
    }
}

pub struct SshConnection {
    pub(crate) handle: client::Handle<SshHandler>,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// "publickey" 或 "password"，写进审计用
    pub auth_method: &'static str,
    /// 主机公钥全文（已信任的那一份）
    pub host_key: String,
    /// 本次是否为首次见到该主机
    pub host_key_first_seen: bool,
}

pub struct ExecOutcome {
    pub exit_code: Option<u32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

/// 从 handler 状态里还原"为什么被拒"——russh 只会把 Ok(false) 笼统报成
/// "Unknown server key"，不还原就成了"拦住了但说不清理由"。
fn rejection_reason(st: &HostKeyState) -> Option<String> {
    if st.corrupted {
        return Some(
            "known_hosts 文件损坏或不可解析，已拒绝连接（防止通过破坏该文件绕过主机身份校验）"
                .to_string(),
        );
    }
    if st.rejected {
        let presented = st.line.clone().unwrap_or_default();
        let stored = st.stored.clone().unwrap_or_default();
        return Some(format!(
            "主机密钥不匹配，已拒绝连接（记录 {}，对方 {}）",
            hostkey::short_id(&stored),
            hostkey::short_id(&presented)
        ));
    }
    None
}

async fn open(
    host: &str,
    port: u16,
    known: Arc<Mutex<KnownHosts>>,
) -> Result<(client::Handle<SshHandler>, Arc<Mutex<HostKeyState>>), String> {
    let addr = format!("{host}:{port}");
    let state = Arc::new(Mutex::new(HostKeyState::default()));
    let handler = SshHandler {
        addr,
        known,
        state: state.clone(),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        ..client::Config::default()
    });
    match client::connect(config, (host, port), handler).await {
        Ok(handle) => Ok((handle, state)),
        Err(e) => {
            let st = state.lock().unwrap_or_else(|e| e.into_inner());
            match rejection_reason(&st) {
                Some(reason) => Err(reason),
                None => Err(format!("ssh connect failed: {e}")),
            }
        }
    }
}

fn take_host_key(state: &Arc<Mutex<HostKeyState>>) -> Result<(String, bool), String> {
    let st = state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(reason) = rejection_reason(&st) {
        return Err(reason);
    }
    let line = st.line.clone().unwrap_or_default();
    Ok((line, st.first_seen))
}

/// 口令认证。只用于临时联调，正式版默认关闭口令、强制密钥。
pub async fn connect_password(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    known: Arc<Mutex<KnownHosts>>,
) -> Result<SshConnection, String> {
    let (mut handle, state) = open(host, port, known).await?;
    let (host_key, first_seen) = take_host_key(&state)?;

    let auth = handle
        .authenticate_password(user.to_string(), password.to_string())
        .await
        .map_err(|e| format!("ssh auth error: {e}"))?;
    if !auth.success() {
        return Err("ssh authentication rejected (password)".to_string());
    }
    Ok(SshConnection {
        handle,
        host: host.to_string(),
        port,
        user: user.to_string(),
        auth_method: "password",
        host_key,
        host_key_first_seen: first_seen,
    })
}

/// 密钥认证。私钥在网关本地读取，MCP 侧无法获得。
pub async fn connect_key(
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    known: Arc<Mutex<KnownHosts>>,
) -> Result<SshConnection, String> {
    let key = load_secret_key(Path::new(key_path), None)
        .map_err(|e| format!("load private key failed {key_path}: {e}"))?;
    finish_key_connect(host, port, user, key, known).await
}

/// B1：从内存 PEM 建连（私钥由 vault 解密后传入，不经文件系统，不落盘）。
pub async fn connect_key_material(
    host: &str,
    port: u16,
    user: &str,
    key_pem: &str,
    known: Arc<Mutex<KnownHosts>>,
) -> Result<SshConnection, String> {
    let key = russh::keys::decode_secret_key(key_pem, None)
        .map_err(|e| format!("load private key failed (vault): {e}"))?;
    finish_key_connect(host, port, user, key, known).await
}

/// 密钥建连的公共尾段：建通道、取主机密钥、公钥认证。
async fn finish_key_connect(
    host: &str,
    port: u16,
    user: &str,
    key: russh::keys::PrivateKey,
    known: Arc<Mutex<KnownHosts>>,
) -> Result<SshConnection, String> {
    let (mut handle, state) = open(host, port, known).await?;
    let (host_key, first_seen) = take_host_key(&state)?;

    let auth = handle
        .authenticate_publickey(
            user.to_string(),
            PrivateKeyWithHashAlg::new(Arc::new(key), None),
        )
        .await
        .map_err(|e| format!("ssh auth error: {e}"))?;
    if !auth.success() {
        return Err("ssh authentication rejected (publickey)".to_string());
    }
    Ok(SshConnection {
        handle,
        host: host.to_string(),
        port,
        user: user.to_string(),
        auth_method: "publickey",
        host_key,
        host_key_first_seen: first_seen,
    })
}

impl SshConnection {
    /// 执行一条非交互命令。超时后不再等待，但远端进程会继续跑——
    /// 这正是计划书 4.9 要求"长命令任务化"的原因，此处先如实暴露。
    pub async fn exec(&mut self, command: &str, timeout: Duration) -> Result<ExecOutcome, String> {
        let mut channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| format!("channel open failed: {e}"))?;

        channel
            .exec(true, command)
            .await
            .map_err(|e| format!("exec failed: {e}"))?;

        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let mut exit_code: Option<u32> = None;
        let mut timed_out = false;

        loop {
            match tokio::time::timeout(timeout, channel.wait()).await {
                Ok(Some(msg)) => match msg {
                    ChannelMsg::Data { ref data } => stdout.extend_from_slice(data),
                    ChannelMsg::ExtendedData { ref data, .. } => stderr.extend_from_slice(data),
                    ChannelMsg::ExitStatus { exit_status } => {
                        exit_code = Some(exit_status);
                    }
                    ChannelMsg::Eof | ChannelMsg::Close => {}
                    _ => {}
                },
                Ok(None) => break,
                Err(_) => {
                    timed_out = true;
                    break;
                }
            }
        }

        Ok(ExecOutcome {
            exit_code,
            stdout: String::from_utf8_lossy(&stdout).to_string(),
            stderr: String::from_utf8_lossy(&stderr).to_string(),
            timed_out,
        })
    }
}
