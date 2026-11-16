import { ref, computed } from 'vue'
import { gwFetch, gwError, markGwFailure } from '../adapters/useGateway'

// ---------- 审批 ----------
export const pendingTasks = ref<any[]>([])
export const rejectNotes = ref<Record<string, string>>({})

export const pendingCount = computed(() => pendingTasks.value.length)

// ---------- A4 断连置顶 ----------
// 「客户端已断开」判定（客户端侧近似，网关安全面不动）：
//  1) 任务在本次客户端会话启动前就已挂起 —— 期间客户端不在线；
//  2) 会话内观察到与网关失联（传输层失败），失联开始前已挂起的任务。
// 命中的任务置顶并标注「客户端已断开期间挂起」，附人工终止入口（POST /task/cancel）。
const appStartMs = Date.now()
export const gwDisconnected = ref(false)
let outageSinceMs: number | null = null
const orphanIds = new Set<string>()

function classifyOrphans(outageMs: number | null) {
  for (const t of pendingTasks.value) {
    if (t.created_ms < appStartMs) orphanIds.add(t.id)
    if (outageMs !== null && t.created_ms <= outageMs) orphanIds.add(t.id)
  }
}

/** 面板视图：断连任务置顶，其余按创建时间倒序。 */
export const pendingTasksView = computed(() => {
  const items = pendingTasks.value.map(t => ({ ...t, orphaned: orphanIds.has(t.id) }))
  items.sort((a, b) => (Number(b.orphaned) - Number(a.orphaned)) || (b.created_ms - a.created_ms))
  return items
})

export async function loadTasks() {
  try {
    const r = await gwFetch('/tasks')
    if (!r.ok) throw new Error(`HTTP ${r.status}`)
    const j = await r.json()
    pendingTasks.value = (j.items || []).filter((t: any) => t.status === 'pending_approval')
    if (outageSinceMs !== null) {
      // 本轮传输成功 = 连接恢复；失联开始时间先捕获再重置，
      // 否则失联窗口判定（规则 2）在恢复轮永远不生效。
      const recoveredMs = outageSinceMs
      outageSinceMs = null
      gwDisconnected.value = false
      classifyOrphans(recoveredMs)
    } else {
      classifyOrphans(null)
    }
  } catch {
    if (outageSinceMs === null) outageSinceMs = Date.now()
    gwDisconnected.value = true
    pendingTasks.value = []
    markGwFailure('加载待审任务失败：网关不可用')  // 不静默吞错，错误态由全局横幅呈现
  }
}

export async function decideTask(id: string, approve: boolean, remember: boolean) {
  const note = rejectNotes.value[id] || ''
  if (!approve && !note.trim()) { alert('拒绝必须填写理由'); return }
  try {
    await gwFetch('/approve', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ task_id: id, approve, remember, note })
    })
    delete rejectNotes.value[id]
    await loadTasks()
  } catch {
    alert('审批提交失败：' + (gwError.value?.message || '请求未完成'))
  }
}

/** A4 人工终止入口：终止网关侧仍在执行/挂起的任务（仅对活动任务生效，后端裁决）。 */
export async function cancelTask(id: string) {
  try {
    const r = await gwFetch('/task/cancel', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ task_id: id })
    })
    if (!r.ok) {
      let msg = `HTTP ${r.status}`
      try { const j = await r.json(); if (j && j.error) msg = j.error } catch { /* 保留状态码 */ }
      alert('终止失败：' + msg)
      return
    }
    await loadTasks()
  } catch {
    alert('终止请求未完成：' + (gwError.value?.message || '网关不可用'))
  }
}

// 待审任务轮询（30s）
export function startApprovalPolling() {
  return setInterval(loadTasks, 30000)
}

export function useApproval() {
  return { pendingTasks, pendingTasksView, rejectNotes, pendingCount, gwDisconnected, loadTasks, decideTask, cancelTask, startApprovalPolling }
}
