mod pty;
mod sftp;
mod transfer;

use pty::PtyState;
use serde::Serialize;
use std::sync::Once;

static START_GATEWAY: Once = Once::new();

#[derive(Serialize)]
struct GatewayStatus {
    running: bool,
    endpoint: String,
    /// 真实探测 SSH 靶机 22 端口；不再用「没报错 = 已连接」推断，避免谎报在线。
    ssh_up: bool,
}

fn ssh_target_up() -> bool {
    use std::net::{SocketAddr, TcpStream};
    let addr: SocketAddr = "127.0.0.1:22".parse().unwrap();
    TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300)).is_ok()
}

#[tauri::command]
fn gateway_status() -> GatewayStatus {
    let endpoint = "http://127.0.0.1:7419/health".to_string();
    let running = match reqwest::blocking::get(&endpoint) { Ok(_) => true, Err(_) => false };
    GatewayStatus { running, endpoint, ssh_up: ssh_target_up() }
}

/// 统一网关请求通道：令牌由 Rust 侧读取并注入，HTTP 由 reqwest 发出。
/// 绕开 WebView 的 CORS 与系统代理（localhost:1420 -> 127.0.0.1:7419 属跨域），
/// 也保证令牌不进入前端 fetch 作用域。
#[derive(Serialize)]
struct GwResponse {
    status: u16,
    body: String,
}

#[tauri::command]
async fn gateway_request(path: String, method: String, body: Option<String>) -> Result<GwResponse, String> {
    let token = gateway_token()?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;
    let m = match method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        _ => return Err(format!("unsupported method: {method}")),
    };
    let url = format!("http://127.0.0.1:7419{path}");
    let mut req = client.request(m, &url).bearer_auth(token);
    if let Some(b) = body {
        req = req.header("Content-Type", "application/json").body(b);
    }
    let resp = req.send().await.map_err(|e| format!("gateway unreachable {url}: {e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(GwResponse { status, body: text })
}

// 【本轮新增】读取主令牌供前端注入 Bearer；只读、无副作用、不落日志。
// 路径口径与网关 crates/dou-gateway/src/auth.rs:14-20 的 data_dir() 逐字一致：
//   base = LOCALAPPDATA | USERPROFILE | HOME | "."；PathBuf::from(base).join("dou").join("token.txt")
// 校验语义与 auth.rs:42 一致：trim 后以 "dou_ak_" 开头且长度 > 16。
// 令牌只经 IPC 返回，绝不写文件 / 打印 / 进日志；错误串不含令牌内容。
#[tauri::command]
fn gateway_token() -> Result<String, String> {
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    let path = std::path::PathBuf::from(base).join("dou").join("token.txt");
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        format!("读取主令牌文件失败 dou/token.txt（回退链 LOCALAPPDATA→USERPROFILE→HOME→.）：{e}")
    })?;
    let token = raw.trim().to_string();
    if !token.starts_with("dou_ak_") || token.len() <= 16 {
        return Err("主令牌格式错误：应为 dou_ak_ 前缀且长度大于 16".into());
    }
    Ok(token)
}

#[tauri::command]
async fn open_pty(app: tauri::AppHandle, state: tauri::State<'_, PtyState>, host: String, port: u16, user: String, key_path: String) -> Result<String, String> {
    pty::open_pty(app, &state, &host, port, &user, &key_path).await
}

#[tauri::command]
async fn pty_write(state: tauri::State<'_, PtyState>, sid: String, data: String) -> Result<(), String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let s = sessions.get(&sid).ok_or_else(|| format!("session {sid} 不存在"))?;
    s.tx.send(data).map_err(|e| e.to_string())
}

#[tauri::command]
async fn sftp_list(host: String, port: u16, user: String, key_path: String, path: String) -> Result<Vec<sftp::SftpEntry>, String> {
    sftp::sftp_list(&host, port, &user, &key_path, &path).await
}

#[tauri::command]
async fn sftp_download(host: String, port: u16, user: String, key_path: String, remote: String) -> Result<String, String> {
    transfer::download(&host, port, &user, &key_path, &remote).await
}

#[tauri::command]
async fn sftp_upload(host: String, port: u16, user: String, key_path: String, local: String, remote: String) -> Result<(), String> {
    transfer::upload(&host, port, &user, &key_path, &local, &remote).await
}

#[tauri::command]
fn sanitize_ansi(s: String) -> String {
    transfer::sanitize_ansi(&s)
}

#[tauri::command]
async fn download_chunked(host: String, port: u16, user: String, key_path: String, remote: String) -> Result<String, String> {
    transfer::download_chunked(&host, port, &user, &key_path, &remote).await
}

#[tauri::command]
async fn remote_stat(host: String, port: u16, user: String, key_path: String, remote: String) -> Result<(u64, i64), String> {
    transfer::remote_stat(&host, port, &user, &key_path, &remote).await
}

#[tauri::command]
async fn download_dir(host: String, port: u16, user: String, key_path: String, remote: String) -> Result<String, String> {
    transfer::download_dir(&host, port, &user, &key_path, &remote).await
}

#[tauri::command]
async fn upload_dir(host: String, port: u16, user: String, key_path: String, local: String, remote: String) -> Result<(), String> {
    transfer::upload_dir(&host, port, &user, &key_path, &local, &remote).await
}

#[tauri::command]
async fn download_resume(host: String, port: u16, user: String, key_path: String, remote: String, offset: u64) -> Result<(String, u64, bool), String> {
    transfer::download_resume(&host, port, &user, &key_path, &remote, offset).await
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("你好，{}！豆 Dou 桌面壳已就绪。", name)
}

fn ensure_gateway_started() {
    START_GATEWAY.call_once(|| {
        let exe = r"E:\dou-target\debug\dou-gateway.exe";
        if std::path::Path::new(exe).exists() {
            let _ = std::process::Command::new(exe)
                .env("DOU_SSH", "dou@127.0.0.1:22")
                .env("DOU_SSH_KEY", r"C:\Users\mrhua\AppData\Local\dou\keys\id_ed25519")
                .spawn();
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    ensure_gateway_started();
    std::thread::sleep(std::time::Duration::from_millis(800));
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .manage(PtyState::default())
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 关窗改成隐藏，不断 SSH 连接
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![greet, gateway_status, gateway_token, gateway_request, open_pty, pty_write, sftp_list, sftp_download, sftp_upload, sanitize_ansi, download_chunked, remote_stat, download_dir, upload_dir, download_resume])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
