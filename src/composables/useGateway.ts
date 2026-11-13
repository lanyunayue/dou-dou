import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { GwErrorInfo, GatewayStatus } from '../types'

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

// 唯一网关请求出口：走 Tauri IPC -> Rust reqwest（绕开 WebView CORS 与系统代理）。
// 令牌由 Rust 侧读 token.txt 并注入 Bearer；gwToken() 仅作 fail-closed 前置校验。
export async function gwFetch(path: string, opts: RequestInit = {}): Promise<Response> {
  await gwToken()                       // 失败即中止，绝不发无令牌请求
  const method = String(opts.method || 'GET').toUpperCase()
  const body = opts.body == null ? null : String(opts.body)
  let status = 0
  let text = ''
  try {
    const r = await invoke<{ status: number, body: string }>('gateway_request', { path, method, body })
    status = r.status
    text = r.body
  } catch (e) {
    gwError.value = { code: 'GATEWAY_OFFLINE', message: `网关离线：无法连接 127.0.0.1:7419（${String(e).slice(0, 90)}）`, retryable: true }
    throw new Error('网关离线')
  }
  if (status === 401) {
    gwError.value = { code: 'AUTH_FAILED', message: '鉴权失败（401）：令牌与网关不一致，请检查 DOU_DEV_TOKEN 与 token.txt 是否同源', retryable: true }
  } else if (status === 429) {
    gwError.value = { code: 'THROTTLED', message: '请求被限流（429）：请按 Retry-After 稍后重试，勿连续刷新', retryable: true }
  } else if (status < 200 || status >= 300) {
    gwError.value = { code: 'GATEWAY_OFFLINE', message: `网关返回异常状态 ${status}`, retryable: true }
  } else {
    gwError.value = null
  }
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => JSON.parse(text),
    text: async () => text,
  } as Response
}

// 占位符本地替换：仅用于展示 / 复制，绝不回传网络（/onboard/write 只 POST {client}）
const TOKEN_PLACEHOLDER = '${DOU_TOKEN}'
export async function replaceTokenPlaceholder(snippet: string): Promise<string> {
  if (!snippet.includes(TOKEN_PLACEHOLDER)) return snippet
  const t = await gwToken()
  return snippet.split(TOKEN_PLACEHOLDER).join(t)
}

// 状态灯分层：/health 探活成功 ≠ 所有功能可用（探活 / 鉴权 / 靶机三层单一真源）
export const connSummary = computed(() => {
  if (!status.value.running) return '网关未就绪 · HTTP 管理功能不可用'
  if (gwError.value) return `网关在线 · ${gwError.value.message}`
  // 以 Rust 侧对 22 端口的真实探测为准，绝不因「没报过错」就写已连接。
  const ssh = status.value.ssh_up
    ? '靶机在线（终端/文件/传输可用）'
    : '靶机未连接（22 端口不可达，终端/文件/传输不可用）'
  return `网关在线（HTTP 管理可用）· ${ssh}`
})

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
