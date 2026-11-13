//! Tauri 端：文件传输、广播、导出。
//!
//! 所有连接统一经 `ssh::ssh_connect`（强制主机密钥校验 + 公钥认证），
//! 不再各自 `Ok(true)` 盲目信任。保留既有 `download/upload/...` 命令供前端文件树/编辑器使用；
//! 新增 `remote_stat_session` 供传输队列与文件夹同步在已有会话上复用，避免重复建连。

use crate::ssh;
use russh::ChannelMsg;
use std::path::Path;
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

/// 上传本地文件到远端（`cat > remote`）。
pub async fn upload(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    local: &str,
    remote: &str,
) -> Result<(), String> {
    let content = std::fs::read(local).map_err(|e| format!("读本地: {e}"))?;
    let mut session = ssh::ssh_connect(app, host, port, user, key_path).await?;
    let mut channel = session.channel_open_session().await.map_err(|e| e.to_string())?;
    channel.exec(true, format!("cat > {remote}")).await.map_err(|e| e.to_string())?;
    channel.data(std::io::Cursor::new(content)).await.map_err(|e| e.to_string())?;
    channel.eof().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// 分块下载：每次读 64KB，支持大文件。
pub async fn download_chunked(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    remote: &str,
) -> Result<String, String> {
    let mut session = ssh::ssh_connect(app, host, port, user, key_path).await?;
    let mut buf = Vec::new();
    let chunk_size: u64 = 65536; // 64KB
    let mut offset: u64 = 0;
    loop {
        let mut channel = session.channel_open_session().await.map_err(|e| e.to_string())?;
        channel
            .exec(true, format!("dd if={remote} bs={chunk_size} skip={offset} count=1 2>/dev/null"))
            .await
            .map_err(|e| e.to_string())?;
        let mut chunk = Vec::new();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => chunk.extend_from_slice(&data),
                ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
                _ => {}
            }
        }
        if chunk.is_empty() {
            break;
        }
        buf.extend_from_slice(&chunk);
        if (chunk.len() as u64) < chunk_size {
            break;
        }
        offset += 1;
        if offset > 1024 * 1024 {
            break;
        } // 安全上限 64GB
    }
    let local = std::env::temp_dir().join("dou_download_chunked.bin");
    std::fs::write(&local, &buf).map_err(|e| e.to_string())?;
    Ok(local.to_string_lossy().to_string())
}

/// 导出终端日志（去 ANSI 序列）。
pub fn sanitize_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            while let Some(&n) = chars.peek() {
                chars.next();
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
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

/// 递归下载远端目录：tar 打包成 .tar.gz 落到本地（F-03）。
pub async fn download_dir(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    remote: &str,
) -> Result<String, String> {
    let path = Path::new(remote);
    let parent = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string());
    let base = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| remote.to_string());
    let mut session = ssh::ssh_connect(app, host, port, user, key_path).await?;
    let mut channel = session.channel_open_session().await.map_err(|e| e.to_string())?;
    channel
        .exec(true, format!("tar czf - -C {parent} {base}"))
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => buf.extend_from_slice(&data),
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    let local = std::env::temp_dir().join(format!("dou_dir_{}.tar.gz", base));
    std::fs::write(&local, &buf).map_err(|e| e.to_string())?;
    Ok(local.to_string_lossy().to_string())
}

/// 递归上传本地目录：本地打包 tar.gz，管道到远端解包（F-03）。
pub async fn upload_dir(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    local: &str,
    remote: &str,
) -> Result<(), String> {
    let local_path = Path::new(local);
    let parent = local_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    let base = local_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| local.to_string());
    let pack = std::process::Command::new("tar")
        .args(["-czf", "-", "-C", &parent, &base])
        .output()
        .map_err(|e| format!("本地打包: {e}"))?;
    if !pack.status.success() {
        return Err(format!(
            "打包失败: {}",
            String::from_utf8_lossy(&pack.stderr)
        ));
    }
    let remote_path = Path::new(remote);
    let r_parent = remote_path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string());
    let mut session = ssh::ssh_connect(app, host, port, user, key_path).await?;
    let mut channel = session.channel_open_session().await.map_err(|e| e.to_string())?;
    channel
        .exec(true, format!("tar xzf - -C {r_parent}"))
        .await
        .map_err(|e| e.to_string())?;
    for chunk in pack.stdout.chunks(65536) {
        channel
            .data(std::io::Cursor::new(chunk.to_vec()))
            .await
            .map_err(|e| e.to_string())?;
    }
    channel.eof().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// 断点续传下载：从 given_offset 开始追加，返回 (本地路径, 已完成 offset, 是否结束)。
pub async fn download_resume(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    remote: &str,
    offset: u64,
) -> Result<(String, u64, bool), String> {
    let mut session = ssh::ssh_connect(app, host, port, user, key_path).await?;
    let local = std::env::temp_dir().join("dou_resume.bin");
    let chunk: u64 = 65536;
    let mut channel = session.channel_open_session().await.map_err(|e| e.to_string())?;
    channel
        .exec(true, format!("dd if={remote} bs={chunk} skip={offset} count=1 2>/dev/null"))
        .await
        .map_err(|e| e.to_string())?;
    let mut cbuf = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => cbuf.extend_from_slice(&data),
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&local)
        .map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut f, &cbuf).map_err(|e| e.to_string())?;
    let done = cbuf.len() < chunk as usize;
    Ok((local.to_string_lossy().to_string(), offset + (cbuf.len() as u64), done))
}
