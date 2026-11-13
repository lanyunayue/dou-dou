import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

// ---------- 主机密钥 ----------
export const hostKeys = ref<any[]>([])

export async function loadHostKeys() { try { hostKeys.value = await invoke('host_key_list') } catch { hostKeys.value = [] } }
export async function forgetHost(h: string, p: number) {
  try { await invoke('host_key_forget', { host: h, port: p }); await loadHostKeys() }
  catch (e: any) { alert('遗忘失败: ' + e) }
}

// host_key_event 订阅：TOFU 首次记录 / 密钥变更告警
export async function registerHostKeyListener(): Promise<UnlistenFn> {
  return await listen('host_key_event', (e: any) => {
    const p = e.payload
    if (p.kind === 'first_seen') {
      alert(`新主机密钥（首次连接，请肉眼确认指纹）\n${p.host}:${p.port}\nSHA256: ${p.fingerprint}`)
    } else if (p.kind === 'mismatch') {
      alert(`⚠ 主机密钥变更 / 疑似中间人！已拒绝连接\n${p.host}:${p.port}\n记录: ${p.expected}\n收到: ${p.got}`)
    }
    loadHostKeys()
  })
}

export function useHostKeys() {
  return { hostKeys, loadHostKeys, forgetHost }
}
