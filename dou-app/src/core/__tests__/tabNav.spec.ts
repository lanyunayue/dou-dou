import { describe, it, expect } from 'vitest'
import { nextTabSid, prevTabSid } from '../../shared/tabNav'

const sids = ['a', 'b', 'c']

describe('nextTabSid', () => {
  it('顺序向后切换', () => {
    expect(nextTabSid(sids, 'a')).toBe('b')
    expect(nextTabSid(sids, 'b')).toBe('c')
  })
  it('末尾环形回到第一个', () => {
    expect(nextTabSid(sids, 'c')).toBe('a')
  })
  it('空列表返回 undefined', () => {
    expect(nextTabSid([], 'a')).toBeUndefined()
  })
  it('当前不在列表时回到第一个', () => {
    expect(nextTabSid(sids, 'zzz')).toBe('a')
  })
})

describe('prevTabSid', () => {
  it('向前切换', () => {
    expect(prevTabSid(sids, 'c')).toBe('b')
    expect(prevTabSid(sids, 'b')).toBe('a')
  })
  it('开头环形回到末尾', () => {
    expect(prevTabSid(sids, 'a')).toBe('c')
  })
  it('空列表返回 undefined', () => {
    expect(prevTabSid([], 'a')).toBeUndefined()
  })
  it('当前不在列表时回到第一个', () => {
    expect(prevTabSid(sids, 'zzz')).toBe('a')
  })
})
