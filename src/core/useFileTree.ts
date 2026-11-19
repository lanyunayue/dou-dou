import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { sshArgs, sshOfflineReason, audit } from './useSsh'
import { openRemoteFile } from './useEditor'
import { useRequestGuard } from './useRequestGuard'
import { loadTq } from './useTransferQueue'
import { notifyWarn, notifySuccess, notifyError } from '../shared/feedback'
import { baseName, joinRemote, parentRemote } from '../shared/pathUtils'
import { filterVisibleFiles } from '../shared/fileFilter'
import type { RemoteFile } from '../shared/types'

// 文件树状态：当前目录 / 目录项 / 过滤器
export const cwd = ref('/home/dou')
export const files = ref<RemoteFile[]>([])
export const showHidden = ref(false)
export const fileFilter = ref('')
// 当前选中的目录项（下载 / 后续右键菜单复用）
export const selectedFile = ref<RemoteFile | null>(null)
// 路径跳转输入框文本（回车提交到 cwd）
export const pathInput = ref('')
// 文件树独立错误态（R3）：不再复用连接级 sshError，避免误盖终端离线层
export const filesError = ref('')

// 竞态守卫：快速连续切换目录时，先发的慢响应不得覆盖新目录结果
const loadGuard = useRequestGuard()

export const visibleFiles = computed(() =>
  filterVisibleFiles(files.value, showHidden.value, fileFilter.value),
)

export async function loadFiles(path: string) {
  const args = sshArgs()
  if (!args) { filesError.value = '尚未连接远程主机：请先在终端新建连接'; files.value = []; return }
  const id = loadGuard.begin()
  cwd.value = path
  pathInput.value = path
  selectedFile.value = null
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
  loadFiles(parentRemote(cwd.value))
}

/** 刷新当前目录（远端被外部改动后手动重载）。 */
export function refreshFiles() {
  loadFiles(cwd.value || '/home/dou')
}

/** 路径栏回车跳转：归一化空路径，非法路径由后端校验兜底。 */
export function gotoPath() {
  const p = pathInput.value.trim()
  if (!p) { pathInput.value = cwd.value; return }
  loadFiles(p.startsWith('/') ? p : joinRemote(cwd.value, p))
}

/** 下载当前选中的文件；目录打包下载暂未开放。 */
export async function downloadSelected() {
  const args = sshArgs()
  if (!args) { notifyWarn('尚未连接远程主机：请先在终端新建连接'); return }
  const f = selectedFile.value
  if (!f) { notifyWarn('请先在列表中选中一个文件（目录打包下载暂未开放）'); return }
  if (f.is_dir) { notifyWarn('暂不支持下载整个目录，请先选中具体文件'); return }
  const remote = joinRemote(cwd.value, f.name)
  try {
    const p = await invoke('sftp_download', { ...args, remote })
    notifySuccess('已下载到: ' + p)
  } catch (e: any) {
    notifyError('下载失败：' + sshOfflineReason(e))
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 下载失败：${sshOfflineReason(e)}`)
  }
}

// R-拖拽上传：文件面板接收本地文件拖入，逐个入队 upload 到当前远端目录。
// 队列内部连接沿用传输队列既有目标（与下载一致；多主机统一属 B1 范畴）。
export const dragActive = ref(false)

function enqueueUploads(paths: string[]) {
  if (!sshArgs()) { notifyWarn('尚未连接远程主机：请先在终端新建连接'); return }
  const files = paths.filter(p => baseName(p))
  if (!files.length) { notifyWarn('未检测到可上传的文件（暂不支持拖拽目录，目录请用传输队列同步）'); return }
  for (const local of files) {
    const name = baseName(local)
    const remote = joinRemote(cwd.value, name)
    invoke('tq_enqueue', { kind: 'upload', label: `上传 ${name}`, remote, local })
      .catch((e) => { notifyError('「' + name + '」入队失败：' + sshOfflineReason(e)) })
  }
  notifySuccess(`已将 ${files.length} 个文件加入上传队列（目标目录 ${cwd.value}）；可在「传输队列」查看进度`)
  void loadTq()
}

// 注册窗口级拖拽事件：面板挂载期间接收；enter/over 高亮，drop 入队，leave 取消高亮。
export async function registerDropListener(): Promise<UnlistenFn> {
  const wv = getCurrentWebview()
  return await wv.onDragDropEvent((e) => {
    const t = e.payload.type
    if (t === 'enter' || t === 'over') { dragActive.value = true; return }
    if (t === 'leave') { dragActive.value = false; return }
    if (t === 'drop') {
      dragActive.value = false
      enqueueUploads(e.payload.paths ?? [])
    }
  })
}

// 点击目录项：目录则下钻，文件则交给编辑器打开；同时记录为当前选中项。
export async function onFile(f: RemoteFile) {
  selectedFile.value = f
  if (f.is_dir) { loadFiles(joinRemote(cwd.value, f.name)); return }
  await openRemoteFile(joinRemote(cwd.value, f.name))
}

export function useFileTree() {
  return {
    cwd, files, showHidden, fileFilter, visibleFiles, dragActive,
    selectedFile, pathInput,
    loadFiles, goUp, refreshFiles, gotoPath, downloadSelected, onFile, registerDropListener,
  }
}
