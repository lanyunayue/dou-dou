// 主机监控（host_stats 事件流，Rust 侧 monitor.rs 每 5s 经 /proc 采集一次）。
// 数据仅内存态：会话断开（host_stats_off）即清空，前端监控条随之隐藏。
import { ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { HostStats } from '../shared/types'

export const hostStats = ref<HostStats | null>(null)

export async function registerMonitorListener(): Promise<UnlistenFn[]> {
  const offStats = await listen<HostStats>('host_stats', (e) => {
    hostStats.value = e.payload
  })
  const off = await listen<unknown>('host_stats_off', () => {
    hostStats.value = null
  })
  return [offStats, off]
}
