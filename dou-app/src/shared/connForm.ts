/**
 * 连接表单内联校验纯函数（无副作用，可独立单测）。
 *
 * 只做格式与必填校验，不发起任何连接；错误按字段返回人话提示，
 * 由表单层内联展示并据此禁用提交按钮。
 */

export interface ConnFormInput {
  host: string
  port: string
  user: string
  auth: 'key' | 'password'
  password?: string
}

export interface ConnFormErrors {
  host?: string
  port?: string
  user?: string
  password?: string
}

/** 端口合法区间：1–65535。 */
export function isValidPort(port: string): boolean {
  const t = port.trim()
  if (!/^\d+$/.test(t)) return false
  const n = Number(t)
  return Number.isInteger(n) && n >= 1 && n <= 65535
}

/**
 * 校验连接表单，返回字段级错误；空对象表示通过。
 * 主机只做「非空 + 不含空白」的宽松校验，不强行限制内网域名 / IPv6 写法。
 */
export function validateConnForm(input: ConnFormInput): ConnFormErrors {
  const errors: ConnFormErrors = {}

  const host = input.host.trim()
  if (!host) {
    errors.host = '请填写主机地址'
  } else if (/\s/.test(host)) {
    errors.host = '主机地址不能包含空格'
  }

  if (!input.port.trim()) {
    errors.port = '请填写端口'
  } else if (!isValidPort(input.port)) {
    errors.port = '端口需为 1–65535 的数字'
  }

  if (!input.user.trim()) {
    errors.user = '请填写用户名'
  }

  if (input.auth === 'password' && !(input.password ?? '').trim()) {
    errors.password = '密码认证需填写密码'
  }

  return errors
}

/** 是否存在任意字段错误。 */
export function connFormValid(errors: ConnFormErrors): boolean {
  return Object.keys(errors).length === 0
}
