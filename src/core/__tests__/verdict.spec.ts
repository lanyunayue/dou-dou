import { describe, it, expect } from 'vitest'
import { verdictTone } from '../../shared/verdict'

describe('verdictTone 放行类', () => {
  it('auto 只读自动放行归绿', () => expect(verdictTone('auto')).toBe('allow'))
  it('allowed/allow/granted/ok 归绿', () => {
    expect(verdictTone('allowed')).toBe('allow')
    expect(verdictTone('allow')).toBe('allow')
    expect(verdictTone('granted')).toBe('allow')
    expect(verdictTone('ok')).toBe('allow')
  })
  it('大小写不敏感并去首尾空白', () => {
    expect(verdictTone(' AUTO ')).toBe('allow')
    expect(verdictTone('Allowed')).toBe('allow')
  })
})

describe('verdictTone 拒绝类', () => {
  it('denied/deny/blocked 归红', () => {
    expect(verdictTone('denied')).toBe('deny')
    expect(verdictTone('deny')).toBe('deny')
    expect(verdictTone('blocked')).toBe('deny')
  })
})

describe('verdictTone 待审与未知', () => {
  it('pending_approval 待审落琥珀', () => expect(verdictTone('pending_approval')).toBe('ask'))
  it('执行结果 done/failed 不判安全拒绝，落琥珀', () => {
    expect(verdictTone('done')).toBe('ask')
    expect(verdictTone('failed')).toBe('ask')
  })
  it('空值与未知取值落琥珀', () => {
    expect(verdictTone('')).toBe('ask')
    expect(verdictTone(undefined)).toBe('ask')
    expect(verdictTone('nonsense')).toBe('ask')
  })
})
