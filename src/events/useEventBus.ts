// 事件总线（简单 mitt 替代）
// 保留原 App.vue 中的实现语义：emit 只负责把数据投递给同名订阅者。
const bus = new Map<string, Function[]>()

/** 订阅事件 */
export function on(evt: string, fn: Function) {
  const list = bus.get(evt) ?? []
  list.push(fn)
  bus.set(evt, list)
}

/** 取消订阅 */
export function off(evt: string, fn: Function) {
  const list = bus.get(evt)
  if (!list) return
  bus.set(evt, list.filter(f => f !== fn))
}

/** 广播事件 */
export function emit(evt: string, data?: any) {
  bus.get(evt)?.forEach(fn => fn(data))
}

export function useEventBus() {
  return { on, off, emit }
}
