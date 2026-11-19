/**
 * 审计 verdict 语义分类（纯函数，无副作用）。
 *
 * 后端审计 verdict 取值来自两个维度，这里归并为三档展示色：
 * - 判定维度：auto（只读自动放行）/ pending_approval（待审）/ blocked（阻断）
 * - 鉴权结果：allowed / denied（工具、路径、会话锁拒绝）
 * - 历史兼容别名：allow / granted / ok / deny
 *
 * 执行结果类（done / failed）属退出码语义，不在此判定为安全拒绝，统一落 ask。
 */

export type VerdictTone = 'allow' | 'deny' | 'ask'

const ALLOW = new Set(['auto', 'allow', 'allowed', 'granted', 'ok'])
const DENY = new Set(['deny', 'denied', 'blocked'])

/** 把后端 verdict 字符串归并为 allow（绿）/ deny（红）/ ask（琥珀）。大小写不敏感，空值落 ask。 */
export function verdictTone(v?: string): VerdictTone {
  const k = (v ?? '').trim().toLowerCase()
  if (ALLOW.has(k)) return 'allow'
  if (DENY.has(k)) return 'deny'
  return 'ask'
}

/** tone → 审计条目 CSS 类（allow 绿 / deny 红 / ask 琥珀）。 */
export function verdictClass(v?: string): string {
  const tone = verdictTone(v)
  return tone === 'allow' ? 'v-allow' : tone === 'deny' ? 'v-deny' : 'v-ask'
}
