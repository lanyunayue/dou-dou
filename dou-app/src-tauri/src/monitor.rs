//! 主机监控：SSH 会话建立后周期采集 CPU / 内存 / 网络速率，事件推给前端。
//!
//! 学 MobaXterm 的服务器监控条行为：数据全部来自单次 exec 内嵌两次采样的
//! `/proc` 快照（不依赖远端安装任何 agent），会话断开（exec 失败）即自动停表。
//! CPU 用命令内 `sleep 0.6` 前后两次 `/proc/stat` 求差；网络速率用两次采集
//! 之间的真实时钟差与累计字节差求出。

use russh::ChannelMsg;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use crate::ssh::DouHandle;

/// 一轮采集的结构化结果（事件 `host_stats` 的 payload）。
#[derive(Clone, serde::Serialize)]
pub struct HostStats {
    pub cpu_pct: f64,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub rx_kbps: f64,
    pub tx_kbps: f64,
    pub uptime_s: u64,
}

/// 单次 exec 内完成两次 CPU 采样 + 一次内存/网卡/运行时长快照。
const SAMPLE_CMD: &str = "head -1 /proc/stat; sleep 0.6; head -1 /proc/stat; \
     grep -E '^(MemTotal|MemAvailable)' /proc/meminfo; \
     tail -n +3 /proc/net/dev; cat /proc/uptime";

struct MonInner {
    /// russh 0.65 的 `Handle` 不支持 Clone，且 `channel_open_session` 只需
    /// `&self`：pty 与监控条用 `Arc` 共享同一会话句柄。
    handle: Arc<DouHandle>,
    /// 上轮采集的（rx 字节，tx 字节，时刻），用于求速率；首轮为 None。
    last_net: Option<(u64, u64, Instant)>,
}

static MON: OnceLock<Mutex<Option<MonInner>>> = OnceLock::new();
static RUNNING: AtomicBool = AtomicBool::new(false);

fn mon() -> &'static Mutex<Option<MonInner>> {
    MON.get_or_init(|| Mutex::new(None))
}

/// pty 会话建立成功后挂上监控：替换句柄，未在跑则启动采集循环。
pub fn attach(handle: Arc<DouHandle>, app: AppHandle) {
    *mon().lock().unwrap_or_else(|e| e.into_inner()) = Some(MonInner {
        handle,
        last_net: None,
    });
    if !RUNNING.swap(true, Ordering::SeqCst) {
        tokio::spawn(monitor_loop(app));
    }
}

async fn monitor_loop(app: AppHandle) {
    loop {
        let handle = {
            let guard = mon().lock().unwrap_or_else(|e| e.into_inner());
            guard.as_ref().map(|m| m.handle.clone())
        };
        let Some(h) = handle else { break };
        match collect(&h).await {
            Ok((stats, rx, tx)) => {
                let now = Instant::now();
                let mut guard = mon().lock().unwrap_or_else(|e| e.into_inner());
                let Some(m) = guard.as_mut() else { break };
                let mut s = stats;
                if let Some((lr, lt, lt_t)) = m.last_net {
                    let dt = lt_t.elapsed().as_secs_f64();
                    if dt > 1.0 {
                        s.rx_kbps = rx.saturating_sub(lr) as f64 / dt / 1024.0;
                        s.tx_kbps = tx.saturating_sub(lt) as f64 / dt / 1024.0;
                    }
                }
                m.last_net = Some((rx, tx, now));
                let _ = app.emit("host_stats", serde_json::json!(s));
            }
            Err(_) => {
                // R7：失败先区分「主机不支持」与「会话断开」——非 Linux（无 /proc）给
                // 边界说明而非静默隐藏；探测失败/远端就是 Linux 则按会话断开处理。
                match probe_os(&h).await {
                    Some(os) if !os.contains("Linux") => {
                        let _ = app.emit(
                            "host_stats_unsupported",
                            serde_json::json!({ "os": os.chars().take(60).collect::<String>() }),
                        );
                    }
                    _ => {
                        let _ = app.emit("host_stats_off", ());
                    }
                }
                mon().lock().unwrap_or_else(|e| e.into_inner()).take();
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    RUNNING.store(false, Ordering::SeqCst);
}

/// R7：远端系统探测（exec `uname -s`），用于区分「非 Linux 主机」与「会话断开」。
/// Windows 主机普遍无 uname，shell 的报错信息会出现在 stderr（ExtendedData）里，
/// 同样视为「非 Linux」。探测失败（channel 断开等）返回 None，按会话断开处理。
async fn probe_os(h: &DouHandle) -> Option<String> {
    let mut ch = h.channel_open_session().await.ok()?;
    ch.exec(true, "uname -s").await.ok()?;

    let mut out = String::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let msg = tokio::time::timeout_at(deadline.into(), ch.wait())
            .await
            .ok()?;
        match msg {
            Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                out.push_str(&String::from_utf8_lossy(&data));
            }
            Some(ChannelMsg::Eof) | Some(ChannelMsg::ExitStatus { .. }) | None => break,
            _ => {}
        }
    }
    let s = out.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

async fn collect(h: &DouHandle) -> Result<(HostStats, u64, u64), String> {
    let mut ch = h
        .channel_open_session()
        .await
        .map_err(|e| format!("开采集 channel: {e}"))?;
    ch.exec(true, SAMPLE_CMD)
        .await
        .map_err(|e| format!("采集 exec: {e}"))?;

    let mut out = String::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let msg = tokio::time::timeout_at(deadline.into(), ch.wait())
            .await
            .map_err(|_| "采集超时".to_string())?;
        match msg {
            Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                out.push_str(&String::from_utf8_lossy(&data));
            }
            Some(ChannelMsg::Eof) | Some(ChannelMsg::ExitStatus { .. }) | None => break,
            _ => {}
        }
    }
    parse_sample(&out).map_err(|e| format!("解析 /proc 采样: {e}"))
}

/// 解析 SAMPLE_CMD 的输出。顺序固定：stat1 / stat2 / meminfo×2 / netdev×N / uptime。
fn parse_sample(text: &str) -> Result<(HostStats, u64, u64), String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());

    let stat1 = parse_cpu_line(lines.next().ok_or("缺少 /proc/stat 首次采样")?)?;
    let stat2 = parse_cpu_line(lines.next().ok_or("缺少 /proc/stat 二次采样")?)?;
    let (d_total, d_idle) = (
        stat2.1 as f64 - stat1.1 as f64,
        stat2.0 as f64 - stat1.0 as f64,
    );
    let cpu_pct = if d_total > 0.0 {
        (100.0 * (1.0 - d_idle / d_total)).clamp(0.0, 100.0)
    } else {
        0.0
    };

    let mut mem_total = 0u64;
    let mut mem_avail = 0u64;
    for line in lines.by_ref() {
        if let Some((k, v)) = line.split_once(':') {
            let v: u64 = v
                .trim()
                .split_whitespace()
                .next()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0);
            match k.trim() {
                "MemTotal" => mem_total = v,
                "MemAvailable" => mem_avail = v,
                _ => break,
            }
        } else {
            break;
        }
        if mem_total > 0 && mem_avail > 0 {
            break;
        }
    }
    if mem_total == 0 {
        return Err("缺少 MemTotal".into());
    }

    // /proc/net/dev：`iface: rx_bytes packets ... | tx_bytes packets ...`
    // rx bytes 是冒号后第 1 个数值，tx bytes 是第 9 个。跳过 lo 回环。
    let mut rx_total = 0u64;
    let mut tx_total = 0u64;
    let mut has_net = false;
    let mut uptime_s = 0u64;
    for line in lines.by_ref() {
        if let Some((iface, rest)) = line.split_once(':') {
            if iface.trim() == "lo" {
                continue;
            }
            let nums: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|x| x.parse().ok())
                .collect();
            if nums.len() >= 9 {
                rx_total += nums[0];
                tx_total += nums[8];
                has_net = true;
            }
        } else if let Ok(u) = line
            .trim()
            .split_whitespace()
            .next()
            .unwrap_or("")
            .parse::<f64>()
        {
            uptime_s = u as u64;
        }
    }
    if !has_net {
        return Err("缺少 /proc/net/dev 数据".into());
    }

    let stats = HostStats {
        cpu_pct,
        mem_total_mb: mem_total / 1024,
        mem_used_mb: mem_total.saturating_sub(mem_avail) / 1024,
        rx_kbps: 0.0,
        tx_kbps: 0.0,
        uptime_s,
    };
    Ok((stats, rx_total, tx_total))
}

/// `/proc/stat` cpu 行 → (idle+iowait, 总 jiffies)。字段：user nice system idle
/// iowait irq softirq steal guest guest_nice。
fn parse_cpu_line(line: &str) -> Result<(u64, u64), String> {
    let nums: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|x| x.parse().ok())
        .collect();
    if nums.len() < 5 {
        return Err(format!("cpu 行字段不足: {line}"));
    }
    let idle = nums[3] + nums.get(4).copied().unwrap_or(0);
    let total: u64 = nums.iter().sum();
    Ok((idle, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
cpu  100 0 100 800 0 0 0 0 0 0
cpu  200 0 100 800 0 0 0 0 0 0
MemTotal:       4096000 kB
MemAvailable:   2048000 kB
  eth0: 1048576 100 0 0 0 0 0 0 524288 50 0 0 0 0 0 0
  lo: 100 1 0 0 0 0 0 0 100 1 0 0 0 0 0 0
12345.67 23456.78
";

    #[test]
    fn parse_sample_full() {
        let (s, rx, tx) = parse_sample(SAMPLE).expect("样本应可解析");
        // CPU：total delta = (200+100+800) - (100+100+800) = 100，idle delta = 0 → 100%
        assert!((s.cpu_pct - 100.0).abs() < 0.01);
        assert_eq!(s.mem_total_mb, 4000);
        assert_eq!(s.mem_used_mb, 2000);
        // rx=1048576（1MiB），tx=524288；lo 被跳过
        assert_eq!(rx, 1048576);
        assert_eq!(tx, 524288);
        assert_eq!(s.uptime_s, 12345);
    }

    #[test]
    fn parse_cpu_line_partial_fields() {
        // 5 字段（user nice system idle iowait）是最小可解析形态
        let (idle, total) = parse_cpu_line("cpu  10 20 30 40 0").expect("5 字段应可解析");
        assert_eq!(idle, 40);
        assert_eq!(total, 100);
        // 缺 iowait 会把 idle 计算错位，必须报错
        assert!(
            parse_cpu_line("cpu  10 20 30 40").is_err(),
            "少于 5 字段应报错"
        );
    }

    #[test]
    fn parse_sample_missing_mem_errors() {
        let bad = "cpu  1 2 3 4 5\ncpu  2 3 4 5 6\n  eth0: 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1 1\n";
        assert!(parse_sample(bad).is_err());
    }
}
