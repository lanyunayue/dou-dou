//! 连接错误的可诊断化。
//!
//! 原始错误串（尤其 Windows 的 `os error 10061`）对用户没有意义，
//! 这里统一映射成稳定错误码 + 一句人话，MCP 结果与启动日志共用同一套。

pub struct ErrorInfo {
    pub code: &'static str,
    pub human: String,
}

pub fn classify(raw: &str) -> ErrorInfo {
    let lower = raw.to_lowercase();

    if raw.contains("known_hosts 文件损坏") {
        return ErrorInfo {
            code: "E_KNOWN_HOSTS_CORRUPT",
            human: "known_hosts 损坏或不可读，已拒绝连接。修复或删除该文件后重新确认主机公钥。"
                .into(),
        };
    }
    if raw.contains("主机密钥不匹配") {
        return ErrorInfo {
            code: "E_HOSTKEY_MISMATCH",
            human: "主机密钥与本地记录不一致。可能是对方换了机器，也可能是中间人；已拒绝连接。"
                .into(),
        };
    }
    if lower.contains("10061") || lower.contains("refused") || lower.contains("积极拒绝") {
        return ErrorInfo {
            code: "E_CONN_REFUSED",
            human: "目标计算机拒绝连接：sshd 未启动，或端口不对。".into(),
        };
    }
    if lower.contains("timed out") || lower.contains("超时") || lower.contains("timeout") {
        return ErrorInfo {
            code: "E_CONN_TIMEOUT",
            human: "连接超时：网络不通、端口被拦，或目标在墙外。".into(),
        };
    }
    if lower.contains("authentication rejected") {
        return ErrorInfo {
            code: "E_AUTH_FAILED",
            human: "认证被拒绝：公钥未写入远端 authorized_keys，或口令错误。".into(),
        };
    }
    if lower.contains("load private key failed") {
        return ErrorInfo {
            code: "E_KEY_LOAD",
            human: "私钥读取失败：路径不存在、格式不支持，或私钥本身带口令。".into(),
        };
    }
    if lower.contains("unknown key") || lower.contains("unsupported") {
        return ErrorInfo {
            code: "E_UNSUPPORTED_ALGO",
            human: "算法不被支持：该后端（ring）不包含 RSA 私钥解析，请改用 ed25519。".into(),
        };
    }
    ErrorInfo {
        code: "E_SSH_UNKNOWN",
        human: format!("SSH 失败：{raw}"),
    }
}

/// 给 MCP 结果用的 JSON 片段
pub fn to_json(raw: &str) -> serde_json::Value {
    let info = classify(raw);
    serde_json::json!({
        "status": "error",
        "code": info.code,
        "message": info.human,
        "raw": raw,
    })
}
