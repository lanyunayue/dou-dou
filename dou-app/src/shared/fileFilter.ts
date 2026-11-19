/** 文件树可见项过滤：纯函数，不碰 Tauri / DOM，便于单测。 */

/** 参与可见性判定的最小字段（只需 name；其余属性透传保留）。 */
export interface VisibleFile {
  name: string
  [key: string]: unknown
}

/**
 * 按隐藏文件开关与名称关键词过滤。
 * - 隐藏文件：名称以 `.` 开头，showHidden 关闭时剔除；
 * - 关键词：不区分大小写子串匹配名称，空白查询不过滤；
 * - 不修改入参数组。
 */
export function filterVisibleFiles<T extends VisibleFile>(
  files: T[],
  showHidden: boolean,
  query: string,
): T[] {
  const q = (query ?? '').trim().toLowerCase()
  return files.filter((f) => {
    if (!showHidden && f.name.startsWith('.')) return false
    if (q && !f.name.toLowerCase().includes(q)) return false
    return true
  })
}
