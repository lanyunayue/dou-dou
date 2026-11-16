//! Tauri 端 pty 会话：开 SSH pty channel 或本地 Shell 进程，流式推给前端 xterm。

use russh::ChannelMsg;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::monitor;
use crate::ssh;
use zeroize::Zeroizing;

/// 一个 pty 会话。
///
/// 会话 id 即 `PtyState::sessions` 的 key，不另存一份；
/// 连接目标（host/user）在 `open_pty` 入口已打过日志（不含凭据），也无需在会话里冗余留存。
pub struct PtySession {
    pub tx: mpsc::UnboundedSender<String>,
}

#[derive(Default)]
pub struct PtyState {
    pub sessions: Mutex<HashMap<String, PtySession>>,
}

/// 开 pty 会话（SSH），返回 session_id。
///
/// `password` 为 Some 时走密码认证（密钥参数被忽略）；None 走私钥路径。
/// 密码只经内存传给 SSH 认证，**不落盘、不写日志**。
pub async fn open_pty(
    app: AppHandle,
    state: &PtyState,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    password: Option<Zeroizing<String>>,
) -> Result<String, String> {
    eprintln!(
        "[pty] open_pty host={host} port={port} user={user} auth={}",
        if password.is_some() {
            "password"
        } else {
            "publickey"
        }
    );
    let session = if let Some(pw) = password {
        ssh::ssh_connect_with_password(app.clone(), host, port, user, pw).await
    } else {
        ssh::ssh_connect(app.clone(), host, port, user, key_path).await
    };
    let session = match session {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[pty] connect failed host={host}:{port} err={e}");
            return Err(e);
        }
    };

    // 会话建立成功：挂上主机监控（exec 失败会自动停表）。
    // russh 0.65 的 Handle 不支持 Clone；channel_open_session 只需 &self，
    // 用 Arc 共享同一会话句柄，会话存活由 channel（内部持有 sender）保持。
    let session = std::sync::Arc::new(session);
    monitor::attach(session.clone(), app.clone());

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("开 channel: {e}"))?;
    channel
        .request_pty(false, "xterm", 80, 24, 640, 480, &[])
        .await
        .map_err(|e| format!("pty: {e}"))?;
    // F-06 长连接保护：优先挂到既有 tmux 会话，没有则新建；
    // 关窗断开后远端 tmux 会话保留，重开窗口可 attach 接管。
    let tmux_cmd = "tmux new-session -A -s dou 2>/dev/null || exec $SHELL";
    channel
        .exec(true, tmux_cmd)
        .await
        .map_err(|e| format!("shell: {e}"))?;

    let sid = uuid::Uuid::new_v4().to_string();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // 后台：读 channel stdout，推事件
    let sid_e = sid.clone();
    let app_e = app.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                maybe_msg = channel.wait() => {
                    match maybe_msg {
                        Some(ChannelMsg::Data { data }) => {
                            let s = String::from_utf8_lossy(&data).to_string();
                            let _ = app_e.emit("pty_data", serde_json::json!({"sid": sid_e, "data": s}));
                        }
                        Some(ChannelMsg::ExtendedData { data, .. }) => {
                            let s = String::from_utf8_lossy(&data).to_string();
                            let _ = app_e.emit("pty_data", serde_json::json!({"sid": sid_e, "data": s}));
                        }
                        Some(ChannelMsg::Eof) | Some(ChannelMsg::ExitStatus { .. }) => {
                            let _ = app_e.emit("pty_exit", serde_json::json!({"sid": sid_e}));
                            break;
                        }
                        _ => {}
                    }
                }
                Some(input) = rx.recv() => {
                    let _ = channel.data(std::io::Cursor::new(input.into_bytes())).await;
                }
                else => break,
            }
        }
    });

    // 存到 state
    {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(sid.clone(), PtySession { tx });
    }

    Ok(sid)
}

/// 开本地 Shell 会话（学 MobaXterm 的 Local terminal）：无需靶机，直接 spawn
/// 本机 PowerShell，stdin/stdout 管道桥接到同一套 pty_data/pty_write 通道。
/// 靶机不可达时的终端兜底，交互体验为管道模式（无 ConPTY 的完整 VT 控制）。
pub async fn open_local_pty(app: AppHandle, state: &PtyState) -> Result<String, String> {
    eprintln!("[pty] open_local_pty (powershell)");
    let mut cmd = tokio::process::Command::new("powershell");
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-NoExit",
        "-Command",
        // 管道模式显式统一 UTF-8（中文 Windows 默认 GBK 会导致 xterm 乱码），并落到用户目录。
        "[Console]::InputEncoding=[Text.Encoding]::UTF8; [Console]::OutputEncoding=[Text.Encoding]::UTF8; Set-Location $HOME",
    ])
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动本地 PowerShell: {e}"))?;

    let mut stdout = child.stdout.take().ok_or("stdout 不可读")?;
    let mut stderr = child.stderr.take().ok_or("stderr 不可读")?;
    let mut stdin = child.stdin.take().ok_or("stdin 不可写")?;

    let sid = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::unbounded_channel::<String>();

    // stdout / stderr → pty_data（两个独立 task，避免单 task select 在一方 EOF 后饿死另一方）
    let app_o = app.clone();
    let sid_o = sid.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_o.emit("pty_data", serde_json::json!({"sid": sid_o, "data": s}));
                }
            }
        }
    });
    let app_e2 = app.clone();
    let sid_e2 = sid.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_e2.emit("pty_data", serde_json::json!({"sid": sid_e2, "data": s}));
                }
            }
        }
    });

    // 前端输入 → stdin
    tokio::spawn(async move {
        let mut rx = rx;
        while let Some(input) = rx.recv().await {
            if stdin.write_all(input.as_bytes()).await.is_err() {
                break;
            }
            let _ = stdin.flush().await;
        }
    });

    // 退出 → pty_exit
    let sid_w = sid.clone();
    let app_w = app.clone();
    tokio::spawn(async move {
        let _ = child.wait().await;
        let _ = app_w.emit("pty_exit", serde_json::json!({"sid": sid_w}));
    });

    {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(sid.clone(), PtySession { tx });
    }

    Ok(sid)
}

/// 前端输入写回 channel。
pub async fn pty_write(state: &PtyState, sid: &str, data: &str) -> Result<(), String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let s = sessions
        .get(sid)
        .ok_or_else(|| format!("session {sid} 不存在"))?;
    s.tx.send(data.to_string()).map_err(|e| e.to_string())
}
