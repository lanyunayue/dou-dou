import { describe, it, expect } from 'vitest'
import { authLabel } from '../../shared/credLabel'

describe('authLabel 认证标签', () => {
  it('有加密材料优先判为已加密', () => {
    expect(authLabel('password', 'enc-xyz')).toBe('🔑 已加密')
    expect(authLabel('key', 'enc-xyz')).toBe('🔑 已加密')
  })

  it('无加密材料时密码认证单独标注', () => {
    expect(authLabel('password', undefined)).toBe('🔒 密码认证（连接时输入密码）')
  })

  it('密钥或未配方式回落未配钥', () => {
    expect(authLabel('key', undefined)).toBe('— 未配钥')
    expect(authLabel(undefined, null)).toBe('— 未配钥')
  })
})
