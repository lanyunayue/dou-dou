import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { notifyWarn, notifyError, notifySuccess } from '../shared/feedback'
import { fmtBytes } from '../shared/format'

// ---------- 传输队列 / 同步 ----------
export const tqTasks = ref<any[]>([])
export const tqRemote = ref('')
export const tqLocal = ref('')
export const tqType = ref('download')
export const syncLocal = ref('')
export const syncRemote = ref('')
export const syncMode = ref('push')

// 提交中防重：入队 / 同步各自一把锁，避免连点重复发任务。
export const busyEnqueue = ref(false)
export const busySync = ref(false)

// fmtBytes 纯格式化函数位于 shared/format.ts，此处重新导出供面板解构。
export { fmtBytes }

export async function loadTq() { try { tqTasks.value = await invoke('tq_list') } catch { tqTasks.value = [] } }

export async function enqueueTq() {
  if (busyEnqueue.value) return
  if (!tqRemote.value.trim() || !tqLocal.value.trim()) { notifyWarn('请填写远端路径与本地路径'); return }
  busyEnqueue.value = true
  try {
    await invoke('tq_enqueue', { kind: tqType.value, label: tqRemote.value, remote: tqRemote.value.trim(), local: tqLocal.value.trim() })
    await loadTq()
    notifySuccess('任务已加入队列')
  } catch (e: any) { notifyError('入队失败: ' + (e?.message ?? e)) }
  finally { busyEnqueue.value = false }
}

export async function tqCancel(id: string) { await invoke('tq_cancel', { id }); await loadTq() }
export async function tqPause(id: string) { await invoke('tq_pause', { id }); await loadTq() }
export async function tqResume(id: string) { await invoke('tq_resume', { id }); await loadTq() }
export async function tqClear() { await invoke('tq_clear'); await loadTq() }

/** 失败任务重试：文件任务按原远端/本地重新入队；同步任务按原目录与模式重新发起。 */
export async function retryTask(task: any) {
  if (!task) return
  if (String(task.kind).startsWith('sync:')) {
    const mode = String(task.kind).split(':')[1] || 'push'
    await doSyncWith(task.local, task.remote, mode)
    return
  }
  try {
    await invoke('tq_enqueue', { kind: task.kind, label: task.label, remote: task.remote, local: task.local })
    await loadTq()
    notifySuccess('已重新加入队列')
  } catch (e: any) { notifyError('重试失败: ' + (e?.message ?? e)) }
}

async function doSyncWith(localDir: string, remoteDir: string, mode: string) {
  if (busySync.value) return
  busySync.value = true
  try {
    await invoke('sync_folders', { localDir, remoteDir, mode })
    await loadTq()
    notifySuccess('同步任务已重新发起')
  } catch (e: any) { notifyError('同步失败: ' + (e?.message ?? e)) }
  finally { busySync.value = false }
}

export async function doSync() {
  if (!syncLocal.value.trim() || !syncRemote.value.trim()) { notifyWarn('请填写本地与远端目录'); return }
  await doSyncWith(syncLocal.value.trim(), syncRemote.value.trim(), syncMode.value)
}

// tq_progress 订阅：队列进度实时刷新
export async function registerTqListener(): Promise<UnlistenFn> {
  return await listen('tq_progress', (e: any) => { tqTasks.value = e.payload })
}

export function useTransferQueue() {
  return {
    tqTasks, tqRemote, tqLocal, tqType, syncLocal, syncRemote, syncMode,
    busyEnqueue, busySync,
    fmtBytes, loadTq, enqueueTq, tqCancel, tqPause, tqResume, tqClear, doSync, retryTask,
  }
}
