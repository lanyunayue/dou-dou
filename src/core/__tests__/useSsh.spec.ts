import { describe, it, expect } from 'vitest'
import { sshOfflineReason } from '../useSsh'

describe('sshOfflineReason 错误码转人话', () => {
  it('10061 / refused / 连接被拒绝 → 端口未监听', () => {
    expect(sshOfflineReason({ message: 'os error 10061' })).toContain('连接被拒绝')
    expect(sshOfflineReason({ message: 'Connection refused' })).toContain('连接被拒绝')
  })
  it('timeout / 超时 → 主机不可达', () => {
    expect(sshOfflineReason({ message: 'Operation timed out' })).toContain('连接超时')
    expect(sshOfflineReason({ message: '连接超时' })).toContain('连接超时')
  })
  it('authentication / 认证 → 认证失败', () => {
    expect(sshOfflineReason({ message: 'Permission denied (authentication failed)' })).toContain('认证失败')
  })
  it('key / 密钥 / not found → 密钥不可用', () => {
    expect(sshOfflineReason({ message: 'no such file or directory: id_rsa' })).toContain('密钥不可用')
    expect(sshOfflineReason({ message: '私钥密钥路径无效' })).toContain('密钥不可用')
  })
  it('空错误兜底为不可用', () => {
    expect(sshOfflineReason(null)).toBe('连接失败：目标主机不可用')
    expect(sshOfflineReason('')).toBe('连接失败：目标主机不可用')
  })
  it('未知错误回显并截断到 100 字', () => {
    const long = 'x'.repeat(200)
    const out = sshOfflineReason({ message: long })
    expect(out).toContain('连接失败：')
    expect(out.length).toBeLessThanOrEqual('连接失败：'.length + 100)
  })
})
