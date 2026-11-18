/** MobaXterm ini 会话解析：纯函数，不碰文件系统 / localStorage，便于单测。 */

export interface MobaSession {
  user: string
  host: string
  port: number
  alias?: string
  source: 'mobaxterm'
}

/**
 * 解析 MobaXterm.ini 文本，识别两种 SSH 配置形式：
 * - 形式一：`SessionN=ssh://user@host:port` 直连行；
 * - 形式二：`[别名]` 段内 `Username=` / `Hostname=` / `Port=`。
 * 段形式在进入新段或文件结束时 flush；user/host 齐全才入库，端口缺省 22。
 * 不修改入参文本。
 */
export function parseMobaSessions(txt: string): MobaSession[] {
  const sessions: MobaSession[] = []
  const lines = (txt ?? '').split(/\r?\n/)

  // 形式一：ssh://user@host:port
  for (const line of lines) {
    const m = line.match(/ssh:\/\/([^@]+)@([^:]+):(\d+)/)
    if (m) {
      sessions.push({ user: m[1], host: m[2], port: Number(m[3]), source: 'mobaxterm' })
    }
  }

  // 形式二：[别名] 段内键值
  let cur: { alias: string; user?: string; host?: string; port?: number } | null = null
  const flush = () => {
    if (cur && cur.user && cur.host) {
      sessions.push({
        user: cur.user,
        host: cur.host,
        port: cur.port ?? 22,
        alias: cur.alias,
        source: 'mobaxterm',
      })
    }
  }
  for (const line of lines) {
    const sec = line.match(/^\[(.+)\]$/)
    if (sec) {
      flush()
      cur = { alias: sec[1] }
      continue
    }
    if (cur && /^(Username|Hostname|Port)=/i.test(line)) {
      const [k, v] = line.split('=')
      if (/username/i.test(k)) cur.user = v
      else if (/hostname/i.test(k)) cur.host = v
      else if (/port/i.test(k)) cur.port = Number(v)
    }
  }
  flush()

  return sessions
}
