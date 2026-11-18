//! Tauri 端 SFTP 树状浏览：用 exec 跑 ls 解析。

use serde::Serialize;
use tauri::AppHandle;

use crate::ssh;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SftpEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// 解析 `ls -1p` 输出：每行一个条目，以 / 结尾是目录；目录排在前，再按名称升序。
/// 纯函数，便于离线单测；连接与收流在 `sftp_list`。
fn parse_ls_entries(text: &str) -> Vec<SftpEntry> {
    let mut out: Vec<SftpEntry> = text
        .lines()
        .filter_map(|line| {
            let name = line.trim_end_matches('/').to_string();
            if name.is_empty() {
                return None;
            }
            let is_dir = line.ends_with('/');
            Some(SftpEntry {
                name,
                is_dir,
                size: 0,
            })
        })
        .collect();
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    out
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
    let session = ssh::ssh_connect(app, host, port, user, key_path).await?;

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .exec(true, format!("ls -1p {path}"))
        .await
        .map_err(|e| e.to_string())?;

    let mut raw = String::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { data } => {
                raw.push_str(&String::from_utf8_lossy(&data));
            }
            russh::ChannelMsg::Eof | russh::ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    Ok(parse_ls_entries(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ls_marks_dirs_and_sorts_dirs_first() {
        let text = "readme.txt\npub/\napp/\nconfig.json\n";
        let entries = parse_ls_entries(text);
        assert_eq!(
            entries,
            vec![
                SftpEntry { name: "app".into(), is_dir: true, size: 0 },
                SftpEntry { name: "pub".into(), is_dir: true, size: 0 },
                SftpEntry { name: "config.json".into(), is_dir: false, size: 0 },
                SftpEntry { name: "readme.txt".into(), is_dir: false, size: 0 },
            ]
        );
    }

    #[test]
    fn parse_ls_skips_empty_lines() {
        let entries = parse_ls_entries("\n\nfile.txt\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "file.txt");
        assert!(!entries[0].is_dir);
    }

    #[test]
    fn parse_ls_trailing_slash_stripped_from_name() {
        let entries = parse_ls_entries("subdir/\n");
        assert_eq!(entries[0].name, "subdir");
        assert!(entries[0].is_dir);
    }
}
