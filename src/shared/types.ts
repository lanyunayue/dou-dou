import type { Terminal } from 'xterm'

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
  selected?: boolean
  badge?: number
}
