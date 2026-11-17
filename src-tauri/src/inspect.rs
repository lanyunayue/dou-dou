//! C1 主机巡检：只读命令输出的结构化解析（纯函数，不连远端）。
//!
//! 上层在已建立的 SSH 会话上执行 `ps` / `df` / `ss` / `w` 等只读命令，
//! 本模块只负责把 stdout 解析成表格行。解析做发行版兜底：识别不了的行跳过，
//! 而不是整段报错——某条命令在不同发行版格式有差异时，面板降级展示而不是崩掉。
//!
//! 注意：本模块当前只含解析与单测；真实 exec 采集、事件推送、界面渲染
//! 待一台真实 Linux 靶机就绪后再接（见计划 C1）。

use serde::Serialize;

/// 进程表一行（对应 `ps -eo pid,user,%cpu,%mem,etime,cmd`）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProcRow {
    pub pid: u32,
    pub user: String,
    pub cpu_pct: f64,
    pub mem_pct: f64,
    /// 原始 elapsed 串（`MM:SS` / `HH:MM:SS` / `DD-HH:MM:SS`），直接展示用。
    pub etime: String,
    /// 换算成秒，便于排序；解析不出为 0。
    pub etime_s: u64,
    pub cmd: String,
}

/// 磁盘分区一行（对应 `df -h`）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MountRow {
    pub fs: String,
    pub size: String,
    pub used: String,
    pub avail: String,
    /// 使用率 0–100，解析不出为 0。
    pub use_pct: f64,
    pub mounted_on: String,
}

/// 监听端口一行（对应 `ss -tlnp`）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PortRow {
    pub local_addr: String,
    pub port: u16,
    /// 进程名；无 `-p` 权限或远端未给出时为空串。
    pub process: String,
    pub pid: Option<u32>,
}

/// 在线用户一行（对应 `w`）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UserRow {
    pub user: String,
    pub tty: String,
    /// 来源 IP / 主机名；本地登录为 `-`。
    pub from: String,
    pub idle: String,
    pub what: String,
}

/// 解析 `ps -eo pid,user,%cpu,%mem,etime,cmd` 输出。
///
/// 前 5 列是固定字段，剩余空格全部归入命令行；表头行（含 `PID`）跳过。
pub fn parse_ps(text: &str) -> Vec<ProcRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.split_whitespace().next().is_none() {
            continue;
        }
        let mut it = line.split_whitespace();
        let (Some(pid), Some(user), Some(cpu), Some(mem), Some(etime)) =
            (it.next(), it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        // 表头行：PID 解析失败即跳过。
        let Ok(pid) = pid.parse::<u32>() else {
            continue;
        };
        let cpu_pct = cpu.parse::<f64>().unwrap_or(0.0);
        let mem_pct = mem.parse::<f64>().unwrap_or(0.0);
        let cmd = it.collect::<Vec<_>>().join(" ");
        if cmd.is_empty() {
            continue;
        }
        rows.push(ProcRow {
            pid,
            user: user.to_string(),
            cpu_pct,
            mem_pct,
            etime: etime.to_string(),
            etime_s: parse_etime(etime),
            cmd,
        });
    }
    rows
}

/// 把 ps 的 ELAPSED 换算成秒：`DD-HH:MM:SS` / `HH:MM:SS` / `MM:SS`。
fn parse_etime(s: &str) -> u64 {
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<u64>().unwrap_or(0), r),
        None => (0, s),
    };
    let parts: Vec<&str> = rest.split(':').collect();
    let (h, m, sec) = match parts.len() {
        3 => (
            parts[0].parse::<u64>().unwrap_or(0),
            parts[1].parse::<u64>().unwrap_or(0),
            parts[2].parse::<u64>().unwrap_or(0),
        ),
        2 => (
            0,
            parts[0].parse::<u64>().unwrap_or(0),
            parts[1].parse::<u64>().unwrap_or(0),
        ),
        _ => return 0,
    };
    days * 86_400 + h * 3_600 + m * 60 + sec
}

/// 解析 `df -h` 输出。
pub fn parse_df(text: &str) -> Vec<MountRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.split_whitespace().next().is_none() {
            continue;
        }
        let mut it = line.split_whitespace();
        let (Some(fs), Some(size), Some(used), Some(avail), Some(use_pct)) =
            (it.next(), it.next(), it.next(), it.next(), it.next())
        else {
            continue;
        };
        // 表头行：Use% 列不是百分比数字，跳过。
        let Ok(pct) = use_pct.trim_end_matches('%').parse::<f64>() else {
            continue;
        };
        let mounted_on = it.collect::<Vec<_>>().join(" ");
        if mounted_on.is_empty() {
            continue;
        }
        rows.push(MountRow {
            fs: fs.to_string(),
            size: size.to_string(),
            used: used.to_string(),
            avail: avail.to_string(),
            use_pct: pct.clamp(0.0, 100.0),
            mounted_on,
        });
    }
    rows
}

/// 解析 `ss -tlnp` 输出（只取监听行；无 `-p` 权限时进程列为空）。
pub fn parse_ss(text: &str) -> Vec<PortRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() || tokens.first() != Some(&"LISTEN") {
            continue;
        }
        // 字段：State Recv-Q Send-Q Local Peer [Process]
        let Some(local) = tokens.get(3) else { continue };
        let Some((addr, port)) = split_host_port(local) else {
            continue;
        };
        let proc_field = tokens.get(5).copied().unwrap_or("");
        let (process, pid) = parse_ss_process(proc_field);
        rows.push(PortRow {
            local_addr: addr,
            port,
            process,
            pid,
        });
    }
    rows
}

/// 拆 `addr:port`，兼容 IPv6 `[::]:22`。
fn split_host_port(local: &str) -> Option<(String, u16)> {
    let idx = local.rfind(':')?;
    let port: u16 = local[idx + 1..].parse().ok()?;
    let addr = local[..idx].trim_matches('[').trim_matches(']').to_string();
    Some((addr, port))
}

/// 解析 `users:(("name",pid=123,fd=3))`；格式不符返回 (空, None)。
fn parse_ss_process(field: &str) -> (String, Option<u32>) {
    let Some(start) = field.find("(\"") else {
        return (String::new(), None);
    };
    let rest = &field[start + 2..];
    let Some(end) = rest.find('"') else {
        return (String::new(), None);
    };
    let name = rest[..end].to_string();
    let pid = rest[end..]
        .find("pid=")
        .map(|p| &rest[end + p + 4..])
        .and_then(|tail| tail.split(',').next())
        .and_then(|d| d.parse::<u32>().ok());
    (name, pid)
}

/// 解析 `w` 输出：跳过负载首行与表头，剩余按用户/终端/来源/登录/空闲/... 拆。
pub fn parse_w(text: &str) -> Vec<UserRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }
        // 首行含 "load average"；表头首列是 USER。两者都跳过。
        if line.contains("load average") || tokens.first() == Some(&"USER") {
            continue;
        }
        // user tty from login@ idle jcpu pcpu what...
        let (Some(user), Some(tty), Some(from), Some(_login), Some(idle)) = (
            tokens.first(),
            tokens.get(1),
            tokens.get(2),
            tokens.get(3),
            tokens.get(4),
        ) else {
            continue;
        };
        let what = tokens.get(7..).unwrap_or(&[]).join(" ");
        rows.push(UserRow {
            user: (*user).to_string(),
            tty: (*tty).to_string(),
            from: (*from).to_string(),
            idle: (*idle).to_string(),
            what,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const PS_SAMPLE: &str = "\
  PID USER         %CPU %MEM     ELAPSED COMMAND
  1234 www-data    0.5  1.2   10:32:15 nginx: worker process
  567 root         12.3  8.1  01:23:45 /usr/bin/python3 app.py --port 8080
  88  mysql        0.0  3.4  3-04:11:02 mysqld
";

    #[test]
    fn parse_ps_rows() {
        let rows = parse_ps(PS_SAMPLE);
        assert_eq!(rows.len(), 3);
        let first = &rows[0];
        assert_eq!(first.pid, 1234);
        assert_eq!(first.user, "www-data");
        assert!((first.cpu_pct - 0.5).abs() < 0.01);
        assert_eq!(first.cmd, "nginx: worker process");
        assert_eq!(first.etime_s, 10 * 3600 + 32 * 60 + 15);
        // 天-时:分:秒
        assert_eq!(rows[2].etime_s, 3 * 86_400 + 4 * 3_600 + 11 * 60 + 2);
        // 命令行带参数也整体保留
        assert!(rows[1].cmd.contains("--port 8080"));
    }

    #[test]
    fn parse_ps_skips_header_and_garbage() {
        let rows = parse_ps(
            "PID USER %CPU %MEM ELAPSED COMMAND\ngarbage line\n  100 root 1.0 1.0 0:01 sleep 1\n",
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].pid, 100);
    }

    const DF_SAMPLE: &str = "\
Filesystem      Size  Used Avail Use% Mounted on
/dev/sda1        40G   12G   26G  32% /
tmpfs           7.8G     0  7.8G   0% /tmp
/dev/sdb1      500G  120G  355G  26% /data
";

    #[test]
    fn parse_df_rows() {
        let rows = parse_df(DF_SAMPLE);
        assert_eq!(rows.len(), 3);
        let root = &rows[0];
        assert_eq!(root.fs, "/dev/sda1");
        assert_eq!(root.size, "40G");
        assert_eq!(root.mounted_on, "/");
        assert!((root.use_pct - 32.0).abs() < 0.01);
        assert_eq!(rows[1].mounted_on, "/tmp");
        assert_eq!(rows[2].mounted_on, "/data");
    }

    const SS_SAMPLE: &str = "\
State    Recv-Q   Send-Q   Local Address:Port   Peer Address:Port  Process
LISTEN   0        128      0.0.0.0:22          0.0.0.0:*         users:(\"sshd\",pid=842,fd=3)
LISTEN   0        128      [::]:80             [::]:*            users:(\"nginx\",pid=1234,fd=6)
LISTEN   0        4096     127.0.0.1:631       127.0.0.1:*
";

    #[test]
    fn parse_ss_rows() {
        let rows = parse_ss(SS_SAMPLE);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].port, 22);
        assert_eq!(rows[0].local_addr, "0.0.0.0");
        assert_eq!(rows[0].process, "sshd");
        assert_eq!(rows[0].pid, Some(842));
        // IPv6 地址剥括号
        assert_eq!(rows[1].local_addr, "::");
        assert_eq!(rows[1].port, 80);
        assert_eq!(rows[1].process, "nginx");
        // 无 -p 权限：进程空、pid None
        assert_eq!(rows[2].port, 631);
        assert!(rows[2].process.is_empty());
        assert_eq!(rows[2].pid, None);
    }

    const W_SAMPLE: &str = "\
 10:32:15 up 12:34,  2 users,  load average: 0.10, 0.08, 0.05
USER     TTY      FROM             LOGIN@   IDLE   JCPU   PCPU WHAT
wwx      pts/0    10.0.0.5         09:10    1.20s  0.35s  0.05s -bash
root     tty1     -                08:00    3.00  1.00s  0.10s /sbin/init
";

    #[test]
    fn parse_w_rows() {
        let rows = parse_w(W_SAMPLE);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].user, "wwx");
        assert_eq!(rows[0].tty, "pts/0");
        assert_eq!(rows[0].from, "10.0.0.5");
        assert_eq!(rows[0].what, "-bash");
        assert_eq!(rows[1].from, "-");
        assert_eq!(rows[1].what, "/sbin/init");
    }
}
