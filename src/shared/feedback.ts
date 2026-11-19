/**
 * 统一反馈出口：全仓 alert()/confirm() 的唯一收敛点。
 *
 * 浏览器（Tauri WebView）环境：信息 / 结果类反馈走非阻塞 toast（自动消失、可叠放），
 * 不再用阻塞模态 alert 打断操作节奏；危险操作确认仍走原生 confirm。
 * 单测环境是 node（无 DOM）：回退到裸 alert，vi.stubGlobal('alert', mock) 仍能命中断言。
 */
import { ref } from 'vue'

export type ToastKind = 'info' | 'success' | 'warn' | 'error'

export interface ToastItem {
  id: number
  kind: ToastKind
  message: string
}

/** 当前待展示的 toast（供 ToastHost 渲染）。 */
export const toasts = ref<ToastItem[]>([])

let toastSeq = 0

/** 各类 toast 的自动消失时长（毫秒）；错误 / 警告留更久便于读完。 */
const TOAST_DURATION: Record<ToastKind, number> = {
  info: 2600,
  success: 2600,
  warn: 4200,
  error: 4200,
}

/** 是否处于可用 DOM 环境；node 单测无 document，走 alert 回退。 */
function hasDom(): boolean {
  return typeof document !== 'undefined' && typeof document.createElement === 'function'
}

/** 手动关闭一条 toast。 */
export function dismissToast(id: number): void {
  toasts.value = toasts.value.filter(t => t.id !== id)
}

/**
 * 信息 / 结果反馈。默认非阻塞 toast；无 DOM 环境回退 alert。
 * @param kind 成功 / 警告 / 错误用对应配色，错误与警告停留更久。
 */
export function notify(message: string, kind: ToastKind = 'info'): void {
  if (!hasDom()) {
    alert(message)
    return
  }
  const id = ++toastSeq
  toasts.value.push({ id, kind, message })
  window.setTimeout(() => dismissToast(id), TOAST_DURATION[kind])
}

/** 成功反馈（绿色 toast）。 */
export function notifySuccess(message: string): void { notify(message, 'success') }

/** 警告反馈（黄色 toast）。 */
export function notifyWarn(message: string): void { notify(message, 'warn') }

/** 错误反馈（红色 toast，停留更久）。 */
export function notifyError(message: string): void { notify(message, 'error') }

/** 需要用户确认的危险操作（原 confirm()），返回是否继续。 */
export function askConfirm(message: string): boolean {
  return confirm(message)
}
