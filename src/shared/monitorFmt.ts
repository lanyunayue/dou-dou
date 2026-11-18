/** 监控条展示格式化：纯函数，不碰响应式状态 / DOM，便于单测。 */

/** 内存占用百分比（0-100，取整）；total 缺失或 ≤0 时兜底 0，避免除零。 */
export function memPercent(usedMb: number, totalMb: number): number {
  if (!totalMb || totalMb <= 0) return 0
  return Math.round((usedMb / Math.max(totalMb, 1)) * 100)
}

/** 占用分档色：<60 正常 / 60-85 偏高 warm / ≥85 告警 hot。 */
export function meterClass(v: number): 'hot' | 'warm' | '' {
  return v >= 85 ? 'hot' : v >= 60 ? 'warm' : ''
}

/** 网络速率格式化：≥1024 以 MB/s，否则 KB/s，一位小数。 */
export function fmtKbps(k: number): string {
  return k >= 1024 ? `${(k / 1024).toFixed(1)}MB/s` : `${k.toFixed(1)}KB/s`
}

/** 运行时长格式化：不足 1 小时只显示分钟，否则 h+m。 */
export function fmtUptime(sec: number): string {
  const h = Math.floor(sec / 3600)
  const m = Math.floor((sec % 3600) / 60)
  return h ? `${h}h${m}m` : `${m}m`
}
