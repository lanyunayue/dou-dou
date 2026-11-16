import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { gwFetch } from './useGateway'

// ---------- 客户端终端信任库（Tauri 侧 known_hosts） ----------
export const hostKeys = ref<any[]>([])

export async function loadHostKeys() { try { hostKeys.value = await invoke('host_key_list') } catch { hostKeys.value = [] } }
export async function forgetHost(h: string, p: number) {
  if (!confirm(`确认遗忘 ${h}:${p} 的密钥记录？\n遗忘后下次连接将重新出现「首次记录」提示，请届时肉眼核对指纹。\n仅在你已通过其他渠道确认对方确实换了密钥（合法轮换）时才应继续。`)) return
  try { await invoke('host_key_forget', { host: h, port: p }); await loadHostKeys() }
  catch (e: any) { alert('遗忘失败: ' + e) }
}

// ---------- 网关信任库（dou-gateway 的 known_hosts.json，B2） ----------
// 网关与终端是两套独立信任库：这里经 gwFetch 管理网关侧的 TOFU 记录。
export const gatewayHostKeys = ref<{ addr: string, key: string }[]>([])

export async function loadGatewayHostKeys() {
  try {
    const r = await gwFetch('/hostkey/list')
    if (!r.ok) { gatewayHostKeys.value = []; return }
    const v = await r.json()
    gatewayHostKeys.value = v.items || []
  } catch { gatewayHostKeys.value = [] }
}

export async function forgetGatewayHost(addr: string) {
  if (!confirm(`确认遗忘网关信任库中的 ${addr} ？\n遗忘后下次网关连接将重新走首次记录（TOFU），请届时肉眼核对指纹。\n仅在你已通过其他渠道确认对方确实换了密钥（合法轮换）时才应继续。`)) return
  try {
    const r = await gwFetch('/hostkey/forget', { method: 'POST', body: JSON.stringify({ addr }) })
    if (!r.ok) { alert('遗忘失败：网关返回异常状态 ' + r.status); return }
    await loadGatewayHostKeys()
  } catch (e: any) { alert('遗忘失败: ' + e) }
}

// host_key_event 订阅：TOFU 首次记录 / 密钥变更告警
export async function registerHostKeyListener(): Promise<UnlistenFn> {
  return await listen('host_key_event', (e: any) => {
    const p = e.payload
    if (p.kind === 'first_seen') {
      alert(`新主机密钥（首次连接，请肉眼确认指纹）\n${p.host}:${p.port}\nSHA256: ${p.fingerprint}`)
    } else if (p.kind === 'mismatch') {
      alert(`⚠ 主机密钥变更 / 疑似中间人！已拒绝连接\n${p.host}:${p.port}\n记录: ${p.expected}\n收到: ${p.got}\n\n请先通过其他渠道人工核实对方身份；确认合法换钥后，到「主机密钥」页遗忘旧记录再重连重新确认指纹。无法核实则保持拒绝。`)
    }
    loadHostKeys()
  })
}

export function useHostKeys() {
  return { hostKeys, gatewayHostKeys, loadHostKeys, loadGatewayHostKeys, forgetHost, forgetGatewayHost }
}
