/**
 * 请求竞态守卫：requestId 防过期响应。
 *
 * 场景：连续触发两次异步加载（如快速切换目录），先发的慢响应后到，
 * 若不做防护会用旧数据覆盖新数据。调用方在发起前 begin() 取号，
 * 响应到达后 isCurrent(id) 校验——非当前号即过期，丢弃。
 */

export interface RequestGuard {
  /** 发起新请求前调用，取一个递增序号。 */
  begin: () => number
  /** 响应到达时校验：序号仍是最新则 true；已被后续 begin 淘汰则 false。 */
  isCurrent: (id: number) => boolean
}

export function useRequestGuard(): RequestGuard {
  let seq = 0
  return {
    begin: () => ++seq,
    isCurrent: (id: number) => id === seq,
  }
}
