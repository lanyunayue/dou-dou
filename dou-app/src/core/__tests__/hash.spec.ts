import { describe, it, expect } from 'vitest'
import { hash8 } from '../../shared/hash'

describe('hash8 短指纹', () => {
  it('同输入结果确定', async () => {
    expect(await hash8('127.0.0.1:22')).toBe(await hash8('127.0.0.1:22'))
  })

  it('返回 8 位小写十六进制', async () => {
    const h = await hash8('dou')
    expect(h).toMatch(/^[0-9a-f]{8}$/)
  })

  it('不同输入得到不同指纹', async () => {
    expect(await hash8('host-a')).not.toBe(await hash8('host-b'))
  })

  it('命中 SHA-256 已知向量前 4 字节', async () => {
    // SHA-256("abc") = ba7816bf... 取前 4 字节
    expect(await hash8('abc')).toBe('ba7816bf')
  })

  it('空串不抛异常且仍出 8 位 hex', async () => {
    expect(await hash8('')).toMatch(/^[0-9a-f]{8}$/)
  })
})
