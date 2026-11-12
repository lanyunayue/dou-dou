//! 统一鉴权（W-15）：axum 单层中间件收口全部 14 条路由。
//!
//! 设计约束（对应 `docs/handover/05` §2 / §7）：
//! - **唯一 Bearer 解析点**在本文件；`handle_mcp` 不再自行解析令牌，
//!   改为从 `Extension<AuthContext>` 取认证结果。
//! - `EXEMPT_PATHS` 集中常量：`/health` 豁免，且仅返回 `{"status":"ok"}`。
//!   Q4 桥接形态若改判，只需改这一处常量 + `health()` 返回体。
//! - `M_ONLY_PATHS` 集中常量：可批准危险命令 / 签发凭据 / 代写配置 /
//!   导出审计 / 查看控制台等高敏端点，仅主令牌可用。
//! - 认证失败经 `throttle::record_failure` 计数，达到阈值后临时锁定；
//!   成功立即清零。状态库不可用按 fail-closed 返回 503。
//! - 所有拒绝都写审计（`auth` / `auth_throttled`），不可绕过。

use crate::{audit, auth, scope, throttle, AppState};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use serde_json::json;

/// 豁免清单。保持 `len() <= 1`；改动此常量即可随 Q4 拍板切换。
pub const EXEMPT_PATHS: &[&str] = &["/health"];

/// 仅主令牌可用的高敏端点。
pub const M_ONLY_PATHS: &[&str] = &[
    "/approve",
    "/audit/export",
    "/onboard/write",
    "/onboard/config",
    "/console",
    "/scope/tokens",
    "/scope/revoke",
];

#[derive(Debug, Clone)]
pub struct AuthContext {
    pub subject_key: String,
    pub is_master: bool,
    pub cred: Option<scope::Credential>,
}

fn json_status(status: StatusCode, code: &str, msg: &str) -> Response {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json; charset=utf-8")
        .body(
            serde_json::to_string(&json!({ "error": code, "message": msg }))
                .unwrap_or_default()
                .into(),
        )
        .unwrap()
}

/// `/mcp` 的 401 保持 JSON-RPC 错误体，兼容 MCP 客户端。
fn mcp_unauthorized() -> Response {
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header("Content-Type", "application/json; charset=utf-8")
        .body(
            r#"{"jsonrpc":"2.0","error":{"code":-32001,"message":"invalid or missing bearer token"},"id":null}"#
                .into(),
        )
        .unwrap()
}

pub async fn require_auth(State(st): State<AppState>, mut req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();

    // 探活：唯一豁免，不计数、不写审计、不回传任何敏感信息。
    if EXEMPT_PATHS.contains(&path.as_str()) {
        return next.run(req).await;
    }

    let presented = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let is_master = !presented.is_empty() && auth::check_token(&presented, &st.token);
    let kind = if is_master {
        throttle::SubjectKind::Master
    } else if presented.starts_with("dou_ak_") {
        throttle::SubjectKind::Device
    } else {
        throttle::SubjectKind::Anonymous
    };
    // 主体键按出示值自指哈希：错令牌无法指定他人 cred_id。
    let key = throttle::subject_key(kind, &presented);
    let cfg = throttle::Config::from_env();

    match throttle::check(&key, &cfg) {
        throttle::Verdict::Locked { retry_after_ms } => {
            audit::write(
                &st.audit,
                audit::event(
                    "agent",
                    "auth_throttled",
                    &path,
                    "denied",
                    json!({ "subject": key, "retry_after_ms": retry_after_ms }),
                ),
            );
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("Retry-After", ((retry_after_ms + 999) / 1000).to_string())
                .header("Content-Type", "application/json; charset=utf-8")
                .body(
                    serde_json::to_string(
                        &json!({ "error": "rate_limited", "retry_after_ms": retry_after_ms }),
                    )
                    .unwrap_or_default()
                    .into(),
                )
                .unwrap();
        }
        throttle::Verdict::Unavailable => {
            return json_status(
                StatusCode::SERVICE_UNAVAILABLE,
                "state_unavailable",
                "状态库不可用，拒绝请求（fail-closed）",
            );
        }
        throttle::Verdict::Allow => {}
    }

    let cred = if is_master {
        None
    } else {
        scope::verify_token(&presented)
    };

    if !is_master && cred.is_none() {
        let just_locked = throttle::record_failure(&key, &cfg).ok().flatten();
        audit::write(
            &st.audit,
            audit::event(
                "agent",
                "auth",
                &path,
                "denied",
                json!({
                    "reason": "bad token",
                    "subject": key,
                    "throttle_triggered": just_locked.is_some(),
                }),
            ),
        );
        return if path == "/mcp" {
            mcp_unauthorized()
        } else {
            json_status(
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "invalid or missing bearer token",
            )
        };
    }

    throttle::record_success(&key);

    if M_ONLY_PATHS.contains(&path.as_str()) && !is_master {
        audit::write(
            &st.audit,
            audit::event(
                "agent",
                "auth",
                &path,
                "denied",
                json!({ "reason": "master-only endpoint", "subject": key }),
            ),
        );
        return json_status(
            StatusCode::FORBIDDEN,
            "forbidden",
            "该端点仅主令牌可用（M-only）",
        );
    }

    req.extensions_mut().insert(AuthContext {
        subject_key: key,
        is_master,
        cred,
    });
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exemption_list_is_minimal() {
        assert!(EXEMPT_PATHS.len() <= 1, "豁免路径必须收敛到 <= 1 处");
        assert_eq!(EXEMPT_PATHS, &["/health"]);
    }

    #[test]
    fn all_sensitive_endpoints_are_m_only() {
        for p in [
            "/approve",
            "/audit/export",
            "/onboard/write",
            "/onboard/config",
            "/console",
            "/scope/tokens",
            "/scope/revoke",
        ] {
            assert!(M_ONLY_PATHS.contains(&p), "{p} 必须 M-only");
        }
    }

    #[test]
    fn mcp_and_health_are_not_m_only() {
        assert!(!M_ONLY_PATHS.contains(&"/mcp"));
        assert!(!M_ONLY_PATHS.contains(&"/health"));
    }
}
