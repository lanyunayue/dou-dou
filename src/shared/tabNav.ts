/** R15 标签页键盘导航：纯函数，不碰 Tauri / DOM，便于单测。 */

/** 环形取下一个标签 sid；空列表返回 undefined；当前不在列表则回到第一个。 */
export function nextTabSid(sids: string[], active: string): string | undefined {
  if (sids.length === 0) return undefined
  const i = sids.indexOf(active)
  if (i === -1) return sids[0]
  return sids[(i + 1) % sids.length]
}

/** 环形取上一个标签 sid；空列表返回 undefined；当前不在列表则回到第一个。 */
export function prevTabSid(sids: string[], active: string): string | undefined {
  if (sids.length === 0) return undefined
  const i = sids.indexOf(active)
  if (i === -1) return sids[0]
  return sids[(i - 1 + sids.length) % sids.length]
}
