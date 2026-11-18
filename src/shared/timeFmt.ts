/**
 * 时间展示纯函数（无副作用、不依赖 Tauri/网关）。
 */

/** epoch 毫秒 → 本地可读时间；假值回落 '-'。 */
export function fmtMs(ms: number | null | undefined): string {
  if (!ms) return '-'
  return new Date(ms).toLocaleString('zh-CN', { hour12: false })
}

/**
 * 审计条目时间：优先 ts_ms，其次 ts；空 / 非数值 / 非正时间戳回落原值或 '-'。
 */
export function fmtAuditTime(tsMs: unknown, ts: unknown): string {
  const raw = (tsMs ?? ts ?? '') as unknown
  const n = Number(raw)
  if (!raw || !Number.isFinite(n) || n <= 0) return String(raw || '-')
  return new Date(n).toLocaleString('zh-CN', { hour12: false })
}
