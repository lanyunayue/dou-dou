import { describe, it, expect } from 'vitest'
import { validateConnForm, connFormValid, isValidPort } from '../../shared/connForm'

const base = { host: '192.168.1.10', port: '22', user: 'root', auth: 'key' as const }

describe('validateConnForm 连接表单校验', () => {
  it('合法密钥表单通过', () => {
    expect(validateConnForm(base)).toEqual({})
    expect(connFormValid(validateConnForm(base))).toBe(true)
  })

  it('主机必填且不能含空格', () => {
    expect(validateConnForm({ ...base, host: '  ' }).host).toBe('请填写主机地址')
    expect(validateConnForm({ ...base, host: '192.168.1.10 x' }).host).toBe('主机地址不能包含空格')
  })

  it('端口必填且限定 1–65535', () => {
    expect(validateConnForm({ ...base, port: '' }).port).toBe('请填写端口')
    expect(validateConnForm({ ...base, port: 'abc' }).port).toBe('端口需为 1–65535 的数字')
    expect(validateConnForm({ ...base, port: '0' }).port).toBe('端口需为 1–65535 的数字')
    expect(validateConnForm({ ...base, port: '65536' }).port).toBe('端口需为 1–65535 的数字')
    expect(validateConnForm({ ...base, port: '22.5' }).port).toBe('端口需为 1–65535 的数字')
  })

  it('用户必填', () => {
    expect(validateConnForm({ ...base, user: '  ' }).user).toBe('请填写用户名')
  })

  it('密码模式密码必填，密钥模式不校验密码', () => {
    expect(validateConnForm({ ...base, auth: 'password', password: '' }).password).toBe('密码认证需填写密码')
    expect(validateConnForm({ ...base, auth: 'password', password: '  ' }).password).toBe('密码认证需填写密码')
    expect(validateConnForm({ ...base, auth: 'password', password: 'secret' })).toEqual({})
    expect(validateConnForm({ ...base, auth: 'key', password: '' })).toEqual({})
  })
})

describe('isValidPort 端口判定', () => {
  it('边界值', () => {
    expect(isValidPort('1')).toBe(true)
    expect(isValidPort('22')).toBe(true)
    expect(isValidPort('65535')).toBe(true)
  })
  it('非法值', () => {
    expect(isValidPort('0')).toBe(false)
    expect(isValidPort('65536')).toBe(false)
    expect(isValidPort('')).toBe(false)
    expect(isValidPort('abc')).toBe(false)
    expect(isValidPort(' 22 ')).toBe(true)
  })
})
