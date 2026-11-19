/**
 * 展示层纯格式化函数（无副作用、不依赖 Tauri/网关）。
 */

/** 字节数人类可读：B / KB / MB / GB，1024 进制；非正数落 "0 B"。 */
export function fmtBytes(n: number): string {
  if (!n) return '0 B'
  const u = ['B', 'KB', 'MB', 'GB']
  let i = 0
  let x = n
  while (x >= 1024 && i < u.length - 1) {
    x /= 1024
    i++
  }
  return (i ? x.toFixed(1) : String(x)) + ' ' + u[i]
}
