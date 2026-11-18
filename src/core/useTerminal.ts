import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { sendNotification } from '@tauri-apps/plugin-notification'
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager'
import { Terminal } from 'xterm'
import { FitAddon } from 'xterm-addon-fit'
import { SearchAddon } from 'xterm-addon-search'
import { WebglAddon } from '@xterm/addon-webgl'
import { activeTarget, SSH_KEY, sshError, audit, handleSshFailure, type SshTarget } from './useSsh'
import { notify } from '../shared/feedback'
import { nextTabSid, prevTabSid } from '../shared/tabNav'
import type { TerminalTab, SessionMeta } from '../shared/types'

// 终端状态：多会话标签 / 当前会话 / 检索 / 广播
export const termEl = ref<HTMLDivElement>()
export const tabs = ref<TerminalTab[]>([])
export const activeSid = ref('')
export const sessionFilter = ref('')
export const broadcast = ref('')
// 新建连接表单：无会话时默认呈现（R1 首屏主路径），可手动开关
export const showConnectForm = ref(true)
let windowFocused = true

// R11 会话恢复：标签元数据（档案名/认证方式/主机地址，绝不含密码）持久化到
// localStorage；重启后终端区提供「恢复上次会话」一键入口。密码认证会话无法
// 自动恢复（密码不落盘是有意为之的安全边界），恢复时跳过并计数提示。
const SESSION_META_KEY = 'dou_session_meta'

export const lastSessions = ref<SessionMeta[]>(readLastSessions())
export const restoringSessions = ref(false)
export const restoreEntryVisible = computed(() => lastSessions.value.length > 0 && tabs.value.length === 0)
export const restorableCount = computed(() =>
  lastSessions.value.filter(m => m.kind === 'local' || m.auth !== 'password').length
)

function readLastSessions(): SessionMeta[] {
  try {
    const list = JSON.parse(localStorage.getItem(SESSION_META_KEY) ?? '[]')
    return Array.isArray(list)
      ? list.filter(m => m && typeof m.kind === 'string' && typeof m.ts === 'number')
      : []
  } catch { return [] }
}

function persistSessionMeta() {
  try {
    const list = tabs.value.map(t => t.meta).filter((m): m is SessionMeta => !!m)
    localStorage.setItem(SESSION_META_KEY, JSON.stringify(list))
  } catch { /* 存储不可用时跳过：会话恢复是尽力而为的辅助功能 */ }
}

// 表单可见 = 无任何会话，或用户主动打开（连接成功后自动收起）
export const connectFormVisible = computed(() => tabs.value.length === 0 || showConnectForm.value)

export const visibleTabs = computed(() => tabs.value.filter(t =>
  !sessionFilter.value || t.title.toLowerCase().includes(sessionFilter.value.toLowerCase())
))

// R6 降噪派生：检索框仅在会话 ≥5 时出现；广播框仅在勾选 ≥1 时出现（单会话时是纯噪声）
export const selectedCount = computed(() => tabs.value.filter(t => t.selected).length)
export const showSessionFilter = computed(() => tabs.value.length >= 5)

// R13 剪贴板：终端复制粘贴统一走官方插件（WebView2 的 navigator.clipboard.readText
// 权限不可靠；读写失败静默——剪贴板故障不应向会话注入内容或打断终端）。
async function copyTermSelection(term: Terminal) {
  const sel = term.getSelection()
  if (!sel) return
  try { await writeText(sel) } catch { /* 剪贴板不可用时静默 */ }
}
// 粘贴去重锁：IME/合成键盘环境下 Ctrl+Shift+V 的 keydown 会被重复派发（实测连粘 2-5 次），
// 150ms 内的重复粘贴调用直接丢弃（人手连按不可能快于此）。
let lastPasteAt = 0
async function pasteClipboardToTerm(term: Terminal) {
  const now = Date.now()
  if (now - lastPasteAt < 150) return
  lastPasteAt = now
  try {
    const text = await readText()
    if (text) { term.paste(text) }
  } catch { /* 读取失败静默：权限或焦点问题 */ }
}

// R13 右键菜单状态：终端区右键弹出（复制/粘贴/全选/清屏），坐标相对 .term-wrap
export const termCtxMenu = ref<{ visible: boolean; x: number; y: number }>({ visible: false, x: 0, y: 0 })

// 多标签共用同一终端挂载容器，contextmenu 按元素去重绑定一次即可覆盖全部会话；
// 容器被重挂载（视图重建）时按新元素重新绑定。
let ctxMenuBoundEl: HTMLElement | null = null
function onTermContextMenu(ev: MouseEvent) {
  ev.preventDefault()
  const wrap = termEl.value?.parentElement
  if (!wrap) return
  const r = wrap.getBoundingClientRect()
  // 菜单约 130×140，贴边时往回收，避免溢出面板
  const x = Math.max(0, Math.min(ev.clientX - r.left, r.width - 130))
  const y = Math.max(0, Math.min(ev.clientY - r.top, r.height - 140))
  termCtxMenu.value = { visible: true, x, y }
}

// 每会话独立的 xterm 实例（fit / search 插件随标签保存，切换显示时按容器重排）
function createTerm(): { term: Terminal; fit: FitAddon; search: SearchAddon } {
  const term = new Terminal({ fontSize: 13, convertEol: true })
  const fit = new FitAddon()
  term.loadAddon(fit)
  const search = new SearchAddon()
  term.loadAddon(search)
  term.open(termEl.value!)
  try { fit.fit() } catch { /* 容器未就绪时跳过，切换标签时再 fit */ }
  // F-05：优先 WebGL，初始化失败自动回退 Canvas（渲染器回退，非连接错误）
  try { void term.loadAddon(new WebglAddon()) } catch { /* 回退默认 Canvas */ }
  // R13 复制粘贴：Ctrl+Shift+C/V 接管；Ctrl+C 有选区时复制（无选区放行给 shell 的 SIGINT）；
  // Ctrl+V 粘贴（原字面 ^V 按 Windows 终端惯例让位给粘贴）；选中即复制（MobaXterm/PuTTY 惯例）。
  term.attachCustomKeyEventHandler((ev) => {
    if (ev.type !== 'keydown') return true
    const k = ev.key.toLowerCase()
    if (ev.ctrlKey && ev.shiftKey && k === 'c') { void copyTermSelection(term); return false }
    if (ev.ctrlKey && ev.shiftKey && k === 'v') {
      void pasteClipboardToTerm(term); return false
    }
    if (ev.ctrlKey && !ev.shiftKey && k === 'c' && term.hasSelection()) { void copyTermSelection(term); return false }
    if (ev.ctrlKey && !ev.shiftKey && k === 'v') { void pasteClipboardToTerm(term); return false }
    return true
  })
  term.onSelectionChange(() => { void copyTermSelection(term) })
  if (termEl.value && ctxMenuBoundEl !== termEl.value) {
    termEl.value.addEventListener('contextmenu', onTermContextMenu)
    ctxMenuBoundEl = termEl.value
  }
  return { term, fit, search }
}

// R13 右键菜单动作：作用于当前活动会话，动作后关闭菜单
function activeTerm(): Terminal | undefined {
  return tabs.value.find(x => x.sid === activeSid.value)?.term
}
export function termCtxClose() {
  termCtxMenu.value = { visible: false, x: 0, y: 0 }
}
export function termCtxCopy() { const t = activeTerm(); if (t) void copyTermSelection(t); termCtxClose() }
export function termCtxPaste() { const t = activeTerm(); if (t) void pasteClipboardToTerm(t); termCtxClose() }
export function termCtxSelectAll() { const t = activeTerm(); if (t) t.selectAll(); termCtxClose() }
export function termCtxClear() { const t = activeTerm(); if (t) t.clear(); termCtxClose() }

// 多标签共用一个挂载点：只显示当前会话，避免多个终端在同一容器里纵向堆叠
function showActiveTerm() {
  for (const t of tabs.value) {
    const el = t.term.element
    if (!el) continue
    const active = t.sid === activeSid.value
    el.style.display = active ? '' : 'none'
    if (active) { try { t.fit?.fit() } catch { /* 忽略 */ } }
  }
}

// 新建连接（学 MobaXterm 的会话对话框：地址/端口/用户/认证一气呵成，R1+R5）。
// 密码只经本调用内存传给 Rust（Zeroizing 用后即弃），不落盘不写日志。
export interface ConnectOptions extends SshTarget {
  auth: 'key' | 'password'
  password?: string
}

export async function connectSsh(o: ConnectOptions) {
  const host = o.host.trim(), user = o.user.trim()
  if (!host) { sshError.value = '请填写主机地址'; return }
  if (!user) { sshError.value = '请填写用户名'; return }
  if (!(o.port >= 1 && o.port <= 65535)) { sshError.value = '端口需在 1-65535 之间'; return }
  if (o.auth === 'password' && !o.password) {
    sshError.value = '密码认证：请填写连接密码（仅驻留内存，不落盘）'
    return
  }
  try {
    const sid = await invoke('open_pty', {
      host, port: o.port, user, keyPath: SSH_KEY,
      password: o.auth === 'password' ? o.password : undefined,
    }) as string
    sshError.value = ''
    activeTarget.value = { host, port: o.port, user }
    attachSession(sid, `${user}@${host}`, `[已连接 ${user}@${host}:${o.port}（${o.auth === 'password' ? '密码' : '密钥'}认证）]\r\n`, {
      meta: { kind: 'ssh', title: `${user}@${host}`, host, port: o.port, user, auth: o.auth, ts: Date.now() },
    })
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 新建会话 ${sid.slice(0,8)} → ${user}@${host}（${o.auth === 'password' ? '密码' : '密钥'}认证）`)
  } catch (e: any) {
    handleSshFailure(e, '连接主机')
    // 不写裸 os error、不伪造「已连接」；错误由连接表单内联呈现，可改参数重试
  }
}

// 会话落地共用件：创建 xterm、写欢迎行、注册键盘写入、入 tabs（连接表单/档案连接/本地 Shell 共用）
// R4：fit 产生的 resize 一律上报 pty_resize（ConPTY 会话同步尺寸，其余模式 Rust 侧 no-op）
function attachSession(sid: string, title: string, welcome: string, extra?: Partial<TerminalTab>) {
  const { term, fit, search } = createTerm()
  term.write(welcome)
  term.onData(async (data) => { await invoke('pty_write', { sid, data }).catch(()=>{}) })
  term.onResize(({ cols, rows }) => { void invoke('pty_resize', { sid, cols, rows }).catch(()=>{}) })
  tabs.value.push({ sid, title, term, fit, search, ...extra })
  activeSid.value = sid
  showConnectForm.value = false
  showActiveTerm()
  persistSessionMeta()
}

// 本地 Shell（学 MobaXterm 的 Local terminal）：无需远程主机，本机 PowerShell 直开。
// R4：Rust 优先 ConPTY 真终端（vim/htop/resize 不乱），系统不支持时回退管道模式——
// 标题与欢迎行标注「基础模式」，提醒用户画面交互受限。
export async function openLocalTab() {
  try {
    const r = await invoke<{ sid: string, mode: 'conpty' | 'pipe' }>('open_local_pty')
    sshError.value = ''
    const isConpty = r.mode === 'conpty'
    attachSession(
      r.sid,
      isConpty ? '本地 Shell' : '本地 Shell（基础模式）',
      `[本地 Shell（Windows PowerShell · ${isConpty ? 'ConPTY 真终端' : '基础模式：系统不支持 ConPTY，画面交互受限'}）]\r\n`,
      { local: true, meta: { kind: 'local', title: '本地 Shell', ts: Date.now() } }
    )
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 打开本地 Shell ${r.sid.slice(0,8)}（${isConpty ? 'ConPTY' : '管道'}模式）`)
  } catch (e: any) {
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 本地 Shell 启动失败：${String(e?.message ?? e).slice(0, 80)}`)
  }
}

// R8 档案一键连接主路径：连接管理卡片的主操作（按钮 / 双击卡片）。
// 密钥目标 = 先校验保险库解锁且密钥可解密（vault_get_key）→ open_pty(key_path) 开标签；
// 密码目标 = 本次密码 open_pty（密码仅本次调用内存，用完即弃）。
// 返回 false = 失败（sshError 已带 R3 人话文案）；保险库锁定直接抛错由页面 notify。
export interface ArchiveTarget {
  name: string
  host: string
  port: number
  user: string
  key_path?: string
  enc_key?: string | null
}

export async function connectByTarget(t: ArchiveTarget, password?: string): Promise<boolean> {
  if (t.enc_key) {
    try { await invoke('vault_get_key', { name: t.name }) }
    catch (e: any) {
      audit.value.unshift(`[${new Date().toLocaleTimeString()}] 档案连接 ${t.name} 失败：保险库未解锁或密钥不可用`)
      throw new Error('保险库未解锁或密钥不可用：请到「连接管理」输入主密码解锁后重试')
    }
  }
  try {
    const sid = await invoke('open_pty', {
      host: t.host, port: t.port, user: t.user,
      keyPath: t.key_path || SSH_KEY,
      password: password ?? undefined,
    }) as string
    sshError.value = ''
    activeTarget.value = { host: t.host, port: t.port, user: t.user }
    attachSession(sid, `${t.user}@${t.host}`, `[已连接 ${t.user}@${t.host}:${t.port}（档案「${t.name}」，${t.enc_key ? '密钥' : '密码'}认证）]\r\n`, {
      meta: {
        kind: 'ssh', title: `${t.user}@${t.host}`, host: t.host, port: t.port, user: t.user,
        auth: t.enc_key ? 'key' : 'password', archiveName: t.name, keyPath: t.key_path, ts: Date.now(),
      },
    })
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 档案连接 ${sid.slice(0,8)} → ${t.user}@${t.host}（${t.name}）`)
    return true
  } catch (e: any) {
    handleSshFailure(e, `连接档案「${t.name}」`)
    return false
  }
}

// 「+」按钮：开/关新建连接表单（无会话时表单常显，不可关）
export function toggleConnectForm() {
  showConnectForm.value = tabs.value.length === 0 ? true : !showConnectForm.value
}

export function switchTab(sid: string) {
  activeSid.value = sid
  const t = tabs.value.find(x=>x.sid===sid)
  if (t) t.badge = 0
  showActiveTerm()
}

export function closeTab(sid: string) {
  tabs.value = tabs.value.filter(t=>t.sid!==sid)
  if (activeSid.value === sid && tabs.value.length) switchTab(tabs.value[0].sid)
  else showActiveTerm()
  persistSessionMeta()
}

// R15 标签快捷键：Ctrl+Tab 下一个 / Ctrl+Shift+Tab 上一个 / Ctrl+W 关闭当前。
// 导航为环形；无标签时动作空转。
export function nextTab() {
  const n = nextTabSid(tabs.value.map(t => t.sid), activeSid.value)
  if (n) switchTab(n)
}

export function prevTab() {
  const n = prevTabSid(tabs.value.map(t => t.sid), activeSid.value)
  if (n) switchTab(n)
}

export function closeActiveTab() {
  if (activeSid.value) closeTab(activeSid.value)
}

// R14 终端查找：查找条可见性 + 查询词，作用于当前活动标签的 search 插件。
// 第一版用选区定位当前匹配（不挂全量装饰色）；默认忽略大小写、非正则。
export const searchVisible = ref(false)
export const searchQuery = ref('')

function activeSearch(): SearchAddon | undefined {
  return tabs.value.find(x => x.sid === activeSid.value)?.search
}

export function openSearch() { searchVisible.value = true }

export function closeSearch() {
  searchVisible.value = false
  searchQuery.value = ''
  activeSearch()?.clearDecorations()
}

export function findNextMatch() {
  const q = searchQuery.value
  if (!q) return
  activeSearch()?.findNext(q, { caseSensitive: false })
}

export function findPrevMatch() {
  const q = searchQuery.value
  if (!q) return
  activeSearch()?.findPrevious(q, { caseSensitive: false })
}

// R11：一键恢复上次会话。本地 Shell 直接重开；密钥认证档案按存档参数重连；
// 密码认证会话跳过（密码不落盘，无法自动恢复——安全边界，恢复完成时明确告知）。
export async function restoreLastSessions() {
  if (restoringSessions.value) return
  restoringSessions.value = true
  try {
    let needPw = 0, failed = 0
    for (const m of lastSessions.value) {
      if (m.kind === 'local') { await openLocalTab(); continue }
      if (m.auth === 'password' || !m.host) { needPw++; continue }
      const ok = await connectByTarget({
        name: m.archiveName ?? `${m.user ?? ''}@${m.host}`,
        host: m.host, port: m.port ?? 22, user: m.user ?? 'root', key_path: m.keyPath,
      })
      if (!ok) failed++
    }
    lastSessions.value = []
    const notes: string[] = []
    if (needPw) notes.push(`${needPw} 个密码认证会话需重新输入密码（密码不落盘，无法自动恢复）`)
    if (failed) notes.push(`${failed} 个会话恢复失败（主机不可达或密钥不可用）`)
    notify(notes.length ? `会话恢复完成：${notes.join('；')}` : '上次会话已恢复')
  } finally {
    restoringSessions.value = false
  }
}

export function cleanExport(raw: string): string {
  return raw
    .replace(/\x1b\[[0-9;?]*[A-Za-z]/g, '')   // ANSI CSI
    .replace(/\x1b\][^\x07]*\x07/g, '')        // OSC
    .replace(/\x0c/g, '')                       // 分页符
    .replace(/[^\n]*\r/g, '')                   // 回车覆盖字节
    .replace(/\x1b[()][AB012]/g, '')            // 字符集切换
}

export function exportTerm() {
  const t = tabs.value.find(x=>x.sid===activeSid.value)
  if (!t) return
  const lines: string[] = []
  const buf = t.term.buffer.active
  for (let i = 0; i < buf.length; i++) lines.push(buf.getLine(i)?.translateToString(true) ?? '')
  const clean = cleanExport(lines.join('\n'))
  const blob = new Blob([clean], { type: 'text/plain;charset=utf-8' })
  const a = document.createElement('a')
  a.href = URL.createObjectURL(blob)
  a.download = `dou_terminal_${Date.now()}.txt`
  a.click()
  URL.revokeObjectURL(a.href)
}

export async function doBroadcast() {
  const cmd = broadcast.value + '\n'
  const targets = tabs.value.filter(t => t.selected)
  const list = targets.length ? targets : tabs.value
  for (const t of list) {
    await invoke('pty_write', { sid: t.sid, data: cmd }).catch(() => {
      audit.value.unshift(`[${new Date().toLocaleTimeString()}] 广播写入失败：会话 ${t.sid.slice(0,8)} 不可用`)
    })
  }
  broadcast.value = ''
}

// pty_data 订阅：写入终端；窗口失焦时累计角标并发系统通知（F-07）
// 说明：订阅在 App 级别注册（应用生命周期内常驻），否则跨视图切换会丢事件。
export async function registerTerminalListener(): Promise<UnlistenFn> {
  return await listen('pty_data', (e: any) => {
    const p = e.payload
    const t = tabs.value.find(x=>x.sid===p.sid)
    if (t) {
      t.term.write(p.data)
      // 窗口失焦：累计角标并发系统通知（F-07）
      if (!windowFocused) {
        t.badge = (t.badge ?? 0) + 1
        if (t.badge === 1) {
          try { sendNotification({ title: '豆 Dou', body: `${t.title} 有新输出` }) } catch { /* 忽略通知失败 */ }
        }
      }
    }
  })
}

// 窗口焦点订阅：用于判断是否需要角标 + 系统通知
export async function registerWindowFocusListener(): Promise<UnlistenFn> {
  const win = getCurrentWindow()
  const onFocus = () => { windowFocused = true }
  const onBlur = () => { windowFocused = false }
  const unlisten = await win.onFocusChanged(({ payload: focused }) => { windowFocused = focused })
  void onFocus; void onBlur
  return unlisten
}

export function useTerminal() {
  return {
    termEl,
    tabs,
    activeSid,
    sessionFilter,
    broadcast,
    visibleTabs,
    connectSsh,
    toggleConnectForm,
    openLocalTab,
    switchTab,
    closeTab,
    nextTab,
    prevTab,
    closeActiveTab,
    searchVisible,
    searchQuery,
    openSearch,
    closeSearch,
    findNextMatch,
    findPrevMatch,
    cleanExport,
    exportTerm,
    doBroadcast,
  }
}
