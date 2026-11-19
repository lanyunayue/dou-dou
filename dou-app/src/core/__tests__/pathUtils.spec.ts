import { describe, it, expect } from 'vitest'
import { baseName, joinRemote, parsePwdOutput, parentRemote } from '../../shared/pathUtils'

describe('baseName', () => {
  it('取正斜杠路径末段', () => {
    expect(baseName('/home/dou/a.txt')).toBe('a.txt')
  })
  it('兼容反斜杠路径', () => {
    expect(baseName('C:\\Users\\dou\\b.zip')).toBe('b.zip')
  })
  it('纯文件名原样返回', () => {
    expect(baseName('c.txt')).toBe('c.txt')
  })
  it('空路径返回空串', () => {
    expect(baseName('')).toBe('')
  })
})

describe('joinRemote', () => {
  it('目录与文件名拼接', () => {
    expect(joinRemote('/home/dou', 'a.txt')).toBe('/home/dou/a.txt')
  })
  it('去掉目录尾斜杠与文件名首斜杠', () => {
    expect(joinRemote('/home/dou/', '/a.txt')).toBe('/home/dou/a.txt')
  })
  it('根目录拼接', () => {
    expect(joinRemote('/', 'pub')).toBe('/pub')
  })
  it('空文件名回退目录本身', () => {
    expect(joinRemote('/home/dou', '')).toBe('/home/dou')
  })
})

describe('parsePwdOutput', () => {
  it('取干净 pwd 输出', () => {
    expect(parsePwdOutput('/home/dou')).toBe('/home/dou')
  })
  it('剥离 ANSI 转义后取路径', () => {
    expect(parsePwdOutput('\x1b[1;32m/home/dou\x1b[0m')).toBe('/home/dou')
  })
  it('多行输出取最后一个绝对路径', () => {
    expect(parsePwdOutput('last login\n/tmp\n/home/dou')).toBe('/home/dou')
  })
  it('相对路径或空输出返回 null', () => {
    expect(parsePwdOutput('relative-dir')).toBeNull()
    expect(parsePwdOutput('')).toBeNull()
  })
})

describe('parentRemote', () => {
  it('根目录与空路径返回 /', () => {
    expect(parentRemote('/')).toBe('/')
    expect(parentRemote('')).toBe('/')
  })
  it('单层目录回到 /', () => {
    expect(parentRemote('/home')).toBe('/')
  })
  it('多级目录去掉末段', () => {
    expect(parentRemote('/home/dou')).toBe('/home')
    expect(parentRemote('/a/b/c')).toBe('/a/b')
  })
  it('忽略尾斜杠', () => {
    expect(parentRemote('/home/dou/')).toBe('/home')
  })
})
