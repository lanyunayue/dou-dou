//! 隔离区静态扫描。
//!
//! 计划书 4.3/4.10：下载到隔离区的文件先扫描，再决定是否落到远端。
//! ClamAV 走独立进程调用（clamscan），不链接 GPL。
//! 本机没装 Windows 版 clamscan 时，降级调 `wsl clamscan`（WSL 侧已装）。
//! 都没有则诚实记录"未扫描"，不假装扫了。

use std::path::Path;
use std::process::Command;

/// 四类处置结果（计划书 F-23）：放行 / 告警 / 阻断 / 待审。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    Allow,
    Alert,
    Deny,
    Pending,
}

impl Disposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Disposition::Allow => "allow",
            Disposition::Alert => "alert",
            Disposition::Deny => "deny",
            Disposition::Pending => "pending_review",
        }
    }
}

pub struct ScanOutcome {
    pub engine: String,
    pub infected: bool,
    pub detail: String,
    pub disposition: Disposition,
}

/// 高危脚本/混淆规则（特征库之外的第二道检查）。
const SUSPICIOUS_PATTERNS: &[&str] = &[
    "/bin/sh -c", "bash -c", "powershell -enc", "FromBase64String", "eval(base64",
    "nc -e", "ncat -e", "mkfifo", "chmod +x /tmp", "curl -k |", "wget -qO- |",
    "reverse shell", "/dev/tcp/", "base64 -d | sh",
];

/// 把 Windows 路径转成 WSL 路径（C:\... → /mnt/c/...）
fn to_wsl_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if let Some(rest) = s.strip_prefix("C:") {
        format!("/mnt/c{}", rest)
    } else if let Some(rest) = s.strip_prefix("E:") {
        format!("/mnt/e{}", rest)
    } else {
        s
    }
}

fn script_rule_hit(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    for pat in SUSPICIOUS_PATTERNS {
        if text.to_ascii_lowercase().contains(pat) {
            return Some((*pat).to_string());
        }
    }
    None
}

pub fn scan_file(path: &Path) -> ScanOutcome {
    let bytes = std::fs::read(path).unwrap_or_default();
    let rule_hit = script_rule_hit(&bytes);

    // 1. 先试 Windows 本机 clamscan
    if let Ok(out) = Command::new("clamscan").arg("--no-summary").arg(path).output() {
        let code = out.status.code().unwrap_or(0);
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let infected = code == 1;
        let disposition = if infected {
            Disposition::Deny
        } else if rule_hit.is_some() {
            Disposition::Pending
        } else {
            Disposition::Allow
        };
        return ScanOutcome {
            engine: "clamav-windows".into(),
            infected,
            detail: format!("{} 规则命中: {:?}", stdout.trim(), rule_hit),
            disposition,
        };
    }

    // 2. 再试 WSL clamscan
    let wsl_path = to_wsl_path(path);
    if let Ok(out) = Command::new("wsl")
        .args(["-d", "Ubuntu-22.04", "--exec", "clamscan", "--no-summary", &wsl_path])
        .output()
    {
        let code = out.status.code().unwrap_or(0);
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        let infected = code == 1;
        let disposition = if infected {
            Disposition::Deny
        } else if rule_hit.is_some() {
            Disposition::Pending
        } else {
            Disposition::Allow
        };
        return ScanOutcome {
            engine: "clamav-wsl".into(),
            infected,
            detail: if stdout.trim().is_empty() { stderr } else { stdout },
            disposition,
        };
    }

    // 3. 降级：无特征库，仅靠脚本规则
    let disposition = match &rule_hit {
        Some(_) => Disposition::Alert,
        None => Disposition::Allow,
    };
    ScanOutcome {
        engine: "none".into(),
        infected: false,
        detail: format!(
            "本机与 WSL 均无 clamscan；仅脚本规则检查，命中: {:?}",
            rule_hit
        ),
        disposition,
    }
}
