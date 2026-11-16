import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ---------- 传输队列 / 同步 ----------
export const tqTasks = ref<any[]>([])
export const tqRemote = ref('')
export const tqLocal = ref('')
export const tqType = ref('download')
export const syncLocal = ref('')
export const syncRemote = ref('')
export const syncMode = ref('push')

export function fmtBytes(n: number) {
  if (!n) return '0 B'
  const u = ['B', 'KB', 'MB', 'GB']; let i = 0; let x = n
  while (x >= 1024 && i < u.length - 1) { x /= 1024; i++ }
  return (i ? x.toFixed(1) : String(x)) + ' ' + u[i]
}

export async function loadTq() { try { tqTasks.value = await invoke('tq_list') } catch { tqTasks.value = [] } }
export async function enqueueTq() {
  if (!tqRemote.value || !tqLocal.value) { alert('请填写远端路径与本地路径'); return }
  try { await invoke('tq_enqueue', { kind: tqType.value, label: tqRemote.value, remote: tqRemote.value, local: tqLocal.value }); await loadTq() }
  catch (e: any) { alert('入队失败: ' + e) }
}
export async function tqCancel(id: string) { await invoke('tq_cancel', { id }); await loadTq() }
export async function tqPause(id: string) { await invoke('tq_pause', { id }); await loadTq() }
export async function tqResume(id: string) { await invoke('tq_resume', { id }); await loadTq() }
export async function tqClear() { await invoke('tq_clear'); await loadTq() }
export async function doSync() {
  if (!syncLocal.value || !syncRemote.value) { alert('请填写本地与远端目录'); return }
  try { await invoke('sync_folders', { localDir: syncLocal.value, remoteDir: syncRemote.value, mode: syncMode.value }); await loadTq() }
  catch (e: any) { alert('同步失败: ' + e) }
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
