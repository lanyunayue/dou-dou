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
            human: "主机密钥与本地记录不一致。可能是对方换了机器，也可能是中间人；已拒绝连接。\
                    请先通过其他渠道人工核实对方身份：确认合法换钥后，在客户端「主机密钥」页\
                    对网关信任库执行遗忘并重连（重新出现首次记录提示时肉眼确认指纹）；\
                    无法核实则保持拒绝。"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_hosts_corrupt() {
        assert_eq!(
            classify("known_hosts 文件损坏").code,
            "E_KNOWN_HOSTS_CORRUPT"
        );
    }

    #[test]
    fn classifies_hostkey_mismatch() {
        assert_eq!(classify("主机密钥不匹配").code, "E_HOSTKEY_MISMATCH");
    }

    #[test]
    fn classifies_connection_refused_variants() {
        assert_eq!(classify("os error 10061").code, "E_CONN_REFUSED");
        assert_eq!(classify("Connection refused").code, "E_CONN_REFUSED");
        assert_eq!(classify("积极拒绝").code, "E_CONN_REFUSED");
    }

    #[test]
    fn classifies_timeout_variants() {
        assert_eq!(classify("Connection timed out").code, "E_CONN_TIMEOUT");
        assert_eq!(classify("连接超时").code, "E_CONN_TIMEOUT");
        assert_eq!(classify("operation timeout").code, "E_CONN_TIMEOUT");
    }

    #[test]
    fn classifies_auth_and_key_and_algo() {
        assert_eq!(classify("authentication rejected").code, "E_AUTH_FAILED");
        assert_eq!(classify("load private key failed").code, "E_KEY_LOAD");
        assert_eq!(classify("unknown key type").code, "E_UNSUPPORTED_ALGO");
        assert_eq!(classify("unsupported algorithm").code, "E_UNSUPPORTED_ALGO");
    }

    #[test]
    fn unknown_falls_back_and_keeps_raw() {
        let info = classify("some weird ssh failure");
        assert_eq!(info.code, "E_SSH_UNKNOWN");
        assert!(info.human.contains("some weird ssh failure"));
    }

    #[test]
    fn to_json_carries_status_code_message_raw() {
        let v = to_json("os error 10061");
        assert_eq!(v["status"], "error");
        assert_eq!(v["code"], "E_CONN_REFUSED");
        assert_eq!(v["raw"], "os error 10061");
        assert!(v["message"].as_str().unwrap().contains("拒绝连接"));
    }
}
