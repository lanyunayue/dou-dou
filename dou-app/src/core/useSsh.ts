import { ref } from 'vue'

// SSH 密钥路径：默认由 Rust 侧 ssh::ssh_target() 依据 DOU_SSH_KEY / LOCALAPPDATA
// 推导；也可在连接表单手动填写，或用 dou-app/.env.local 的 VITE_DOU_SSH_KEY 覆盖。
export const SSH_KEY = (import.meta.env.VITE_DOU_SSH_KEY as string | undefined) ?? ''

// 当前远程目标：连接表单提交成功后写入，终端/文件树/编辑器共用。
// 不再钉死任何机器地址（R1：首屏新建连接，去内测默认值）。
export interface SshTarget { host: string; port: number; user: string }
export const activeTarget = ref<SshTarget | null>(null)

// 目标不可达原因：把裸 `os error 10061` 等转成人话，绝不渲染原始错误串
export function sshOfflineReason(e: any): string {
  const s = String(e?.message ?? e ?? '')
  if (/10061|refused|连接被拒绝/i.test(s)) return '连接被拒绝：目标端口未监听（主机未启动或地址/端口不对）'
  if (/timed out|timeout|超时/i.test(s)) return '连接超时：目标主机不可达'
  if (/authentication|auth failed|认证/i.test(s)) return 'SSH 认证失败：检查密码/密钥与账户是否匹配'
  if (/no such file|not found|key|密钥/i.test(s)) return 'SSH 密钥不可用：未配置或路径无效，可在连接表单改用密码认证'
  return s ? `连接失败：${s.slice(0, 100)}` : '连接失败：目标主机不可用'
}

export const sshError = ref('')

// 审计轨迹（内存态，不落盘）
export const audit = ref<string[]>(['启动豆 Dou 桌面壳', '网关连接成功'])

export function handleSshFailure(e: any, ctx: string) {
  sshError.value = sshOfflineReason(e)
  audit.value.unshift(`[${new Date().toLocaleTimeString()}] ${ctx}失败：${sshError.value}`)
}

// 文件树/编辑器等远端操作的统一入参；未连接时返回 null，由调用方给出引导
export function sshArgs(): { host: string; port: number; user: string; keyPath: string } | null {
  const t = activeTarget.value
  if (!t) return null
  return { host: t.host, port: t.port, user: t.user, keyPath: SSH_KEY }
}

export function useSsh() {
  return {
    activeTarget,
    SSH_KEY,
    sshError,
    audit,
    sshOfflineReason,
    handleSshFailure,
    sshArgs,
  }
}
