import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { sshArgs, sshOfflineReason, audit } from './useSsh'
import { openRemoteFile } from './useEditor'
import { useRequestGuard } from './useRequestGuard'
import { notify } from '../shared/feedback'
import type { RemoteFile } from '../shared/types'

// 文件树状态：当前目录 / 目录项 / 过滤器
export const cwd = ref('/home/dou')
export const files = ref<RemoteFile[]>([])
export const showHidden = ref(false)
export const fileFilter = ref('')
// 文件树独立错误态（R3）：不再复用连接级 sshError，避免误盖终端离线层
export const filesError = ref('')

// 竞态守卫：快速连续切换目录时，先发的慢响应不得覆盖新目录结果
const loadGuard = useRequestGuard()

export const visibleFiles = computed(() => files.value.filter(f => {
  if (!showHidden.value && f.name.startsWith('.')) return false
  if (fileFilter.value && !f.name.toLowerCase().includes(fileFilter.value.toLowerCase())) return false
  return true
}))

export async function loadFiles(path: string) {
  const args = sshArgs()
  if (!args) { filesError.value = '尚未连接远程主机：请先在终端新建连接'; files.value = []; return }
  const id = loadGuard.begin()
  cwd.value = path
  try {
    const list = await invoke<RemoteFile[]>('sftp_list', { ...args, path })
    if (!loadGuard.isCurrent(id)) return
    files.value = list
    filesError.value = ''
  } catch (e: any) {
    if (!loadGuard.isCurrent(id)) return
    files.value = []
    filesError.value = sshOfflineReason(e)
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 列出远端目录失败：${filesError.value}`)
  }
}

export function goUp() {
  if (cwd.value === '/' || cwd.value === '') return
  const parent = cwd.value.split('/').filter(Boolean).slice(0, -1).join('/')
  loadFiles(parent === '' ? '/' : '/' + parent)
}

export async function downloadCwd() {
  const args = sshArgs()
  if (!args) { notify('尚未连接远程主机：请先在终端新建连接'); return }
  try {
    const p = await invoke('sftp_download', { ...args, remote: cwd.value + '/dou_put_test.txt' })
    notify('已下载到: ' + p)
  } catch (e: any) {
    notify('下载失败：' + sshOfflineReason(e))
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 下载失败：${sshOfflineReason(e)}`)
  }
}

// 点击目录项：目录则下钻，文件则交给编辑器打开
export async function onFile(f: RemoteFile) {
  if (f.is_dir) { loadFiles(cwd.value === '/' ? '/' + f.name : cwd.value + '/' + f.name); return }
  const path = cwd.value === '/' ? '/' + f.name : cwd.value + '/' + f.name
  await openRemoteFile(path)
}

export function useFileTree() {
  return { cwd, files, showHidden, fileFilter, visibleFiles, loadFiles, goUp, downloadCwd, onFile }
}
