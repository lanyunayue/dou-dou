import { sshError } from './useSsh'
import { connectSsh } from './useTerminal'
import { cwd, loadFiles } from './useFileTree'

// 靶机重试：先重连靶机，再按当前目录重新拉取文件列表
export async function retrySsh() {
  sshError.value = ''
  await connectSsh()
  await loadFiles(cwd.value || '/home/dou')
}

export function useWorkspace() {
  return { retrySsh }
}
