import { describe, it, expect } from 'vitest'
import { cleanExport } from '../../shared/ansi'

describe('cleanExport 终端导出清洗', () => {
  it('普通可见文本原样保留', () => {
    expect(cleanExport('hello world\n第二行')).toBe('hello world\n第二行')
  })

  it('剥离 CSI 颜色/样式序列', () => {
    expect(cleanExport('\x1b[31m红色\x1b[0m 普通')).toBe('红色 普通')
  })

  it('剥离 OSC 标题/通知序列', () => {
    expect(cleanExport('\x1b]0;窗口标题\x07保留')).toBe('保留')
  })

  it('剥离分页符', () => {
    expect(cleanExport('a\x0cb')).toBe('ab')
  })

  it('剥离回车覆盖留下最终行', () => {
    expect(cleanExport('line1\rline2')).toBe('line2')
  })

  it('剥离字符集切换序列', () => {
    expect(cleanExport('\x1b(Btext')).toBe('text')
  })
})
