import { describe, it, expect } from 'vitest'
import { summarizeConn } from '../../shared/connSummary'

const err = { message: '鉴权失败（401）' }

describe('summarizeConn 状态分层', () => {
  it('网关未运行优先显示未就绪（即使带错误）', () => {
    expect(summarizeConn(false, false, err)).toBe('网关未就绪 · HTTP 管理功能不可用')
    expect(summarizeConn(false, true, null)).toBe('网关未就绪 · HTTP 管理功能不可用')
  })
  it('在线但有错误时附带错误信息', () => {
    expect(summarizeConn(true, false, err)).toBe('网关在线 · 鉴权失败（401）')
  })
  it('在线无错误且 SSH 已连 → 已连接', () => {
    expect(summarizeConn(true, true, null)).toContain('远程主机已连接')
  })
  it('在线无错误但 SSH 未连 → 未连接', () => {
    expect(summarizeConn(true, false, null)).toContain('远程主机未连接')
  })
})
