import { ref } from 'vue'
import { gwFetch, gwError, markGwFailure } from './useGateway'

// ---------- 凭据与作用域 ----------
export const tokens = ref<any[]>([])
export const issueTier = ref('observe')
export const issueClient = ref('web')
export const issuedToken = ref('')

export async function loadTokens() {
  try {
    const r = await gwFetch('/scope/tokens')
    const j = await r.json()
    tokens.value = j.items || []
  } catch {
    tokens.value = []
    markGwFailure('加载凭据列表失败：网关不可用')  // 不静默吞错
  }
}

export async function issueToken() {
  issuedToken.value = ''
  try {
    const r = await gwFetch('/scope/tokens', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tier: issueTier.value, client_id: issueClient.value })
    })
    const j = await r.json()
    if (j.token) { issuedToken.value = j.token; await loadTokens() }
    else alert('签发失败: ' + (j.error || ''))
  } catch {
    alert('签发失败：' + (gwError.value?.message || '请求未完成'))
  }
}

export async function revokeToken(id: string) {
  if (!confirm('吊销后下一次请求即被拒，记录保留。确认？')) return
  try {
    await gwFetch('/scope/revoke', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id })
    })
    await loadTokens()
  } catch {
    alert('吊销失败：' + (gwError.value?.message || '请求未完成'))
  }
}

export async function copyIssued() { await navigator.clipboard.writeText(issuedToken.value); alert('已复制') }

export function fmtTime(sec: number) { return sec ? new Date(sec * 1000).toLocaleString() : '-' }

export function useCredentials() {
  return { tokens, issueTier, issueClient, issuedToken, loadTokens, issueToken, revokeToken, copyIssued, fmtTime }
}
