/**
 * 档案认证方式展示标签纯函数（无副作用）。
 */

/**
 * 认证标签：有加密材料视为已加密；否则按方式区分密码认证 / 未配钥。
 * @param auth 认证方式 'key' | 'password'
 * @param encKey 入库加密材料，非空即视为已加密
 */
export function authLabel(auth: string | undefined, encKey: string | undefined | null): string {
  if (encKey) return '🔑 已加密'
  return auth === 'password' ? '🔒 密码认证（连接时输入密码）' : '— 未配钥'
}
