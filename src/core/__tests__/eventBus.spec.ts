import { describe, it, expect, vi } from 'vitest'
import { on, off, emit } from '../../events/useEventBus'

describe('useEventBus on/off/emit', () => {
  it('订阅后广播收到数据', () => {
    const fn = vi.fn()
    on('test:one', fn)
    emit('test:one', { x: 1 })
    expect(fn).toHaveBeenCalledWith({ x: 1 })
    off('test:one', fn)
  })

  it('多个订阅者都收到同一事件', () => {
    const a = vi.fn()
    const b = vi.fn()
    on('test:multi', a)
    on('test:multi', b)
    emit('test:multi', 'hi')
    expect(a).toHaveBeenCalledWith('hi')
    expect(b).toHaveBeenCalledWith('hi')
    off('test:multi', a)
    off('test:multi', b)
  })

  it('取消订阅后不再收到', () => {
    const fn = vi.fn()
    on('test:off', fn)
    off('test:off', fn)
    emit('test:off', 1)
    expect(fn).not.toHaveBeenCalled()
  })

  it('只移除指定订阅者，不影响同名其他订阅者', () => {
    const a = vi.fn()
    const b = vi.fn()
    on('test:mixed', a)
    on('test:mixed', b)
    off('test:mixed', a)
    emit('test:mixed', 2)
    expect(a).not.toHaveBeenCalled()
    expect(b).toHaveBeenCalledWith(2)
    off('test:mixed', b)
  })

  it('未知事件广播不报错', () => {
    expect(() => emit('test:nope', 1)).not.toThrow()
  })
})
