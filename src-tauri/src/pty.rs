//! Tauri 端 pty 会话：开 SSH pty channel，流式推给前端 xterm。

use russh::ChannelMsg;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::ssh;

pub struct PtySession {
    pub id: String,
    pub host: String,
    pub user: String,
    pub tx: mpsc::UnboundedSender<String>,
}

#[derive(Default)]
pub struct PtyState {
    pub sessions: Mutex<HashMap<String, PtySession>>,
}

/// 开 pty 会话，返回 session_id。
pub async fn open_pty(
    app: AppHandle,
    state: &PtyState,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
) -> Result<String, String> {
    eprintln!("[pty] open_pty host={host} port={port} user={user} key={key_path}");
    let mut session = match ssh::ssh_connect(app.clone(), host, port, user, key_path).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[pty] connect failed host={host}:{port} err={e}");
            return Err(e);
        }
    };

    let mut channel = session.channel_open_session().await
        .map_err(|e| format!("开 channel: {e}"))?;
    channel.request_pty(false, "xterm", 80, 24, 640, 480, &[])
        .await.map_err(|e| format!("pty: {e}"))?;
    // F-06 长连接保护：优先挂到既有 tmux 会话，没有则新建；
    // 关窗断开后远端 tmux 会话保留，重开窗口可 attach 接管。
    let tmux_cmd = "tmux new-session -A -s dou 2>/dev/null || exec $SHELL";
    channel.exec(true, tmux_cmd).await.map_err(|e| format!("shell: {e}"))?;

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
        sessions.insert(sid.clone(), PtySession {
            id: sid.clone(),
            host: host.to_string(),
            user: user.to_string(),
            tx,
        });
    }

    Ok(sid)
}

/// 前端输入写回 channel。
pub async fn pty_write(state: &PtyState, sid: &str, data: &str) -> Result<(), String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let s = sessions.get(sid).ok_or_else(|| format!("session {sid} 不存在"))?;
    s.tx.send(data.to_string()).map_err(|e| e.to_string())
}
