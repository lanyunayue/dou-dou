import { sshError, activeTarget } from '../core/useSsh'
import { connectSsh, showConnectForm } from '../core/useTerminal'
import { cwd, loadFiles, files } from '../core/useFileTree'

// 一键重试（R1 后语义）：密钥模式且已连接过 → 直接按原参数重连并刷新文件列表；
// 密码模式或从未连接 → 打开新建连接表单（密码按设计不落盘，不能自动重放）
export async function retrySsh() {
  sshError.value = ''
  if (activeTarget.value) {
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
