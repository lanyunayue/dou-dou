import { resetGwToken, gwError, refresh } from './useGateway'
import { fetchOnboard } from './useOnboard'
import { loadTasks } from './useApproval'
import { loadTokens } from './useCredentials'

// 网关统一重试：清令牌 → 刷网关状态 → 重拉各面板数据
export async function retryGw() {
  resetGwToken()
  gwError.value = null
  await refresh()
  await fetchOnboard()
  await loadTasks()
  await loadTokens()
}

export function useRecovery() {
  return { retryGw }
}
