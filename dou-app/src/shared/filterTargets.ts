/** R16 主机档案列表过滤：纯函数，不碰 Tauri 调用，便于单测。 */

/** 参与过滤的非敏感字段（不含私钥/密码）。 */
export interface FilterableTarget {
  name: string
  host: string
  user: string
}

/**
 * 按名称 / 主机 / 用户名做不区分大小写的子串匹配。
 * 查询串空白时原样返回；不修改入参数组。
 */
export function filterTargets<T extends FilterableTarget>(targets: T[], query: string): T[] {
  const q = (query ?? '').trim().toLowerCase()
  if (!q) return targets
  return targets.filter((t) =>
    [t.name, t.host, t.user].some((f) => (f ?? '').toLowerCase().includes(q)),
  )
}
