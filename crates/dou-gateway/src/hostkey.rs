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

#[derive(Debug)]
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
            Some(s) => {
                let trimmed = s.trim();
                // 兼容两种落盘格式：
                //   1) 网关原生：{ "host:port": "公钥全文" }
                //   2) 客户端（Tauri）：[ { host, port, fingerprint, ... } ]
                // 空文件 / 空数组视为尚无记录，不判损坏；只有既非对象又非数组才算解析失败。
                if trimmed.is_empty() {
                    return Self { path, map: HashMap::new(), load_ok: true };
                }
                if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(trimmed) {
                    return Self { path, map, load_ok: true };
                }
                if let Ok(entries) =
                    serde_json::from_str::<Vec<serde_json::Value>>(trimmed)
                {
                    let mut map = HashMap::new();
                    for e in entries {
                        let addr = e
                            .get("host")
                            .and_then(|v| v.as_str())
                            .map(|h| match e.get("port").and_then(|p| p.as_u64()) {
                                Some(p) => format!("{h}:{p}"),
                                None => h.to_string(),
                            });
                        let key = e
                            .get("fingerprint")
                            .and_then(|v| v.as_str())
                            .or_else(|| e.get("host_key").and_then(|v| v.as_str()));
                        if let (Some(addr), Some(key)) = (addr, key) {
                            map.insert(addr, key.to_string());
                        }
                    }
                    return Self { path, map, load_ok: true };
                }
                Self {
                    path,
                    map: HashMap::new(),
                    load_ok: false,
                }
            }
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

    /// 已记录的主机条目（按地址排序，仅用于展示；密钥给短摘要）。
    pub fn entries(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = self
            .map
            .iter()
            .map(|(k, s)| (k.clone(), s.clone()))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// 遗忘一条主机记录（B2 二次确认闭环：密钥轮换后由用户显式重信任）。
    /// 返回是否确有记录被移除（以内存为准）。下次连接将走 FirstSeen 重新记录。
    /// flush 失败不改变返回值：最坏情形是重启后旧记录从磁盘复活 → 再次
    /// Mismatch 拒绝，方向仍然 fail-closed，不会产生任何连接旁路。
    pub fn forget(&mut self, addr: &str) -> bool {
        if self.map.remove(addr).is_none() {
            return false;
        }
        let _ = self.flush();
        true
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forget_then_recheck_is_first_seen() {
        let mut kh = KnownHosts::default();
        assert!(matches!(
            kh.check_and_record("h1:22", "ssh-ed25519 AAAA_old"),
            Check::FirstSeen
        ));
        assert!(kh.forget("h1:22"));
        assert!(matches!(
            kh.check_and_record("h1:22", "ssh-ed25519 AAAA_new"),
            Check::FirstSeen
        ));
    }

    #[test]
    fn forget_unknown_addr_returns_false() {
        let mut kh = KnownHosts::default();
        assert!(!kh.forget("ghost:22"));
    }

    #[test]
    fn empty_array_is_not_corrupted() {
        let dir = std::env::temp_dir().join("dou_kh_empty_array_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("known_hosts.json");
        fs::write(&p, "[]").unwrap();
        let kh = KnownHosts::load(p.clone());
        assert!(kh.load_ok, "空数组不应判为损坏");
        assert!(kh.map.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn client_array_format_loads_entries() {
        let dir = std::env::temp_dir().join("dou_kh_array_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("known_hosts.json");
        fs::write(
            &p,
            r#"[{"host":"example.test","port":22,"fingerprint":"SHA256:abc","first_seen":1,"last_seen":1}]"#,
        )
        .unwrap();
        let kh = KnownHosts::load(p.clone());
        assert!(kh.load_ok);
        assert_eq!(kh.map.get("example.test:22"), Some(&"SHA256:abc".to_string()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn entries_sorted_and_mismatch_keeps_record() {
        let mut kh = KnownHosts::default();
        let _ = kh.check_and_record("b:22", "ssh-ed25519 AAAA_b");
        let _ = kh.check_and_record("a:22", "ssh-ed25519 AAAA_a");
        let entries = kh.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "a:22");
        // 换钥不匹配：记录保留原值，遗忘前重连依旧拒绝
        match kh.check_and_record("a:22", "ssh-ed25519 AAAA_rotated") {
            Check::Mismatch { stored } => assert_eq!(stored, "ssh-ed25519 AAAA_a"),
            other => panic!("期望 Mismatch，实际 {other:?}"),
        }
        assert_eq!(kh.entries().len(), 2);
    }
}
