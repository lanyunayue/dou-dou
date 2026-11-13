import { ref, computed } from 'vue'
import { gwFetch, gwError, markGwFailure } from './useGateway'

// ---------- 审批 ----------
export const pendingTasks = ref<any[]>([])
export const rejectNotes = ref<Record<string, string>>({})

export const pendingCount = computed(() => pendingTasks.value.length)

export async function loadTasks() {
  try {
    const r = await gwFetch('/tasks')
    const j = await r.json()
    pendingTasks.value = (j.items || []).filter((t: any) => t.status === 'pending_approval')
  } catch {
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

// 待审任务轮询（30s）
export function startApprovalPolling() {
  return setInterval(loadTasks, 30000)
}

export function useApproval() {
  return { pendingTasks, rejectNotes, pendingCount, loadTasks, decideTask, startApprovalPolling }
}
