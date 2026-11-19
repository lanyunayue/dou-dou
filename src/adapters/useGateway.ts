import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { GwErrorInfo, GatewayStatus } from '../shared/types'
import { summarizeConn } from '../shared/connSummary'
import { backoffMs, sleep, isRetryableHttpStatus, isIdempotentMethod, MAX_AUTO_RETRY } from '../shared/retry'

// 网关地址固定由 Rust 侧 `gateway_request` 使用 http://127.0.0.1:7419，
// 前端不再直连 fetch（WebView 跨域 CORS 会挡住响应读取）。

export const status = ref<GatewayStatus>({ running: false, endpoint: 'http://127.0.0.1:7419/health', ssh_up: false })
export const gwError = ref<GwErrorInfo | null>(null)

// ---------- 统一鉴权请求层（唯一 fetch 出口） ----------
// 令牌只在模块内存态缓存，绝不写 localStorage / 日志 / 源码 / 提交 / URL / body。
let _gwTokenCache: string | null = null
let _gwTokenPending: Promise<string> | null = null

export function resetGwToken() { _gwTokenCache = null; _gwTokenPending = null }
export function dismissGwError() { gwError.value = null }
export function markGwFailure(message: string) {
  if (!gwError.value) gwError.value = { code: 'GATEWAY_OFFLINE', message, retryable: true }
}

// fail-closed：取令牌失败即 throw，绝不发无令牌请求（否则 401→429，正是本轮要修的根因）
export async function gwToken(): Promise<string> {
  if (_gwTokenCache) return _gwTokenCache
  if (_gwTokenPending) return _gwTokenPending
  _gwTokenPending = (async () => {
    try {
      const t = await invoke<string>('gateway_token')
      if (!t || !t.startsWith('dou_ak_')) throw new Error('invalid token')
      _gwTokenCache = t
      return t
    } catch {
      gwError.value = {
        code: 'TOKEN_UNAVAILABLE',
        message: '令牌不可用：无法读取 %LOCALAPPDATA%/dou/token.txt，请确认网关已启动并生成主令牌。已中止全部请求（fail-closed），不会发送无令牌请求。',
        retryable: true
      }
      throw new Error('令牌不可用，请检查 dou/token.txt')
    } finally {
      _gwTokenPending = null
    }
  })()
  return _gwTokenPending
}

function pseudoResponse(status: number, text: string): Response {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => JSON.parse(text),
    text: async () => text,
  } as Response
}

// 唯一网关请求出口：走 Tauri IPC -> Rust reqwest（绕开 WebView CORS 与系统代理）。
// 令牌由 Rust 侧读 token.txt 并注入 Bearer；gwToken() 仅作 fail-closed 前置校验。
// 自动重试仅覆盖「幂等方法（GET/HEAD）」的传输层瞬断与 5xx；写操作（POST 等）
// 失败后服务端可能已生效，重放会造成重复放行/签发，一律交用户手动重试。
// 401/429/鉴权失败同样不重试（fail-closed，不触发限流）。
export async function gwFetch(path: string, opts: RequestInit = {}): Promise<Response> {
  await gwToken()                       // 失败即中止，绝不发无令牌请求
  const method = String(opts.method || 'GET').toUpperCase()
  const canAutoRetry = isIdempotentMethod(method)
  const body = opts.body == null ? null : String(opts.body)
  let status = 0
  let text = ''

  for (let attempt = 0; attempt <= MAX_AUTO_RETRY; attempt++) {
    try {
      const r = await invoke<{ status: number, body: string }>('gateway_request', { path, method, body })
      status = r.status
      text = r.body
    } catch (e) {
      // 传输层离线 / IPC 瞬断：仅幂等方法且尚有次数才退避重试；写操作直接报错交用户。
      if (canAutoRetry && attempt < MAX_AUTO_RETRY) { await sleep(backoffMs(attempt)); continue }
      gwError.value = { code: 'GATEWAY_OFFLINE', message: `网关离线：无法连接 127.0.0.1:7419（${String(e).slice(0, 90)}）`, retryable: true }
      throw new Error('网关离线')
    }

    if (status === 401) {
      gwError.value = { code: 'AUTH_FAILED', message: '鉴权失败（401）：令牌与网关不一致，请检查 DOU_DEV_TOKEN 与 token.txt 是否同源', retryable: true }
      break
    }
    if (status === 429) {
      gwError.value = { code: 'THROTTLED', message: '请求被限流（429）：请按 Retry-After 稍后重试，勿连续刷新', retryable: true }
      break
    }
    if (canAutoRetry && isRetryableHttpStatus(status) && attempt < MAX_AUTO_RETRY) {
      await sleep(backoffMs(attempt))
      continue
    }
    if (status < 200 || status >= 300) {
      gwError.value = { code: 'GATEWAY_OFFLINE', message: `网关返回异常状态 ${status}`, retryable: true }
      break
    }
    gwError.value = null
    return pseudoResponse(status, text)
  }
  return pseudoResponse(status, text)
}

// 占位符本地替换：仅用于展示 / 复制，绝不回传网络（/onboard/write 只 POST {client}）
const TOKEN_PLACEHOLDER = '${DOU_TOKEN}'
export async function replaceTokenPlaceholder(snippet: string): Promise<string> {
  if (!snippet.includes(TOKEN_PLACEHOLDER)) return snippet
  const t = await gwToken()
  return snippet.split(TOKEN_PLACEHOLDER).join(t)
}

// 状态灯分层：/health 探活成功 ≠ 所有功能可用（探活 / 鉴权 / 远程主机三层单一真源）
export const connSummary = computed(() =>
  summarizeConn(status.value.running, status.value.ssh_up, gwError.value),
)

export async function refresh() { status.value = await invoke('gateway_status') }

export function useGateway() {
  return {
    status,
    gwError,
    connSummary,
    gwToken,
    gwFetch,
    markGwFailure,
    dismissGwError,
    resetGwToken,
    replaceTokenPlaceholder,
    refresh,
  }
}
