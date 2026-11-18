import { describe, it, expect } from 'vitest'
import { LARGE_FILE_LIMIT, isLargeFile, largeFileNotice } from '../../shared/editorFile'

describe('isLargeFile 阈值（2MB）', () => {
  it('2MB 整及以下不算大文件', () => {
    expect(isLargeFile(LARGE_FILE_LIMIT)).toBe(false)
    expect(isLargeFile(1024 * 1024)).toBe(false)
  })
  it('超过 2MB 判为大文件', () => {
    expect(isLargeFile(LARGE_FILE_LIMIT + 1)).toBe(true)
    expect(isLargeFile(5 * 1024 * 1024)).toBe(true)
  })
})

describe('largeFileNotice 文案', () => {
  it('按 MB 一位小数展示', () => {
    expect(largeFileNotice(2 * 1024 * 1024)).toBe('// 文件较大（2.0 MB），已懒加载，不直接展开全文')
    expect(largeFileNotice(3 * 1024 * 1024)).toContain('3.0 MB')
  })
})
