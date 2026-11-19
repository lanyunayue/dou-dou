/**
 * 标识指纹纯函数（无副作用、仅依赖 WebCrypto）。
 */

/** 取 SHA-256 前 4 字节，拼成 8 位小写十六进制短指纹；用于去标识化的侧边展示。 */
export async function hash8(s: string): Promise<string> {
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(s))
  return Array.from(new Uint8Array(buf))
    .slice(0, 4)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}
