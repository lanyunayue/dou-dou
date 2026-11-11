//! 主机密钥信任（TOFU）。
//!
//! 策略：首次连接记录该主机的公钥全文；之后每次连接比对全文，
//! 不匹配即拒绝连接（不提供"继续连接"的旁路）。
//!
//! 注意：这里比对的是**公钥全文**，不是 OpenSSH 的 SHA256 指纹字符串。
//! 两者安全性等价（指纹本来就是公钥的摘要），但展示格式不同，
//! 因此界面上写的是"主机公钥"，不冒充 `SHA256:` 前缀，避免用户拿它去和
//! `ssh-keygen -lf` 的结果对不上。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct KnownHosts {
    path: PathBuf,
    map: HashMap<String, String>,
    /// known_hosts 存在但无法解析时为 false。此时一律拒绝连接，
    /// 绝不降级为"首次信任"——否则删改该文件即可绕过主机身份校验。
    load_ok: bool,
}

impl Default for KnownHosts {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            map: HashMap::new(),
            load_ok: true,
        }
    }
}

pub enum Check {
    /// 与已记录的公钥一致
    Match,
    /// 该主机第一次出现，已记录
    FirstSeen,
    /// 与该主机已记录的公钥不一致——可能是换了机器，也可能是中间人
    Mismatch { stored: String },
    /// known_hosts 文件损坏/不可读，拒绝一切连接
    StoreCorrupted,
}

impl KnownHosts {
    pub fn load(path: PathBuf) -> Self {
        let raw = fs::read_to_string(&path).ok();
        match raw {
            None => Self {
                path,
                map: HashMap::new(),
                load_ok: true,
            },
            Some(s) => match serde_json::from_str::<HashMap<String, String>>(&s) {
                Ok(map) => Self {
                    path,
                    map,
                    load_ok: true,
                },
                Err(_) => Self {
                    path,
                    map: HashMap::new(),
                    load_ok: false,
                },
            },
        }
    }

    pub fn check_and_record(&mut self, addr: &str, presented: &str) -> Check {
        if !self.load_ok {
            return Check::StoreCorrupted;
        }
        match self.map.get(addr) {
            Some(stored) if stored == presented => Check::Match,
            Some(stored) => Check::Mismatch {
                stored: stored.clone(),
            },
            None => {
                self.map.insert(addr.to_string(), presented.to_string());
                let _ = self.flush();
                Check::FirstSeen
            }
        }
    }

    fn flush(&self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(&self.map).map_err(|e| e.to_string())?;
        fs::write(&self.path, json).map_err(|e| format!("write {}: {e}", self.path.display()))
    }
}

/// 公钥行的短展示：算法名 + base64 首尾各几位。
/// 只用于日志与界面辨识，不用于比对。
pub fn short_id(line: &str) -> String {
    let mut parts = line.split_whitespace();
    let algo = parts.next().unwrap_or("?");
    let blob = parts.next().unwrap_or("");
    let head: String = blob.chars().take(12).collect();
    let tail: String = blob
        .chars()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{algo}:{head}…{tail}")
}
