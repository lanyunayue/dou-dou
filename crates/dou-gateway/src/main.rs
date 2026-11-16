//! dou-gateway：本地回环 MCP 网关。
//!
//! 只做一件事：证明"代理以受限方式操作服务器"这条链路在协议层与传输层成立。
//! 不包含终端渲染、SFTP 界面、编辑器 —— 那些是终端形态的工作，不是差异化。

mod audit;
mod auth;
mod authz;
mod errors;
mod hostkey;
mod policy;
mod rpc;
mod sanitize;
mod scan;
mod scope;
mod snapshot;
mod sftp;
mod ssh;
mod state;
mod store;
mod tasks;
mod throttle;
mod tools;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::{Html, Response},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

const DEFAULT_BIND: &str = "127.0.0.1:7419";
const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Clone)]
pub struct AppState {
    pub token: Arc<String>,
    pub audit: Arc<PathBuf>,
    pub dev: bool,
    pub sessions: Arc<tokio::sync::Mutex<HashMap<String, ssh::SshConnection>>>,
    pub taskhub: Arc<tasks::TaskHub>,
    pub sanitizer: Arc<sanitize::Sanitizer>,
    /// 配额计数：cred_id → (累计调用次数, 当前活跃并发数)。
    pub quota: Arc<tokio::sync::Mutex<HashMap<String, (u64, u64)>>>,
    /// 会话租约：session_id → 起始秒；单次任务最长 LEASE_SECS。
    pub leases: Arc<tokio::sync::Mutex<HashMap<String, u64>>>,
}

#[tokio::main]
async fn main() {
    let token = auth::load_or_create_token();
    let audit_path = audit::default_path();
    audit::init_chain(&audit_path);
    audit::init_sqlite();
    if let Err(e) = store::init_schema() {
        eprintln!("[store] schema 初始化失败: {e}");
    } else if let Err(e) = store::seed_builtin_rules() {
        eprintln!("[store] 内置规则写入失败: {e}");
    }
    if let Err(e) = state::init_state_schema() {
        eprintln!("[state] 状态表初始化失败: {e}");
    }
    // 默认关闭 dev：不显式设置 DOU_DEV_MODE=1 时，禁止本机命令兜底执行。
    let dev = std::env::var("DOU_DEV_MODE").map(|v| v == "1").unwrap_or(false);
    let bind = std::env::var("DOU_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_string());

    if !is_loopback(&bind) {
        eprintln!("[fatal] DOU_BIND 必须是回环地址，拒绝监听 {bind}");
        std::process::exit(2);
    }

    let sessions: Arc<tokio::sync::Mutex<HashMap<String, ssh::SshConnection>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    println!("[dou-gateway]  │ token  {}", auth::mask_token(&token));
    println!("[dou-gateway]  │ audit  {}", audit_path.display());
    println!("[dou-gateway]  │ dev    {dev}");

    // 尖刀阶段的会话来源：环境变量 DOU_SSH=user@host:port + DOU_SSH_PASS / DOU_SSH_KEY
    // 正式版由界面「连接管理」页写入，不走环境变量。
    let known_path = data_dir().join("known_hosts.json");
    let known = Arc::new(std::sync::Mutex::new(hostkey::KnownHosts::load(
        known_path.clone(),
    )));
    println!("[dou-gateway]  │ known  {}", known_path.display());

    if let Ok(spec) = std::env::var("DOU_SSH") {
        let password = std::env::var("DOU_SSH_PASS").unwrap_or_default();
        match parse_spec(&spec) {
            Some((user, host, port)) => {
                print!("[dou-gateway]  ├ ssh    {spec} … ");
                let key_path = std::env::var("DOU_SSH_KEY").ok();
                let attempt = match key_path {
                    Some(k) => ssh::connect_key(&host, port, &user, &k, known.clone()).await,
                    None => {
                        ssh::connect_password(&host, port, &user, &password, known.clone()).await
                    }
                };
                match attempt {
                    Ok(conn) => {
                        let short = hostkey::short_id(&conn.host_key);
                        let first_seen = conn.host_key_first_seen;
                        println!(
                            "ok ({}@{}:{} via {} · 主机公钥 {} · {})",
                            conn.user,
                            conn.host,
                            conn.port,
                            conn.auth_method,
                            short,
                            if first_seen { "首次记录" } else { "与记录一致" }
                        );
                        let method = conn.auth_method;
                        sessions.lock().await.insert("default".to_string(), conn);
                        audit::write(
                            &audit_path,
                            audit::event(
                                "human",
                                "connect",
                                &spec,
                                "allowed",
                                json!({
                                    "via": "DOU_SSH env",
                                    "auth": method,
                                    "host_key": short,
                                    "first_seen": first_seen
                                }),
                            ),
                        );
                    }
                    Err(e) => {
                        let info = errors::classify(&e);
                        println!("FAILED [{}] {}", info.code, info.human);
                        audit::write(
                            &audit_path,
                            audit::event(
                                "human",
                                "connect",
                                &spec,
                                "denied",
                                json!({ "code": info.code, "detail": e }),
                            ),
                        );
                    }
                }
            }
            None => println!("[dou-gateway]  ├ ssh    DOU_SSH 格式错误，应为 user@host:port"),
        }
    }

    println!("[dou-gateway]  └ bind   http://{bind}/mcp");

    let state = AppState {
        token: Arc::new(token),
        audit: Arc::new(audit_path),
        dev,
        sessions,
        taskhub: Arc::new(tasks::TaskHub::new()),
        sanitizer: Arc::new(sanitize::Sanitizer::new()),
        quota: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        leases: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/mcp", post(handle_mcp))
        .route("/health", axum::routing::get(health))
        .route("/audit", get(audit_tail))
        .route("/audit/query", get(audit_query))
        .route("/audit/verify", get(audit_verify))
        .route("/audit/export", get(audit_export))
        .route("/tasks", get(tasks_list))
        .route("/approve", axum::routing::post(approve_task))
        .route("/task/cancel", axum::routing::post(cancel_task))
        .route("/session/lock", axum::routing::post(session_lock))
        .route("/session/unlock", axum::routing::post(session_unlock))
        .route("/scope/tokens", get(scope_tokens).post(issue_token))
        .route("/scope/revoke", axum::routing::post(revoke_token))
        .route("/onboard/config", get(onboard_config))
        .route("/onboard/write", axum::routing::post(onboard_write))
        .route("/console", get(console_page))
        .with_state(state.clone())
        // W-15：单层中间件统一收口全部路由；唯一 Bearer 解析点在 authz.rs。
        .layer(axum::middleware::from_fn_with_state(state, authz::require_auth));

    let listener = match tokio::net::TcpListener::bind(&bind).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[fatal] 端口绑定失败 {bind}: {e}（可能已有实例在跑）");
            std::process::exit(3);
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("[fatal] 服务退出: {e}");
        std::process::exit(4);
    }
}

/// Q4 拍板前沿用架构建议：豁免鉴权 + 最小信息 + 不写审计。
/// 改判只需改 `authz::EXEMPT_PATHS` 一处常量。
async fn health() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json; charset=utf-8")
        .body(r#"{"status":"ok"}"#.into())
        .unwrap()
}

async fn audit_tail(State(st): State<AppState>) -> Response {
    let items = audit::tail(&st.audit, 50);
    let body = serde_json::to_string_pretty(&json!({ "count": items.len(), "items": items }))
        .unwrap_or_default();
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json; charset=utf-8")
        .body(body.into())
        .unwrap()
}

async fn audit_query(State(st): State<AppState>, uri: axum::http::Uri) -> Response {
    let params: std::collections::HashMap<String, String> = uri
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let since = params.get("since_ms").and_then(|s| s.parse::<u64>().ok());
    let source = params.get("source").map(|s| s.as_str());
    let limit = params.get("limit").and_then(|s| s.parse::<usize>().ok()).unwrap_or(200);
    let items = audit::query(&st.audit, since, source, limit);
    let body = serde_json::to_string_pretty(&json!({ "count": items.len(), "items": items }))
        .unwrap_or_default();
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json; charset=utf-8")
        .body(body.into())
        .unwrap()
}

async fn audit_verify(State(st): State<AppState>) -> Response {
    let entries = audit::entry_count(&st.audit);
    let (ok, broken) = audit::verify_chain(&st.audit);
    // 空链时 chain_ok=true 不代表已验证；必须同时给出 entries / verifiable。
    let body = serde_json::to_string(&json!({
        "chain_ok": ok, "broken_index": broken,
        "entries": entries, "verifiable": entries > 0
    }))
        .unwrap_or_default();
    Response::builder()
        .status(if ok { StatusCode::OK } else { StatusCode::CONFLICT })
        .header("Content-Type", "application/json; charset=utf-8")
        .body(body.into())
        .unwrap()
}

async fn console_page(State(st): State<AppState>) -> Html<String> {
    let sessions = st.sessions.lock().await;
    let s = sessions
        .iter()
        .map(|(k, v)| format!("{k} → {}@{}:{}", v.user, v.host, v.port))
        .collect::<Vec<_>>()
        .join("<br>");
    let items = audit::tail(&st.audit, 20);
    let rows = items
        .iter()
        .map(|v| {
            let ts = v.get("ts_ms").and_then(|x| x.as_str()).unwrap_or("");
            let src = v.get("source").and_then(|x| x.as_str()).unwrap_or("");
            let act = v.get("action").and_then(|x| x.as_str()).unwrap_or("");
            let vd = v.get("verdict").and_then(|x| x.as_str()).unwrap_or("");
            let tgt = v.get("target").and_then(|x| x.as_str()).unwrap_or("");
            format!("<tr><td>{ts}</td><td>{src}</td><td>{act}</td><td>{vd}</td><td>{tgt}</td></tr>")
        })
        .collect::<Vec<_>>()
        .join("");
    let pending = st.taskhub.list_pending().await;
    let pending_html = if pending.is_empty() {
        "<p style='color:#3d6b4f;font-size:13px;'>没有待审任务</p>".to_string()
    } else {
        pending.iter().map(|t| {
            format!(
                "<div style='border:1px solid #dcdcd5;border-radius:6px;padding:10px;margin:8px 0;'>\
                 <div style='font-family:monospace;font-size:13px;'>{}</div>\
                 <div style='color:#6b6b64;font-size:12px;margin:4px 0;'>原因：{}</div>\
                 <div><button onclick=\"decide('{}',true)\" style='background:#3d6b4f;color:#fff;border:none;padding:6px 14px;border-radius:4px;cursor:pointer;margin-right:8px;'>批准执行</button>\
                 <button onclick=\"decide('{}',false)\" style='background:#a34a4a;color:#fff;border:none;padding:6px 14px;border-radius:4px;cursor:pointer;'>拒绝</button></div>\
                 </div>",
                t.command, t.reason, t.id, t.id
            )
        }).collect::<Vec<_>>().join("")
    };
    let html = format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>豆 Dou 控制台</title>
<style>body{{font-family:-apple-system,'Segoe UI',sans-serif;background:#f5f5f2;color:#1b1b19;margin:0;padding:24px;}}
h1{{font-size:20px;}} h2{{font-size:15px;margin-top:24px;}}
.card{{background:#fff;border:1px solid #dcdcd5;border-radius:8px;padding:16px;margin-bottom:16px;}}
table{{border-collapse:collapse;width:100%;font-size:12.5px;}} td,th{{border-bottom:1px solid #e5e5e0;padding:6px 8px;text-align:left;}}
code{{background:#f0efe9;padding:2px 6px;border-radius:3px;}} .ok{{color:#3d6b4f;}} .bad{{color:#a34a4a;}}
</style></head><body>
<h1>豆 Dou · 本地控制台</h1>
<div class="card"><b>连接</b><br>{s}</div>
<div class="card"><b>待审任务（{n_pending}）</b>{pending_html}</div>
<div class="card"><b>接入向导</b><br>
<code>MCP: http://127.0.0.1:7419/mcp</code><br>
<code>令牌请在本地 token.txt 查看（控制台不展示完整凭据）</code><br>
<span class="ok">代理不接触私钥；所有动作经策略判定并写审计。</span></div>
<div class="card"><b>最近审计（20 条）</b>
<table><tr><th>ts</th><th>来源</th><th>动作</th><th>判定</th><th>目标</th></tr>{rows}</table>
<br><a href="/audit/verify" target="_blank">校验审计链</a> · <a href="/audit/export" target="_blank">导出 CSV</a> · <a href="/tasks" target="_blank">任务列表</a>
</div>
<script>
async function decide(id, approve) {{
  await fetch('/approve', {{method:'POST',headers:{{'Content-Type':'application/json'}},body:JSON.stringify({{task_id:id,approve:approve}})}});
  location.reload();
}}
</script>
</body></html>"#,
        s = s, rows = rows, pending_html = pending_html, n_pending = pending.len()
    );
    Html(html)
}

async fn audit_export(State(st): State<AppState>) -> Response {
    // CSV 导出：ts, source, action, target, verdict, trace_id
    let mut csv = String::from("ts_ms,source,action,target,verdict,trace_id\n");
    let items = audit::tail(&st.audit, 10_000);
    for v in &items {
        let f = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").replace(',', "，");
        csv.push_str(&format!("{},{},{},{},{},{}\n", f("ts_ms"), f("source"), f("action"), f("target"), f("verdict"), f("trace_id")));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/csv; charset=utf-8")
        .header("Content-Disposition", "attachment; filename=dou_audit.csv")
        .body(csv.into())
        .unwrap()
}

async fn tasks_list(State(st): State<AppState>) -> Response {
    // A3 审批超时：读取任务列表时惰性触发 5min 超时拒绝。
    let expired = st.taskhub.expire_stale_pending(5 * 60 * 1000).await;
    if expired > 0 {
        audit::write(&st.audit, audit::event(
            "system", "approval_timeout", "-", "denied",
            json!({ "expired_count": expired, "timeout_min": 5 }),
        ));
    }
    let tasks = st.taskhub.list_all().await;
    let items: Vec<Value> = tasks.iter().map(tasks::task_to_json).collect();
    let body = serde_json::to_string(&json!({"count": items.len(), "items": items})).unwrap_or_default();
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json; charset=utf-8")
        .body(body.into())
        .unwrap()
}

/// A2 终止任务。body: { "task_id": "task_xxx" }。
async fn cancel_task(State(st): State<AppState>, body: String) -> Response {
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let id = v.get("task_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
    if id.is_empty() {
        return json_err(StatusCode::BAD_REQUEST, "task_id 必填");
    }
    match st.taskhub.cancel(&id).await {
        Ok(Some(t)) => {
            audit::write(&st.audit, audit::event(
                "human", "task_cancel", &id, "allowed",
                json!({ "command": t.command, "prev_status": "running_or_pending" }),
            ));
            json_ok(&json!({ "ok": true, "task_id": id, "status": "cancelled" }))
        }
        Ok(None) => Response::builder().status(StatusCode::NOT_FOUND).body("{}".into()).unwrap(),
        Err(e) => json_err(StatusCode::BAD_REQUEST, &e),
    }
}

// ---------- A1 会话锁：人工占用 / 释放 ----------

/// 人工占用会话（谁先占用谁持有，人工优先）。
/// body: { "session": "default" }。占用后代理在该会话的调用将被拒绝。
async fn session_lock(State(st): State<AppState>, body: String) -> Response {
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let session = v.get("session").and_then(|x| x.as_str()).unwrap_or("default").to_string();
    // 人工占用强制成功（force=true），覆盖任何既有占用。
    let ok = state::session_try_lock(&session, "human", true);
    audit::write(&st.audit, audit::event(
        "human", "session_lock", &session, if ok { "allowed" } else { "failed" },
        json!({}),
    ));
    if ok {
        json_ok(&json!({ "ok": true, "session": session, "owner": "human" }))
    } else {
        json_err(StatusCode::INTERNAL_SERVER_ERROR, "占用会话失败（状态库不可用）")
    }
}

/// 释放会话锁。body: { "session": "default" }。
async fn session_unlock(State(st): State<AppState>, body: String) -> Response {
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let session = v.get("session").and_then(|x| x.as_str()).unwrap_or("default").to_string();
    let ok = state::session_unlock(&session, "human", true);
    audit::write(&st.audit, audit::event(
        "human", "session_unlock", &session, if ok { "allowed" } else { "failed" },
        json!({}),
    ));
    if ok {
        json_ok(&json!({ "ok": true, "session": session, "released": true }))
    } else {
        json_err(StatusCode::INTERNAL_SERVER_ERROR, "释放会话失败（状态库不可用）")
    }
}

// ---------- 凭据与作用域 ----------

async fn scope_tokens() -> Response {
    match scope::list_active() {
        Ok(items) => json_ok(&json!({ "count": items.len(), "items": items })),
        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

async fn issue_token(State(st): State<AppState>, body: String) -> Response {
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let tier = v.get("tier").and_then(|x| x.as_str()).unwrap_or("observe").to_string();
    let client_id = v.get("client_id").and_then(|x| x.as_str()).unwrap_or("unknown").to_string();
    let scope = scope::default_scope(&tier, &client_id);
    match scope::issue_token(&tier, scope) {
        Ok((plain, rec)) => {
            audit::write(&st.audit, audit::event("human", "issue_token", &rec.id, "allowed",
                json!({ "tier": tier, "client_id": client_id, "expires_at": rec.expires_at })));
            // 明文仅本次返回一次，库里只存哈希。
            json_ok(&json!({ "token": plain, "id": rec.id, "tier": tier, "scope": rec.scope, "expires_at": rec.expires_at }))
        }
        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

async fn revoke_token(State(st): State<AppState>, body: String) -> Response {
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
    match scope::revoke_token(&id) {
        Ok(_) => {
            audit::write(&st.audit, audit::event("human", "revoke_token", &id, "allowed", json!({})));
            json_ok(&json!({ "ok": true }))
        }
        Err(e) => json_err(StatusCode::BAD_REQUEST, &e),
    }
}

// ---------- 接入向导：配置片段生成 ----------

async fn onboard_config(State(st): State<AppState>, uri: axum::http::Uri) -> Response {
    let params: std::collections::HashMap<String, String> = uri.query().unwrap_or("")
        .split('&').filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string())).collect();
    let client = params.get("client").map(|s| s.as_str()).unwrap_or("workbuddy");
    let endpoint = format!("http://127.0.0.1:7419/mcp");
    // 安全：HTTP 响应绝不回吐完整主令牌。片段用占位符；
    // 真实令牌由 /onboard/write 在服务端本地代写，或用户从 token.txt 手动替换。
    let auth = "Bearer ${DOU_TOKEN}".to_string();
    let (format, top_key, snippet) = match client {
        "codex" => (
            "toml",
            "mcp_servers",
            format!("[mcp_servers.dou]\nurl = \"{endpoint}\"\nheaders.Authorization = \"{auth}\"\n# 单工具默认 60s 超时，长命令请调大或走任务化\n"),
        ),
        "vscode" => {
            let body = json!({ "servers": { "dou": { "url": endpoint, "headers": { "Authorization": auth } } } });
            ("json", "servers", body.to_string())
        }
        _ => {
            let body = json!({ "mcpServers": { "dou": { "url": endpoint, "headers": { "Authorization": auth } } } });
            ("json", "mcpServers", body.to_string())
        }
    };
    let paths: Value = json!({
        "workbuddy": ["~/.workbuddy/mcp.json", "~/.codebuddy/.mcp.json"],
        "codex": ["~/.codex/config.toml", ".codex/config.toml"],
        "claude_code": [".mcp.json", "~/.claude.json"],
        "claude_desktop": ["claude_desktop_config.json"],
        "vscode": [".vscode/mcp.json"],
        "cursor": [".cursor/mcp.json"],
    });
    json_ok(&json!({
        "client": client, "format": format, "top_key": top_key,
        "config_paths": paths.get(client).cloned().unwrap_or(Value::Null),
        "endpoint": endpoint, "snippet": snippet,
        "note": "纯 JSON 不支持注释；首次连接需在客户端界面点信任；地址须以 /mcp 结尾。"
    }))
}

fn json_ok(v: &Value) -> Response {
    Response::builder().status(StatusCode::OK)
        .header("Content-Type", "application/json; charset=utf-8")
        .body(serde_json::to_string(v).unwrap_or_default().into()).unwrap()
}

fn json_err(code: StatusCode, msg: &str) -> Response {
    Response::builder().status(code)
        .header("Content-Type", "application/json; charset=utf-8")
        .body(serde_json::to_string(&json!({ "error": msg })).unwrap_or_default().into()).unwrap()
}

fn expand_home(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
        return format!("{}/{}", home, rest);
    }
    p.to_string()
}

/// 代写配置：备份 → 合并条目 → 解析校验 → 失败回滚（计划书 4.7 三条硬约束）。
async fn onboard_write(State(st): State<AppState>, body: String) -> Response {
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let client = v.get("client").and_then(|x| x.as_str()).unwrap_or("workbuddy").to_string();

    // TOML（Codex）不做结构化代写，降级为复制片段，避免写坏配置。
    if client == "codex" {
        return json_err(StatusCode::BAD_REQUEST, "Codex 为 TOML 配置，本端不代写，请用复制片段粘贴");
    }

    let candidates: Vec<&str> = match client.as_str() {
        "claude_code" => vec![".mcp.json", "~/.claude.json"],
        "claude_desktop" => vec!["claude_desktop_config.json"],
        "vscode" => vec![".vscode/mcp.json"],
        "cursor" => vec![".cursor/mcp.json"],
        _ => vec!["~/.workbuddy/mcp.json", "~/.codebuddy/.mcp.json"],
    };
    let top_key = if client == "vscode" { "servers" } else { "mcpServers" };
    let endpoint = "http://127.0.0.1:7419/mcp".to_string();
    let dou_entry = json!({ "url": endpoint, "headers": { "Authorization": format!("Bearer {}", st.token) } });

    for raw in candidates {
        let path = expand_home(raw);
        let parent = std::path::Path::new(&path).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        // 父目录不存在则跳过下一个候选。
        if !std::path::Path::new(&parent).exists() { continue; }

        let orig = std::fs::read_to_string(&path).unwrap_or_default();
        let backup: Option<String> = if !orig.trim().is_empty() {
            let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let bk = format!("{path}.bak.{ts}");
            let _ = std::fs::write(&bk, &orig);
            Some(bk)
        } else { None };

        let mut existing: Value = if orig.trim().is_empty() {
            json!({})
        } else {
            match serde_json::from_str::<Value>(&orig) { Ok(x) => x, Err(_) => {
                // 原文件不是合法 JSON：不覆盖，回报。
                return json_err(StatusCode::BAD_REQUEST, "现有配置不是合法 JSON，已跳过；请先手动备份");
            }}
        };

        let existed = existing.get(top_key).and_then(|t| t.get("dou")).is_some();
        if !existing.is_object() { existing = json!({}); }
        let top = existing.as_object_mut().unwrap();
        let top_obj = top.entry(top_key.to_string()).or_insert(json!({}));
        top_obj["dou"] = dou_entry.clone();

        let out = match serde_json::to_string_pretty(&existing) { Ok(x) => x, Err(e) => {
            return json_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("序列化失败: {e}"));
        }};
        if let Err(e) = std::fs::write(&path, &out) {
            return json_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("写入失败: {e}"));
        }

        // 写后解析校验；失败自动回滚。
        let verify = std::fs::read_to_string(&path).and_then(|s| serde_json::from_str::<Value>(&s).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad json")));
        if verify.is_err() {
            if let Some(bk) = &backup { let _ = std::fs::copy(bk, &path); }
            return json_err(StatusCode::BAD_REQUEST, "写入后校验失败，已自动回滚");
        }

        audit::write(&st.audit, audit::event("human", "onboard_write", &client, "allowed",
            json!({ "path": path, "backed_up": backup, "overwrote_existed": existed })));
        return json_ok(&json!({
            "written": true, "path": path, "backed_up": backup,
            "overwrote_existed": existed, "rolled_back": false,
            "message": if existed { "已更新同名条目（原文件已备份）" } else { "已追加 dou 条目" }
        }));
    }
    json_err(StatusCode::BAD_REQUEST, "没有可写的配置目录，请用复制片段方式接入")
}

async fn approve_task(State(st): State<AppState>, body: String) -> Response {
    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let id: String = v.get("task_id").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let approve = v.get("approve").and_then(|x| x.as_bool()).unwrap_or(false);
    let note: String = v.get("note").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let remember = v.get("remember").and_then(|x| x.as_bool()).unwrap_or(false);
    // 拒绝必须填理由，理由进审计（计划书 4.9）。
    if !approve && note.trim().is_empty() {
        return json_err(StatusCode::BAD_REQUEST, "拒绝必须填写理由，理由将写入审计");
    }
    match st.taskhub.decide(&id, approve, &note, remember).await {
        Ok(Some(_)) => {
            audit::write(&st.audit, audit::event(
                "human", if approve { "approve" } else { "reject" }, &id,
                if approve { "allowed" } else { "denied" },
                json!({ "note": note, "remember": remember }),
            ));
            // 批准后后台执行
            if approve {
                let hub = st.taskhub.clone();
                let sessions = st.sessions.clone();
                let audit = st.audit.clone();
                let sanitizer = st.sanitizer.clone();
                let tid = id.to_string();
                tokio::spawn(async move {
                    let task = hub.get(&tid).await;
                    let Some(task) = task else { return };
                    hub.mark_running(&tid).await;
                    let mut map = sessions.lock().await;
                    let out = match map.get_mut(&task.session) {
                        Some(c) => c.exec(&task.command, std::time::Duration::from_secs(600)).await,
                        None => Err("session gone".to_string()),
                    };
                    let (stdout, stderr, code) = match out {
                        Ok(o) => {
                            let s = sanitizer.sanitize(&o.stdout);
                            let e = sanitizer.sanitize(&o.stderr);
                            (s, e, o.exit_code.unwrap_or(0) as i32)
                        }
                        Err(e) => (String::new(), e, 127),
                    };
                    hub.finish(&tid, stdout, stderr, code).await;
                    audit::write(&audit, audit::event(
                        "human", "approve_exec", &tid, if code == 0 { "allowed" } else { "failed" },
                        json!({"command": task.command, "exit_code": code, "note": note}),
                    ));
                });
            }
            Response::builder().status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(r#"{"ok":true}"#.into()).unwrap()
        }
        Ok(None) => Response::builder().status(StatusCode::NOT_FOUND).body("{}".into()).unwrap(),
        Err(e) => Response::builder().status(StatusCode::BAD_REQUEST).body(e.into()).unwrap(),
    }
}


/// 鉴权已由 `authz::require_auth` 中间件统一完成（W-15）。
/// 本 handler 不再解析 Bearer，只从 Extension 取认证结果，
/// 保证全仓唯一令牌解析点。
async fn handle_mcp(
    State(st): State<AppState>,
    Extension(ctx): Extension<authz::AuthContext>,
    body: String,
) -> Response {
    let cred = ctx.cred.clone();
    let cred_ref = cred.as_ref();

    let req: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return rpc::http_err(StatusCode::BAD_REQUEST, -32700, &format!("parse error: {e}")),
    };

    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = req.get("id").cloned().unwrap_or(Value::Null);

    // 无 id 即通知，按规范不回包
    if id.is_null() {
        return rpc::accepted();
    }

    match method {
        "initialize" => {
            let sid = audit::trace_id();
            audit::write(
                &st.audit,
                audit::event("agent", "initialize", "-", "allowed", json!({ "session_id": sid })),
            );
            rpc::ok(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "logging": {}
                    },
                    "serverInfo": { "name": "dou-gateway", "version": "0.2.0" },
                    "instructions": "豆本地网关。所有动作经策略判定并写入审计。首次连接请在客户端界面确认信任。"
                }),
                Some(&sid),
            )
        }
        "ping" => rpc::ok(id, json!({}), None),
        "tools/list" => rpc::ok(id, tools::list(), None),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            rpc::ok(id, tools::call(&st, name, &args, cred_ref).await, None)
        }
        _ => rpc::err(id, -32601, &format!("method not found: {method}"), None),
    }
}

fn is_loopback(bind: &str) -> bool {
    bind.starts_with("127.0.0.1:") || bind.starts_with("localhost:") || bind.starts_with("[::1]:")
}

fn data_dir() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .map(|p| PathBuf::from(p).join("dou"))
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn parse_spec(spec: &str) -> Option<(String, String, u16)> {
    let (user, rest) = spec.split_once('@')?;
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().ok()?),
        None => (rest, 22u16),
    };
    if user.is_empty() || host.is_empty() {
        return None;
    }
    Some((user.to_string(), host.to_string(), port))
}
