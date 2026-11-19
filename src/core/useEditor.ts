import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { sshArgs, sshOfflineReason, audit } from './useSsh'
import { emit } from '../events/useEventBus'
import { notify, notifySuccess, notifyWarn, notifyError, askConfirm } from '../shared/feedback'
import { isLargeFile, largeFileNotice } from '../shared/editorFile'
import type { RemoteStat } from '../shared/types'

// 打开远端文件时的原始文本：用于判断用户是否真的改动过，避免空写覆盖。
let openedText = ''

// 编辑器状态：当前打开的远端文件 / 远端属性（size, mtime）/ 文本内容
export const editingFile = ref('')
export const editingStat = ref<RemoteStat | null>(null)
export const editorText = ref('')
// 保存提交中防重：避免连点重复触发写入 / 审计。
export const saving = ref(false)

// 读写通道开关：A4 已接通 remote_read / remote_write（SSH exec 真读写）。
// 保留此标志便于后续灰度：置 false 即回到草稿预览态，不假装写回远端。
export const editorWritable = ref(true)

// 编辑器错误走 notify + 审计（R3）：不写连接级 sshError，避免误盖终端离线层
function editorFail(e: unknown, ctx: string) {
  const reason = sshOfflineReason(e)
  audit.value.unshift(`[${new Date().toLocaleTimeString()}] ${ctx}失败：${reason}`)
  notifyError(`${ctx}失败：${reason}`)
}

// 打开远端文件：先 stat，小文件读全文填入编辑器；超过 2MB 懒加载（不展开、且禁止保存）
export async function openRemoteFile(path: string) {
  editingFile.value = path
  editorText.value = ''
  openedText = ''
  const args = sshArgs()
  if (!args) { notify('尚未连接远程主机：请先在终端新建连接'); editingStat.value = null; return }
  try {
    const st = await invoke('remote_stat', { ...args, remote: path }) as [number, number]
    editingStat.value = { size: st[0], mtime: st[1] }
    // F-04 大文件懒加载：超过 2MB 不读全文（避免撑爆编辑器），以提示文本占位且禁止保存
    if (isLargeFile(editingStat.value.size)) {
      editorText.value = largeFileNotice(editingStat.value.size)
      openedText = editorText.value
    } else {
      // A4 读通道：真实读取远端文本填入编辑器
      editorText.value = await invoke('remote_read', { ...args, remote: path }) as string
      openedText = editorText.value
    }
  } catch (e: any) {
    editingStat.value = null
    editorText.value = ''
    openedText = ''
    editorFail(e, '读取远端文件')
  }
  emit('file:opened', path)
}

export async function saveFile() {
  if (saving.value) return
  if (!editorWritable.value) {
    notifyWarn('保存通道已关闭：当前为草稿预览，修改不会写回远端')
    return
  }
  const args = sshArgs()
  if (!args) { notify('保存中止：尚未连接远程主机'); return }
  // 大文件懒加载态禁止保存：编辑器里是提示文本而非全文，写回会摧毁原文件。
  if (editingStat.value && isLargeFile(editingStat.value.size)) {
    notifyWarn('大文件为懒加载预览（未读全文），禁止保存以免覆盖远端原文件')
    return
  }
  if (!editingFile.value) { notifyWarn('没有正在编辑的文件'); return }
  // 内容未改动则跳过写入，避免无谓覆盖远端 mtime 与审计噪音。
  if (editorText.value === openedText) {
    notify('内容未改动，无需保存')
    return
  }
  saving.value = true
  try {
    // 1) 比对远端 mtime，防止覆盖他人改动（F-04）。失败必须中止（F-5：原实现漏 return）。
    try {
      const st = await invoke('remote_stat', { ...args, remote: editingFile.value }) as [number, number]
      if (editingStat.value && Number(st[1]) !== editingStat.value.mtime) {
        if (!askConfirm(`远端 ${editingFile.value} 已被改动（mtime 不一致），仍要覆盖保存吗？`)) return
      }
    } catch (e: any) {
      editorFail(e, '比对远端 mtime')
      notify('保存中止：无法确认远端文件状态')
      return // F-5 修复：比对失败绝不继续写入
    }
    // 2) A4 写通道：真实覆盖写回远端
    await invoke('remote_write', { ...args, remote: editingFile.value, content: editorText.value })
    // 3) 写成功后刷新本地 stat，避免下次保存误判 mtime
    try {
      const st = await invoke('remote_stat', { ...args, remote: editingFile.value }) as [number, number]
      editingStat.value = { size: st[0], mtime: st[1] }
      openedText = editorText.value
    } catch { /* 写入已成功，stat 刷新失败不影响结果 */ }
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 保存 ${editingFile.value} → SSH 写回成功`)
    emit('file:saved', editingFile.value)
    notifySuccess(`已保存 ${editingFile.value}`)
  } catch (e: any) {
    editorFail(e, '写回远端')
  } finally {
    saving.value = false
  }
}

export function useEditor() {
  return { editingFile, editingStat, editorText, saving, editorWritable, openRemoteFile, saveFile }
}
