//! 任务与审批闭环。
//!
//! 计划书 4.9：变更类命令不直接执行，而是挂起成 task，等人工批准/拒绝。
//! 长命令（>30s）也转任务句柄，避免客户端放弃后重跑。

use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
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
}

#[derive(Debug, Clone)]
pub struct Task {
    pub id: String,
    pub session: String,
    pub command: String,
    pub status: TaskStatus,
    pub reason: String,
    pub approver_note: String,
    /// 本次任务内记住该命令免审（仅执行档可记）。
    pub remember: bool,
    /// 审批策略档位：observe | execute | change；change 档不可记住放行。
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
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

fn new_id() -> String {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("task_{:x}", n)
}

impl TaskHub {
    pub fn new() -> Self {
        Self { tasks: Arc::new(Mutex::new(Vec::new())) }
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
        Ok(Some(tasks[pos].clone()))
    }

    pub async fn mark_running(&self, id: &str) {
        let mut tasks = self.tasks.lock().await;
        if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
            t.status = TaskStatus::Running;
        }
    }

    pub async fn finish(&self, id: &str, stdout: String, stderr: String, code: i32) {
        let mut tasks = self.tasks.lock().await;
        if let Some(t) = tasks.iter_mut().find(|t| t.id == id) {
            t.status = TaskStatus::Done;
            t.stdout = stdout;
            t.stderr = stderr;
            t.exit_code = Some(code);
            t.done_ms = Some(now_ms());
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
