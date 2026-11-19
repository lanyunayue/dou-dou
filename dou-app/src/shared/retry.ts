/**
 * 重试 / 退避纯函数（无副作用，可独立单测）。
 *
 * 安全边界：自动重试只覆盖传输层瞬断与 5xx 瞬时故障；401 / 429 / 鉴权失败
 * 与其余 4xx 一律不自动重试，避免触发限流或绕过鉴权（fail-closed）。
 */

/** 指数退避延迟（毫秒），attempt 从 0 起；结果封顶 max。 */
export function backoffMs(attempt: number, base = 200, max = 1500): number {
  const a = Math.max(0, Math.floor(attempt))
  return Math.min(max, base * Math.pow(2, a))
}

/** 等待指定毫秒（单测可被 vi.useFakeTimers 控制）。 */
export function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/**
 * 该 HTTP 状态码是否允许自动重试。
 * 仅 5xx 瞬时故障可重试；401（鉴权）、429（限流）与其余 4xx 不重试。
 */
export function isRetryableHttpStatus(status: number): boolean {
  return status >= 500 && status <= 599
}

/**
 * 自动重试只允许幂等方法（GET / HEAD）。
 * POST / PUT / PATCH / DELETE 等写操作失败后可能服务端已生效，重放会造成
 * 重复放行 / 重复签发等副作用，一律交回用户手动重试。
 */
export function isIdempotentMethod(method: string): boolean {
  const m = String(method || 'GET').trim().toUpperCase()
  return m === 'GET' || m === 'HEAD'
}

/** 自动重试次数上限（传输层离线 / 5xx）。 */
export const MAX_AUTO_RETRY = 2
