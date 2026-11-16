import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { sendNotification } from '@tauri-apps/plugin-notification'
import { Terminal } from 'xterm'
import { FitAddon } from 'xterm-addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import { SSH_HOST, SSH_PORT, SSH_USER, SSH_KEY, authMode, sshPassword, sshError, audit, handleSshFailure } from './useSsh'
import type { TerminalTab } from '../shared/types'

// 终端状态：多会话标签 / 当前会话 / 检索 / 广播
export const termEl = ref<HTMLDivElement>()
export const tabs = ref<TerminalTab[]>([])
export const activeSid = ref('')
export const sessionFilter = ref('')
export const broadcast = ref('')
let windowFocused = true

export const visibleTabs = computed(() => tabs.value.filter(t =>
  !sessionFilter.value || t.title.toLowerCase().includes(sessionFilter.value.toLowerCase())
))

// 每会话独立的 xterm 实例（fit 插件随标签保存，切换显示时按容器重排）
function createTerm(): { term: Terminal; fit: FitAddon } {
  const term = new Terminal({ fontSize: 13, convertEol: true })
  const fit = new FitAddon()
  term.loadAddon(fit)
  term.open(termEl.value!)
  try { fit.fit() } catch { /* 容器未就绪时跳过，切换标签时再 fit */ }
  // F-05：优先 WebGL，初始化失败自动回退 Canvas（渲染器回退，非连接错误）
  try { void term.loadAddon(new WebglAddon()) } catch { /* 回退默认 Canvas */ }
  return { term, fit }
}

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

// 连接靶机（学 MobaXterm：密钥失败兜不了底时可用密码认证）；
// 失败进入优雅离线态，不黑屏、不裸 os error、不伪造连接成功
export async function connectSsh() {
  try {
    const pw = authMode.value === 'password' ? sshPassword.value : ''
    if (authMode.value === 'password' && !pw) {
      sshError.value = '密码认证：请先在工具条输入连接密码（仅驻留内存，不落盘）'
      return
    }
    const sid = await invoke('open_pty', {
      host: SSH_HOST, port: SSH_PORT, user: SSH_USER, keyPath: SSH_KEY,
      password: pw || undefined,
    }) as string
    sshError.value = ''
    const { term, fit } = createTerm()
    term.write(`[已连接靶机 ${SSH_USER}@${SSH_HOST}:${SSH_PORT}（${authMode.value === 'password' ? '密码' : '密钥'}认证）]\r\n`)
    term.onData(async (data) => { await invoke('pty_write', { sid, data }).catch(()=>{}) })
    tabs.value.push({ sid, title: `会话 ${tabs.value.length+1}`, term, fit })
    activeSid.value = sid
    showActiveTerm()
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 新建会话 ${sid.slice(0,8)}（${authMode.value === 'password' ? '密码' : '密钥'}认证）`)
  } catch (e: any) {
    handleSshFailure(e, '连接靶机')
    // 不写裸 os error、不伪造「已连接」；离线态由模板覆盖层呈现，含一键重试
  }
}

// 本地 Shell（学 MobaXterm 的 Local terminal）：无需靶机，本机 PowerShell 直开
export async function openLocalTab() {
  try {
    const sid = await invoke('open_local_pty') as string
    sshError.value = ''
    const { term, fit } = createTerm()
    term.write('[本地 Shell（Windows PowerShell，管道模式）]\r\n')
    term.onData(async (data) => { await invoke('pty_write', { sid, data }).catch(()=>{}) })
    tabs.value.push({ sid, title: '本地 Shell', term, fit, local: true })
    activeSid.value = sid
    showActiveTerm()
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 打开本地 Shell ${sid.slice(0,8)}`)
  } catch (e: any) {
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 本地 Shell 启动失败：${String(e?.message ?? e).slice(0, 80)}`)
  }
}

export async function newTab() { await connectSsh() }

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
    newTab,
    openLocalTab,
    switchTab,
    closeTab,
    cleanExport,
    exportTerm,
    doBroadcast,
  }
}
