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
    std::path::PathBuf::from(base).join("dou").join("sanitize_rules.json")
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
        let Ok(text) = std::fs::read_to_string(config_path()) else { return };
        let Ok(json) = serde_json::from_str::<Value>(&text) else { return };
        let Some(arr) = json.as_array() else { return };
        for rule in arr {
            let Some(pattern) = rule.get("pattern").and_then(|v| v.as_str()) else { continue };
            let replacement = rule.get("replacement").and_then(|v| v.as_str()).unwrap_or("[REDACTED]");
            if let Ok(re) = Regex::new(pattern) {
                self.extra.push((re, replacement.to_string()));
            }
        }
    }

    pub fn sanitize(&self, text: &str) -> String {
        let mut s = text.to_string();
        s = self.priv_key.replace_all(&s, "[REDACTED:PRIVATE KEY]").to_string();
        s = self.aws_key.replace_all(&s, "[REDACTED:AWS KEY]").to_string();
        s = self.bearer.replace_all(&s, "Bearer [REDACTED]").to_string();
        s = self.jwt.replace_all(&s, "[REDACTED:JWT]").to_string();
        s = self.internal_ip.replace_all(&s, "[REDACTED:IP]").to_string();
        s = self.ssh_path.replace_all(&s, "[REDACTED:PATH]").to_string();
        for (re, repl) in &self.extra {
            s = re.replace_all(&s, repl.as_str()).to_string();
        }
        s
    }
}

impl Default for Sanitizer {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_private_key() {
        let s = Sanitizer::new();
        let out = s.sanitize("-----BEGIN OPENSSH PRIVATE KEY-----\nxxx\n-----END OPENSSH PRIVATE KEY-----");
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
}
