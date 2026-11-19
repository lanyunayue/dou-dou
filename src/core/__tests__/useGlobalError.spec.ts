import { describe, it, expect } from 'vitest'
import { errMessage } from '../useGlobalError'

describe('errMessage 异常消息提取', () => {
  it('Error 取 message', () => {
    expect(errMessage(new Error('boom'))).toBe('boom')
  })
  it('字符串 / 数字原样转字符串', () => {
    expect(errMessage('oops')).toBe('oops')
    expect(errMessage(42)).toBe('42')
  })
  it('null / undefined 回落未知错误', () => {
    expect(errMessage(null)).toBe('未知错误')
    expect(errMessage(undefined)).toBe('未知错误')
  })
})
