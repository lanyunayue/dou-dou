import { tabs, activeSid } from '../core/useTerminal'
import { cwd } from '../core/useFileTree'
import { notify, notifyWarn } from '../shared/feedback'
import { parseMobaSessions } from '../shared/mobaImport'
import { hash8 } from '../shared/hash'

// ---------- 侧边栏动作：快照 / 导入 MobaXterm ----------
export { hash8 }

const SNAP_KEY = 'dou_snaps'
const IMPORT_KEY = 'dou_imported'

function readLocalList(key: string): any[] {
  try {
    const v = JSON.parse(localStorage.getItem(key) || '[]')
    return Array.isArray(v) ? v : []
  } catch { return [] }
}

/** 本机本地存储中的快照条数（仅浏览器本地，未上传网关）。 */
export function localSnapshotCount(): number { return readLocalList(SNAP_KEY).length }

/** 本机本地存储中已导入的 MobaXterm 会话条数（未进保险库档案）。 */
export function localImportedCount(): number { return readLocalList(IMPORT_KEY).length }

export async function snapshot() {
  const t = tabs.value.find(x => x.sid === activeSid.value)
  const history = t ? (t.term.buffer.active.getLine(0)?.translateToString(true) ?? '').slice(-200) : ''
  const snap = { time: new Date().toLocaleString(), cwd: cwd.value, sid: activeSid.value, history }
  const list = readLocalList(SNAP_KEY)
  list.push(snap)
  localStorage.setItem(SNAP_KEY, JSON.stringify(list))
  const h = await hash8('127.0.0.1:22'), u = await hash8('dou'), s = await hash8(activeSid.value)
  notify(`快照已存到本机本地存储\n三段指纹: ${h}/${u}/${s}\n历史: ${history.slice(-50)}...\n共 ${list.length} 个快照（暂未上传网关）`)
}

export async function importMoba() {
  const fs = await import('@tauri-apps/plugin-fs')
  const paths = ['C:/Program Files (x86)/MobaXterm/MobaXterm.ini','C:/Program Files/MobaXterm/MobaXterm.ini']
  for (const p of paths) {
    try {
      const txt = await fs.readTextFile(p)
      const sessions = parseMobaSessions(txt)
      const existing = readLocalList(IMPORT_KEY)
      const merged = [...existing, ...sessions]
      localStorage.setItem(IMPORT_KEY, JSON.stringify(merged))
      notify(`已找到: ${p}\n解析到 ${sessions.length} 个 SSH 会话，共 ${merged.length} 个（暂存本机本地存储，未进保险库档案）`)
      return
    } catch { continue } // 该路径未安装 MobaXterm，继续尝试下一路径
  }
  notifyWarn('未找到 MobaXterm 配置（本机未安装或路径不同）')
}

export function useShell() {
  return { hash8, snapshot, importMoba, localSnapshotCount, localImportedCount }
}
