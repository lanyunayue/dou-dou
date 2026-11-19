import { describe, it, expect } from 'vitest'
import { clampFontSize, DEFAULT_FONT_SIZE, FONT_SIZE_OPTIONS } from '../../shared/termPrefs'

describe('clampFontSize 终端字号档位归一', () => {
  it('落在档位内的数字原样返回', () => {
    expect(clampFontSize(13)).toBe(13)
    expect(clampFontSize(16)).toBe(16)
  })

  it('字符串数字可解析时归一到档位', () => {
    expect(clampFontSize('12')).toBe(12)
  })

  it('非档位 / 非法输入回退默认字号', () => {
    expect(clampFontSize(20)).toBe(DEFAULT_FONT_SIZE)
    expect(clampFontSize(0)).toBe(DEFAULT_FONT_SIZE)
    expect(clampFontSize('abc')).toBe(DEFAULT_FONT_SIZE)
    expect(clampFontSize(undefined)).toBe(DEFAULT_FONT_SIZE)
    expect(clampFontSize(null)).toBe(DEFAULT_FONT_SIZE)
  })

  it('浮点取整后命中档位', () => {
    expect(clampFontSize(13.4)).toBe(13)
  })

  it('档位列表非空且包含默认值', () => {
    expect(FONT_SIZE_OPTIONS).toContain(DEFAULT_FONT_SIZE)
  })
})
