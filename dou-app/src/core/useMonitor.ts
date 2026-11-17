// 主机监控（host_stats 事件流，Rust 侧 monitor.rs 每 5s 经 /proc 采集一次）。
// 数据仅内存态：会话断开（host_stats_off）即清空；R7：非 Linux 主机（host_stats_unsupported）
// 显示边界说明而不是静默隐藏，让用户知道是「不支持」而非「坏了」。
import { ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { HostStats } from '../shared/types'

export const hostStats = ref<HostStats | null>(null)
// R7：主机边界说明（非 Linux / 无 /proc 时由 Rust 侧探测给出）
export const monitorUnsupported = ref('')

export async function registerMonitorListener(): Promise<UnlistenFn[]> {
  const offStats = await listen<HostStats>('host_stats', (e) => {
    hostStats.value = e.payload
    monitorUnsupported.value = ''
  })
  const off = await listen<unknown>('host_stats_off', () => {
    hostStats.value = null
    monitorUnsupported.value = ''
  })
  const unsup = await listen<{ os: string }>('host_stats_unsupported', (e) => {
    hostStats.value = null
    const os = e.payload?.os?.trim() || '未知系统'
    monitorUnsupported.value = `监控暂不支持该主机系统（${os}）——监控条当前仅支持 Linux 主机，其余功能不受影响`
  })
  return [offStats, off, unsup]
}
