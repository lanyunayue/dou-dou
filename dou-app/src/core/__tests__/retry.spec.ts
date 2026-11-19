import { describe, it, expect } from 'vitest'
import { backoffMs, isRetryableHttpStatus, isIdempotentMethod, MAX_AUTO_RETRY } from '../../shared/retry'

describe('backoffMs 指数退避', () => {
  it('按 2 的幂增长并封顶', () => {
    expect(backoffMs(0, 200, 1500)).toBe(200)
    expect(backoffMs(1, 200, 1500)).toBe(400)
    expect(backoffMs(2, 200, 1500)).toBe(800)
    expect(backoffMs(5, 200, 1500)).toBe(1500)
  })
  it('非法 attempt 回落 0 档', () => {
    expect(backoffMs(-3, 200, 1500)).toBe(200)
  })
})

describe('isRetryableHttpStatus 可重试状态', () => {
  it('仅 5xx 可重试', () => {
    expect(isRetryableHttpStatus(500)).toBe(true)
    expect(isRetryableHttpStatus(502)).toBe(true)
    expect(isRetryableHttpStatus(503)).toBe(true)
    expect(isRetryableHttpStatus(599)).toBe(true)
  })
  it('鉴权 / 限流 / 4xx / 2xx 不重试', () => {
    expect(isRetryableHttpStatus(401)).toBe(false)
    expect(isRetryableHttpStatus(429)).toBe(false)
    expect(isRetryableHttpStatus(403)).toBe(false)
    expect(isRetryableHttpStatus(404)).toBe(false)
    expect(isRetryableHttpStatus(200)).toBe(false)
  })
  it('重试上限为 2', () => {
    expect(MAX_AUTO_RETRY).toBe(2)
  })
})

describe('isIdempotentMethod 可自动重试的幂等方法', () => {
  it('仅 GET / HEAD 可重试（大小写与空白归一）', () => {
    expect(isIdempotentMethod('GET')).toBe(true)
    expect(isIdempotentMethod('get')).toBe(true)
    expect(isIdempotentMethod(' HEAD ')).toBe(true)
    expect(isIdempotentMethod('')).toBe(true) // 默认 GET
  })
  it('写操作一律不重试，防止重复放行 / 签发', () => {
    expect(isIdempotentMethod('POST')).toBe(false)
    expect(isIdempotentMethod('PUT')).toBe(false)
    expect(isIdempotentMethod('PATCH')).toBe(false)
    expect(isIdempotentMethod('DELETE')).toBe(false)
  })
})
