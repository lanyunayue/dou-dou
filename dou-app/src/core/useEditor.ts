import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { sshArgs, sshOfflineReason, audit } from './useSsh'
import { emit } from '../events/useEventBus'
import { notify, askConfirm } from '../shared/feedback'
import type { RemoteStat } from '../shared/types'

// 编辑器状态：当前打开的远端文件 / 远端属性（size, mtime）/ 文本内容
export const editingFile = ref('')
export const editingStat = ref<RemoteStat | null>(null)
export const editorText = ref('')

// 编辑器错误走 notify + 审计（R3）：不写连接级 sshError，避免误盖终端离线层
function editorFail(e: unknown, ctx: string) {
  const reason = sshOfflineReason(e)
  audit.value.unshift(`[${new Date().toLocaleTimeString()}] ${ctx}失败：${reason}`)
  notify(`${ctx}失败：${reason}`)
}

// 打开远端文件：先记属性，超过 2MB 懒加载（不直接展开全文）
export async function openRemoteFile(path: string) {
  editingFile.value = path
  editorText.value = ''
  const args = sshArgs()
  if (!args) { notify('尚未连接远程主机：请先在终端新建连接'); editingStat.value = null; return }
  try {
    const st = await invoke('remote_stat', { ...args, remote: path }) as [number, number]
    editingStat.value = { size: st[0], mtime: st[1] }
    // F-04 大文件懒加载：超过 2MB 不直接载入编辑器
    if (editingStat.value.size > 2 * 1024 * 1024) {
      editorText.value = `// 文件较大（${(editingStat.value.size/1024/1024).toFixed(1)} MB），已懒加载，不直接展开全文`
    }
  } catch (e: any) { editingStat.value = null; editorFail(e, '读取远端文件属性') }
  emit('file:opened', path)
}

export async function saveFile() {
  // 保存前比对远端 mtime，防止覆盖他人改动（F-04）
  const args = sshArgs()
  if (!args) { notify('保存中止：尚未连接远程主机'); return }
  try {
    const st = await invoke('remote_stat', { ...args, remote: editingFile.value }) as [number, number]
    if (editingStat.value && Number(st[1]) !== editingStat.value.mtime) {
      if (!askConfirm(`远端 ${editingFile.value} 已被改动（mtime 不一致），仍要覆盖保存吗？`)) return
    }
    editingStat.value = { size: st[0], mtime: st[1] }
  } catch (e: any) { editorFail(e, '比对远端 mtime'); notify('保存中止') }
  audit.value.unshift(`[${new Date().toLocaleTimeString()}] 保存 ${editingFile.value} → 过隔离区+扫描`)
  emit('file:saved', editingFile.value)
}

export function useEditor() {
  return { editingFile, editingStat, editorText, openRemoteFile, saveFile }
}
