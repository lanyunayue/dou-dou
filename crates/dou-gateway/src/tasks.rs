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
    Cancelled,
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
            TaskStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "approved" => TaskStatus::Approved,
            "rejected" => TaskStatus::Rejected,
            "running" => TaskStatus::Running,
            "done" => TaskStatus::Done,
            "failed" => TaskStatus::Failed,
            "cancelled" => TaskStatus::Cancelled,
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
    /// A4：审批卡片四要素之一，目标路径（exec 为会话，put/get 为远端路径）。
    pub target_path: String,
    /// A4：历史同类命令次数（create 时快照，用于「预计影响」提示）。
    pub history_count: u32,
    /// A4：预计影响（人话描述，如「修改文件」「删除文件」）。
    pub impact: String,
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
             stdout, stderr, exit_code, created_ms, decided_ms, done_ms,
             target_path, history_count, impact)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
         ON CONFLICT(id) DO UPDATE SET
            status=excluded.status, reason=excluded.reason,
            approver_note=excluded.approver_note, remember=excluded.remember,
            policy=excluded.policy, stdout=excluded.stdout, stderr=excluded.stderr,
            exit_code=excluded.exit_code, decided_ms=excluded.decided_ms,
            done_ms=excluded.done_ms, target_path=excluded.target_path,
            history_count=excluded.history_count, impact=excluded.impact",
        rusqlite::params![
            t.id,
            t.session,
            t.command,
            t.status.as_str(),
            t.reason,
            t.approver_note,
            t.remember as i64,
            t.policy,
            t.stdout,
            t.stderr,
            t.exit_code,
            t.created_ms as i64,
            t.decided_ms.map(|v| v as i64),
            t.done_ms.map(|v| v as i64),
            t.target_path,
            t.history_count as i64,
            t.impact,
        ],
    );
}

/// 重启恢复：从状态库读回全部任务。
fn load_all() -> Vec<Task> {
    let Ok(conn) = state::open_conn() else {
        return vec![];
    };
    let mut stmt = match conn.prepare(
        "SELECT id, session, command, status, reason, approver_note, remember, policy,
                stdout, stderr, exit_code, created_ms, decided_ms, done_ms,
                target_path, history_count, impact
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
            target_path: r.get(14).unwrap_or_default(),
            history_count: r.get::<_, Option<i64>>(15)?.unwrap_or(0) as u32,
            impact: r.get(16).unwrap_or_default(),
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

    pub async fn create_pending(
        &self,
        session: &str,
        command: &str,
        reason: &str,
        policy: &str,
    ) -> String {
        self.create_pending_with(session, command, reason, policy, "", "", "")
            .await
    }

    /// 完整版创建：携带审批卡片四要素（目标路径 / 历史次数 / 预计影响）。
    #[allow(clippy::too_many_arguments)]
    pub async fn create_pending_with(
        &self,
        session: &str,
        command: &str,
        reason: &str,
        policy: &str,
        target_path: &str,
        impact: &str,
        _extra: &str,
    ) -> String {
        let id = new_id();
        // 历史同类次数：同名命令（归一化首段）在库中的出现次数，用于「预计影响」提示。
        let history_count = Self::count_command_history(command);
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
            target_path: target_path.to_string(),
            history_count,
            impact: impact.to_string(),
        };
        persist(&task);
        self.tasks.lock().await.push(task);
        id
    }

    /// A3 审批超时：将超过 `timeout_ms` 仍未决的 pending 任务自动置为拒绝。
    /// 返回本次被超时拒绝的任务数（供调用方写审计）。
    pub async fn expire_stale_pending(&self, timeout_ms: u128) -> usize {
        let now = now_ms();
        let mut tasks = self.tasks.lock().await;
        let mut expired = 0usize;
        for t in tasks.iter_mut() {
            if t.status == TaskStatus::PendingApproval
                && now.saturating_sub(t.created_ms) > timeout_ms
            {
                t.status = TaskStatus::Rejected;
                t.reason = format!(
                    "{}（审批超时自动拒绝）",
                    t.reason.trim_end_matches("（审批超时自动拒绝）")
                );
                t.decided_ms = Some(now);
                persist(t);
                expired += 1;
            }
        }
        expired
    }

    /// A2 终止任务：仅 running / pending 可终止；已终态返回 Err。
    pub async fn cancel(&self, id: &str) -> Result<Option<Task>, String> {
        let mut tasks = self.tasks.lock().await;
        let pos = tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| format!("task {id} not found"))?;
        match tasks[pos].status {
            TaskStatus::Running | TaskStatus::PendingApproval => {}
            _ => {
                return Err(format!(
                    "task {id} 已终态（{}），不可终止",
                    tasks[pos].status.as_str()
                ))
            }
        }
        tasks[pos].status = TaskStatus::Cancelled;
        tasks[pos].done_ms = Some(now_ms());
        let snapshot = tasks[pos].clone();
        persist(&snapshot);
        Ok(Some(snapshot))
    }

    /// 查询某命令的历史出现次数（归一化到首命令词）。
    fn count_command_history(command: &str) -> u32 {
        let Ok(conn) = state::open_conn() else {
            return 0;
        };
        let head = command
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if head.is_empty() {
            return 0;
        }
        let mut stmt =
            match conn.prepare("SELECT COUNT(*) FROM task_state WHERE lower(command) LIKE ?1") {
                Ok(s) => s,
                Err(_) => return 0,
            };
        stmt.query_row(rusqlite::params![format!("{head}%")], |r| {
            r.get::<_, i64>(0)
        })
        .map(|n| n as u32)
        .unwrap_or(0)
    }

    pub async fn decide(
        &self,
        id: &str,
        approve: bool,
        note: &str,
        remember: bool,
    ) -> Result<Option<Task>, String> {
        let mut tasks = self.tasks.lock().await;
        let pos = tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| format!("task {id} not found"))?;
        if tasks[pos].status != TaskStatus::PendingApproval {
            return Err(format!(
                "task {id} not pending (status={})",
                tasks[pos].status.as_str()
            ));
        }
        if approve && remember && tasks[pos].policy == "change" {
            return Err("变更档不可关闭审批：不允许“本次任务内记住放行”，请逐条确认".to_string());
        }
        tasks[pos].status = if approve {
            TaskStatus::Approved
        } else {
            TaskStatus::Rejected
        };
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
            t.status = if code == 0 {
                TaskStatus::Done
            } else {
                TaskStatus::Failed
            };
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
        self.tasks
            .lock()
            .await
            .iter()
            .filter(|t| t.status == TaskStatus::PendingApproval)
            .cloned()
            .collect()
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
        "target_path": t.target_path,
        "history_count": t.history_count,
        "impact": t.impact,
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
            TaskStatus::Cancelled,
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

    #[test]
    fn cancel_only_works_on_active_tasks() {
        // 用 tokio 运行时驱动 cancel 的状态机
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let hub = TaskHub {
                tasks: Arc::new(Mutex::new(vec![
                    Task {
                        id: "t1".into(),
                        session: "s".into(),
                        command: "rm -f x".into(),
                        status: TaskStatus::PendingApproval,
                        reason: String::new(),
                        approver_note: String::new(),
                        remember: false,
                        policy: "change".into(),
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: None,
                        created_ms: 1,
                        decided_ms: None,
                        done_ms: None,
                        target_path: String::new(),
                        history_count: 0,
                        impact: String::new(),
                    },
                    Task {
                        id: "t2".into(),
                        session: "s".into(),
                        command: "ls".into(),
                        status: TaskStatus::Done,
                        reason: String::new(),
                        approver_note: String::new(),
                        remember: false,
                        policy: "execute".into(),
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: Some(0),
                        created_ms: 1,
                        decided_ms: Some(2),
                        done_ms: Some(3),
                        target_path: String::new(),
                        history_count: 0,
                        impact: String::new(),
                    },
                ])),
            };
            // pending 可终止
            let r = hub.cancel("t1").await.unwrap().unwrap();
            assert_eq!(r.status, TaskStatus::Cancelled);
            // 终态不可终止
            assert!(hub.cancel("t2").await.is_err());
            // 不存在报错
            assert!(hub.cancel("nope").await.is_err());
        });
    }

    #[test]
    fn expire_stale_pending_rejects_only_stale() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let hub = TaskHub {
                tasks: Arc::new(Mutex::new(vec![
                    Task {
                        id: "old".into(),
                        session: "s".into(),
                        command: "rm x".into(),
                        status: TaskStatus::PendingApproval,
                        reason: "mutating".into(),
                        approver_note: String::new(),
                        remember: false,
                        policy: "change".into(),
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: None,
                        created_ms: 1,
                        decided_ms: None,
                        done_ms: None,
                        target_path: String::new(),
                        history_count: 0,
                        impact: String::new(),
                    },
                    Task {
                        id: "fresh".into(),
                        session: "s".into(),
                        command: "rm y".into(),
                        status: TaskStatus::PendingApproval,
                        reason: "mutating".into(),
                        approver_note: String::new(),
                        remember: false,
                        policy: "change".into(),
                        stdout: String::new(),
                        stderr: String::new(),
                        exit_code: None,
                        created_ms: u128::MAX,
                        decided_ms: None,
                        done_ms: None,
                        target_path: String::new(),
                        history_count: 0,
                        impact: String::new(),
                    },
                ])),
            };
            let expired = hub.expire_stale_pending(5 * 60 * 1000).await;
            assert_eq!(expired, 1);
            let all = hub.list_all().await;
            let old = all.iter().find(|t| t.id == "old").unwrap();
            assert_eq!(old.status, TaskStatus::Rejected);
            let fresh = all.iter().find(|t| t.id == "fresh").unwrap();
            assert_eq!(fresh.status, TaskStatus::PendingApproval);
        });
    }

    #[test]
    fn task_to_json_preserves_contract_fields() {
        let t = Task {
            id: "t-json".into(),
            session: "s".into(),
            command: "ls -la".into(),
            status: TaskStatus::Done,
            reason: "read-only".into(),
            approver_note: "ok".into(),
            remember: true,
            policy: "observe".into(),
            stdout: "out".into(),
            stderr: "err".into(),
            exit_code: Some(0),
            created_ms: 1_700_000_000_000,
            decided_ms: Some(2),
            done_ms: Some(3),
            target_path: "/a/b".into(),
            history_count: 5,
            impact: "read".into(),
        };
        let j = task_to_json(&t);
        assert_eq!(j["id"], "t-json");
        assert_eq!(j["command"], "ls -la");
        assert_eq!(j["status"], "done");
        assert_eq!(j["reason"], "read-only");
        assert_eq!(j["impact"], "read");
        assert_eq!(j["target_path"], "/a/b");
        assert_eq!(j["history_count"], 5);
        assert_eq!(j["policy"], "observe");
        assert_eq!(j["exit_code"].as_i64(), Some(0));
        assert_eq!(j["created_ms"].as_u64(), Some(1_700_000_000_000));
    }
}
