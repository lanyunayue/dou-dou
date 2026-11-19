import { describe, it, expect } from 'vitest'
import { filterVisibleFiles } from '../../shared/fileFilter'

const files = [
  { name: 'README.md', is_dir: false },
  { name: '.hidden', is_dir: false },
  { name: 'src', is_dir: true },
  { name: 'package.json', is_dir: false },
]

describe('filterVisibleFiles 隐藏文件', () => {
  it('默认剔除点开头隐藏文件', () => {
    const out = filterVisibleFiles(files, false, '')
    expect(out.map(f => f.name)).toEqual(['README.md', 'src', 'package.json'])
  })
  it('showHidden 为 true 保留隐藏文件', () => {
    const out = filterVisibleFiles(files, true, '')
    expect(out).toHaveLength(4)
  })
})

describe('filterVisibleFiles 关键词', () => {
  it('大小写不敏感子串匹配', () => {
    expect(filterVisibleFiles(files, true, 'PACK').map(f => f.name)).toEqual(['package.json'])
    expect(filterVisibleFiles(files, true, 'readme').map(f => f.name)).toEqual(['README.md'])
  })
  it('空白查询不过滤', () => {
    expect(filterVisibleFiles(files, true, '   ')).toHaveLength(4)
  })
  it('不修改入参数组', () => {
    const snapshot = files.map(f => f.name)
    filterVisibleFiles(files, false, 'src')
    expect(files.map(f => f.name)).toEqual(snapshot)
  })
})
