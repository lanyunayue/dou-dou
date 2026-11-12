//! 任务与审批闭环（W-11 状态写穿）。
//!
//! 计划书 4.9：变更类命令不直接执行，而是挂起成 task，等人工批准/拒绝。
//! 本轮升级：每次状态变更**先落库再更新内存**，进程重启后从 `task_state` 恢复，
//! 待审任务不再因重启丢失。`sessions` 因含 `russh::client::Handle`
//! 仍只作运行时句柄缓存，重启需重连（见 `state.rs`）。

use crate::state;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    PendingApproval,
    Approved,
    Rejected,
    Running,
    Done,
    Failed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::PendingApproval => "pending_approval",
            TaskStatus::Approved => "approved",
            TaskStatus::Rejected => "rejected",
            TaskStatus::Running => "running",
            TaskStatus::Done => "done",
            TaskStatus::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "approved" => TaskStatus::Approved,
            "rejected" => TaskStatus::Rejected,
            "running" => TaskStatus::Running,
            "done" => TaskStatus::Done,
            "failed" => TaskStatus::Failed,
            _ => TaskStatus::PendingApproval,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub session: String,
    pub command: String,
    pub status: TaskStatus,
    pub reason: String,
    pub approver_note: String,
    pub remember: bool,
    pub policy: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub created_ms: u128,
    pub decided_ms: Option<u128>,
    pub done_ms: Option<u128>,
}

pub struct TaskHub {
    pub tasks: Arc<Mutex<Vec<Task>>>,
}

fn now_ms() -> u128 {
    state::now_ms() as u128
}

fn new_id() -> String {
    format!("task_{}", crate::auth::rand_hex(12))
}

/// 写穿：状态变更先落库（失败不阻断内存流程，下次变更会重试）。
fn persist(t: &Task) {
    let Ok(conn) = state::open_conn() else { return };
    let _ = conn.execute(
        "INSERT INTO task_state
            (id, session, command, status, reason, approver_note, remember, policy,
             stdout, stderr, exit_code, created_ms, decided_ms, done_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
         ON CONFLICT(id) DO UPDATE SET
            status=excluded.status, reason=excluded.reason,
            approver_note=excluded.approver_note, remember=excluded.remember,
            policy=excluded.policy, stdout=excluded.stdout, stderr=excluded.stderr,
            exit_code=excluded.exit_code, decided_ms=excluded.decided_ms,
            done_ms=excluded.done_ms",
        rusqlite::params![
            t.id, t.session, t.command, t.status.as_str(), t.reason,
            t.approver_note, t.remember as i64, t.policy,
            t.stdout, t.stderr, t.exit_code,
            t.created_ms as i64,
            t.decided_ms.map(|v| v as i64),
            t.done_ms.map(|v| v as i64),
        ],
    );
}

/// 重启恢复：从状态库读回全部任务。
fn load_all() -> Vec<Task> {
    let Ok(conn) = state::open_conn() else { return vec![] };
    let mut stmt = match conn.prepare(
        "SELECT id, session, command, status, reason, approver_note, remember, policy,
                stdout, stderr, exit_code, created_ms, decided_ms, done_ms
           FROM task_state ORDER BY created_ms",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let rows = stmt.query_map([], |r| {
        Ok(Task {
            id: r.get(0)?,
            session: r.get(1)?,
            command: r.get(2)?,
            status: TaskStatus::from_str(&r.get::<_, String>(3)?),
            reason: r.get(4)?,
            approver_note: r.get(5)?,
            remember: r.get::<_, i64>(6)? != 0,
            policy: r.get(7)?,
            stdout: r.get(8)?,
            stderr: r.get(9)?,
            exit_code: r.get(10)?,
            created_ms: r.get::<_, i64>(11)? as u128,
            decided_ms: r.get::<_, Option<i64>>(12)?.map(|v| v as u128),
            done_ms: r.get::<_, Option<i64>>(13)?.map(|v| v as u128),
        })
    });
    match rows {
        Ok(it) => it.filter_map(|r| r.ok()).collect(),
        Err(_) => vec![],
    }
}

impl TaskHub {
    /// 启动时从库恢复；表不存在时返回空集（由 state::init_state_schema 建表）。
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(load_all())),
        }
    }

    pub async fn create_pending(&self, session: &str, command: &str, reason: &str, policy: &str) -> String {
        let id = new_id();
        let task = Task {
            id: id.clone(),
            session: session.to_string(),
            command: command.to_string(),
            status: TaskStatus::PendingApproval,
            reason: reason.to_string(),
            approver_note: String::new(),
            remember: false,
            policy: policy.to_string(),
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            created_ms: now_ms(),
            decided_ms: None,
            done_ms: None,
        };
        persist(&task);
        self.tasks.lock().await.push(task);
        id
    }

    pub async fn decide(&self, id: &str, approve: bool, note: &str, remember: bool) -> Result<Option<Task>, String> {
        let mut tasks = self.tasks.lock().await;
        let pos = tasks.iter().position(|t| t.id == id).ok_or_else(|| format!("task {id} not found"))?;
        if tasks[pos].status != TaskStatus::PendingApproval {
            return Err(format!("task {id} not pending (status={})", tasks[pos].status.as_str()));
        }
        if approve && remember && tasks[pos].policy == "change" {
            return Err("变更档不可关闭审批：不允许“本次任务内记住放行”，请逐条确认".to_string());
        }
        tasks[pos].status = if approve { TaskStatus::Approved } else { TaskStatus::Rejected };
        tasks[pos].approver_note = note.to_string();
        tasks[pos].remember = remember;
        tasks[pos].decided_ms = Some(now_ms());
        let snapshot = tasks[pos].clone();
        persist(&snapshot);
        Ok(Some(snapshot))
    }

    pub async fn mark_running(&self, id: &str) {
        let mut tasks = self.tasks.lock().await;
        if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
            t.status = TaskStatus::Running;
            persist(t);
        }
    }

    pub async fn finish(&self, id: &str, stdout: String, stderr: String, code: i32) {
        let mut tasks = self.tasks.lock().await;
        if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
            t.status = if code == 0 { TaskStatus::Done } else { TaskStatus::Failed };
            t.stdout = stdout;
            t.stderr = stderr;
            t.exit_code = Some(code);
            t.done_ms = Some(now_ms());
            persist(t);
        }
    }

    pub async fn get(&self, id: &str) -> Option<Task> {
        self.tasks.lock().await.iter().find(|t| t.id == id).cloned()
    }

    pub async fn list_pending(&self) -> Vec<Task> {
        self.tasks.lock().await.iter().filter(|t| t.status == TaskStatus::PendingApproval).cloned().collect()
    }

    pub async fn list_all(&self) -> Vec<Task> {
        self.tasks.lock().await.iter().cloned().collect()
    }
}

pub fn task_to_json(t: &Task) -> Value {
    json!({
        "id": t.id,
        "session": t.session,
        "command": t.command,
        "status": t.status.as_str(),
        "reason": t.reason,
        "approver_note": t.approver_note,
        "remember": t.remember,
        "policy": t.policy,
        "exit_code": t.exit_code,
        "created_ms": t.created_ms,
        "decided_ms": t.decided_ms,
        "done_ms": t.done_ms,
        "stdout": t.stdout,
        "stderr": t.stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_roundtrip() {
        let all = vec![
            TaskStatus::PendingApproval,
            TaskStatus::Approved,
            TaskStatus::Rejected,
            TaskStatus::Running,
            TaskStatus::Done,
            TaskStatus::Failed,
        ];
        for s in all {
            assert_eq!(TaskStatus::from_str(s.as_str()), s);
        }
    }

    #[test]
    fn ids_are_csprng_based_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..500 {
            assert!(seen.insert(new_id()));
        }
    }
}
