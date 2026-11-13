//! Tauri 端 SFTP 树状浏览：用 exec 跑 ls 解析。

use serde::Serialize;
use tauri::AppHandle;

use crate::ssh;

#[derive(Serialize)]
pub struct SftpEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// 列目录：exec `ls -1p <path>`，每行一个条目，以 / 结尾是目录。
pub async fn sftp_list(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    path: &str,
) -> Result<Vec<SftpEntry>, String> {
    let mut session = ssh::ssh_connect(app, host, port, user, key_path).await?;

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .exec(true, format!("ls -1p {path}"))
        .await
        .map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { data } => {
                let s = String::from_utf8_lossy(&data);
                for line in s.lines() {
                    let name = line.trim_end_matches('/').to_string();
                    if name.is_empty() {
                        continue;
                    }
                    let is_dir = line.ends_with('/');
                    out.push(SftpEntry {
                        name,
                        is_dir,
                        size: 0,
                    });
                }
            }
            russh::ChannelMsg::Eof | russh::ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(out)
}
