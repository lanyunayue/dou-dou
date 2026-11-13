//! 传输队列 + 断点续传 + 文件夹同步（参考 Xftp 的 Xtransport 后台引擎）。
//!
//! 设计：UI 仅发指令（`tq_enqueue` / `tq_pause` / `tq_resume` / `tq_cancel`），
//! 后台单线程处理器（`run_loop`）逐步执行并实时上报进度（`tq_progress` 事件）。
//! 下载/上传均支持分块与断点续传；`sync:*` 任务做递归文件夹镜像/更新。

use crate::ssh;
use crate::transfer;
use russh::ChannelMsg;
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

const CHUNK: u64 = 65536;

#[derive(Clone, Serialize)]
pub struct TransferTask {
    pub id: String,
    pub kind: String, // download | upload | sync:push | sync:pull | sync:update
    pub label: String,
    pub remote: String,
    pub local: String,
    pub total: u64,
    pub done: u64,
    pub status: String, // Queued | Active | Paused | Done | Failed | Canceled
    pub error: Option<String>,
}

#[derive(Default)]
pub struct TransferQueue {
    pub tasks: Mutex<Vec<TransferTask>>,
    pub control: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub app: Mutex<Option<AppHandle>>,
    pub started: Mutex<bool>,
}

pub fn list(q: &TransferQueue) -> Vec<TransferTask> {
    q.tasks.lock().unwrap().clone()
}

fn with_task<F: FnOnce(&mut TransferTask)>(q: &TransferQueue, id: &str, f: F) {
    if let Ok(mut t) = q.tasks.lock() {
        if let Some(t) = t.iter_mut().find(|t| t.id == id) {
            f(t);
        }
    }
}

fn emit_progress(q: &TransferQueue) {
    if let Some(app) = q.app.lock().unwrap().clone() {
        let _ = app.emit("tq_progress", list(q));
    }
}

fn app_handle(q: &TransferQueue) -> Result<AppHandle, String> {
    q.app
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "队列未绑定 AppHandle".into())
}

pub fn enqueue(
    q: &Arc<TransferQueue>,
    app: AppHandle,
    kind: &str,
    label: &str,
    remote: &str,
    local: &str,
) -> String {
    *q.app.lock().unwrap() = Some(app.clone());
    let id = uuid::Uuid::new_v4().to_string();
    q.control
        .lock()
        .unwrap()
        .insert(id.clone(), Arc::new(AtomicBool::new(false)));
    q.tasks.lock().unwrap().push(TransferTask {
        id: id.clone(),
        kind: kind.to_string(),
        label: label.to_string(),
        remote: remote.to_string(),
        local: local.to_string(),
        total: 0,
        done: 0,
        status: "Queued".into(),
        error: None,
    });
    ensure_runner(q.clone());
    id
}

pub fn cancel(q: &TransferQueue, id: &str) {
    if let Some(f) = q.control.lock().unwrap().get(id) {
        f.store(true, Ordering::SeqCst);
    }
    with_task(q, id, |t| {
        if t.status != "Done" && t.status != "Failed" {
            t.status = "Canceled".into();
        }
    });
    emit_progress(q);
}

pub fn pause(q: &TransferQueue, id: &str) {
    with_task(q, id, |t| {
        if t.status == "Queued" || t.status == "Active" {
            t.status = "Paused".into();
        }
    });
    emit_progress(q);
}

pub fn resume(q: &TransferQueue, id: &str) {
    with_task(q, id, |t| {
        if t.status == "Paused" {
            t.status = "Queued".into();
        }
    });
    emit_progress(q);
}

pub fn clear_finished(q: &TransferQueue) {
    q.tasks
        .lock()
        .unwrap()
        .retain(|t| t.status != "Done" && t.status != "Failed" && t.status != "Canceled");
    emit_progress(q);
}

fn ensure_runner(q: Arc<TransferQueue>) {
    {
        let mut started = q.started.lock().unwrap();
        if *started {
            return;
        }
        *started = true;
    }
    tokio::spawn(run_loop(q));
}

async fn run_loop(q: Arc<TransferQueue>) {
    loop {
        let next = {
            let mut tasks = q.tasks.lock().unwrap();
            if let Some(t) = tasks.iter_mut().find(|t| t.status == "Queued") {
                t.status = "Active".into();
                Some(t.id.clone())
            } else {
                None
            }
        };
        match next {
            Some(id) => {
                let res = run_one(&q, &id).await;
                with_task(&q, &id, |t| {
                    if let Err(e) = &res {
                        if t.status != "Paused" && t.status != "Canceled" {
                            t.status = "Failed".into();
                            t.error = Some(e.clone());
                        }
                    } else if t.status != "Paused" && t.status != "Canceled" {
                        t.status = "Done".into();
                        t.done = t.total;
                    }
                });
                emit_progress(&q);
            }
            None => tokio::time::sleep(std::time::Duration::from_millis(300)).await,
        }
    }
}

async fn run_one(q: &Arc<TransferQueue>, id: &str) -> Result<(), String> {
    let (kind, remote, local) = {
        let tasks = q.tasks.lock().unwrap();
        let t = tasks.iter().find(|t| t.id == id).ok_or("任务不存在")?;
        (t.kind.clone(), t.remote.clone(), t.local.clone())
    };
    if let Some(mode) = kind.strip_prefix("sync:") {
        run_sync(q, id, &local, &remote, mode).await
    } else if kind == "download" {
        run_download(q, id, &remote, &local).await
    } else if kind == "upload" {
        run_upload(q, id, &remote, &local).await
    } else {
        Err(format!("未知传输类型: {kind}"))
    }
}

fn canceled(q: &TransferQueue, id: &str) -> bool {
    q.control
        .lock()
        .unwrap()
        .get(id)
        .map(|f| f.load(Ordering::SeqCst))
        .unwrap_or(false)
}

fn is_paused(q: &TransferQueue, id: &str) -> bool {
    q.tasks
        .lock()
        .unwrap()
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.status == "Paused")
        .unwrap_or(false)
}

async fn run_download(
    q: &Arc<TransferQueue>,
    id: &str,
    remote: &str,
    local: &str,
) -> Result<(), String> {
    let t = ssh::ssh_target();
    let app = app_handle(q)?;
    let mut session = ssh::ssh_connect(app, &t.host, t.port, &t.user, &t.key_path).await?;
    let (total, _) = transfer::remote_stat_session(&session, remote)
        .await
        .unwrap_or((0, 0));
    with_task(q, id, |x| x.total = total);
    emit_progress(q);

    let local_path = Path::new(local);
    if let Some(parent) = local_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let existing = std::fs::metadata(local_path).map(|m| m.len()).unwrap_or(0);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(local_path)
        .map_err(|e| e.to_string())?;
    let mut offset = existing / CHUNK;
    let mut written = existing;

    loop {
        if canceled(q, id) {
            with_task(q, id, |x| x.status = "Canceled".into());
            return Ok(());
        }
        if is_paused(q, id) {
            with_task(q, id, |x| x.status = "Paused".into());
            return Ok(());
        }
        let mut channel = session
            .channel_open_session()
            .await
            .map_err(|e| e.to_string())?;
        channel
            .exec(
                true,
                format!("dd if={remote} bs={CHUNK} skip={offset} count=1 2>/dev/null"),
            )
            .await
            .map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => buf.extend_from_slice(&data),
                ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
                _ => {}
            }
        }
        if buf.is_empty() {
            break;
        }
        f.write_all(&buf).map_err(|e| e.to_string())?;
        written += buf.len() as u64;
        offset += 1;
        with_task(q, id, |x| x.done = written);
        emit_progress(q);
        if (buf.len() as u64) < CHUNK {
            break;
        }
        if total != 0 && written >= total {
            break;
        }
    }
    with_task(q, id, |x| {
        x.done = written;
        x.total = written.max(x.total);
    });
    emit_progress(q);
    Ok(())
}

async fn run_upload(
    q: &Arc<TransferQueue>,
    id: &str,
    remote: &str,
    local: &str,
) -> Result<(), String> {
    let t = ssh::ssh_target();
    let app = app_handle(q)?;
    let mut session = ssh::ssh_connect(app, &t.host, t.port, &t.user, &t.key_path).await?;
    let local_size = std::fs::metadata(local).map(|m| m.len()).unwrap_or(0);
    let remote_size = transfer::remote_stat_session(&session, remote)
        .await
        .map(|(s, _)| s)
        .unwrap_or(0);
    let start = remote_size.min(local_size);
    with_task(q, id, |x| {
        x.total = local_size;
        x.done = start;
    });
    emit_progress(q);

    let mut f = std::fs::File::open(local).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut written = start;

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    // 续传用 append，首传用 truncate（cat >）
    let cmd = if start == 0 {
        format!("cat > {remote}")
    } else {
        format!("cat >> {remote}")
    };
    channel.exec(true, cmd).await.map_err(|e| e.to_string())?;

    let mut chunk_buf = vec![0u8; CHUNK as usize];
    loop {
        if canceled(q, id) {
            with_task(q, id, |x| x.status = "Canceled".into());
            let _ = channel.eof().await;
            return Ok(());
        }
        if is_paused(q, id) {
            with_task(q, id, |x| x.status = "Paused".into());
            let _ = channel.eof().await;
            return Ok(());
        }
        let n = f.read(&mut chunk_buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        channel
            .data(std::io::Cursor::new(chunk_buf[..n].to_vec()))
            .await
            .map_err(|e| e.to_string())?;
        written += n as u64;
        with_task(q, id, |x| x.done = written);
        emit_progress(q);
    }
    channel.eof().await.map_err(|e| e.to_string())?;
    with_task(q, id, |x| x.done = written);
    emit_progress(q);
    Ok(())
}

// ------------------------- 文件夹同步 -------------------------

/// 在会话上递归列远端文件：`find root -type f -exec stat ...`，返回 (relpath, size, mtime)。
async fn remote_find(
    session: &ssh::DouHandle,
    root: &str,
) -> Result<HashMap<String, (u64, i64)>, String> {
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .exec(
            true,
            format!("find {root} -type f -exec stat -c '%s %Y %n' {{}} + 2>/dev/null"),
        )
        .await
        .map_err(|e| e.to_string())?;
    let mut out = String::new();
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { data } => out.push_str(&String::from_utf8_lossy(&data)),
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    let mut map = HashMap::new();
    let root = root.trim_end_matches('/');
    for line in out.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let size = parts[0].parse::<u64>().unwrap_or(0);
        let mtime = parts[1].parse::<i64>().unwrap_or(0);
        let path = parts[2..].join(" ");
        let rel = path
            .strip_prefix(&format!("{root}/"))
            .unwrap_or(&path)
            .to_string();
        map.insert(rel, (size, mtime));
    }
    Ok(map)
}

fn local_find(root: &str) -> HashMap<String, (u64, i64)> {
    let mut map = HashMap::new();
    let root_path = Path::new(root);
    fn walk(dir: &Path, root: &Path, map: &mut HashMap<String, (u64, i64)>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, root, map);
                } else if p.is_file() {
                    if let (Ok(meta), Some(rel)) = (
                        p.metadata(),
                        p.strip_prefix(root)
                            .ok()
                            .map(|r| r.to_string_lossy().replace('\\', "/")),
                    ) {
                        map.insert(
                            rel,
                            (
                                meta.len(),
                                meta.modified()
                                    .ok()
                                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                                    .map(|d| d.as_secs() as i64)
                                    .unwrap_or(0),
                            ),
                        );
                    }
                }
            }
        }
    }
    walk(root_path, root_path, &mut map);
    map
}

async fn ensure_remote_dir(session: &ssh::DouHandle, dir: &str) -> Result<(), String> {
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    channel
        .exec(true, format!("mkdir -p {dir}"))
        .await
        .map_err(|e| e.to_string())?;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
            _ => {}
        }
    }
    Ok(())
}

async fn upload_file_session(
    session: &ssh::DouHandle,
    local: &str,
    remote: &str,
) -> Result<(), String> {
    let local_size = std::fs::metadata(local).map(|m| m.len()).unwrap_or(0);
    let remote_size = transfer::remote_stat_session(session, remote)
        .await
        .map(|(s, _)| s)
        .unwrap_or(0);
    let start = remote_size.min(local_size);
    let mut f = std::fs::File::open(local).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| e.to_string())?;
    let cmd = if start == 0 {
        format!("cat > {remote}")
    } else {
        format!("cat >> {remote}")
    };
    channel.exec(true, cmd).await.map_err(|e| e.to_string())?;
    let mut chunk_buf = vec![0u8; CHUNK as usize];
    let mut r = std::io::BufReader::new(f);
    loop {
        let n = r.read(&mut chunk_buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        channel
            .data(std::io::Cursor::new(chunk_buf[..n].to_vec()))
            .await
            .map_err(|e| e.to_string())?;
    }
    channel.eof().await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn download_file_session(
    session: &ssh::DouHandle,
    remote: &str,
    local: &str,
) -> Result<(), String> {
    if let Some(parent) = Path::new(local).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let existing = std::fs::metadata(local).map(|m| m.len()).unwrap_or(0);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(local)
        .map_err(|e| e.to_string())?;
    let mut offset = existing / CHUNK;
    loop {
        let mut channel = session
            .channel_open_session()
            .await
            .map_err(|e| e.to_string())?;
        channel
            .exec(
                true,
                format!("dd if={remote} bs={CHUNK} skip={offset} count=1 2>/dev/null"),
            )
            .await
            .map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => buf.extend_from_slice(&data),
                ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
                _ => {}
            }
        }
        if buf.is_empty() {
            break;
        }
        f.write_all(&buf).map_err(|e| e.to_string())?;
        offset += 1;
        if (buf.len() as u64) < CHUNK {
            break;
        }
    }
    Ok(())
}

async fn run_sync(
    q: &Arc<TransferQueue>,
    id: &str,
    local_dir: &str,
    remote_dir: &str,
    mode: &str,
) -> Result<(), String> {
    // mode: push（本地→远端镜像）| pull（远端→本地镜像）| update（仅更优，不删）
    let t = ssh::ssh_target();
    let app = app_handle(q)?;
    let mut session = ssh::ssh_connect(app, &t.host, t.port, &t.user, &t.key_path).await?;

    // 三种模式语义：push=本地→远端镜像(删远端多余)；pull=远端→本地镜像(删本地多余)；
    // update=按更优方向同步(只传更新的)，不删除任一侧多余文件。
    let (push, do_delete) = match mode {
        "pull" => (false, true),
        "update" => (true, false),
        _ => (true, true), // push 及未知默认按 push 镜像
    };

    let local_map = local_find(local_dir);
    let remote_map = remote_find(&session, remote_dir).await?;

    // 收集需要传输的文件（相对路径）
    let mut to_transfer: Vec<String> = Vec::new();
    if push {
        // 本地有、远端缺/更旧/大小不同 → 上传
        for (rel, (lsize, lmtime)) in &local_map {
            match remote_map.get(rel) {
                None => to_transfer.push(rel.clone()),
                Some((rsize, rmtime)) => {
                    if *rsize != *lsize || *rmtime < *lmtime {
                        to_transfer.push(rel.clone());
                    }
                }
            }
        }
        if do_delete {
            for rel in remote_map.keys() {
                if !local_map.contains_key(rel) {
                    to_transfer.push(format!("__DEL__{rel}"));
                }
            }
        }
    } else {
        // pull：远端有、本地缺/更旧 → 下载
        for (rel, (rsize, rmtime)) in &remote_map {
            match local_map.get(rel) {
                None => to_transfer.push(rel.clone()),
                Some((lsize, lmtime)) => {
                    if *lsize != *rsize || *lmtime < *rmtime {
                        to_transfer.push(rel.clone());
                    }
                }
            }
        }
        if do_delete {
            for rel in local_map.keys() {
                if !remote_map.contains_key(rel) {
                    to_transfer.push(format!("__DEL__{rel}"));
                }
            }
        }
    }

    let total = to_transfer.len() as u64;
    with_task(q, id, |x| x.total = total);
    emit_progress(q);
    let mut done = 0u64;

    for item in &to_transfer {
        if canceled(q, id) {
            with_task(q, id, |x| x.status = "Canceled".into());
            return Ok(());
        }
        if is_paused(q, id) {
            with_task(q, id, |x| x.status = "Paused".into());
            return Ok(());
        }
        if let Some(rel) = item.strip_prefix("__DEL__") {
            if push {
                let rp = format!("{}/{}", remote_dir.trim_end_matches('/'), rel);
                let mut ch = session
                    .channel_open_session()
                    .await
                    .map_err(|e| e.to_string())?;
                ch.exec(true, format!("rm -f {rp}"))
                    .await
                    .map_err(|e| e.to_string())?;
                while let Some(m) = ch.wait().await {
                    match m {
                        ChannelMsg::Eof | ChannelMsg::ExitStatus { .. } => break,
                        _ => {}
                    }
                }
            } else {
                let lp = format!("{}/{}", local_dir.trim_end_matches('/'), rel);
                let _ = std::fs::remove_file(lp);
            }
        } else if push {
            let lp = format!("{}/{}", local_dir.trim_end_matches('/'), item);
            let rp = format!("{}/{}", remote_dir.trim_end_matches('/'), item);
            if let Some(parent) = Path::new(&rp).parent() {
                ensure_remote_dir(&session, &parent.to_string_lossy()).await?;
            }
            upload_file_session(&session, &lp, &rp).await?;
        } else {
            let rp = format!("{}/{}", remote_dir.trim_end_matches('/'), item);
            let lp = format!("{}/{}", local_dir.trim_end_matches('/'), item);
            download_file_session(&session, &rp, &lp).await?;
        }
        done += 1;
        with_task(q, id, |x| x.done = done);
        emit_progress(q);
    }
    Ok(())
}
