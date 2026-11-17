/**
 * 统一反馈出口：全仓 alert()/confirm() 的唯一收敛点。
 *
 * 为什么用裸调用而非 window.alert：
 * 1. 浏览器（Tauri WebView）中裸 alert 即 window.alert，行为不变；
 * 2. 单测环境是 node（无 window），vi.stubGlobal('alert', mock) stub 的
 *    是 globalThis.alert —— 裸调用恰好命中桩，现有 12 例断言无需改动。
 *
 * S3-1 页面化之后，notify 可在 UI 层替换为 toast 渲染（保持签名不变）。
 */

/** 信息/结果反馈（原 alert()）。 */
export function notify(message: string): void {
  alert(message)
}

/** 需要用户确认的危险操作（原 confirm()），返回是否继续。 */
export function askConfirm(message: string): boolean {
  return confirm(message)
}
