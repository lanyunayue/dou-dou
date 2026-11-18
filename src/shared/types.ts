import type { Terminal } from 'xterm'
import type { FitAddon } from 'xterm-addon-fit'
import type { SearchAddon } from 'xterm-addon-search'

/** 统一可见错误态载荷（fail-closed / 401 / 429 / 网关离线） */
export interface GwErrorInfo {
  code: 'TOKEN_UNAVAILABLE' | 'AUTH_FAILED' | 'THROTTLED' | 'GATEWAY_OFFLINE'
  message: string
  retryable: boolean
}

/** 网关探活状态（gateway_status 返回） */
export interface GatewayStatus {
  running: boolean
  endpoint: string
  ssh_up: boolean
}

/** 远端目录项（sftp_list 返回） */
export interface RemoteFile {
  name: string
  is_dir: boolean
  [key: string]: any
}

/** 远端文件属性（remote_stat 返回 [size, mtime]） */
export interface RemoteStat {
  size: number
  mtime: number
}

/** 终端会话标签（多标签页） */
export interface TerminalTab {
  sid: string
  title: string
  term: Terminal
  /** 每会话独立的 fit 插件（切换标签显示时按容器重排） */
  fit?: FitAddon
  /** R14 终端查找插件实例 */
  search?: SearchAddon
  selected?: boolean
  badge?: number
  /** 本地 Shell 会话（不走 SSH，远程主机不可达时也可用） */
  local?: boolean
  /** R11 会话元数据（持久化恢复用，不含密码等敏感信息） */
  meta?: SessionMeta
}

/** R11 会话元数据：只存档案名/认证方式/主机地址等非敏感信息，绝不含密码。
 *  密码认证会话因此重启后无法自动恢复——这是有意的安全边界，不是缺陷。 */
export interface SessionMeta {
  kind: 'ssh' | 'local'
  title: string
  host?: string
  port?: number
  user?: string
  auth?: 'key' | 'password'
  archiveName?: string
  /** 非敏感：磁盘密钥路径，恢复时直接复用 */
  keyPath?: string
  ts: number
}

/** 主机监控一轮采样（host_stats 事件 payload，Rust monitor.rs） */
export interface HostStats {
  cpu_pct: number
  mem_used_mb: number
  mem_total_mb: number
  rx_kbps: number
  tx_kbps: number
  uptime_s: number
}
