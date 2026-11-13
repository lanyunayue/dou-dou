//! `dou-ssh-selfcheck`：受治理远程操作主线 · 端到端自校验（无外部 Linux 靶机替代方案）。
//!
//! 在进程内启动 `dou_ssh_loopback` 回环 SSH 服务，然后复用与豆 Dou app 完全一致的
//! russh 高层 API 调用路径，验证：
//!   A. 主机密钥 TOFU：首次(FirstSeen) → 重复(KnownMatch) → 变更(Mismatch 拒绝)
//!   B. 公钥认证连接成功
//!   C. 上传：分块 + 断点续传（cat > / cat >>）+ 内容一致性
//!   D. 下载：分块（dd if=… skip=… count=1）+ 内容一致性
//!   E. 文件夹同步：push（镜像 + 删远端多余）/ pull（镜像 + 删本地多余）
//!
//! 全部断言通过则退出码 0，否则 1；并输出可读 PASS/FAIL 报告（同时写 report 文件）。

use dou_ssh_loopback::{find_git_bash, start_server};
use rand::Rng;
use russh::client;
use russh::keys::ssh_key::{HashAlg, LineEnding, PublicKey};
use russh::keys::{Algorithm, PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::ChannelMsg;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ------------------------- 主机密钥 TOFU 存储（与 app/ssh.rs 语义一致） -------------------------

#[derive(Clone, Debug, PartialEq)]
enum Verdict {
    FirstSeen(String),
    KnownMatch,
    Mismatch { expected: String, got: String },
}

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    host: String,
    port: u16,
    fingerprint: String,
    first_seen: u64,
    last_seen: u64,
}

#[derive(Serialize, Deserialize, Default)]
struct StoreFile {
    entries: Vec<Entry>,
}

struct HostKeyStore {
    path: PathBuf,
    data: StoreFile,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl HostKeyStore {
    fn load(path: &Path) -> Self {
        let data = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        HostKeyStore {
            path: path.to_path_buf(),
            data,
        }
    }
    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = serde_json::to_string_pretty(&self.data) {
            let _ = std::fs::write(&self.path, s);
        }
    }
    fn verify_or_learn(&mut self, host: &str, port: u16, fp: &str) -> Verdict {
        if let Some(e) = self
            .data
            .entries
            .iter_mut()
            .find(|e| e.host == host && e.port == port)
        {
            if e.fingerprint == fp {
                e.last_seen = now();
                self.save();
                Verdict::KnownMatch
            } else {
                Verdict::Mismatch {
                    expected: e.fingerprint.clone(),
                    got: fp.to_string(),
                }
            }
        } else {
            let t = now();
            self.data.entries.push(Entry {
                host: host.to_string(),
                port,
                fingerprint: fp.to_string(),
                first_seen: t,
                last_seen: t,
            });
            self.save();
            Verdict::FirstSeen(fp.to_string())
        }
    }
    #[allow(dead_code)]
    fn forget(&mut self, host: &str, port: u16) {
        self.data
            .entries
            .retain(|e| !(e.host == host && e.port == port));
        self.save();
    }

    /// 测试辅助：把某主机已学到的指纹「复制」到另一个端口，模拟客户端在新端口上
    /// 仍持有旧密钥记录（服务端已轮换密钥）→ 应触发 Mismatch 拒绝。
    fn seed(&mut self, host: &str, port: u16, fp: &str) {
        self.data
            .entries
            .retain(|e| !(e.host == host && e.port == port));
        let t = now();
        self.data.entries.push(Entry {
            host: host.to_string(),
            port,
            fingerprint: fp.to_string(),
            first_seen: t,
            last_seen: t,
        });
        self.save();
    }

    fn fingerprint_of(&self, host: &str, port: u16) -> Option<String> {
        self.data
            .entries
            .iter()
            .find(|e| e.host == host && e.port == port)
            .map(|e| e.fingerprint.clone())
    }
}

// ------------------------- 客户端连接（复用 app 同款 russh API） -------------------------

struct ClientHandler {
    host: String,
    port: u16,
    store: Arc<Mutex<HostKeyStore>>,
    last: Arc<Mutex<Option<Verdict>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fp = host_key_fingerprint(key);
        let v = self
            .store
            .lock()
            .unwrap()
            .verify_or_learn(&self.host, self.port, &fp);
        *self.last.lock().unwrap() = Some(v.clone());
        // Mismatch（疑似中间人或密钥轮换）→ 拒绝握手，防 MITM。
        Ok(!matches!(v, Verdict::Mismatch { .. }))
    }
}

pub type DouHandle = client::Handle<ClientHandler>;

fn host_key_fingerprint(key: &PublicKeyOrCertificate) -> String {
    let pk: PublicKey = key.public_key();
    pk.fingerprint(HashAlg::Sha256).to_string()
}

/// 统一连接入口：主机密钥校验（connect 期间）+ 公钥认证，返回已认证会话与最新判定。
async fn ssh_connect(
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    store: Arc<Mutex<HostKeyStore>>,
) -> (Result<DouHandle, String>, Verdict) {
    let last = Arc::new(Mutex::new(None));
    let config = Arc::new(client::Config::default());
    let handler = ClientHandler {
        host: host.to_string(),
        port,
        store: store.clone(),
        last: last.clone(),
    };
    let mut session = match tokio::time::timeout(
        Duration::from_secs(15),
        client::connect(config, (host, port), handler),
    )
    .await
    {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return (
                Err(format!("连接失败: {e}")),
                last.lock().unwrap().clone().unwrap_or(Verdict::KnownMatch),
            )
        }
        Err(_) => {
            return (
                Err("连接超时(15s)：服务端握手未完成".into()),
                last.lock().unwrap().clone().unwrap_or(Verdict::KnownMatch),
            )
        }
    };
    let key = match PrivateKey::read_openssh_file(Path::new(key_path)) {
        Ok(k) => k,
        Err(e) => {
            return (
                Err(format!("读私钥: {e}")),
                last.lock().unwrap().clone().unwrap_or(Verdict::KnownMatch),
            )
        }
    };
    if let Err(e) = session
        .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), None))
        .await
    {
        return (
            Err(format!("认证失败: {e}")),
            last.lock().unwrap().clone().unwrap_or(Verdict::KnownMatch),
        );
    }
    let v = last.lock().unwrap().clone().unwrap_or(Verdict::KnownMatch);
    (Ok(session), v)
}

// ------------------------- 传输逻辑（与 app/transfer_queue.rs 完全一致的模式） -------------------------

const CHUNK: u64 = 65536;

async fn remote_stat(session: &DouHandle, remote: &str) -> u64 {
    let mut ch = session
        .channel_open_session()
        .await
        .expect("open stat channel");
    ch.exec(true, format!("stat -c '%s' {remote} 2>/dev/null"))
        .await
        .expect("exec stat");
    let mut out = String::new();
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Data { data } => out.push_str(&String::from_utf8_lossy(&data)),
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    out.trim().parse::<u64>().unwrap_or(0)
}

async fn download_file(session: &DouHandle, remote: &str, local: &str) -> u64 {
    if let Some(p) = Path::new(local).parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let existing = std::fs::metadata(local).map(|m| m.len()).unwrap_or(0);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(local)
        .expect("open local for download");
    let total = remote_stat(session, remote).await;
    let mut offset = existing / CHUNK;
    let mut written = existing;
    loop {
        let mut ch = session
            .channel_open_session()
            .await
            .expect("open dd channel");
        ch.exec(
            true,
            format!("dd if={remote} bs={CHUNK} skip={offset} count=1 2>/dev/null"),
        )
        .await
        .expect("exec dd");
        let mut buf = Vec::new();
        while let Some(msg) = ch.wait().await {
            match msg {
                ChannelMsg::Data { data } => buf.extend_from_slice(&data),
                ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
                _ => {}
            }
        }
        if buf.is_empty() {
            break;
        }
        f.write_all(&buf).expect("write local");
        written += buf.len() as u64;
        offset += 1;
        if (buf.len() as u64) < CHUNK {
            break;
        }
        if total != 0 && written >= total {
            break;
        }
    }
    written
}

async fn upload_file(session: &DouHandle, local: &str, remote: &str) -> u64 {
    let local_size = std::fs::metadata(local).map(|m| m.len()).unwrap_or(0);
    let remote_size = remote_stat(session, remote).await;
    let start = remote_size.min(local_size);
    let mut f = std::fs::File::open(local).expect("open local for upload");
    f.seek(SeekFrom::Start(start)).expect("seek");
    let mut ch = session
        .channel_open_session()
        .await
        .expect("open cat channel");
    let cmd = if start == 0 {
        format!("cat > {remote}")
    } else {
        format!("cat >> {remote}")
    };
    ch.exec(true, cmd).await.expect("exec cat");
    let mut buf = vec![0u8; CHUNK as usize];
    let mut written = start;
    loop {
        let n = f.read(&mut buf).expect("read local");
        if n == 0 {
            break;
        }
        ch.data(std::io::Cursor::new(buf[..n].to_vec()))
            .await
            .expect("send data");
        written += n as u64;
    }
    ch.eof().await.expect("eof");
    // 等待服务端结束，确保 `cat >`/`cat >>` 已把 stdin 完全落盘，避免读回时竞态。
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    written
}

async fn read_remote_bytes(session: &DouHandle, remote: &str) -> Vec<u8> {
    let mut ch = session
        .channel_open_session()
        .await
        .expect("open cat channel");
    ch.exec(true, format!("cat {remote}")).await.expect("exec cat");
    let mut out = Vec::new();
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Data { data } => out.extend_from_slice(&data),
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    out
}

async fn truncate_remote(session: &DouHandle, remote: &str, size: u64) {
    let mut ch = session
        .channel_open_session()
        .await
        .expect("open truncate channel");
    ch.exec(
        true,
        format!("head -c {size} {remote} > {remote}.tmp && mv -f {remote}.tmp {remote}"),
    )
    .await
    .expect("exec truncate");
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
}

async fn remote_find(session: &DouHandle, root: &str) -> HashMap<String, (u64, i64)> {
    let mut ch = session
        .channel_open_session()
        .await
        .expect("open find channel");
    ch.exec(
        true,
        format!("find {root} -type f -exec stat -c '%s %Y %n' {{}} + 2>/dev/null"),
    )
    .await
    .expect("exec find");
    let mut out = String::new();
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Data { data } => out.push_str(&String::from_utf8_lossy(&data)),
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    let mut map = HashMap::new();
    for line in out.lines() {
        let p: Vec<&str> = line.split_whitespace().collect();
        if p.len() < 3 {
            continue;
        }
        let size = p[0].parse::<u64>().unwrap_or(0);
        let mtime = p[1].parse::<i64>().unwrap_or(0);
        let path = p[2..].join(" ");
        let rel = path.strip_prefix("./").unwrap_or(&path).to_string();
        map.insert(rel, (size, mtime));
    }
    map
}

fn local_find(root: &str) -> HashMap<String, (u64, i64)> {
    let mut map = HashMap::new();
    fn walk(dir: &Path, root: &Path, map: &mut HashMap<String, (u64, i64)>) {
        if let Ok(es) = std::fs::read_dir(dir) {
            for e in es.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, root, map);
                } else if p.is_file() {
                    if let (Ok(m), Some(rel)) = (
                        p.metadata(),
                        p.strip_prefix(root)
                            .ok()
                            .map(|r| r.to_string_lossy().replace('\\', "/")),
                    ) {
                        map.insert(
                            rel,
                            (
                                m.len(),
                                m.modified()
                                    .ok()
                                    .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                                    .map(|d| d.as_secs() as i64)
                                    .unwrap_or(0),
                            ),
                        );
                    }
                }
            }
        }
    }
    walk(Path::new(root), Path::new(root), &mut map);
    map
}

async fn ensure_remote_dir(session: &DouHandle, dir: &str) {
    let mut ch = session
        .channel_open_session()
        .await
        .expect("open mkdir channel");
    ch.exec(true, format!("mkdir -p {dir}"))
        .await
        .expect("exec mkdir");
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
}

async fn sync_push(session: &DouHandle, local_dir: &str, remote_dir: &str) {
    let local = local_find(local_dir);
    let remote = remote_find(session, remote_dir).await;
    for (rel, (ls, lm)) in &local {
        let need = match remote.get(rel) {
            None => true,
            Some((rs, rm)) => *rs != *ls || *rm < *lm,
        };
        if need {
            let lp = format!("{}/{}", local_dir.trim_end_matches('/'), rel);
            let rp = format!("{}/{}", remote_dir.trim_end_matches('/'), rel);
            if let Some(parent) = Path::new(&rp).parent() {
                ensure_remote_dir(session, &parent.to_string_lossy()).await;
            }
            upload_file(session, &lp, &rp).await;
        }
    }
    for rel in remote.keys() {
        if !local.contains_key(rel) {
            let rp = format!("{}/{}", remote_dir.trim_end_matches('/'), rel);
            let mut ch = session
                .channel_open_session()
                .await
                .expect("open rm channel");
            ch.exec(true, format!("rm -f {rp}"))
                .await
                .expect("exec rm");
            while let Some(msg) = ch.wait().await {
                match msg {
                    ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
                    _ => {}
                }
            }
        }
    }
}

async fn sync_pull(session: &DouHandle, remote_dir: &str, local_dir: &str) {
    let remote = remote_find(session, remote_dir).await;
    let local = local_find(local_dir);
    for (rel, (rs, rm)) in &remote {
        let need = match local.get(rel) {
            None => true,
            Some((ls, lm)) => *ls != *rs || *lm < *rm,
        };
        if need {
            let rp = format!("{}/{}", remote_dir.trim_end_matches('/'), rel);
            let lp = format!("{}/{}", local_dir.trim_end_matches('/'), rel);
            download_file(session, &rp, &lp).await;
        }
    }
    for rel in local.keys() {
        if !remote.contains_key(rel) {
            let lp = format!("{}/{}", local_dir.trim_end_matches('/'), rel);
            let _ = std::fs::remove_file(lp);
        }
    }
}

// ------------------------- 测试辅助 -------------------------

fn make_random(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    rand::rng().fill_bytes(&mut buf);
    buf
}

async fn wait_port(host: &str, port: u16) {
    for _ in 0..200 {
        if std::net::TcpStream::connect((host, port)).is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_port_down(host: &str, port: u16) {
    for _ in 0..200 {
        if std::net::TcpStream::connect((host, port)).is_err() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn start_server_retry(
    bind: &str,
    key: &Path,
    root: &Path,
    bash: &str,
) -> russh::server::RunningServerHandle {
    for attempt in 0..20 {
        match start_server(bind, key, root, bash) {
            Ok(h) => return h,
            Err(e) => {
                eprintln!("启动重试 {attempt}: {e}");
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
    }
    panic!("无法启动回环服务");
}

struct Report {
    items: Vec<(String, bool, String)>,
    file: PathBuf,
}

impl Report {
    fn new(file: PathBuf) -> Self {
        Report {
            items: Vec::new(),
            file,
        }
    }
    fn record(&mut self, name: &str, ok: bool, detail: &str) {
        println!(
            "  [{}] {} — {}",
            if ok { "PASS" } else { "FAIL" },
            name,
            detail
        );
        self.items.push((name.to_string(), ok, detail.to_string()));
    }
    fn finish(&self) -> usize {
        let failed = self.items.iter().filter(|(_, ok, _)| !ok).count();
        let mut lines = String::new();
        lines.push_str("==== 受治理远程操作主线 · 端到端自校验汇总 ====\n");
        for (n, ok, d) in &self.items {
            lines.push_str(&format!(
                "{} {}: {}\n",
                if *ok { "✓ PASS" } else { "✗ FAIL" },
                n,
                d
            ));
        }
        lines.push_str(&format!(
            "\n总计 {} 项，通过 {}，失败 {}\n",
            self.items.len(),
            self.items.len() - failed,
            failed
        ));
        println!("\n{lines}");
        if let Some(parent) = self.file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.file, &lines);
        failed
    }
}

// ------------------------- 主流程 -------------------------

#[tokio::main]
async fn main() {
    let tmp = std::env::temp_dir().join("dou_loopback_test");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");

    let host_key_a = tmp.join("host_key_a");
    let host_key_b = tmp.join("host_key_b");
    let client_key = tmp.join("client_key");
    let remote_root = tmp.join("remote");
    let local_root = tmp.join("local");
    let known_hosts = tmp.join("known_hosts.json");
    let report_file = tmp.join("selfcheck_report.txt");
    std::fs::create_dir_all(&remote_root).expect("create remote root");
    std::fs::create_dir_all(&local_root).expect("create local root");

    // 生成客户端 ed25519 私钥（供公钥认证使用）。
    let ckey = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("gen client key");
    std::fs::write(&client_key, ckey.to_openssh(LineEnding::LF).expect("openssh"))
        .expect("write client key");

    let bash = find_git_bash();
    println!("Git Bash: {bash}");
    if bash == "bash" {
        println!("[警告] 未探测到 Git Bash，exec 命令可能不可用（需安装 Git for Windows）。");
    }

    let host = "127.0.0.1";
    let port: u16 = 2222;
    let client_key_str = client_key.to_string_lossy().to_string();

    let mut report = Report::new(report_file);

    // 启动 server A（key A）。
    eprintln!("[阶段] 启动回环服务 A ...");
    let store_a = Arc::new(Mutex::new(HostKeyStore::load(&known_hosts)));
    let handle_a = start_server_retry(
        &format!("{host}:{port}"),
        &host_key_a,
        &remote_root,
        &bash,
    );
    eprintln!("[阶段] 等待端口 {host}:{port} 就绪 ...");
    wait_port(host, port).await;
    eprintln!("[阶段] 端口就绪，开始 A1 首次连接");

    // ---- A1 主机密钥 TOFU 首次 ----
    let (r, v) = ssh_connect(host, port, "dou", &client_key_str, store_a.clone()).await;
    let ok = r.is_ok() && matches!(v, Verdict::FirstSeen(_));
    report.record("A1 主机密钥首次连接(TOFU FirstSeen)", ok, &format!("{v:?}"));

    // ---- B1 重复连接 → KnownMatch ----
    let (r, v) = ssh_connect(host, port, "dou", &client_key_str, store_a.clone()).await;
    let ok = r.is_ok() && matches!(v, Verdict::KnownMatch);
    report.record("B1 重复连接(KnownMatch)+公钥认证", ok, &format!("{v:?}"));

    // 复用一条已认证会话做后续传输测试。
    let sess = match ssh_connect(host, port, "dou", &client_key_str, store_a.clone()).await {
        (Ok(s), _) => s,
        (Err(e), _) => {
            report.record("B2 获取已认证会话", false, &e);
            let failed = report.finish();
            std::process::exit(if failed == 0 { 0 } else { 1 });
        }
    };
    report.record("B2 获取已认证会话(复用)", true, "");

    // ---- C1 上传内容一致（分块） ----
    let data = make_random(200 * 1024);
    let big = local_root.join("big.bin");
    std::fs::write(&big, &data).expect("write big");
    let w = upload_file(&sess, &big.to_string_lossy(), "./big.bin").await;
    let got = read_remote_bytes(&sess, "./big.bin").await;
    let ok = w == data.len() as u64 && got == data;
    report.record(
        "C1 上传(分块 cat > + 内容一致)",
        ok,
        &format!("written={w} expected={}", data.len()),
    );

    // ---- C2 断点续传（远端截断后 cat >> 补齐） ----
    truncate_remote(&sess, "./big.bin", 65536).await;
    let before = remote_stat(&sess, "./big.bin").await;
    let w2 = upload_file(&sess, &big.to_string_lossy(), "./big.bin").await;
    let after = remote_stat(&sess, "./big.bin").await;
    let got2 = read_remote_bytes(&sess, "./big.bin").await;
    let ok = before == 65536
        && after == data.len() as u64
        && got2 == data
        && w2 == data.len() as u64;
    report.record(
        "C2 断点续传(远端截断→cat >> 补齐)",
        ok,
        &format!("before={before} after={after} rewritten={w2}"),
    );

    // ---- D1 下载（dd 分块 + 内容一致） ----
    let dl = local_root.join("dl.bin");
    let w3 = download_file(&sess, "./big.bin", &dl.to_string_lossy()).await;
    let got3 = std::fs::read(&dl).expect("read dl");
    let ok = w3 == data.len() as u64 && got3 == data;
    report.record(
        "D1 下载(dd 分块 + 内容一致)",
        ok,
        &format!("written={w3} chunks>1 验证分块逻辑"),
    );

    // ---- E1 同步 push（上传目录树） ----
    let src = local_root.join("tree_src");
    std::fs::create_dir_all(src.join("sub")).expect("mk sub");
    std::fs::write(src.join("a.txt"), b"alpha").expect("write a");
    std::fs::write(src.join("sub/b.txt"), b"beta").expect("write b");
    sync_push(&sess, &src.to_string_lossy(), ".").await;
    let rm = remote_find(&sess, ".").await;
    let ok = rm.contains_key("a.txt") && rm.contains_key("sub/b.txt");
    report.record(
        "E1 同步push(上传目录树)",
        ok,
        &format!("remote files = {:?}", sorted_keys(&rm)),
    );

    // ---- E2 同步 push（删远端多余） ----
    std::fs::remove_file(src.join("a.txt")).expect("rm a");
    sync_push(&sess, &src.to_string_lossy(), ".").await;
    let rm2 = remote_find(&sess, ".").await;
    let ok = !rm2.contains_key("a.txt") && rm2.contains_key("sub/b.txt");
    report.record(
        "E2 同步push(删远端多余文件)",
        ok,
        &format!("remote files = {:?}", sorted_keys(&rm2)),
    );

    // ---- E3 同步 pull（下载树 + 新增文件） ----
    let mut ch = sess.channel_open_session().await.expect("open");
    ch.exec(true, "printf 'gamma' > c.txt")
        .await
        .expect("exec");
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    let dst = local_root.join("tree_dst");
    std::fs::create_dir_all(&dst).expect("mk dst");
    sync_pull(&sess, ".", &dst.to_string_lossy()).await;
    let ok = std::fs::read(dst.join("c.txt")).map(|b| b == b"gamma").unwrap_or(false)
        && std::fs::read(dst.join("sub/b.txt")).is_ok()
        && std::fs::metadata(dst.join("a.txt")).is_err();
    report.record(
        "E3 同步pull(下载树+新增+不拉已删)",
        ok,
        "",
    );

    // ---- F1 主机密钥变更拒绝（MITM 防护） ----
    // 用独立端口启动「密钥已轮换」的服务（避免 Box::leak 的 listener 占用原端口），
    // 并把原端口学到的旧指纹预置到该端口 → 客户端应检测到 Mismatch 并拒绝握手。
    let port_b: u16 = 2223;
    let old_fp = store_a.lock().unwrap().fingerprint_of(host, port);
    if let Some(fp) = old_fp {
        store_a.lock().unwrap().seed(host, port_b, &fp);
    }
    eprintln!("[阶段] 启动密钥已轮换的服务 B 于端口 {port_b} ...");
    let _handle_b = start_server_retry(
        &format!("{host}:{port_b}"),
        &host_key_b,
        &remote_root,
        &bash,
    );
    wait_port(host, port_b).await;
    let (r, v) = ssh_connect(host, port_b, "dou", &client_key_str, store_a.clone()).await;
    let ok = r.is_err() && matches!(v, Verdict::Mismatch { .. });
    report.record(
        "F1 主机密钥变更拒绝(Mismatch→连接失败)",
        ok,
        &format!("connected_ok={} verdict={v:?}", r.is_ok()),
    );
    handle_a.shutdown("done".into());

    let failed = report.finish();
    println!("证据目录: {tmp:?}");
    std::process::exit(if failed == 0 { 0 } else { 1 });
}

fn sorted_keys<K: Ord + Clone, V>(map: &HashMap<K, V>) -> Vec<K> {
    let mut v: Vec<K> = map.keys().cloned().collect();
    v.sort();
    v
}
