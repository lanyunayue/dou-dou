//! 会话侧实验快照与三段指纹（计划书 F-24 / F-25）。
//!
//! 三段指纹口径（与计划书附录 C 一致，不是 host/user/sid）：
//!   1. 命令内容哈希
//!   2. 参数规范化串哈希（去空白、排序后的参数）
//!   3. 数据集路径哈希（命令中出现的路径参数）
//! 三段拼接后再做一次 sha256，作为快照指纹；同一脚本换数据集视为不同实验。
//!
//! 被动采集：命令原文、工作目录、进程号、GPU 占用、日志尾部；不要求用户改脚本。

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::store;

fn sha256_hex(data: &str) -> String {
    let mut h = Sha256::new();
    h.update(data.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// 从命令里提取路径类参数（数据集/产物目录）。
fn extract_dataset_path(cmd: &str) -> String {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    tokens
        .iter()
        .filter(|t| {
            t.starts_with('/') || t.starts_with("./") || t.starts_with("../") || t.contains('/')
        })
        .last()
        .copied()
        .unwrap_or("")
        .to_string()
}

/// 参数规范化：去掉选项（- 开头）与命令本身，剩余参数排序后拼接。
fn normalize_args(cmd: &str) -> String {
    let mut args: Vec<&str> = cmd
        .split_whitespace()
        .filter(|t| !t.starts_with('-'))
        .collect();
    if !args.is_empty() {
        args.remove(0); // 去掉命令头
    }
    args.sort();
    args.join(" ")
}

/// 计算三段指纹。返回 (指纹, 命令哈希, 参数哈希, 数据集哈希)。
pub fn fingerprint(command: &str) -> (String, String, String, String) {
    let cmd_h = sha256_hex(command);
    let arg_h = sha256_hex(&normalize_args(command));
    let ds = extract_dataset_path(command);
    let ds_h = sha256_hex(&ds);
    let combined = sha256_hex(&format!("{cmd_h}|{arg_h}|{ds_h}"));
    (combined, cmd_h, arg_h, ds_h)
}

/// 采集上下文：从 exec 结果与补采命令解析。
pub struct Context {
    pub cwd: String,
    pub pid: String,
    pub gpu: String,
    pub log_tail: String,
}

impl Context {
    pub fn empty() -> Self {
        Self {
            cwd: String::new(),
            pid: String::new(),
            gpu: String::new(),
            log_tail: String::new(),
        }
    }

    /// 从聚合补采输出解析：`pwd\n---\nps 行\n---\ngpu 行`
    pub fn from_probe_output(out: &str, log_tail: &str) -> Self {
        let mut parts = out.split("---");
        let cwd = parts.next().unwrap_or("").trim().to_string();
        let ps_line = parts.next().unwrap_or("").trim().to_string();
        let gpu = parts.next().unwrap_or("").trim().to_string();
        let pid = ps_line.split_whitespace().next().unwrap_or("").to_string();
        Self {
            cwd,
            pid,
            gpu,
            log_tail: log_tail
                .chars()
                .rev()
                .take(400)
                .collect::<String>()
                .chars()
                .rev()
                .collect(),
        }
    }
}

/// 落一条快照；若命中历史指纹，返回历史记录用于重复提示。
pub fn record(
    session_id: &str,
    project_id: Option<&str>,
    command: &str,
    ctx: &Context,
    result_dir: &str,
) -> Result<Value, String> {
    let (fp, _cmd_h, _arg_h, _ds_h) = fingerprint(command);
    let dataset = extract_dataset_path(command);

    // 先查历史（重复实验提示）
    let prior = store::find_snapshot_by_fp(&fp).unwrap_or(None);

    // id 带毫秒时间戳后缀：同指纹重跑各占一行，重复实验由 runs 聚合与 prior 查询发现。
    let id = format!("snap_{}_{}", &fp[..12], store::now_ms());
    store::upsert_snapshot(
        &id,
        project_id,
        Some(session_id),
        command,
        &normalize_args(command),
        &dataset,
        &fp,
        result_dir,
    )?;

    let mut out = json!({
        "snapshot_id": id,
        "fingerprint": fp,
        "dataset_path": dataset,
        "cwd": ctx.cwd,
        "pid": ctx.pid,
        "gpu": ctx.gpu,
        "log_tail": ctx.log_tail,
        "duplicate": prior.is_some(),
    });
    if let Some(p) = prior {
        out["prior"] = p;
    }
    Ok(out)
}

/// 补采命令：cwd / pid / GPU。在远端执行一次，结果交给 Context::from_probe_output。
pub fn probe_command() -> &'static str {
    "pwd; echo ---; ps -o pid= -p $$ 2>/dev/null; echo ---; nvidia-smi --query-gpu=memory.used,utilization.gpu --format=csv,noheader 2>/dev/null || echo 'no-gpu'"
}
