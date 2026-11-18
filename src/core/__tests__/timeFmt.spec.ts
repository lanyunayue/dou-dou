import { describe, it, expect } from 'vitest'
import { fmtMs, fmtAuditTime } from '../../shared/timeFmt'

describe('fmtMs 基础时间格式化', () => {
  it('假值回落占位符', () => {
    expect(fmtMs(0)).toBe('-')
    expect(fmtMs(undefined)).toBe('-')
    expect(fmtMs(null)).toBe('-')
  })

  it('正常毫秒解析为含年份的本地时间', () => {
    const ms = new Date('2026-11-18T00:00:00+08:00').getTime()
    expect(fmtMs(ms)).toContain('2026')
  })
})

describe('fmtAuditTime 审计条目时间兜底', () => {
  it('优先使用 ts_ms', () => {
    const ms = new Date('2026-11-18T00:00:00+08:00').getTime()
    expect(fmtAuditTime(String(ms), '1700000000000')).toContain('2026')
  })

  it('ts_ms 缺失时回退 ts', () => {
    const ms = new Date('2026-11-18T00:00:00+08:00').getTime()
    expect(fmtAuditTime(undefined, String(ms))).toContain('2026')
  })

  it('空值回落占位符', () => {
    expect(fmtAuditTime(undefined, undefined)).toBe('-')
    expect(fmtAuditTime('', '')).toBe('-')
  })

  it('非数字原样保留', () => {
    expect(fmtAuditTime('n/a', undefined)).toBe('n/a')
  })

  it('非正时间戳但原始值非空时保留原值', () => {
    expect(fmtAuditTime('0', undefined)).toBe('0')
  })
})
