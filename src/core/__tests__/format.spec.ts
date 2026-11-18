import { describe, it, expect } from 'vitest'
import { fmtBytes } from '../../shared/format'

describe('fmtBytes 字节分级', () => {
  it('0 与 NaN 落 0 B；负数沿用整数 B 换算（既有行为，未额外拦截）', () => {
    expect(fmtBytes(0)).toBe('0 B')
    expect(fmtBytes(NaN)).toBe('0 B')
    expect(fmtBytes(-10)).toBe('-10 B')
  })
  it('不足 1KB 按整数 B 显示', () => {
    expect(fmtBytes(512)).toBe('512 B')
    expect(fmtBytes(999)).toBe('999 B')
  })
  it('1024 进制换算到 KB/MB/GB', () => {
    expect(fmtBytes(1024)).toBe('1.0 KB')
    expect(fmtBytes(1536)).toBe('1.5 KB')
    expect(fmtBytes(1048576)).toBe('1.0 MB')
    expect(fmtBytes(1073741824)).toBe('1.0 GB')
  })
  it('超过 GB 上限停在 GB 档，不再进位', () => {
    const out = fmtBytes(1073741824 * 1024)
    expect(out.endsWith('GB')).toBe(true)
  })
})
