//! 双视图脱敏。
//!
//! 计划书 4.10：代理读到的日志/说明文件/依赖帮助文本都可能夹带指令，
//! 返回给代理时必须遮蔽：私钥块、云密钥、Bearer token、内网 IP、绝对路径下的敏感文件。
//! 人视图不脱敏，代理视图脱敏。

use regex::Regex;
use serde_json::Value;

pub struct Sanitizer {
    priv_key: Regex,
    aws_key: Regex,
    bearer: Regex,
    jwt: Regex,
    internal_ip: Regex,
    ssh_path: Regex,
    /// 用户/部署方追加的可配置规则（F-26）：(正则, 替换串)。
    extra: Vec<(Regex, String)>,
}

fn config_path() -> std::path::PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(base)
        .join("dou")
        .join("sanitize_rules.json")
}

impl Sanitizer {
    pub fn new() -> Self {
        let mut s = Self {
            priv_key: Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----").unwrap(),
            aws_key: Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
            bearer: Regex::new(r"(?i)bearer\s+[A-Za-z0-9._\-]+").unwrap(),
            jwt: Regex::new(r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}").unwrap(),
            internal_ip: Regex::new(r"\b(10\.\d{1,3}\.\d{1,3}\.\d{1,3}|192\.168\.\d{1,3}\.\d{1,3}|172\.(1[6-9]|2\d|3[01])\.\d{1,3}\.\d{1,3})\b").unwrap(),
            ssh_path: Regex::new(r#"(/home/[^\s"]*\.ssh/[^\s"]+|/root/\.ssh/[^\s"]+|/etc/shadow|/etc/sudoers[^\s]*)"#).unwrap(),
            extra: Vec::new(),
        };
        s.load_extra_rules();
        s
    }

    /// 从 sanitize_rules.json 读取追加规则；文件缺失或非法则保持默认，不阻断。
    fn load_extra_rules(&mut self) {
        let Ok(text) = std::fs::read_to_string(config_path()) else {
            return;
        };
        let Ok(json) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        let Some(arr) = json.as_array() else { return };
        for rule in arr {
            let Some(pattern) = rule.get("pattern").and_then(|v| v.as_str()) else {
                continue;
            };
            let replacement = rule
                .get("replacement")
                .and_then(|v| v.as_str())
                .unwrap_or("[REDACTED]");
            if let Ok(re) = Regex::new(pattern) {
                self.extra.push((re, replacement.to_string()));
            }
        }
    }

    pub fn sanitize(&self, text: &str) -> String {
        let mut s = text.to_string();
        s = self
            .priv_key
            .replace_all(&s, "[REDACTED:PRIVATE KEY]")
            .to_string();
        s = self
            .aws_key
            .replace_all(&s, "[REDACTED:AWS KEY]")
            .to_string();
        s = self.bearer.replace_all(&s, "Bearer [REDACTED]").to_string();
        s = self.jwt.replace_all(&s, "[REDACTED:JWT]").to_string();
        s = self
            .internal_ip
            .replace_all(&s, "[REDACTED:IP]")
            .to_string();
        s = self.ssh_path.replace_all(&s, "[REDACTED:PATH]").to_string();
        for (re, repl) in &self.extra {
            s = re.replace_all(&s, repl.as_str()).to_string();
        }
        s
    }
}

impl Default for Sanitizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_private_key() {
        let s = Sanitizer::new();
        let out = s.sanitize(
            "-----BEGIN OPENSSH PRIVATE KEY-----\nxxx\n-----END OPENSSH PRIVATE KEY-----",
        );
        assert!(out.contains("[REDACTED:PRIVATE KEY]"));
        assert!(!out.contains("OPENSSH PRIVATE"));
    }
    #[test]
    fn redacts_internal_ip() {
        let s = Sanitizer::new();
        let out = s.sanitize("connect to 192.168.1.10:22 done");
        assert!(out.contains("[REDACTED:IP]"));
    }
    #[test]
    fn leaves_public_content() {
        let s = Sanitizer::new();
        let out = s.sanitize("Linux version 6.18.33.2-microsoft-standard-WSL2");
        assert!(out.contains("Linux version"));
    }

    #[test]
    fn redacts_aws_key() {
        let s = Sanitizer::new();
        let out = s.sanitize("key AKIAIOSFODNN7EXAMPLE here");
        assert!(out.contains("[REDACTED:AWS KEY]"));
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn redacts_bearer_case_insensitive() {
        let s = Sanitizer::new();
        let out = s.sanitize("Authorization: Bearer abc123.def-xyz_9");
        assert!(out.contains("Bearer [REDACTED]"));
        assert!(!out.contains("abc123.def-xyz_9"));
        let lower = s.sanitize("bearer tok_lowercase_001");
        assert!(lower.contains("Bearer [REDACTED]"));
    }

    #[test]
    fn redacts_jwt() {
        let s = Sanitizer::new();
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let out = s.sanitize(&format!("token {jwt}"));
        assert!(out.contains("[REDACTED:JWT]"));
        assert!(!out.contains("eyJhbGciOiJIUzI1NiI"));
    }

    #[test]
    fn redacts_sensitive_paths() {
        let s = Sanitizer::new();
        for p in [
            "/root/.ssh/id_rsa",
            "/home/dou/.ssh/authorized_keys",
            "/etc/shadow",
            "/etc/sudoers.d/99",
        ] {
            let out = s.sanitize(p);
            assert!(out.contains("[REDACTED:PATH]"), "{p} 应被遮蔽");
        }
    }

    #[test]
    fn internal_ip_ranges_redacted_public_kept() {
        let s = Sanitizer::new();
        for ip in ["10.1.2.3", "192.168.1.10", "172.16.0.1", "172.31.9.9"] {
            assert!(s.sanitize(ip).contains("[REDACTED:IP]"), "{ip} 属内网应遮蔽");
        }
        // 公网地址不在内网段，保留
        assert!(!s.sanitize("8.8.8.8").contains("[REDACTED:IP]"));
        assert!(!s.sanitize("1.1.1.1").contains("[REDACTED:IP]"));
        // 172.15 / 172.32 不在 16-31 段，不遮蔽
        assert!(!s.sanitize("172.15.0.1").contains("[REDACTED:IP]"));
        assert!(!s.sanitize("172.32.0.1").contains("[REDACTED:IP]"));
    }

    #[test]
    fn mixed_secrets_cleaned_in_one_pass() {
        let s = Sanitizer::new();
        let text = "host 10.0.0.5 uses Bearer zzz.zzz and AKIAIOSFODNN7EXAMPLE, see /root/.ssh/id_rsa";
        let out = s.sanitize(text);
        assert!(!out.contains("10.0.0.5"));
        assert!(!out.contains("zzz.zzz"));
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!out.contains("/root/.ssh/id_rsa"));
        assert!(out.contains("[REDACTED:IP]"));
    }
}
