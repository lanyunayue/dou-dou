import { describe, it, expect } from 'vitest'
import { fmtTime } from '../useCredentials'

describe('fmtTime 按 epoch 毫秒解析', () => {
  it('空值返回占位符', () => {
    expect(fmtTime(0)).toBe('-')
  })
  it('毫秒时间戳解析为合法日期，不再二次乘 1000', () => {
    // 2026-11-18 00:00:00 UTC+8 对应的毫秒
    const ms = new Date('2026-11-18T00:00:00+08:00').getTime()
    const out = fmtTime(ms)
    expect(out).not.toBe('-')
    // 年份应落在 2026，而不是毫秒被当秒再放大后的异常年份
    expect(out).toContain('2026')
  })
})
