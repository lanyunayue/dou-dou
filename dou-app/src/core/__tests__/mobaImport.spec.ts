import { describe, it, expect } from 'vitest'
import { parseMobaSessions } from '../../shared/mobaImport'

describe('parseMobaSessions 形式一：ssh:// 直连行', () => {
  it('解析 user/host/port', () => {
    const txt = 'Session0=ssh://demo@example.com:22\nSession1=ssh://root@10.0.0.5:2222'
    expect(parseMobaSessions(txt)).toEqual([
      { user: 'demo', host: 'example.com', port: 22, source: 'mobaxterm' },
      { user: 'root', host: '10.0.0.5', port: 2222, source: 'mobaxterm' },
    ])
  })
  it('忽略无关行', () => {
    const txt = 'Version=25.0\n[Misc]\nExtraKey=abc\nSession2=ssh://u@h:22'
    const out = parseMobaSessions(txt)
    expect(out).toHaveLength(1)
    expect(out[0].host).toBe('h')
  })
})

describe('parseMobaSessions 形式二：段内键值', () => {
  const ini = [
    '[web]',
    'Username=deploy',
    'Hostname=web.internal',
    'Port=2200',
    '[db]',
    'Username=dbadmin',
    'Hostname=db.internal',
  ].join('\r\n')

  it('段内 user/host/port/alias 入库', () => {
    const out = parseMobaSessions(ini)
    const web = out.find(s => s.alias === 'web')
    expect(web).toMatchObject({ user: 'deploy', host: 'web.internal', port: 2200 })
  })

  it('端口缺省 22', () => {
    const db = parseMobaSessions(ini).find(s => s.alias === 'db')
    expect(db).toMatchObject({ user: 'dbadmin', host: 'db.internal', port: 22 })
  })

  it('缺 host 的段不入库', () => {
    const txt = '[lonely]\nUsername=onlyuser\n'
    expect(parseMobaSessions(txt)).toHaveLength(0)
  })
})
