//! MCP 工具集（计划书 4.5 表 13 的五个工具，一个不多）。
//!
//! dou.exec 的执行路径：
//!   1. 有 SSH 会话 → 走 russh 在远端执行（真实通道）
//!   2. 无会话且 dev 模式 → 本机执行只读命令（仅用于协议联调）
//!   3. 其余 → 返回待审批

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::{audit, policy, scope, AppState};

pub fn list() -> Value {
    json!({
        "tools": [
            {
                "name": "dou.session_list",
                "description": "列出当前可用的会话与占用状态。只读，不返回地址与凭据。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": { "type": "string", "description": "项目标识，可选" }
                    },
                    "additionalProperties": false
                }
            },
            {
                "name": "dou.exec",
                "description": "在受控会话内执行一条非交互命令。必经策略判定。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session": { "type": "string", "description": "会话标识" },
                        "command": { "type": "string", "description": "命令原文" },
                        "timeout_sec": { "type": "integer", "description": "超时，上限 600", "minimum": 1, "maximum": 600 }
                    },
                    "required": ["session", "command"],
                    "additionalProperties": false
                }
            },
            {
                "name": "dou.read",
                "description": "读取远端文本文件片段，返回脱敏后的内容。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session": { "type": "string" },
                        "path": { "type": "string" },
                        "start_line": { "type": "integer", "minimum": 1 },
                        "end_line": { "type": "integer", "minimum": 1 }
                    },
                    "required": ["session", "path"],
                    "additionalProperties": false
                }
            },
            {
                "name": "dou.put",
                "description": "上传文件。先落隔离区审查，再由用户决定是否落到远端。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session": { "type": "string" },
                        "remote_path": { "type": "string" },
                        "local_ref": { "type": "string", "description": "本机文件引用，不传绝对路径串" }
                    },
                    "required": ["session", "remote_path", "local_ref"],
                    "additionalProperties": false
                }
            },
            {
                "name": "dou.get",
                "description": "下载文件到本机隔离区并计算哈希，是否留用由用户决定。",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session": { "type": "string" },
                        "remote_path": { "type": "string" }
                    },
                    "required": ["session", "remote_path"],
                    "additionalProperties": false
                }
            }
        ]
    })
}

pub async fn call(st: &AppState, name: &str, args: &Value, cred: Option<&scope::Credential>) -> Value {
    // 作用域与配额校验（设备令牌场景；dev 兜底令牌为 None 不拦截）
    if let Some(c) = cred {
        let sc = &c.scope;
        let tool_key = match name {
            "dou.session_list" => "session_list",
            "dou.read" | "dou.get" => "read_file",
            "dou.exec" => "exec",
            "dou.put" => "upload",
            _ => "",
        };
        if !scope::tool_allowed(sc, tool_key) {
            audit::write(&st.audit, audit::event("agent", "scope_deny", "-", "denied",
                json!({ "tool": name, "tool_key": tool_key })));
            return text_result(&format!("tool `{name}` 不在当前凭据作用域内"), true);
        }
        // 配额：累计次数 + 并发上限
        {
            let mut q = st.quota.lock().await;
            let (calls, active) = q.entry(c.id.clone()).or_insert((0, 0));
            if *calls >= sc.quota.max_calls as u64 {
                audit::write(&st.audit, audit::event("agent", "quota_calls", "-", "denied",
                    json!({ "cred": c.id, "max_calls": sc.quota.max_calls })));
                return text_result("凭据调用次数已达上限", true);
            }
            if *active >= sc.quota.concurrency as u64 {
                audit::write(&st.audit, audit::event("agent", "quota_concurrency", "-", "denied",
                    json!({ "cred": c.id, "concurrency": sc.quota.concurrency })));
                return text_result("凭据并发会话已达上限", true);
            }
            *calls += 1;
            *active += 1;
        }
        let result = match name {
            "dou.session_list" => session_list(st).await,
            "dou.exec" => exec(st, args, Some(sc)).await,
            "dou.read" => read(st, args, Some(sc)).await,
            "dou.put" => put(st, args, Some(sc)).await,
            "dou.get" => get(st, args, Some(sc)).await,
            _ => text_result(&format!("unknown tool: {name}"), true),
        };
        let mut q = st.quota.lock().await;
        if let Some((_, active)) = q.get_mut(&c.id) { *active -= 1; }
        return result;
    }
    match name {
        "dou.session_list" => session_list(st).await,
        "dou.exec" => exec(st, args, None).await,
        "dou.read" => read(st, args, None).await,
        "dou.put" => put(st, args, None).await,
        "dou.get" => get(st, args, None).await,
        _ => text_result(&format!("unknown tool: {name}"), true),
    }
}

async fn session_list(st: &AppState) -> Value {
    let map = st.sessions.lock().await;
    let items: Vec<Value> = map
        .iter()
        .map(|(id, c)| {
            let owner = crate::state::session_owner(id).unwrap_or_default();
            json!({
                "id": id,
                "user": c.user,
                "status": "connected",
                "locked": !owner.is_empty(),
                "owner": owner
            })
        })
        .collect();
    text_result(&json!({ "sessions": items }).to_string(), false)
}

async fn exec(st: &AppState, args: &Value, scope: Option<&scope::Scope>) -> Value {
    let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let session_id = args.get("session").and_then(|v| v.as_str()).unwrap_or("default");

    // A1 会话锁：代理调用被人占用时拒绝（人工优先）。
    if let Some(r) = lock_denied(st, session_id, scope.is_some()) {
        return r;
    }

    if let Some(sc) = scope {
        if !sc.command_patterns.is_empty()
            && !sc.command_patterns.iter().any(|pat| cmd.starts_with(pat.trim_end_matches('*')))
        {
            audit::write(&st.audit, audit::event("agent", "scope_cmd_deny", session_id, "denied",
                json!({ "command": cmd })));
            return text_result("命令不在当前凭据的只读命令白名单内", true);
        }
    }
    // 会话租约：单次任务最长 30min，超期需重新配对。
    {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let mut leases = st.leases.lock().await;
        match leases.get(session_id) {
            Some(start) if now.saturating_sub(*start) > crate::auth::LEASE_SECS => {
                audit::write(&st.audit, audit::event("agent", "lease_expired", session_id, "denied",
                    json!({ "lease_secs": crate::auth::LEASE_SECS })));
                return text_result("会话租约已超过 30 分钟，请重新配对接入", true);
            }
            None => { leases.insert(session_id.to_string(), now); }
            _ => {}
        }
    }
    let policy = scope.map(|s| s.approval_policy.clone()).unwrap_or_else(|| "execute".to_string());
    let background = args.get("background").and_then(|v| v.as_bool()).unwrap_or(false);
    // A2：仅当「显式传入 timeout_sec 且 >30」或「background 标志」才任务化；
    // 未传 timeout_sec 的默认同步执行不受影响。
    let explicit_timeout = args.get("timeout_sec").and_then(|v| v.as_u64());
    let long_running = background || explicit_timeout.map(|t| t > 30).unwrap_or(false);
    let timeout_sec = explicit_timeout.unwrap_or(60).min(600);

    let decision = policy::classify(cmd);
    audit::write(
        &st.audit,
        audit::event(
            "agent",
            "exec",
            session_id,
            decision.verdict.as_str(),
            json!({ "command": cmd, "reason": decision.reason }),
        ),
    );

    match decision.verdict {
        policy::Verdict::Blocked => text_result(
            &json!({
                "status": "blocked",
                "reason": decision.reason,
                "hint": "该类命令在观察档与执行档下均不开放，需人工在终端执行"
            })
            .to_string(),
            true,
        ),
        policy::Verdict::Pending => {
            // A4：审批卡片四要素——命令原文已在 task，此处补目标路径 + 预计影响。
            let impact = estimate_impact(cmd);
            let tid = st.taskhub.create_pending_with(
                session_id, cmd, &decision.reason, &policy,
                "", &impact, "",
            ).await;
            text_result(
                &json!({
                    "status": "pending_approval",
                    "task_id": tid,
                    "reason": decision.reason,
                    "segment": decision.segment,
                    "impact": impact,
                    "hint": "在控制台 /console 点批准或拒绝；批准后后台执行"
                })
                .to_string(),
                false,
            )
        }
        policy::Verdict::Auto => {
            // A2 长命令任务化：超时 > 30s 自动移交后台任务句柄，立即返回 task_id。
            // 客户端断开后远端进程继续跑，可查状态、可终止。
            if long_running {
                let impact = estimate_impact(cmd);
                let tid = st.taskhub.create_pending_with(
                    session_id, cmd, "background exec", &policy,
                    "", &impact, "",
                ).await;
                // 直接标记 approved（策略已判 auto，无需人工批）
                let _ = st.taskhub.decide(&tid, true, "auto-approved", false).await;
                let hub = st.taskhub.clone();
                let sessions = st.sessions.clone();
                let audit = st.audit.clone();
                let sanitizer = st.sanitizer.clone();
                let tid2 = tid.clone();
                let cmd2 = cmd.to_string();
                let sid = session_id.to_string();
                tokio::spawn(async move {
                    hub.mark_running(&tid2).await;
                    let mut map = sessions.lock().await;
                    let out = match map.get_mut(&sid) {
                        Some(c) => c.exec(&cmd2, Duration::from_secs(600)).await,
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
                    hub.finish(&tid2, stdout, stderr, code).await;
                    audit::write(&audit, audit::event(
                        "agent", "bg_exec_done", &tid2, if code == 0 { "done" } else { "failed" },
                        json!({"command": cmd2, "exit_code": code}),
                    ));
                });
                return text_result(
                    &json!({
                        "status": "background_task",
                        "task_id": tid,
                        "hint": "用 dou.task_status 或 GET /tasks 查结果"
                    }).to_string(),
                    false,
                );
            }
            // 单会话串行执行，对应计划书默认配额"1 并发"
            let mut map = st.sessions.lock().await;
            if let Some(conn) = map.get_mut(session_id) {
                let host = conn.host.clone();
                let user = conn.user.clone();
                match conn.exec(cmd, Duration::from_secs(timeout_sec)).await {
                    Ok(out) => {
                        // 补采上下文：cwd / pid / GPU（计划书 F-24 被动采集）
                        let probe = conn.exec(crate::snapshot::probe_command(), Duration::from_secs(10)).await;
                        let ctx = match probe {
                            Ok(p) => crate::snapshot::Context::from_probe_output(&p.stdout, &out.stdout),
                            Err(_) => crate::snapshot::Context::empty(),
                        };
                        let snap = crate::snapshot::record(session_id, None, cmd, &ctx, "")
                            .unwrap_or(json!({"duplicate": false}));
                        audit::write(
                            &st.audit,
                            audit::event(
                                "agent",
                                "exec_result",
                                session_id,
                                "done",
                                json!({
                                    "command": cmd,
                                    "exit_code": out.exit_code,
                                    "timed_out": out.timed_out,
                                    "snapshot": snap.get("snapshot_id"),
                                    "duplicate": snap.get("duplicate")
                                }),
                            ),
                        );
                        let safe_stdout = st.sanitizer.sanitize(&out.stdout);
                        let safe_stderr = st.sanitizer.sanitize(&out.stderr);
                        text_result(
                            &json!({
                                "status": "executed_remote",
                                "host": format!("{user}@{host}"),
                                "exit_code": out.exit_code,
                                "timed_out": out.timed_out,
                                "stdout": truncate(&safe_stdout, 4000),
                                "stderr": truncate(&safe_stderr, 1000),
                                "snapshot": snap
                            })
                            .to_string(),
                            out.timed_out || out.exit_code.unwrap_or(0) != 0,
                        )
                    }
                    Err(e) => text_result(&crate::errors::to_json(&e).to_string(), true),
                }
            } else if st.dev {
                let out = run_local(cmd);
                text_result(
                    &json!({
                        "status": "executed_local_dev",
                        "exit_code": out.0,
                        "stdout": truncate(&out.1, 4000),
                        "stderr": truncate(&out.2, 1000),
                        "warning": "开发模式：命令在本机执行，不代表已连上远端服务器"
                    })
                    .to_string(),
                    out.0 != 0,
                )
            } else {
                text_result(
                    &json!({
                        "status": "pending_approval",
                        "reason": format!("session `{session_id}` 不存在或未连接")
                    })
                    .to_string(),
                    false,
                )
            }
        }
    }
}

fn run_local(cmd: &str) -> (i32, String, String) {
    #[cfg(windows)]
    let mut c = {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(cmd);
        c
    };
    #[cfg(not(windows))]
    let mut c = {
        let mut c = std::process::Command::new("sh");
        c.arg("-c").arg(cmd);
        c
    };
    match c.output() {
        Ok(o) => (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).to_string(),
            String::from_utf8_lossy(&o.stderr).to_string(),
        ),
        Err(e) => (-1, String::new(), e.to_string()),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}\n…(truncated, {} chars total)", s.chars().count())
}

/// 路径范围校验（计划书七维作用域之一）。
/// 允许：/home/dou/、/tmp/、少数系统白名单文件；
/// 拒绝：/etc/shadow、/etc/sudoers、.ssh 私钥、/root 等。
fn check_remote_path(path: &str) -> Result<(), String> {
    // B3 逃逸加固第一步：先做词法归一化，把 `..` / `.` / 重复斜杠折叠成真实路径，
    // 否则 `/home/dou/projects/../../etc/shadow` 会被纯字符串 contains 漏判。
    let normalized = normalize_remote_path(path)?;

    // 归一化后做敏感前缀拒绝（用 starts_with 而非 contains，避免误伤合法路径）。
    let denied_prefix = [
        "/etc/shadow", "/etc/sudoers", "/etc/sudoers.d/", "/root/", "/root",
        "/home/dou/.ssh/", "/proc/self/environ", "/proc/sys/",
    ];
    for d in &denied_prefix {
        if normalized == *d || normalized.starts_with(&format!("{d}/")) || normalized.starts_with(d) {
            return Err(format!("路径越出作用域：{d} 类路径禁止代理访问"));
        }
    }
    // 私钥文件名拒绝（不论所在目录）。
    let fname = normalized.rsplit('/').next().unwrap_or("");
    if fname.starts_with("id_rsa") || fname.starts_with("id_ed25519")
        || fname.starts_with("id_ecdsa") || fname == "authorized_keys"
    {
        return Err(format!("私钥/授权文件禁止代理访问：{fname}"));
    }
    // 必须落在允许根下（归一化后比较）。
    if !(normalized.starts_with("/home/dou/") || normalized.starts_with("/tmp/")
        || normalized == "/etc/hostname" || normalized == "/etc/os-release" || normalized == "/etc/passwd")
    {
        return Err(format!("路径 {path} 越出允许根（/home/dou/、/tmp/）"));
    }
    Ok(())
}

/// B3 路径归一化：折叠 `..` / `.` / 重复斜杠，越出根目录时报错。
/// 例：`/home/dou/projects/../../etc/shadow` → `/etc/shadow`（随后被敏感前缀拒绝）。
fn normalize_remote_path(path: &str) -> Result<String, String> {
    if path.is_empty() {
        return Err("路径不能为空".to_string());
    }
    let absolute = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if stack.pop().is_none() && absolute {
                    return Err(format!("路径 {path} 越出根目录（../ 逃逸）"));
                }
            }
            s => stack.push(s),
        }
    }
    if !absolute {
        return Err(format!("仅允许绝对路径：{path}"));
    }
    Ok(format!("/{}", stack.join("/")))
}

async fn read(st: &AppState, args: &Value, scope: Option<&scope::Scope>) -> Value {
    let session_id = args.get("session").and_then(|v| v.as_str()).unwrap_or("default");
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let start = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let end = args.get("end_line").and_then(|v| v.as_u64()).map(|v| v as usize);

    if let Some(r) = lock_denied(st, session_id, scope.is_some()) {
        return r;
    }

    if path.is_empty() {
        return text_result(&json!({"status":"error","message":"path 不能为空"}).to_string(), true);
    }
    if let Some(sc) = scope {
        if !scope::path_allowed(sc, path) {
            audit::write(&st.audit, audit::event("agent", "scope_path_deny", session_id, "denied",
                json!({ "path": path })));
            return text_result(&json!({"status":"path_scope_denied","message":"路径不在当前凭据作用域内"}).to_string(), true);
        }
    }
    if let Err(e) = check_remote_path(path) {
        return text_result(&json!({"status":"path_scope_denied","message":e}).to_string(), true);
    }
    let mut map = st.sessions.lock().await;
    let conn = match map.get_mut(session_id) {
        Some(c) => c,
        None => return text_result(&json!({"status":"error","message":format!("session `{session_id}` 不存在")}).to_string(), true),
    };
    match conn.remote_read(path, start, end, 512_000).await {
        Ok(out) => {
            audit::write(
                &st.audit,
                audit::event("agent", "read", session_id, if out.ok { "allowed" } else { "denied" },
                    json!({"path": path, "bytes": out.bytes, "warning": out.warning})),
            );
            let mut j = serde_json::Map::new();
            j.insert("status".into(), json!(if out.ok { "read" } else { "too_large" }));
            j.insert("path".into(), json!(path));
            j.insert("bytes".into(), json!(out.bytes));
            j.insert("content".into(), json!(out.content));
            if let Some(w) = out.warning { j.insert("warning".into(), json!(w)); }
            text_result(&Value::Object(j).to_string(), !out.ok)
        }
        Err(e) => {
            let s = crate::errors::to_json(&e).to_string();
            text_result(&s, true)
        }
    }
}

async fn put(st: &AppState, args: &Value, scope: Option<&scope::Scope>) -> Value {
    let session_id = args.get("session").and_then(|v| v.as_str()).unwrap_or("default");
    let remote = args.get("remote_path").and_then(|v| v.as_str()).unwrap_or("");
    let local_ref = args.get("local_ref").and_then(|v| v.as_str()).unwrap_or("");
    if let Some(r) = lock_denied(st, session_id, scope.is_some()) {
        return r;
    }
    if remote.is_empty() || local_ref.is_empty() {
        return text_result(&json!({"status":"error","message":"remote_path 与 local_ref 必填"}).to_string(), true);
    }
    if let Some(sc) = scope {
        if !scope::path_allowed(sc, remote) {
            audit::write(&st.audit, audit::event("agent", "scope_path_deny", session_id, "denied",
                json!({ "path": remote })));
            return text_result(&json!({"status":"path_scope_denied","message":"目标路径不在当前凭据作用域内"}).to_string(), true);
        }
    }
    if let Err(e) = check_remote_path(remote) {
        return text_result(&json!({"status":"path_scope_denied","message":e}).to_string(), true);
    }
    let local = std::path::Path::new(local_ref);
    let bytes = match std::fs::read(local) {
        Ok(b) => b,
        Err(e) => return text_result(&json!({"status":"error","message":format!("读不到本机文件 {local_ref}: {e}")}).to_string(), true),
    };
    if bytes.len() > 50 * 1024 * 1024 {
        return text_result(&json!({"status":"error","message":"单文件超 50MB，请走任务化传输"}).to_string(), true);
    }
    let mut map = st.sessions.lock().await;
    let conn = match map.get_mut(session_id) {
        Some(c) => c,
        None => return text_result(&json!({"status":"error","message":format!("session `{session_id}` 不存在")}).to_string(), true),
    };
    match conn.remote_put(&bytes, remote).await {
        Ok((n, digest)) => {
            audit::write(&st.audit, audit::event("agent", "put", session_id, "allowed",
                json!({"remote_path": remote, "bytes": n, "sha256": digest})));
            text_result(&json!({"status":"put","remote_path": remote,"bytes": n,"sha256": digest}).to_string(), false)
        }
        Err(e) => {
            let s = crate::errors::to_json(&e).to_string();
            text_result(&s, true)
        }
    }
}

async fn get(st: &AppState, args: &Value, scope: Option<&scope::Scope>) -> Value {
    let session_id = args.get("session").and_then(|v| v.as_str()).unwrap_or("default");
    let remote = args.get("remote_path").and_then(|v| v.as_str()).unwrap_or("");
    if let Some(r) = lock_denied(st, session_id, scope.is_some()) {
        return r;
    }
    if remote.is_empty() {
        return text_result(&json!({"status":"error","message":"remote_path 必填"}).to_string(), true);
    }
    if let Some(sc) = scope {
        if !scope::path_allowed(sc, remote) {
            audit::write(&st.audit, audit::event("agent", "scope_path_deny", session_id, "denied",
                json!({ "path": remote })));
            return text_result(&json!({"status":"path_scope_denied","message":"路径不在当前凭据作用域内"}).to_string(), true);
        }
    }
    if let Err(e) = check_remote_path(remote) {
        return text_result(&json!({"status":"path_scope_denied","message":e}).to_string(), true);
    }
    let mut map = st.sessions.lock().await;
    let conn = match map.get_mut(session_id) {
        Some(c) => c,
        None => return text_result(&json!({"status":"error","message":format!("session `{session_id}` 不存在")}).to_string(), true),
    };
    match conn.remote_get(remote).await {
        Ok((bytes, digest)) => {
            let dir = crate::sftp::quarantine_dir();
            let _ = std::fs::create_dir_all(&dir);
            let name = std::path::Path::new(remote)
                .file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "download.bin".into());
            let dest = dir.join(format!("{}_{}", &digest[..8], name));
            let _ = std::fs::write(&dest, &bytes);
            // 隔离区静态扫描（计划书 4.3/4.10）：特征库 + 脚本规则双检
            let scan = crate::scan::scan_file(&dest);
            let scan_verdict = scan.disposition.as_str();
            audit::write(&st.audit, audit::event("agent", "get", session_id, scan_verdict,
                json!({"remote_path": remote, "bytes": bytes.len(), "sha256": digest,
                       "local": dest, "scan_engine": scan.engine, "scan_detail": scan.detail})));
            if scan.infected {
                // 感染即删，不让代理读到
                let _ = std::fs::remove_file(&dest);
                text_result(&json!({
                    "status":"rejected_infected",
                    "remote_path": remote,
                    "bytes": bytes.len(),
                    "sha256": digest,
                    "scan_engine": scan.engine,
                    "scan_detail": scan.detail,
                    "disposition": scan_verdict,
                    "note": "文件已在隔离区删除"
                }).to_string(), true)
            } else if scan.disposition == crate::scan::Disposition::Pending {
                // 命中高危规则但未确诊：待人工审查，不自动落远端
                text_result(&json!({
                    "status":"pending_review",
                    "remote_path": remote, "bytes": bytes.len(), "sha256": digest,
                    "local_quarantine": dest.to_string_lossy(),
                    "scan_engine": scan.engine, "scan_detail": scan.detail,
                    "disposition": scan_verdict,
                    "note": "命中高危脚本规则，待人工审查后再决定是否留用"
                }).to_string(), false)
            } else {
                text_result(&json!({
                    "status":"get","remote_path": remote,"bytes": bytes.len(),
                    "sha256": digest,
                    "local_quarantine": dest.to_string_lossy(),
                    "scan_engine": scan.engine,
                    "scan_detail": scan.detail,
                    "disposition": scan_verdict
                }).to_string(), false)
            }
        }
        Err(e) => {
            let s = crate::errors::to_json(&e).to_string();
            text_result(&s, true)
        }
    }
}

/// A4：按命令估算「预计影响」的人话描述，供审批卡片展示。
/// 只做粗粒度判断，不替代策略引擎。
fn estimate_impact(cmd: &str) -> String {
    let lower = cmd.to_ascii_lowercase();
    if lower.contains("rm -rf") || lower.contains("rm -fr") || lower.contains("rmdir") {
        "删除文件/目录".to_string()
    } else if lower.contains("mkfs") || lower.contains("fdisk") || lower.contains("dd if=") {
        "写入块设备/磁盘（高危）".to_string()
    } else if lower.contains("chmod") || lower.contains("chown") || lower.contains("chattr") {
        "修改文件权限/属主".to_string()
    } else if lower.contains("pip install") || lower.contains("apt ") || lower.contains("apt-get")
        || lower.contains("yum ") || lower.contains("npm install") || lower.contains("brew install") {
        "安装软件包".to_string()
    } else if lower.contains("git push") || lower.contains("git commit") {
        "提交/推送代码".to_string()
    } else if lower.contains("sudo") || lower.contains("su ") {
        "提权操作".to_string()
    } else if lower.starts_with("install ") || lower.contains(" install ") {
        "安装文件".to_string()
    } else if lower.contains("mv ") || lower.contains("cp ") {
        "移动/复制文件".to_string()
    } else if lower.contains(">") || lower.contains(">>") {
        "重定向写入文件".to_string()
    } else {
        "执行命令（需人工确认）".to_string()
    }
}

fn not_implemented(name: &str, why: &str) -> Value {
    text_result(
        &json!({ "status": "not_implemented", "tool": name, "reason": why }).to_string(),
        false,
    )
}

/// A1 会话锁：代理（设备令牌）调用被人工占用时拒绝。
/// 返回 `None` 表示放行；`Some` 为拒绝响应（已写审计）。
/// 人工（主令牌，`is_agent == false`）优先，不受锁限制。
fn lock_denied(st: &AppState, session_id: &str, is_agent: bool) -> Option<Value> {
    if !is_agent {
        return None;
    }
    match crate::state::session_owner(session_id) {
        Some(owner) if owner == "human" => {
            audit::write(&st.audit, audit::event(
                "agent", "session_locked", session_id, "denied",
                json!({ "owner": owner }),
            ));
            Some(text_result(
                &json!({
                    "status": "session_locked",
                    "owner": owner,
                    "reason": "该会话正被人工占用，代理请求已拒绝，请等待人工释放"
                }).to_string(),
                true,
            ))
        }
        _ => None,
    }
}

pub fn text_result(text: &str, is_error: bool) -> Value {
    json!({
        "content": [ { "type": "text", "text": text } ],
        "isError": is_error
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_folds_dotdot() {
        assert_eq!(normalize_remote_path("/home/dou/projects/../../etc/shadow").unwrap(), "/home/etc/shadow");
        assert_eq!(normalize_remote_path("/home/dou/a/./b//c").unwrap(), "/home/dou/a/b/c");
        assert_eq!(normalize_remote_path("/tmp/./x/../y").unwrap(), "/tmp/y");
        // 三个 ../ 才回到根
        assert_eq!(normalize_remote_path("/home/dou/projects/../../../etc/shadow").unwrap(), "/etc/shadow");
    }

    #[test]
    fn normalize_rejects_root_escape() {
        assert!(normalize_remote_path("/../../etc/shadow").is_err());
        assert!(normalize_remote_path("../etc/shadow").is_err());
    }

    #[test]
    fn normalize_requires_absolute() {
        assert!(normalize_remote_path("home/dou/x").is_err());
    }

    #[test]
    fn check_path_blocks_dotdot_escape() {
        // 归一化后落到 /etc/shadow，应被拒
        assert!(check_remote_path("/home/dou/projects/../../../etc/shadow").is_err());
        assert!(check_remote_path("/tmp/../../root/.ssh/id_rsa").is_err());
        // 越出 /home/dou 允许根（归一化到 /home/etc），应被拒
        assert!(check_remote_path("/home/dou/projects/../../etc/shadow").is_err());
        // 合法路径放行
        assert!(check_remote_path("/home/dou/data/train.log").is_ok());
        assert!(check_remote_path("/tmp/note.txt").is_ok());
    }

    #[test]
    fn check_path_blocks_private_keys_anywhere() {
        assert!(check_remote_path("/tmp/id_rsa").is_err());
        assert!(check_remote_path("/home/dou/.ssh/id_ed25519").is_err());
        assert!(check_remote_path("/home/dou/data/authorized_keys").is_err());
    }

    #[test]
    fn impact_estimation_smoke() {
        assert!(estimate_impact("rm -rf /data").contains("删除"));
        assert!(estimate_impact("pip install torch").contains("安装"));
        assert!(estimate_impact("git push origin").contains("提交"));
    }
}
