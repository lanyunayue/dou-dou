//! 设备令牌：签发、校验、落盘；OS CSPRNG；运行时掩码。
//!
//! 本轮安全升级：
//! - `rand_hex` / 配对码由 xorshift64*（时间播种、可预测）改为 **OS CSPRNG**；
//!   随机源不可用时直接拒绝签发（fail-closed），不降级为弱随机。
//! - `check_token` 用 `subtle::ConstantTimeEq` 定长比较，长度不等时也走一次比较。
//! - `mask_token` 供启动日志与控制台使用；任何 HTTP 响应/日志不得出现完整主令牌。

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

pub fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("dou")
}

/// CSPRNG 十六进制串。OS 随机源不可用则拒绝签发可预测凭据。
pub fn rand_hex(len: usize) -> String {
    let mut bytes = vec![0u8; len.div_ceil(2)];
    getrandom::fill(&mut bytes).expect("CSPRNG 不可用：拒绝签发可预测凭据（fail-closed）");
    let s: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    s[..len].to_string()
}

pub fn load_or_create_token() -> String {
    if let Ok(t) = std::env::var("DOU_DEV_TOKEN") {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return t;
        }
    }
    let dir = data_dir();
    let path = dir.join("token.txt");
    if let Ok(s) = fs::read_to_string(&path) {
        let s = s.trim().to_string();
        if s.starts_with("dou_ak_") && s.len() > 16 {
            return s;
        }
    }
    let token = format!("dou_ak_{}", rand_hex(24));
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(&path, &token);
    token
}

/// 恒定时间比较。长度不同时也执行一次比较，避免完全提前返回。
pub fn check_token(presented: &str, expected: &str) -> bool {
    let a = presented.as_bytes();
    let b = expected.as_bytes();
    if a.len() != b.len() {
        let _ = b.ct_eq(b);
        return false;
    }
    bool::from(a.ct_eq(b))
}

/// 令牌解析已统一上移至 `authz::require_auth`（W-15，全仓唯一解析点）。
/// 令牌掩码：日志与控制台只显示后 4 位，永不回吐完整凭据。
pub fn mask_token(t: &str) -> String {
    let chars: Vec<char> = t.chars().collect();
    let tail: String = chars.iter().skip(chars.len().saturating_sub(4)).collect();
    format!("dou_ak_****{tail}")
}

/// 三档凭据：配对码（60s）。
static PAIRING: std::sync::Mutex<Option<(String, u64)>> = std::sync::Mutex::new(None);

pub fn issue_pairing_code() -> String {
    let mut b = [0u8; 4];
    getrandom::fill(&mut b).expect("CSPRNG 不可用：拒绝签发可预测配对码（fail-closed）");
    let code = format!("{:06}", u32::from_le_bytes(b) % 1_000_000);
    let exp = now_sec() + 60;
    *PAIRING.lock().unwrap_or_else(|e| e.into_inner()) = Some((code.clone(), exp));
    code
}

pub fn check_pairing(code: &str) -> bool {
    let g = PAIRING.lock().unwrap_or_else(|e| e.into_inner());
    match g.as_ref() {
        Some((c, exp)) => c == code && now_sec() <= *exp,
        None => false,
    }
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 会话租约：token 有效期 30min。
pub const LEASE_SECS: u64 = 1800;

pub fn lease_expiry() -> u64 {
    now_sec() + LEASE_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_never_leaks_full_token() {
        // 合成样本刻意不含 24 位连续 hex，避免命中 CI 凭据扫描正则的误报。
        let t = "dou_ak_SAMPLE_TOKEN_0001";
        let m = mask_token(t);
        assert!(!m.contains(t));
        assert!(m.ends_with("0001"));
        assert!(m.starts_with("dou_ak_****"));
    }

    #[test]
    fn constant_time_check_is_correct() {
        assert!(check_token("abc", "abc"));
        assert!(!check_token("abd", "abc"));
        assert!(!check_token("ab", "abc"));
        assert!(!check_token("", "abc"));
    }

    #[test]
    fn csprng_never_collides_in_a_batch() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            let s = rand_hex(24);
            assert_eq!(s.len(), 24);
            assert!(seen.insert(s), "CSPRNG 同批次出现重复值");
        }
    }

    #[test]
    fn mask_token_short_and_empty_does_not_panic() {
        assert_eq!(mask_token(""), "dou_ak_****");
        assert_eq!(mask_token("ab"), "dou_ak_****ab");
        assert_eq!(mask_token("xyz"), "dou_ak_****xyz");
    }
}
