import { ref } from 'vue'

// SSH 靶机（WSL2；本机回环中继对 russh 连接不稳定，直连 WSL 虚拟网卡 IP）
export const SSH_HOST = '127.0.0.1'
export const SSH_PORT = 22
export const SSH_USER = 'dou'
// 不再钉死任何机器的用户名/盘符：留空时由 Rust 侧 ssh::ssh_target() 依据
// DOU_SSH_KEY / LOCALAPPDATA 推导；也可在接入向导里手动填写，或用
// dou-app/.env.local 的 VITE_DOU_SSH_KEY 覆盖默认值。
export const SSH_KEY = (import.meta.env.VITE_DOU_SSH_KEY as string | undefined) ?? ''

// 靶机离线原因：把裸 `os error 10061` 等转成人话，绝不渲染原始错误串
export function sshOfflineReason(e: any): string {
  const s = String(e?.message ?? e ?? '')
  if (/10061|refused|连接被拒绝/i.test(s)) return '连接被拒绝：目标 127.0.0.1:22 未监听（靶机未启动）'
  if (/timed out|timeout|超时/i.test(s)) return '连接超时：靶机不可达'
  if (/authentication|auth failed|认证/i.test(s)) return 'SSH 认证失败：密钥或账户不匹配'
  if (/no such file|not found|key|密钥/i.test(s)) return 'SSH 密钥不可用：未找到 id_ed25519'
  return s ? `连接失败：${s.slice(0, 100)}` : '连接失败：靶机不可用'
}

export const sshError = ref('')

// 审计轨迹（内存态，不落盘）
export const audit = ref<string[]>(['启动豆 Dou 桌面壳', '网关连接成功'])

export function handleSshFailure(e: any, ctx: string) {
  sshError.value = sshOfflineReason(e)
  audit.value.unshift(`[${new Date().toLocaleTimeString()}] ${ctx}失败：${sshError.value}`)
}

export function useSsh() {
  return {
    SSH_HOST,
    SSH_PORT,
    SSH_USER,
    SSH_KEY,
    sshError,
    audit,
    sshOfflineReason,
    handleSshFailure,
  }
}
