import { sshError, activeTarget } from '../core/useSsh'
import { connectSsh, showConnectForm, lastAuth } from '../core/useTerminal'
import { cwd, loadFiles, files } from '../core/useFileTree'

// 一键重试（R1 后语义）：
// - 密钥模式且已连接过 → 按原参数重连并刷新文件列表；
// - 密码模式 → 回到新建连接表单让用户重输密码（密码按设计不落盘，不能自动重放，
//   也不自动改用密钥，避免与用户刚才的认证方式不符）；
// - 从未连接 → 打开新建连接表单。
export async function retrySsh() {
  sshError.value = ''
  if (activeTarget.value && lastAuth.value === 'key') {
    const t = activeTarget.value
    await connectSsh({ host: t.host, port: t.port, user: t.user, auth: 'key' })
    await loadFiles(cwd.value || '/home/dou')
  } else {
    showConnectForm.value = true
    files.value = []
  }
}

export function useWorkspace() {
  return { retrySsh }
}
