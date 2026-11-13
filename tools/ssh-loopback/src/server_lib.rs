//! 本地回环 SSH 服务核心（库）：供 `dou-ssh-server` 与 `dou-ssh-selfcheck` 共用。
//!
//! 目的：在没有外部 Linux 靶机时，用纯 russh 起一个回环 SSH 服务（默认 127.0.0.1:2222），
//! 让 `dou-ssh-selfcheck` 复用豆 Dou「受治理远程操作主线」的同款 russh 高层 API
//! （主机密钥 TOFU、公钥认证、分块 / 续传 / 文件夹同步），端到端证明这些移植代码正确。
//!
//! exec 命令经 Git Bash 执行（提供 dd / cat / find / stat / mkdir 等 Unix 工具），
//! 使 app 的 `dd if=… bs=… skip=… count=1`、`cat >` / `cat >>`、`find … -exec stat` 等
//! 传输协议可以在 Windows 上被真实执行并验证。

use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Msg, Server as _, Session, VerifiedPublicKey};
use russh::{Channel, ChannelId, Preferred};
use std::os::windows::process::ExitStatusExt;
use std::path::Path;
use std::collections::HashMap;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::Mutex as AsyncMutex;

/// 探测 Git Bash 可执行文件。支持 `GIT_BASH` 环境变量覆盖，否则尝试常见安装路径，
/// 最后回退到 `PATH` 中的 `bash`。
pub fn find_git_bash() -> String {
    if let Ok(p) = std::env::var("GIT_BASH") {
        if !p.is_empty() {
            return p;
        }
    }
    for cand in [
        "C:/Program Files/Git/bin/bash.exe",
        "C:/Program Files (x86)/Git/bin/bash.exe",
        "C:/Git/bin/bash.exe",
        // WorkBuddy 自带 PortableGit（本机实际安装位置）。
        "D:/WorkBuddy/resources/vendor/PortableGit/usr/bin/bash.exe",
        "D:/WorkBuddy/resources/vendor/PortableGit/bin/bash.exe",
    ] {
        if std::path::Path::new(cand).exists() {
            return cand.to_string();
        }
    }
    // 兜底：用当前进程环境中 bash 的真实 Windows 路径（cygpath 结果由调用方注入 GIT_BASH）。
    "bash".to_string()
}

/// 加载主机密钥；若不存在则生成 ed25519 并落盘（用于 TOFU 测试的稳定指纹）。
pub fn load_or_gen_host_key(path: &Path) -> Result<PrivateKey, String> {
    if path.exists() {
        return PrivateKey::read_openssh_file(path)
            .map_err(|e| format!("读主机密钥 {path:?}: {e}"));
    }
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .map_err(|e| e.to_string())?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let pem = key.to_openssh(LineEnding::LF).map_err(|e| e.to_string())?;
    std::fs::write(path, pem).map_err(|e| e.to_string())?;
    Ok(key)
}

/// 每个 exec 通道对应一个 stdin 发送端：客户端的 SSH `data` 消息经此转发给子进程。
type StdinSenders = Arc<AsyncMutex<HashMap<ChannelId, mpsc::UnboundedSender<Vec<u8>>>>>;

#[derive(Clone)]
pub struct Server {
    pub remote_root: std::path::PathBuf,
    pub bash: String,
    pub stdins: StdinSenders,
}

impl Server {
    pub fn new(remote_root: &Path, bash: &str) -> Self {
        Server {
            remote_root: remote_root.to_path_buf(),
            bash: bash.to_string(),
            stdins: Arc::new(AsyncMutex::new(HashMap::new())),
        }
    }
}

impl russh::server::Server for Server {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
    fn handle_session_error(&mut self, _error: <Self::Handler as russh::server::Handler>::Error) {}
}

impl russh::server::Handler for Server {
    type Error = russh::Error;

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: russh::ChannelOpenHandleInner<Msg>,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _key: VerifiedPublicKey<'_>,
    ) -> Result<russh::server::Auth, Self::Error> {
        // 回环测试：无条件接受（与 app 端的「公钥认证」调用路径一致）。
        Ok(russh::server::Auth::Accept)
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let cmd = String::from_utf8_lossy(data).to_string();
        // 先注册 stdin 转发通道，再回 channel_success —— 否则客户端可能在注册前就发 data。
        let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
        self.stdins.lock().await.insert(channel, tx);
        let _ = session.channel_success(channel);
        // 取出会话句柄，交给后台任务执行命令并回传输出（避免阻塞事件循环）。
        let handle = session.handle();
        let bash = self.bash.clone();
        let root = self.remote_root.clone();
        let stdins = self.stdins.clone();
        tokio::spawn(async move {
            run_cmd(&handle, channel, &bash, &root, &cmd, rx, stdins).await;
        });
        Ok(())
    }

    /// 客户端发来的 channel data → 子进程 stdin（上传 `cat >` / `cat >>` 依赖此路径）。
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let tx = self.stdins.lock().await.get(&channel).cloned();
        if let Some(tx) = tx {
            let _ = tx.send(data.to_vec());
        }
        Ok(())
    }

    /// 客户端 EOF → 关闭子进程 stdin，让 `cat` 结束并 flush。
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.stdins.lock().await.remove(&channel);
        Ok(())
    }
}

/// 在 Git Bash 下执行脚本，将 stdout/stderr/exit-status/EOF 经 SSH 通道回传。
async fn run_cmd(
    handle: &russh::server::Handle,
    channel: ChannelId,
    bash: &str,
    root: &Path,
    cmd: &str,
    mut stdin_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    stdins: StdinSenders,
) {
    // 在 remote_root 下执行，规避 Windows 盘符路径转换（MSYS 会把 C:/ 重写成 /c/）。
    // 反斜杠在 bash 双引号里会被当转义符，统一改成正斜杠。
    let root_s = root.display().to_string().replace('\\', "/");
    let script = format!("cd \"{}\" 2>/dev/null; {}", root_s, cmd);
    let mut child = match Command::new(bash)
        .arg("--noprofile")
        .arg("--norc")
        .arg("-c")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            stdins.lock().await.remove(&channel);
            let _ = handle
                .data(channel, format!("spawn error: {e}\n").into_bytes())
                .await;
            let _ = handle.exit_status_request(channel, 127).await;
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
            return;
        }
    };
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let mut child_stdin = child.stdin.take().unwrap();

    // 把客户端 SSH data 消息持续写入子进程 stdin；rx 关闭（客户端 EOF）时 drop stdin。
    let stdin_task = tokio::spawn(async move {
        while let Some(chunk) = stdin_rx.recv().await {
            if child_stdin.write_all(&chunk).await.is_err() {
                break;
            }
        }
        let _ = child_stdin.shutdown().await;
    });

    let h_out = handle.clone();
    let out_task = tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let _ = h_out.data(channel, buf[..n].to_vec()).await;
                }
                Err(_) => break,
            }
        }
    });
    let h_err = handle.clone();
    let err_task = tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let _ = h_err.extended_data(channel, 1, buf[..n].to_vec()).await;
                }
                Err(_) => break,
            }
        }
    });

    let _ = out_task.await;
    let _ = err_task.await;
    // 客户端未显式发 EOF 时也要让读 stdin 的命令结束（否则 `cat >` 会挂住）。
    stdins.lock().await.remove(&channel);
    let _ = stdin_task.await;
    let status: ExitStatus = child
        .wait()
        .await
        .unwrap_or_else(|_| ExitStatus::from_raw(1));
    let code = status.code().unwrap_or(1) as u32;
    let _ = handle.exit_status_request(channel, code).await;
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;
}

/// 启动回环 SSH 服务（后台），返回可用于优雅关停的句柄。
pub fn start_server(
    bind: &str,
    key_path: &Path,
    remote_root: &Path,
    bash: &str,
) -> Result<russh::server::RunningServerHandle, String> {
    let key = load_or_gen_host_key(key_path)?;
    let config = Arc::new(russh::server::Config {
        inactivity_timeout: Some(Duration::from_secs(3600)),
        auth_rejection_time: Duration::from_secs(1),
        auth_rejection_time_initial: Some(Duration::from_secs(0)),
        keys: vec![key],
        preferred: Preferred::default(),
        ..Default::default()
    });
    let std_listener =
        std::net::TcpListener::bind(bind).map_err(|e| format!("bind {bind}: {e}"))?;
    // tokio 的 from_std 要求底层 socket 处于非阻塞模式，否则 accept().await 会阻塞运行时线程。
    std_listener
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;
    let tokio_listener =
        tokio::net::TcpListener::from_std(std_listener).map_err(|e| e.to_string())?;
    // 泄漏到 'static，使 run_on_socket 返回的 future 可被 tokio::spawn（要求 'static）。
    let listener: &'static tokio::net::TcpListener = Box::leak(Box::new(tokio_listener));
    // 泄漏到 'static：run_on_socket 返回的 future 会借用 sh，需 'static 才能 tokio::spawn。
    let sh = Box::leak(Box::new(Server::new(remote_root, bash)));
    let running = sh.run_on_socket(config, listener);
    let handle = running.handle();
    tokio::spawn(async move {
        let _ = running.await;
    });
    Ok(handle)
}
