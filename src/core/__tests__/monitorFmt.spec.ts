import { describe, it, expect } from 'vitest'
import { memPercent, meterClass, fmtKbps, fmtUptime } from '../../shared/monitorFmt'

describe('memPercent', () => {
  it('按 used/total 取整百分比', () => {
    expect(memPercent(512, 1024)).toBe(50)
    expect(memPercent(1, 3)).toBe(33)
  })
  it('total 缺失或 ≤0 兜底 0，不除零', () => {
    expect(memPercent(500, 0)).toBe(0)
    expect(memPercent(500, -1)).toBe(0)
  })
})

describe('meterClass 分档', () => {
  it('<60 正常，60-85 偏高，≥85 告警', () => {
    expect(meterClass(59)).toBe('')
    expect(meterClass(60)).toBe('warm')
    expect(meterClass(84)).toBe('warm')
    expect(meterClass(85)).toBe('hot')
  })
})

describe('fmtKbps 速率', () => {
  it('低于 1024 显示 KB/s', () => {
    expect(fmtKbps(512)).toBe('512.0KB/s')
  })
  it('达到 1024 换算 MB/s，一位小数', () => {
    expect(fmtKbps(1024)).toBe('1.0MB/s')
    expect(fmtKbps(2048)).toBe('2.0MB/s')
  })
})

describe('fmtUptime 时长', () => {
  it('不足 1 小时只显示分钟', () => {
    expect(fmtUptime(90)).toBe('1m')
    expect(fmtUptime(0)).toBe('0m')
  })
  it('满 1 小时显示 h+m', () => {
    expect(fmtUptime(3600)).toBe('1h0m')
    expect(fmtUptime(3661)).toBe('1h1m')
  })
})
