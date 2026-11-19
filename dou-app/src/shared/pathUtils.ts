/** 远端/本地路径小工具：纯函数，不碰 Tauri 与 DOM，便于单测。 */

/** 取路径末段文件名（兼容 `/` 与 Windows `\`）；空路径返回空串。 */
export function baseName(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean)
  return parts[parts.length - 1] ?? ''
}

/** 远端目录 + 文件名拼接：去掉目录尾斜杠与文件名首斜杠，根目录补 `/`。 */
export function joinRemote(dir: string, name: string): string {
  const d = dir.replace(/\/+$/, '')
  const n = name.replace(/^\/+/, '')
  if (!n) return d || '/'
  return d === '' ? '/' + n : d + '/' + n
}

/** 取远端目录的父级绝对路径；根目录、空路径或单层目录返回 `/`。 */
export function parentRemote(p: string): string {
  const parts = p.split('/').filter(Boolean)
  if (parts.length <= 1) return '/'
  return '/' + parts.slice(0, -1).join('/')
}

/**
 * 从 `pwd` 输出（可能混有 ANSI 转义 / 提示符）中提取最后一个绝对路径。
 * 解析层备用：终端 PTY 的当前目录回传需真机/回环验证后再接前端；
 * 取不到合法绝对路径时返回 null，调用方保留文件树现有目录。
 */
export function parsePwdOutput(raw: string): string | null {
  const clean = raw
    .replace(/\x1b\[[0-9;?]*[A-Za-z]/g, '')
    .replace(/\x1b\][^\x07]*\x07/g, '')
  const lines = clean.split(/\r?\n/).map(l => l.trim()).filter(Boolean)
  for (let i = lines.length - 1; i >= 0; i--) {
    const m = lines[i].match(/(?:^|\s)\/[^\s]*/)
    if (m) return m[0].trim()
  }
  return null
}
