/**
 * 连接状态摘要（纯函数）。三层单一真源：探活 running / 鉴权错误 error / SSH 探测 sshUp，
 * 不因「没报过错」就写已连接。error 只需携带 message（GwErrorInfo 的结构子集）。
 */
export function summarizeConn(
  running: boolean,
  sshUp: boolean,
  error: { message: string } | null,
): string {
  if (!running) return '网关未就绪 · HTTP 管理功能不可用'
  if (error) return `网关在线 · ${error.message}`
  const ssh = sshUp
    ? '远程主机已连接（终端/文件/传输可用）'
    : '远程主机未连接（终端/文件/传输暂不可用）'
  return `网关在线（HTTP 管理可用）· ${ssh}`
}
