//! 设备令牌：签发、校验、落盘。
//!
//! 尖刀版规则（对应计划书 4.8 的二级令牌）：
//! - 前缀 dou_ak_，明文只在启动时打印一次
//! - 只接受 Authorization: Bearer 形式
//! - 令牌文件与审计库同目录，后续接入设备绑定与吊销时间戳

use axum::http::HeaderMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("dou")
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

pub fn check(headers: &HeaderMap, expected: &str) -> bool {
    let raw = match headers.get("authorization").and_then(|v| v.to_str().ok()) {
        Some(v) => v,
        None => return false,
    };
    let rest = match raw.strip_prefix("Bearer ") {
        Some(r) => r.trim(),
        None => return false,
    };
    // 定长比较，避免提前返回造成的时序差异
    constant_eq(rest.as_bytes(), expected.as_bytes())
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

fn rand_hex(len: usize) -> String {
    let mut seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9)
        ^ (std::process::id() as u64).wrapping_mul(0x2545_F491_4F6C_DD1D);
    let mut out = String::with_capacity(len);
    while out.len() < len {
        // xorshift64*
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        let v = seed.wrapping_mul(0x2545_F491_4F6C_DD1D);
        for shift in [0u32, 8, 16, 24] {
            if out.len() >= len {
                break;
            }
            out.push(char::from_digit(((v >> shift) & 0x0F) as u32, 16).unwrap_or('0'));
        }
    }
    out
}

/// 三档凭据：配对码（60s）
static PAIRING: std::sync::Mutex<Option<(String, u64)>> = std::sync::Mutex::new(None);

/// 生成 6 位配对码，60s 过期
pub fn issue_pairing_code() -> String {
    let code = format!("{:06}", (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() % 1_000_000)
        .unwrap_or(123456)) % 1_000_000);
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) + 60;
    *PAIRING.lock().unwrap() = Some((code.clone(), exp));
    code
}

/// 校验配对码
pub fn check_pairing(code: &str) -> bool {
    let g = PAIRING.lock().unwrap();
    match g.as_ref() {
        Some((c, exp)) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            c == code && now <= *exp
        }
        None => false,
    }
}

/// 会话租约：token 有效期 30min
pub const LEASE_SECS: u64 = 1800;

pub fn lease_expiry() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() + LEASE_SECS)
        .unwrap_or(0)
}
