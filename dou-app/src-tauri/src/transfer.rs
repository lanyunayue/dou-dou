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
    let session = ssh::ssh_connect(app, host, port, user, key_path).await?;
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .exec(true, format!("cat {remote}"))
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
    let session = ssh::ssh_connect(app, host, port, user, key_path).await?;
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
    let parts: Vec<&str> = out.split_whitespace().collect();
    let size = parts
        .first()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let mtime = parts
        .get(1)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    Ok((size, mtime))
}

/// 把远端路径用单引号包裹用于 shell exec，内部单引号替换为 `'\''`，防注入。
/// 与网关侧路径闸门定位不同：此处仅防命令注入，不做敏感目录拦截（那是受控通道职责）。
fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

/// 读取远端文本文件全文（A4 编辑器读通道）。大文件由前端按 stat 懒加载，不在此截断。
pub async fn remote_read(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    remote: &str,
) -> Result<String, String> {
    if remote.trim().is_empty() {
        return Err("远端路径不能为空".to_string());
    }
    let session = ssh::ssh_connect(app, host, port, user, key_path).await?;
    let channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    let mut channel = channel;
    channel
        .exec(true, format!("cat -- {}", shell_quote(remote)))
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    let mut code: Option<u32> = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => buf.extend_from_slice(&data),
            ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
            ChannelMsg::Eof => break,
            _ => {}
        }
    }
    if let Some(c) = code
        && c != 0
    {
        return Err(format!("远端读取失败（exit {c}）：文件不存在或无权限"));
    }
    String::from_utf8(buf).map_err(|_| "远端文件不是合法 UTF-8 文本，无法在编辑器打开".to_string())
}

/// 写回远端文本文件全文（A4 编辑器写通道），覆盖写；写入后校验远端 exit code。
pub async fn remote_write(
    app: AppHandle,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    remote: &str,
    content: &str,
) -> Result<(), String> {
    if remote.trim().is_empty() {
        return Err("远端路径不能为空".to_string());
    }
    let session = ssh::ssh_connect(app, host, port, user, key_path).await?;
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .exec(true, format!("cat > {}", shell_quote(remote)))
        .await
        .map_err(|e| e.to_string())?;
    channel
        .data(std::io::Cursor::new(content.as_bytes().to_vec()))
        .await
        .map_err(|e| e.to_string())?;
    channel.eof().await.map_err(|e| e.to_string())?;
    let mut code: Option<u32> = None;
    while let Some(msg) = channel.wait().await {
        if let ChannelMsg::ExitStatus { exit_status } = msg {
            code = Some(exit_status);
        }
        if matches!(msg, ChannelMsg::Eof) {
            break;
        }
    }
    match code {
        Some(0) => Ok(()),
        Some(c) => Err(format!("远端写入失败（exit {c}）：无权限或磁盘错误")),
        None => Err("远端写入未返回退出状态，无法确认是否成功".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::shell_quote;

    #[test]
    fn shell_quote_普通路径包单引号() {
        assert_eq!(shell_quote("/home/dou/a.txt"), "'/home/dou/a.txt'");
    }

    #[test]
    fn shell_quote_含空格路径不拆词() {
        assert_eq!(
            shell_quote("/home/dou/my file.txt"),
            "'/home/dou/my file.txt'"
        );
    }

    #[test]
    fn shell_quote_单引号注入被转义() {
        // a'b → 'a'\''b' —— shell 内仍是一个参数，不闭合引号
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
        let evil = "x'; rm -rf /; '";
        let quoted = shell_quote(evil);
        // 外壳首尾仍是单引号，且内部出现反斜杠转义（证明单引号未裸奔）
        assert!(quoted.starts_with('\'') && quoted.ends_with('\''));
        assert!(quoted.contains('\\'));
        assert_ne!(quoted, format!("'{evil}'"));
    }

    #[test]
    fn shell_quote_空串仍是合法参数() {
        assert_eq!(shell_quote(""), "''");
    }
}
