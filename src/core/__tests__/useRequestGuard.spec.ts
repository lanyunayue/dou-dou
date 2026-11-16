import { describe, it, expect } from 'vitest'
import { useRequestGuard } from '../useRequestGuard'

describe('useRequestGuard', () => {
  it('begin 每次返回递增序号', () => {
    const g = useRequestGuard()
    const a = g.begin()
    const b = g.begin()
    expect(b).toBe(a + 1)
  })

  it('当前序号 isCurrent 为 true', () => {
    const g = useRequestGuard()
    const id = g.begin()
    expect(g.isCurrent(id)).toBe(true)
  })

  it('后续 begin 使旧响应过期', () => {
    const g = useRequestGuard()
    const oldId = g.begin()
    g.begin() // 新请求取号，旧号作废
    expect(g.isCurrent(oldId)).toBe(false)
  })

  it('多实例互不影响', () => {
    const g1 = useRequestGuard()
    const g2 = useRequestGuard()
    const id1 = g1.begin()
    g2.begin()
    expect(g1.isCurrent(id1)).toBe(true)
  })
})
