//! Tauri 端 SFTP 树状浏览：用 exec 跑 ls 解析。

use serde::Serialize;

#[derive(Default)]
pub struct SftpState {
    pub connected: bool,
}

#[derive(Serialize)]
pub struct SftpEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// 列目录：exec `ls -1p <path>`，每行一个条目，以 / 结尾是目录。
pub async fn sftp_list(
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    path: &str,
) -> Result<Vec<SftpEntry>, String> {
    use russh::client;
    use russh::keys::PublicKeyOrCertificate;
    use std::sync::Arc;

    struct H;
    impl client::Handler for H {
        type Error = russh::Error;
        async fn check_server_key(&mut self, _k: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
            Ok(true)
        }
    }

    let config = Arc::new(client::Config::default());
    let mut session = client::connect(config, (host, port), H)
        .await
        .map_err(|e| format!("连接: {e}"))?;
    let key = russh::keys::PrivateKey::read_openssh_file(std::path::Path::new(key_path))
        .map_err(|e| format!("私钥: {e}"))?;
    session
        .authenticate_publickey(user, russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None))
        .await
        .map_err(|e| format!("认证: {e}"))?;

    let mut channel = session.channel_open_session().await.map_err(|e| e.to_string())?;
    channel.exec(true, format!("ls -1p {path}")).await.map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            russh::ChannelMsg::Data { data } => {
                let s = String::from_utf8_lossy(&data);
                for line in s.lines() {
                    let name = line.trim_end_matches('/').to_string();
                    if name.is_empty() { continue; }
                    let is_dir = line.ends_with('/');
                    out.push(SftpEntry { name, is_dir, size: 0 });
                }
            }
            russh::ChannelMsg::Eof { .. } | russh::ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(out)
}
