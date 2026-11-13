//! Tauri 端：文件传输、广播、导出。
//!
//! 所有连接统一经 `ssh::ssh_connect`（强制主机密钥校验 + 公钥认证），
//! 不再各自 `Ok(true)` 盲目信任。保留 `download` 命令供前端文件树/编辑器使用；
//! `remote_stat_session` 供传输队列与文件夹同步在已有会话上复用，避免重复建连。

use crate::ssh;
use russh::ChannelMsg;
use tauri::AppHandle;

/// 下载远端文件到本地临时目录（整文件 `cat`）。
pub async fn download(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    remote: &str,
) -> Result<String, String> {
    let mut session = ssh::ssh_connect(app, host, port, user, key_path).await?;
    let mut channel = session.channel_open_session().await.map_err(|e| e.to_string())?;
    channel.exec(true, format!("cat {remote}")).await.map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => buf.extend_from_slice(&data),
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    let local = std::env::temp_dir().join("dou_download.bin");
    std::fs::write(&local, &buf).map_err(|e| e.to_string())?;
    Ok(local.to_string_lossy().to_string())
}

/// 远端文件属性：(size_bytes, mtime_epoch)，供编辑器保存前比对（F-04）。
pub async fn remote_stat(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    remote: &str,
) -> Result<(u64, i64), String> {
    let mut session = ssh::ssh_connect(app, host, port, user, key_path).await?;
    remote_stat_session(&session, remote).await
}

/// 在已有会话上取远端文件属性 (size, mtime)，供队列/同步复用。
pub async fn remote_stat_session(
    session: &ssh::DouHandle,
    remote: &str,
) -> Result<(u64, i64), String> {
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .exec(true, format!("stat -c '%s %Y' {remote}"))
        .await
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => out.push_str(&String::from_utf8_lossy(&data)),
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    let parts: Vec<&str> = out.trim().split_whitespace().collect();
    let size = parts.first().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let mtime = parts.get(1).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    Ok((size, mtime))
}
