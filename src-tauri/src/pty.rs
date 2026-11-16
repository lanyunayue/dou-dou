//! Tauri 端 pty 会话：开 SSH pty channel 或本地 Shell 进程，流式推给前端 xterm。

use russh::ChannelMsg;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::monitor;
use crate::ssh;
use zeroize::Zeroizing;

/// 一个 pty 会话。
///
/// 会话 id 即 `PtyState::sessions` 的 key，不另存一份；
/// 连接目标（host/user）在 `open_pty` 入口已打过日志（不含凭据），也无需在会话里冗余留存。
pub struct PtySession {
    pub tx: mpsc::UnboundedSender<String>,
    /// R4：ConPTY 会话的 resize 通道（cols, rows）；SSH 会话与管道回退模式为 None。
    pub resize: Option<mpsc::UnboundedSender<(u16, u16)>>,
}

#[derive(Default)]
pub struct PtyState {
    pub sessions: Mutex<HashMap<String, PtySession>>,
}

/// 开 pty 会话（SSH），返回 session_id。
///
/// `password` 为 Some 时走密码认证（密钥参数被忽略）；None 走私钥路径。
/// 密码只经内存传给 SSH 认证，**不落盘、不写日志**。
pub async fn open_pty(
    app: AppHandle,
    state: &PtyState,
    host: &str,
    port: u16,
    user: &str,
    key_path: &str,
    password: Option<Zeroizing<String>>,
) -> Result<String, String> {
    eprintln!(
        "[pty] open_pty host={host} port={port} user={user} auth={}",
        if password.is_some() {
            "password"
        } else {
            "publickey"
        }
    );
    let session = if let Some(pw) = password {
        ssh::ssh_connect_with_password(app.clone(), host, port, user, pw).await
    } else {
        ssh::ssh_connect(app.clone(), host, port, user, key_path).await
    };
    let session = match session {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[pty] connect failed host={host}:{port} err={e}");
            return Err(e);
        }
    };

    // 会话建立成功：挂上主机监控（exec 失败会自动停表）。
    // russh 0.65 的 Handle 不支持 Clone；channel_open_session 只需 &self，
    // 用 Arc 共享同一会话句柄，会话存活由 channel（内部持有 sender）保持。
    let session = std::sync::Arc::new(session);
    monitor::attach(session.clone(), app.clone());

    let mut channel = session
        .channel_open_session()
        .await
        .map_err(|e| format!("开 channel: {e}"))?;
    channel
        .request_pty(false, "xterm", 80, 24, 640, 480, &[])
        .await
        .map_err(|e| format!("pty: {e}"))?;
    // F-06 长连接保护：优先挂到既有 tmux 会话，没有则新建；
    // 关窗断开后远端 tmux 会话保留，重开窗口可 attach 接管。
    let tmux_cmd = "tmux new-session -A -s dou 2>/dev/null || exec $SHELL";
    channel
        .exec(true, tmux_cmd)
        .await
        .map_err(|e| format!("shell: {e}"))?;

    let sid = uuid::Uuid::new_v4().to_string();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // 后台：读 channel stdout，推事件
    let sid_e = sid.clone();
    let app_e = app.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                maybe_msg = channel.wait() => {
                    match maybe_msg {
                        Some(ChannelMsg::Data { data }) => {
                            let s = String::from_utf8_lossy(&data).to_string();
                            let _ = app_e.emit("pty_data", serde_json::json!({"sid": sid_e, "data": s}));
                        }
                        Some(ChannelMsg::ExtendedData { data, .. }) => {
                            let s = String::from_utf8_lossy(&data).to_string();
                            let _ = app_e.emit("pty_data", serde_json::json!({"sid": sid_e, "data": s}));
                        }
                        Some(ChannelMsg::Eof) | Some(ChannelMsg::ExitStatus { .. }) => {
                            let _ = app_e.emit("pty_exit", serde_json::json!({"sid": sid_e}));
                            break;
                        }
                        _ => {}
                    }
                }
                Some(input) = rx.recv() => {
                    let _ = channel.data(std::io::Cursor::new(input.into_bytes())).await;
                }
                else => break,
            }
        }
    });

    // 存到 state
    {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(sid.clone(), PtySession { tx, resize: None });
    }

    Ok(sid)
}

/// 开本地 Shell 会话（学 MobaXterm 的 Local terminal）：无需远程主机，直接开本机 PowerShell。
///
/// R4：优先 ConPTY 真终端（vim/htop 画面不乱、Ctrl+C 信号完整、resize 同步）；
/// ConPTY 不可用（Win8/1809 以前等）回退 stdin/stdout 管道模式，前端以「基础模式」横幅标注。
/// 返回 `{sid, mode}`，事件协议（pty_data/pty_write/pty_exit）与 SSH 会话完全一致。
pub async fn open_local_pty(app: AppHandle, state: &PtyState) -> Result<serde_json::Value, String> {
    eprintln!("[pty] open_local_pty");
    #[cfg(windows)]
    {
        match open_local_conpty(app.clone(), state) {
            Ok(sid) => {
                eprintln!("[pty] local shell = ConPTY 真终端");
                return Ok(serde_json::json!({ "sid": sid, "mode": "conpty" }));
            }
            Err(e) => {
                eprintln!("[pty] ConPTY 不可用，回退管道模式: {e}");
            }
        }
    }
    let sid = open_local_pipe(app, state).await?;
    Ok(serde_json::json!({ "sid": sid, "mode": "pipe" }))
}

/// ConPTY 真终端：CreatePseudoConsole + 管道转发 + ResizePseudoConsole。
/// 全程同步 Win32 调用放在独立 OS 线程，不阻塞 tokio runtime。
#[cfg(windows)]
fn open_local_conpty(app: AppHandle, state: &PtyState) -> Result<String, String> {
    use std::io::{Read as _, Write as _};
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::SECURITY_ATTRIBUTES;
    use windows::Win32::System::Console::{
        COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole,
    };
    use windows::Win32::System::Pipes::CreatePipe;
    use windows::Win32::System::Threading::{
        CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
        EXTENDED_STARTUPINFO_PRESENT, GetStartupInfoW, INFINITE, InitializeProcThreadAttributeList,
        LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION,
        STARTUPINFOEXW, STARTUPINFOW, UpdateProcThreadAttribute, WaitForSingleObject,
    };
    use windows::core::{BOOL, PCWSTR, PWSTR};

    unsafe {
        // 1. 两对管道桥接 ConPTY（in_r→ConPTY；ConPTY→out_w 后我们读 out_r、写 in_w）
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: BOOL(0),
        };
        let mut in_r = HANDLE::default();
        let mut in_w = HANDLE::default();
        let mut out_r = HANDLE::default();
        let mut out_w = HANDLE::default();
        CreatePipe(&mut in_r, &mut in_w, Some(&sa), 0)
            .map_err(|e| format!("CreatePipe(in): {e}"))?;
        CreatePipe(&mut out_r, &mut out_w, Some(&sa), 0)
            .map_err(|e| format!("CreatePipe(out): {e}"))?;

        // 2. CreatePseudoConsole（初始 80x24，前端 fit 后经 pty_resize 同步）
        let hpc = CreatePseudoConsole(COORD { X: 80, Y: 24 }, in_r, out_w, 0)
            .map_err(|e| format!("CreatePseudoConsole: {e}"))?;
        // PTY 内侧句柄使命完成，立即关闭，进程侧只留 in_w / out_r
        let _ = CloseHandle(in_r);
        let _ = CloseHandle(out_w);

        // 3. attribute list 挂 PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE
        let mut list_size = 0usize;
        let _ = InitializeProcThreadAttributeList(None, 1, None, &mut list_size);
        if list_size == 0 {
            return Err("InitializeProcThreadAttributeList 取大小失败".into());
        }
        let mut list_buf = vec![0u8; list_size];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(list_buf.as_mut_ptr() as *mut core::ffi::c_void);
        InitializeProcThreadAttributeList(Some(list), 1, None, &mut list_size)
            .map_err(|e| format!("InitializeProcThreadAttributeList: {e}"))?;
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            Some(hpc.0 as *const core::ffi::c_void),
            std::mem::size_of::<HPCON>(),
            None,
            None,
        )
        .map_err(|e| format!("UpdateProcThreadAttribute: {e}"))?;

        let mut si = STARTUPINFOW::default();
        GetStartupInfoW(&mut si);
        let mut ex = STARTUPINFOEXW {
            StartupInfo: si,
            lpAttributeList: list,
        };
        ex.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;

        // 4. spawn PowerShell（ConPTY 模式同样显式切 UTF-8，避免中文 Windows GBK 乱码）
        let mut cmdline: Vec<u16> = "powershell.exe -NoLogo -NoProfile -NoExit -Command \"[Console]::InputEncoding=[Text.Encoding]::UTF8; [Console]::OutputEncoding=[Text.Encoding]::UTF8; Set-Location $HOME\""
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut pi = PROCESS_INFORMATION::default();
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(cmdline.as_mut_ptr())),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            None,
            PCWSTR::null(),
            &ex.StartupInfo,
            &mut pi,
        )
        .map_err(|e| format!("启动本地 PowerShell: {e}"))?;
        DeleteProcThreadAttributeList(list);

        let sid = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        let (resize_tx, mut resize_rx) = mpsc::unbounded_channel::<(u16, u16)>();

        // windows crate 0.62 的句柄类型未实现 Send，跨线程一律传 isize 再重建
        let out_raw = out_r.0 as isize;
        let in_raw = in_w.0 as isize;
        let proc_raw = pi.hProcess.0 as isize;

        // 读线程：ConPTY → pty_data（EOF 只退出本线程；pty_exit 由 wait 线程统一发）
        let app_r = app.clone();
        let sid_r = sid.clone();
        std::thread::spawn(move || {
            let mut f =
                std::mem::ManuallyDrop::new(std::fs::File::from_raw_handle(out_raw as RawHandle));
            let mut buf = [0u8; 8192];
            loop {
                match f.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let s = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ =
                            app_r.emit("pty_data", serde_json::json!({ "sid": sid_r, "data": s }));
                    }
                }
            }
        });

        // 写线程：前端输入 → ConPTY（blocking_recv 在 OS 线程阻塞等待）
        std::thread::spawn(move || {
            let mut f =
                std::mem::ManuallyDrop::new(std::fs::File::from_raw_handle(in_raw as RawHandle));
            let mut rx = rx;
            while let Some(input) = rx.blocking_recv() {
                if f.write_all(input.as_bytes()).is_err() {
                    break;
                }
                let _ = f.flush();
            }
        });

        // resize 线程：持有 hpc，ResizePseudoConsole 同步 xterm 尺寸
        std::thread::spawn(move || {
            while let Some((cols, rows)) = resize_rx.blocking_recv() {
                unsafe {
                    let _ = ResizePseudoConsole(
                        hpc,
                        COORD {
                            X: cols as i16,
                            Y: rows as i16,
                        },
                    );
                }
            }
        });

        // wait 线程：进程退出 → ClosePseudoConsole（断开管道）→ pty_exit
        let app_w = app.clone();
        let sid_w = sid.clone();
        std::thread::spawn(move || {
            unsafe {
                let _ = WaitForSingleObject(HANDLE(proc_raw as *mut core::ffi::c_void), INFINITE);
                ClosePseudoConsole(hpc);
            }
            let _ = app_w.emit("pty_exit", serde_json::json!({ "sid": sid_w }));
        });

        {
            let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
            sessions.insert(
                sid.clone(),
                PtySession {
                    tx,
                    resize: Some(resize_tx),
                },
            );
        }
        Ok(sid)
    }
}

/// 管道模式兜底（ConPTY 不可用时）：stdin/stdout/stderr 桥接到同一套事件协议，
/// 交互体验为基础模式（无完整 VT 控制），前端以横幅标注。
async fn open_local_pipe(app: AppHandle, state: &PtyState) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new("powershell");
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-NoExit",
        "-Command",
        // 管道模式显式统一 UTF-8（中文 Windows 默认 GBK 会导致 xterm 乱码），并落到用户目录。
        "[Console]::InputEncoding=[Text.Encoding]::UTF8; [Console]::OutputEncoding=[Text.Encoding]::UTF8; Set-Location $HOME",
    ])
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("启动本地 PowerShell: {e}"))?;

    let mut stdout = child.stdout.take().ok_or("stdout 不可读")?;
    let mut stderr = child.stderr.take().ok_or("stderr 不可读")?;
    let mut stdin = child.stdin.take().ok_or("stdin 不可写")?;

    let sid = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::unbounded_channel::<String>();

    // stdout / stderr → pty_data（两个独立 task，避免单 task select 在一方 EOF 后饿死另一方）
    let app_o = app.clone();
    let sid_o = sid.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_o.emit("pty_data", serde_json::json!({"sid": sid_o, "data": s}));
                }
            }
        }
    });
    let app_e2 = app.clone();
    let sid_e2 = sid.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_e2.emit("pty_data", serde_json::json!({"sid": sid_e2, "data": s}));
                }
            }
        }
    });

    // 前端输入 → stdin
    tokio::spawn(async move {
        let mut rx = rx;
        while let Some(input) = rx.recv().await {
            if stdin.write_all(input.as_bytes()).await.is_err() {
                break;
            }
            let _ = stdin.flush().await;
        }
    });

    // 退出 → pty_exit
    let sid_w = sid.clone();
    let app_w = app.clone();
    tokio::spawn(async move {
        let _ = child.wait().await;
        let _ = app_w.emit("pty_exit", serde_json::json!({"sid": sid_w}));
    });

    {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(sid.clone(), PtySession { tx, resize: None });
    }

    Ok(sid)
}

/// R4：同步终端尺寸到 ConPTY（仅 ConPTY 会话有 resize 通道，其余模式 no-op）。
pub async fn pty_resize(state: &PtyState, sid: &str, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let s = sessions
        .get(sid)
        .ok_or_else(|| format!("session {sid} 不存在"))?;
    if let Some(r) = &s.resize {
        r.send((cols, rows)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 前端输入写回 channel。
pub async fn pty_write(state: &PtyState, sid: &str, data: &str) -> Result<(), String> {
    let sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    let s = sessions
        .get(sid)
        .ok_or_else(|| format!("session {sid} 不存在"))?;
    s.tx.send(data.to_string()).map_err(|e| e.to_string())
}
