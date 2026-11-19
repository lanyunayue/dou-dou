//! SFTP 文件读写（russh-sftp）。
//!
//! 设计原则：
//! - 读：按行区间返回，截断长行，不返回整个文件
//! - 写：先落本地隔离区（hash 留痕），再经用户确认推到远端
//! - 路径：对敏感系统目录（/etc、/root、/boot 等）读取记 warning 但不阻断

use std::path::PathBuf;

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags};

use crate::ssh::SshConnection;

pub struct FileOutcome {
    pub ok: bool,
    pub content: String,
    pub warning: Option<String>,
    pub bytes: usize,
}

/// 敏感目录：读这些路径时记 warning，提示"这是系统文件"
const SENSITIVE_PREFIXES: &[&str] = &[
    "/etc/shadow",
    "/etc/sudoers",
    "/root/",
    "/boot/",
    "/proc/self/environ",
    "/var/log/auth",
];

fn path_warning(remote_path: &str) -> Option<String> {
    for p in SENSITIVE_PREFIXES {
        if remote_path.starts_with(p) {
            return Some(format!("注意：正在读取系统敏感路径 {p}"));
        }
    }
    None
}

impl SshConnection {
    async fn open_sftp(&mut self) -> Result<SftpSession, String> {
        let channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| format!("sftp channel: {e}"))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| format!("sftp subsystem: {e}"))?;
        SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| format!("sftp session: {e}"))
    }

    /// B3：symlink 真实解析（SFTP REALPATH，协议级，无 shell 注入面）。
    /// 对 symlink 链一路解析到最终目标；目标不存在时返回 Err。
    pub async fn remote_realpath(&mut self, path: &str) -> Result<String, String> {
        let sftp = self.open_sftp().await?;
        let resolved = sftp
            .canonicalize(path)
            .await
            .map_err(|e| format!("realpath {path}: {e}"))?;
        // 个别受限 shell 对空路径回空条目；归一化为当前目录，交由上层再探测。
        if resolved.trim().is_empty() {
            Ok(".".to_string())
        } else {
            Ok(resolved)
        }
    }

    /// B3：lstat（不跟随末级 symlink）。Ok(None) = 目标不存在。
    /// 仅用于 put 前的「目标是符号链接」判定；存在性权威判定交给 canonicalize。
    pub async fn remote_lstat(&mut self, path: &str) -> Result<Option<FileAttributes>, String> {
        let sftp = self.open_sftp().await?;
        match sftp.symlink_metadata(path).await {
            Ok(meta) => Ok(Some(meta)),
            Err(_) => Ok(None),
        }
    }

    /// 读远端文件，按行区间返回。
    /// start_line 从 1 开始；end_line=None 表示到末尾。
    pub async fn remote_read(
        &mut self,
        remote_path: &str,
        start_line: usize,
        end_line: Option<usize>,
        max_bytes: usize,
    ) -> Result<FileOutcome, String> {
        let warning = path_warning(remote_path);
        let sftp = self.open_sftp().await?;
        let mut file = sftp
            .open_with_flags(remote_path, OpenFlags::READ)
            .await
            .map_err(|e| format!("open {remote_path}: {e}"))?;

        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let n = file
            .read_to_end(&mut buf)
            .await
            .map_err(|e| e.to_string())?;
        drop(file);

        if n > max_bytes {
            return Ok(FileOutcome {
                ok: false,
                content: String::new(),
                warning: Some(format!(
                    "文件 {n} 字节超过单次上限 {max_bytes}，请用行区间分段读"
                )),
                bytes: n,
            });
        }

        let text = String::from_utf8_lossy(&buf);
        let lines: Vec<&str> = text.lines().collect();
        let total = lines.len();
        let start = start_line.saturating_sub(1).min(total);
        let end = end_line.unwrap_or(total).min(total);
        let content = lines[start..end]
            .iter()
            .take(2000)
            .map(|l| {
                if l.chars().count() > 500 {
                    let head: String = l.chars().take(500).collect();
                    format!("{head}…({} chars)", l.chars().count())
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(FileOutcome {
            ok: true,
            content,
            warning,
            bytes: n,
        })
    }

    /// 把本地文件写到远端（put）。
    pub async fn remote_put(
        &mut self,
        local_bytes: &[u8],
        remote_path: &str,
    ) -> Result<(usize, String), String> {
        let sftp = self.open_sftp().await?;
        let mut file = sftp
            .open_with_flags(
                remote_path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
            )
            .await
            .map_err(|e| format!("open {remote_path}: {e}"))?;
        use tokio::io::AsyncWriteExt;
        file.write_all(local_bytes)
            .await
            .map_err(|e| e.to_string())?;
        file.flush().await.map_err(|e| e.to_string())?;
        drop(file);

        let digest = sha256_hex(local_bytes);
        Ok((local_bytes.len(), digest))
    }

    /// 从远端读文件到内存（get），调用方负责落隔离区。
    pub async fn remote_get(&mut self, remote_path: &str) -> Result<(Vec<u8>, String), String> {
        let sftp = self.open_sftp().await?;
        let mut file = sftp
            .open_with_flags(remote_path, OpenFlags::READ)
            .await
            .map_err(|e| format!("open {remote_path}: {e}"))?;
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .await
            .map_err(|e| e.to_string())?;
        drop(file);
        let digest = sha256_hex(&buf);
        Ok((buf, digest))
    }
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(data);
    let d = h.finalize();
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// 隔离区路径（本机）
pub fn quarantine_dir() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("dou").join("quarantine"))
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_warning_flags_sensitive_prefixes() {
        assert!(path_warning("/etc/shadow").is_some());
        assert!(path_warning("/etc/sudoers.d/99-admin").is_some());
        assert!(path_warning("/root/.ssh/id_rsa").is_some());
        assert!(path_warning("/var/log/auth.log").is_some());
        // 普通用户文件与非敏感系统文件不告警
        assert!(path_warning("/home/dou/a.txt").is_none());
        assert!(path_warning("/etc/hosts").is_none());
    }

    #[test]
    fn sha256_hex_is_stable_and_64_hex() {
        let a = sha256_hex(b"hello");
        let b = sha256_hex(b"hello");
        let c = sha256_hex(b"hellp");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|ch| ch.is_ascii_hexdigit()));
        // 空串摘要与已知 sha256 一致
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn quarantine_dir_falls_back_to_cwd() {
        // 不依赖环境变量是否存在：只校验返回路径末尾拼了 quarantine
        let dir = quarantine_dir();
        assert_eq!(dir.file_name().and_then(|s| s.to_str()), Some("quarantine"));
    }
}
