import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { SSH_HOST, SSH_PORT, SSH_USER, SSH_KEY, sshError, handleSshFailure } from './useSsh'
import { openRemoteFile } from './useEditor'
import type { RemoteFile } from '../shared/types'

// 文件树状态：当前目录 / 目录项 / 过滤器
export const cwd = ref('/home/dou')
export const files = ref<RemoteFile[]>([])
export const showHidden = ref(false)
export const fileFilter = ref('')

export const visibleFiles = computed(() => files.value.filter(f => {
  if (!showHidden.value && f.name.startsWith('.')) return false
  if (fileFilter.value && !f.name.toLowerCase().includes(fileFilter.value.toLowerCase())) return false
  return true
}))

export async function loadFiles(path: string) {
  cwd.value = path
  try { files.value = await invoke('sftp_list', { host: SSH_HOST, port: SSH_PORT, user: SSH_USER, keyPath: SSH_KEY, path }) }
  catch (e: any) { files.value = []; handleSshFailure(e, '列出远端目录') }
}

export function goUp() {
  if (cwd.value === '/' || cwd.value === '') return
  const parent = cwd.value.split('/').filter(Boolean).slice(0, -1).join('/')
  loadFiles(parent === '' ? '/' : '/' + parent)
}

export async function downloadCwd() {
  try {
    const p = await invoke('sftp_download', { host: SSH_HOST, port: SSH_PORT, user: SSH_USER, keyPath: SSH_KEY, remote: cwd.value + '/dou_put_test.txt' })
    alert('已下载到: ' + p)
  } catch (e: any) {
    handleSshFailure(e, '下载')
    alert('下载失败：' + sshError.value + '\n靶机就绪后点「一键重试」恢复')
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
