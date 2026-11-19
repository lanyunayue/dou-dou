import type { App } from 'vue'
import { gwError } from '../adapters/useGateway'
import { notifyError } from '../shared/feedback'

/** 从任意异常值提取人话消息（纯函数，可单测）。 */
export function errMessage(e: unknown): string {
  if (e instanceof Error) return e.message
  if (e == null) return '未知错误'
  return String(e)
}

function reportUncaught(kind: string, e: unknown) {
  const msg = errMessage(e)
  // 进统一错误条（不可自动重试），并弹一次明确提示，避免静默白屏。
  gwError.value = { code: 'UNCAUGHT', message: `${kind}：${msg}`, retryable: false }
  notifyError(`${kind}：${msg}`)
}

/** Vue 顶层错误处理器：兜底未被 ErrorBoundary 捕获的组件渲染 / 生命周期错误。 */
export function installAppErrorHandler(app: App): void {
  app.config.errorHandler = (err, _instance, info) => {
    reportUncaught(`组件错误（${info}）`, err)
  }
}

/** 绑定 window 级未捕获错误与未处理 Promise 拒绝（WebView / 浏览器环境）。 */
export function installGlobalErrorHandlers(): void {
  if (typeof window === 'undefined') return

  window.addEventListener('error', (event) => {
    reportUncaught('界面发生未预期错误', (event as ErrorEvent)?.message ?? event)
  })

  window.addEventListener('unhandledrejection', (event) => {
    reportUncaught('未处理的异步错误', (event as PromiseRejectionEvent)?.reason)
  })
}
