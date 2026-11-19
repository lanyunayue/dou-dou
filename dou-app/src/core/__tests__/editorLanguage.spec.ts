import { describe, it, expect } from 'vitest'
import { languageForFile } from '../editorLanguage'

describe('languageForFile', () => {
  it('映射 Python', () => {
    expect(languageForFile('app.py')).toBe('python')
  })
  it('映射 yaml（yml / yaml）', () => {
    expect(languageForFile('a.yml')).toBe('yaml')
    expect(languageForFile('b.yaml')).toBe('yaml')
  })
  it('映射 json / markdown / log', () => {
    expect(languageForFile('c.json')).toBe('dou-json')
    expect(languageForFile('d.md')).toBe('markdown')
    expect(languageForFile('e.markdown')).toBe('markdown')
    expect(languageForFile('f.log')).toBe('dou-log')
    expect(languageForFile('g.out')).toBe('dou-log')
  })
  it('映射配置类扩展名', () => {
    for (const ext of ['conf', 'cfg', 'config', 'ini', 'env', 'service']) {
      expect(languageForFile(`h.${ext}`), ext).toBe('dou-conf')
    }
  })
  it('忽略扩展名大小写', () => {
    expect(languageForFile('APP.PY')).toBe('python')
    expect(languageForFile('Data.JSON')).toBe('dou-json')
  })
  it('无扩展名或未知类型回退 plaintext', () => {
    expect(languageForFile('README')).toBe('plaintext')
    expect(languageForFile('x.xyz')).toBe('plaintext')
    expect(languageForFile('')).toBe('plaintext')
  })
})
