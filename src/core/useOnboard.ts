import { ref } from 'vue'
import { gwFetch, gwError, replaceTokenPlaceholder } from '../adapters/useGateway'

// ---------- 接入向导 ----------
export const onboardClient = ref('workbuddy')
export const onboardSnippet = ref('')
export const onboardPaths = ref<string[]>([])
export const onboardNote = ref('')
export const writeResult = ref('')
export const onboardProbe = ref('')

export async function fetchOnboard() {
  try {
    const r = await gwFetch(`/onboard/config?client=${encodeURIComponent(onboardClient.value)}`)
    const j = await r.json()
    // ${DOU_TOKEN} 在本地替换为真实令牌，仅用于展示 / 复制；网关响应体仍是占位符
    onboardSnippet.value = await replaceTokenPlaceholder(j.snippet || '')
    onboardPaths.value = j.config_paths || []
    onboardNote.value = j.note || ''
  } catch {
    onboardSnippet.value = gwError.value
      ? `无法生成片段：${gwError.value.message}`
      : '无法生成片段：网关不可用'
  }
}

export async function copySnippet() {
  try {
    // 复制前再做一次本地替换，确保产物无 ${DOU_TOKEN} 残留
    const text = await replaceTokenPlaceholder(onboardSnippet.value)
    await navigator.clipboard.writeText(text)
    alert('片段已复制（Authorization 占位符已本地替换为可用令牌）')
  } catch {
    alert('复制失败：' + (gwError.value?.message || '令牌不可用'))
  }
}

export async function writeConfig() {
  writeResult.value = ''
  try {
    // 只 POST {client}；真令牌由网关服务端本地代写，前端绝不回传令牌 / 替换后片段
    const r = await gwFetch('/onboard/write', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ client: onboardClient.value })
    })
    const j = await r.json()
    if (j.written) {
      writeResult.value = `${j.message}｜生效文件: ${j.path}${j.backed_up ? `｜备份: ${j.backed_up}` : ''}`
    } else {
      writeResult.value = '代写未执行: ' + (j.error || '未知原因')
    }
  } catch {
    writeResult.value = '代写失败：' + (gwError.value?.message || '请求未完成')
  }
}

// 握手自检：轮询审计记录，确认外部客户端已完成 initialize 握手。
let probeTimer: ReturnType<typeof setInterval> | null = null
export function probeHandshake() {
  onboardProbe.value = '等待客户端连接…'
  const start = Date.now()
  let tries = 0
  if (probeTimer) clearInterval(probeTimer)
  probeTimer = setInterval(async () => {
    tries++
    try {
      const r = await gwFetch('/audit/query?source=agent&limit=30')
      if (r.status === 401 || r.status === 429) {
        // 鉴权 / 限流异常立即停止轮询，不风暴重试
        onboardProbe.value = `检测暂停：${gwError.value?.message || '鉴权/限流异常'}`
        if (probeTimer) clearInterval(probeTimer)
        return
      }
      const j = await r.json()
      const items = j.items || []
      const hit = items.find((x: any) => x.action === 'initialize' && Number(x.ts_ms) >= start - 2000)
      if (hit) {
        onboardProbe.value = '已检测到客户端握手，接入成功'
        if (probeTimer) clearInterval(probeTimer)
      } else if (tries >= 15) {
        onboardProbe.value = '超时未检测到连接，请检查配置后重试'
        if (probeTimer) clearInterval(probeTimer)
      }
    } catch {
      onboardProbe.value = `检测失败：${gwError.value?.message || '网关不可用'}`
      if (probeTimer) clearInterval(probeTimer)
    }
  }, 2000)
}

export function useOnboard() {
  return {
    onboardClient,
    onboardSnippet,
    onboardPaths,
    onboardNote,
    writeResult,
    onboardProbe,
    fetchOnboard,
    copySnippet,
    writeConfig,
    probeHandshake,
  }
}
