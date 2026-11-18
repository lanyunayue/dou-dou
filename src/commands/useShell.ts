import { tabs, activeSid } from '../core/useTerminal'
import { cwd } from '../core/useFileTree'
import { notify } from '../shared/feedback'
import { parseMobaSessions } from '../shared/mobaImport'

// ---------- 侧边栏动作：快照 / 导入 MobaXterm ----------
export async function hash8(s: string): Promise<string> {
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(s))
  return Array.from(new Uint8Array(buf)).slice(0,4).map(b=>b.toString(16).padStart(2,'0')).join('')
}

export async function snapshot() {
  const t = tabs.value.find(x=>x.sid===activeSid.value)
  const history = t ? (t.term.buffer.active.getLine(0)?.translateToString(true) ?? '').slice(-200) : ''
  const snap = { time: new Date().toLocaleString(), cwd: cwd.value, sid: activeSid.value, history }
  const list = JSON.parse(localStorage.getItem('dou_snaps')||'[]')
  list.push(snap)
  localStorage.setItem('dou_snaps', JSON.stringify(list))
  const h = await hash8('127.0.0.1:22'), u = await hash8('dou'), s = await hash8(activeSid.value)
  notify(`快照已存\n三段指纹: ${h}/${u}/${s}\n历史: ${history.slice(-50)}...\n共 ${list.length} 个快照`)
}

export async function importMoba() {
  const fs = await import('@tauri-apps/plugin-fs')
  const paths = ['C:/Program Files (x86)/MobaXterm/MobaXterm.ini','C:/Program Files/MobaXterm/MobaXterm.ini']
  for (const p of paths) {
    try {
      const txt = await fs.readTextFile(p)
      const sessions = parseMobaSessions(txt)
      const existing = JSON.parse(localStorage.getItem('dou_imported')||'[]')
      const merged = [...existing, ...sessions]
      localStorage.setItem('dou_imported', JSON.stringify(merged))
      notify(`已找到: ${p}\n解析到 ${sessions.length} 个 SSH 会话，共入库 ${merged.length} 个`)
      return
    } catch { continue } // 该路径未安装 MobaXterm，继续尝试下一路径
  }
  notify('未找到 MobaXterm 配置（本机未安装或路径不同）')
}

export function useShell() {
  return { hash8, snapshot, importMoba }
}
