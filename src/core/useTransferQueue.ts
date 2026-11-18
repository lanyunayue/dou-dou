import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { notify } from '../shared/feedback'
import { fmtBytes } from '../shared/format'

// ---------- 传输队列 / 同步 ----------
export const tqTasks = ref<any[]>([])
export const tqRemote = ref('')
export const tqLocal = ref('')
export const tqType = ref('download')
export const syncLocal = ref('')
export const syncRemote = ref('')
export const syncMode = ref('push')

// fmtBytes 纯格式化函数位于 shared/format.ts，此处重新导出供面板解构。
export { fmtBytes }

export async function loadTq() { try { tqTasks.value = await invoke('tq_list') } catch { tqTasks.value = [] } }
export async function enqueueTq() {
  if (!tqRemote.value || !tqLocal.value) { notify('请填写远端路径与本地路径'); return }
  try { await invoke('tq_enqueue', { kind: tqType.value, label: tqRemote.value, remote: tqRemote.value, local: tqLocal.value }); await loadTq() }
  catch (e: any) { notify('入队失败: ' + e) }
}
export async function tqCancel(id: string) { await invoke('tq_cancel', { id }); await loadTq() }
export async function tqPause(id: string) { await invoke('tq_pause', { id }); await loadTq() }
export async function tqResume(id: string) { await invoke('tq_resume', { id }); await loadTq() }
export async function tqClear() { await invoke('tq_clear'); await loadTq() }
export async function doSync() {
  if (!syncLocal.value || !syncRemote.value) { notify('请填写本地与远端目录'); return }
  try { await invoke('sync_folders', { localDir: syncLocal.value, remoteDir: syncRemote.value, mode: syncMode.value }); await loadTq() }
  catch (e: any) { notify('同步失败: ' + e) }
}

// tq_progress 订阅：队列进度实时刷新
export async function registerTqListener(): Promise<UnlistenFn> {
  return await listen('tq_progress', (e: any) => { tqTasks.value = e.payload })
}

export function useTransferQueue() {
  return {
    tqTasks, tqRemote, tqLocal, tqType, syncLocal, syncRemote, syncMode,
    fmtBytes, loadTq, enqueueTq, tqCancel, tqPause, tqResume, tqClear, doSync,
  }
}
