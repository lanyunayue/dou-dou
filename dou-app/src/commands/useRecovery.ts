import { resetGwToken, gwError, refresh } from '../adapters/useGateway'
import { fetchOnboard } from '../core/useOnboard'
import { loadTasks } from '../core/useApproval'
import { loadTokens } from '../core/useCredentials'
import { loadTq } from '../core/useTransferQueue'
import { loadHostKeys, loadGatewayHostKeys } from '../core/useHostKeys'

// 网关统一重试：清令牌 → 刷网关状态 → 重拉各面板数据。
// 审计/策略/实验快照的加载在各自面板内，StateShell 自带一键重试；
// 此处覆盖全局常驻数据（向导/任务/令牌/传输队列/主机密钥）。
export async function retryGw() {
  resetGwToken()
  gwError.value = null
  await refresh()
  await fetchOnboard()
  await loadTasks()
  await loadTokens()
  await loadTq()
  await loadHostKeys()
  await loadGatewayHostKeys()
}

export function useRecovery() {
  return { retryGw }
}
