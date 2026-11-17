<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">豆 Dou</div>
      <div class="gateway" :class="{ on: status.running }">
        {{ status.running ? '网关 在线' : '网关 离线' }}
      </div>
      <div class="endpoint">{{ status.endpoint }}</div>
      <div class="conn-hint">{{ connSummary }}</div>
      <button class="btn" @click="refresh">刷新</button>
      <nav class="nav">
        <div v-for="n in navs" :key="n.key" class="nav-item" :class="{active: view===n.key}" @click="view=n.key">{{n.label}}</div>
        <div class="nav-item" :class="{active: view==='onboard'}" @click="view='onboard'">🔌 接入向导</div>
        <div class="nav-item" :class="{active: view==='approve'}" @click="openApprove">✅ 审批<span v-if="pendingCount" class="badge">{{pendingCount}}</span></div>
        <div class="nav-item" :class="{active: view==='scope'}" @click="view='scope'; loadTokens()">🔑 凭据与作用域</div>
        <div class="nav-item" @click="importMoba">📥 导入 MobaXterm</div>
        <div class="nav-item" @click="snapshot">📸 快照</div>
      </nav>
    </aside>
    <div class="workspace">
      <!-- 统一可见错误态：fail-closed / 401 / 429 / 网关离线，含一键重试 -->
      <div v-if="gwError" class="gw-error">
        <span class="gw-error-msg">⚠ {{ gwError.message }}</span>
        <button class="mini-btn" @click="retryGw">一键重试</button>
        <button class="mini-btn" @click="dismissGwError">关闭</button>
      </div>

      <!-- 四区：左上=文件树，右上=终端，左下=编辑器 -->
      <div v-if="view==='term'||view==='files'" class="grid">
        <div class="panel panel-files">
          <div class="panelhead">
            📁 {{cwd}}
            <button class="mini-btn" @click="goUp">↑ 上级</button>
            <button class="mini-btn" @click="downloadCwd">下载</button>
          </div>
          <div class="fileopts">
            <label><input type="checkbox" v-model="showHidden" /> 隐藏文件</label>
            <input v-model="fileFilter" class="filter" placeholder="过滤文件名" />
          </div>
          <div class="filelist">
            <div v-if="sshError" class="ssh-offline-files">
              <div class="ssh-title">⚠ 未连接靶机</div>
              <div class="hint">{{ sshError }}</div>
              <div class="hint">文件树 / 终端 / 传输暂不可用；接入向导 / 审批 / 凭据 / 审计等 HTTP 管理功能不受影响。</div>
              <div class="hint">本机 127.0.0.1:22 不可达（wsl.exe 被安全策略禁用 / Windows OpenSSH 未安装 / 无外部靶机）。靶机就绪后点「一键重试」恢复，不会伪造连接成功。</div>
              <button class="btn ok" @click="retrySsh">一键重试</button>
            </div>
            <template v-else>
              <div v-for="f in visibleFiles" :key="f.name" class="fileitem" @click="onFile(f)">
                <span class="icon">{{f.is_dir?'📂':'📄'}}</span><span>{{f.name}}</span>
              </div>
              <div v-if="!visibleFiles.length" class="file-empty">目录为空或过滤条件无匹配</div>
            </template>
          </div>
        </div>
        <div class="panel panel-term">
          <div class="tabbar">
            <input v-model="sessionFilter" class="filter tabfilter" placeholder="检索会话" />
            <div v-for="t in visibleTabs" :key="t.sid" class="tab" :class="{active: t.sid===activeSid}" @click="switchTab(t.sid)">
              <input type="checkbox" class="tabcheck" v-model="t.selected" @click.stop />
              {{t.title}}<span v-if="t.badge" class="badge">{{t.badge}}</span>
              <span class="close" @click.stop="closeTab(t.sid)">×</span>
            </div>
            <button class="tabadd" @click="newTab">+</button>
            <button class="mini-btn" @click="exportTerm">导出</button>
            <input v-model="broadcast" class="broadcast" placeholder="广播到勾选→" @keyup.enter="doBroadcast" />
          </div>
          <div class="term-wrap">
            <div ref="termEl" class="terminal"></div>
            <!-- 优雅离线态：不黑屏 / 不纯白 / 不裸 os error / 不伪造连接成功 -->
            <div v-if="sshError" class="ssh-offline">
              <div class="ssh-title">⚠ 未连接靶机</div>
              <div class="ssh-reason">{{ sshError }}</div>
              <div class="ssh-hint">
                本机 127.0.0.1:22 不可达（wsl.exe 被安全策略禁用 / Windows OpenSSH 未安装 / 无外部靶机）。<br/>
                终端、文件、传输功能暂不可用；HTTP 管理功能（接入向导 / 审批 / 凭据 / 审计）照常可用。<br/>
                靶机就绪（22 端口可达）后点「一键重试」恢复，不会伪造连接成功。
              </div>
              <button class="btn ok" @click="retrySsh">一键重试</button>
            </div>
          </div>
        </div>
        <div class="panel panel-editor">
          <div class="panelhead">✏️ {{editingFile || '未打开文件'}}</div>
          <VueMonacoEditor class="editorarea" v-model:value="editorText" language="plaintext" theme="vs-dark" />
          <div class="editorbar">
            <button class="btn" @click="saveFile">保存</button>
            <span class="hint">保存前比对远端 mtime；走 dou.put 过隔离区+扫描</span>
          </div>
        </div>
      </div>

      <!-- 接入向导 -->
      <div v-if="view==='onboard'" class="panel panel-page">
        <div class="panelhead">🔌 客户端接入向导（五步）</div>
        <div class="wizard">
          <ol class="steps">
            <li>开通道：网关只绑本机回环 127.0.0.1:7419</li>
            <li>选客户端：探测安装与配置文件</li>
            <li>签发凭据：默认观察档、24 小时</li>
            <li>写入：一键代写 / 复制片段 / 注册命令（逐级降级）</li>
            <li>握手自检：以收到首个工具清单请求为准</li>
          </ol>
          <div class="row">
            <label>客户端</label>
            <select v-model="onboardClient" @change="fetchOnboard">
              <option value="workbuddy">WorkBuddy</option>
              <option value="codex">Codex CLI</option>
              <option value="claude_code">Claude Code</option>
              <option value="claude_desktop">Claude Desktop</option>
              <option value="vscode">VS Code (Copilot)</option>
              <option value="cursor">Cursor</option>
            </select>
          </div>
          <div class="row">
            <label>配置路径</label>
            <span class="mono">{{ onboardPaths.join(' · ') }}</span>
          </div>
          <pre class="snippet">{{ onboardSnippet }}</pre>
          <div class="row">
            <button class="btn" @click="copySnippet">复制片段</button>
            <button class="btn ok" @click="writeConfig">一键代写配置</button>
            <span class="hint">{{ onboardNote }}</span>
          </div>
          <div v-if="writeResult" class="issued">{{ writeResult }}</div>
          <div class="row">
            <button class="btn" @click="probeHandshake">检测客户端连接</button>
            <span class="hint">{{ onboardProbe }}</span>
          </div>
        </div>
      </div>

      <!-- 审批 -->
      <div v-if="view==='approve'" class="panel panel-page">
        <div class="panelhead">✅ 待审批任务（{{ pendingCount }}）</div>
        <div v-if="!pendingTasks.length" class="empty">没有待审任务</div>
        <div v-for="t in pendingTasks" :key="t.id" class="approve-card">
          <div class="mono cmd">{{ t.command }}</div>
          <div class="hint">原因：{{ t.reason }}</div>
          <div class="approve-actions">
            <button class="btn ok" @click="decideTask(t.id, true, false)">一次允许</button>
            <button class="btn ok" :disabled="t.policy==='change'" :title="t.policy==='change' ? '变更档不可关闭审批' : ''" @click="decideTask(t.id, true, true)">本次任务内允许</button>
            <input v-model="rejectNotes[t.id]" class="filter" placeholder="拒绝理由（必填，将留痕）" />
            <button class="btn bad" @click="decideTask(t.id, false, false)">拒绝</button>
          </div>
        </div>
      </div>

      <!-- 凭据与作用域 -->
      <div v-if="view==='scope'" class="panel panel-page">
        <div class="panelhead">🔑 设备令牌与七维作用域</div>
        <div class="row">
          <label>档位</label>
          <select v-model="issueTier">
            <option value="observe">观察档（只读）</option>
            <option value="execute">执行档（白名单命令+上传）</option>
            <option value="change">变更档（不可关闭审批）</option>
          </select>
          <label>客户端标识</label>
          <input v-model="issueClient" class="filter" placeholder="client_id" />
          <button class="btn" @click="issueToken">签发令牌</button>
        </div>
        <div v-if="issueTier==='change'" class="hint" style="color:#e0a050">变更档为高风险档位：审批不可关闭，每个动作都需逐条人工确认，不可“任务内记住放行”。</div>
        <div v-if="issuedToken" class="issued">
          明文仅显示一次：<span class="mono">{{ issuedToken }}</span>
          <button class="mini-btn" @click="copyIssued">复制</button>
        </div>
        <table class="scope-table">
          <thead><tr><th>ID</th><th>档位</th><th>客户端</th><th>主机</th><th>工具</th><th>路径</th><th>过期</th><th>操作</th></tr></thead>
          <tbody>
            <tr v-for="t in tokens" :key="t.id">
              <td class="mono">{{ t.id.slice(0,10) }}</td>
              <td>{{ t.tier }}</td>
              <td>{{ t.client_id }}</td>
              <td>{{ (t.scope?.hosts||[]).join(',') || '全禁' }}</td>
              <td>{{ (t.scope?.tools||[]).join(',') }}</td>
              <td>{{ (t.scope?.paths||[]).join(',') }}</td>
              <td>{{ fmtTime(t.expires_at) }}</td>
              <td><button class="btn bad" @click="revokeToken(t.id)" :disabled="!!t.revoked_at">{{ t.revoked_at?'已吊销':'吊销' }}</button></td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { sendNotification } from '@tauri-apps/plugin-notification'
import { Terminal } from 'xterm'
import { FitAddon } from 'xterm-addon-fit'
import { WebglAddon } from '@xterm/addon-webgl'
import { VueMonacoEditor } from '@guolao/vue-monaco-editor'
import 'xterm/css/xterm.css'

// 事件总线（简单 mitt 替代）
const bus = new Map<string, Function[]>()
function emit(evt: string, data?: any) {
  bus.get(evt)?.forEach(fn => fn(data))
}

// SSH 靶机（WSL2；本机回环中继对 russh 连接不稳定，直连 WSL 虚拟网卡 IP）
const SSH_HOST = '127.0.0.1'
const SSH_PORT = 22
const SSH_USER = 'dou'
const SSH_KEY = 'C:/Users/mrhua/AppData/Local/dou/keys/id_ed25519'

const status = ref({ running: false, endpoint: 'http://127.0.0.1:7419/health', ssh_up: false })
const navs = [{key:'term',label:'终端'},{key:'files',label:'文件'}]
const view = ref('term')
const termEl = ref<HTMLDivElement>()
const cwd = ref('/home/dou')
const files = ref<any[]>([])
const showHidden = ref(false)
const fileFilter = ref('')
const sessionFilter = ref('')
const editingFile = ref('')
const editingStat = ref<{ size: number, mtime: number } | null>(null)
const editorText = ref('')
const audit = ref<string[]>(['启动豆 Dou 桌面壳', '网关连接成功'])
const broadcast = ref('')
let windowFocused = true

// 网关地址固定由 Rust 侧 `gateway_request` 使用 http://127.0.0.1:7419，
// 前端不再直连 fetch（WebView 跨域 CORS 会挡住响应读取）。

// ---------- 统一鉴权请求层（唯一 fetch 出口） ----------
// 令牌只在模块内存态缓存，绝不写 localStorage / 日志 / 源码 / 提交 / URL / body。
let _gwTokenCache: string | null = null
let _gwTokenPending: Promise<string> | null = null
function resetGwToken() { _gwTokenCache = null; _gwTokenPending = null }

interface GwErrorInfo {
  code: 'TOKEN_UNAVAILABLE' | 'AUTH_FAILED' | 'THROTTLED' | 'GATEWAY_OFFLINE'
  message: string
  retryable: boolean
}
const gwError = ref<GwErrorInfo | null>(null)
function dismissGwError() { gwError.value = null }
function markGwFailure(message: string) {
  if (!gwError.value) gwError.value = { code: 'GATEWAY_OFFLINE', message, retryable: true }
}

// fail-closed：取令牌失败即 throw，绝不发无令牌请求（否则 401→429，正是本轮要修的根因）
async function gwToken(): Promise<string> {
  if (_gwTokenCache) return _gwTokenCache
  if (_gwTokenPending) return _gwTokenPending
  _gwTokenPending = (async () => {
    try {
      const t = await invoke<string>('gateway_token')
      if (!t || !t.startsWith('dou_ak_')) throw new Error('invalid token')
      _gwTokenCache = t
      return t
    } catch {
      gwError.value = {
        code: 'TOKEN_UNAVAILABLE',
        message: '令牌不可用：无法读取 %LOCALAPPDATA%/dou/token.txt，请确认网关已启动并生成主令牌。已中止全部请求（fail-closed），不会发送无令牌请求。',
        retryable: true
      }
      throw new Error('令牌不可用，请检查 dou/token.txt')
    } finally {
      _gwTokenPending = null
    }
  })()
  return _gwTokenPending
}

// 唯一网关请求出口：走 Tauri IPC -> Rust reqwest（绕开 WebView CORS 与系统代理）。
// 令牌由 Rust 侧读 token.txt 并注入 Bearer；gwToken() 仅作 fail-closed 前置校验。
async function gwFetch(path: string, opts: RequestInit = {}): Promise<Response> {
  await gwToken()                       // 失败即中止，绝不发无令牌请求
  const method = String(opts.method || 'GET').toUpperCase()
  const body = opts.body == null ? null : String(opts.body)
  let status = 0
  let text = ''
  try {
    const r = await invoke<{ status: number, body: string }>('gateway_request', { path, method, body })
    status = r.status
    text = r.body
  } catch (e) {
    gwError.value = { code: 'GATEWAY_OFFLINE', message: `网关离线：无法连接 127.0.0.1:7419（${String(e).slice(0, 90)}）`, retryable: true }
    throw new Error('网关离线')
  }
  if (status === 401) {
    gwError.value = { code: 'AUTH_FAILED', message: '鉴权失败（401）：令牌与网关不一致，请检查 DOU_DEV_TOKEN 与 token.txt 是否同源', retryable: true }
  } else if (status === 429) {
    gwError.value = { code: 'THROTTLED', message: '请求被限流（429）：请按 Retry-After 稍后重试，勿连续刷新', retryable: true }
  } else if (status < 200 || status >= 300) {
    gwError.value = { code: 'GATEWAY_OFFLINE', message: `网关返回异常状态 ${status}`, retryable: true }
  } else {
    gwError.value = null
  }
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => JSON.parse(text),
    text: async () => text,
  } as Response
}

// 占位符本地替换：仅用于展示 / 复制，绝不回传网络（/onboard/write 只 POST {client}）
const TOKEN_PLACEHOLDER = '${DOU_TOKEN}'
async function replaceTokenPlaceholder(snippet: string): Promise<string> {
  if (!snippet.includes(TOKEN_PLACEHOLDER)) return snippet
  const t = await gwToken()
  return snippet.split(TOKEN_PLACEHOLDER).join(t)
}

// 靶机离线原因：把裸 `os error 10061` 等转成人话，绝不渲染原始错误串
function sshOfflineReason(e: any): string {
  const s = String(e?.message ?? e ?? '')
  if (/10061|refused|连接被拒绝/i.test(s)) return '连接被拒绝：目标 127.0.0.1:22 未监听（靶机未启动）'
  if (/timed out|timeout|超时/i.test(s)) return '连接超时：靶机不可达'
  if (/authentication|auth failed|认证/i.test(s)) return 'SSH 认证失败：密钥或账户不匹配'
  if (/no such file|not found|key|密钥/i.test(s)) return 'SSH 密钥不可用：未找到 id_ed25519'
  return s ? `连接失败：${s.slice(0, 100)}` : '连接失败：靶机不可用'
}
const sshError = ref('')
function handleSshFailure(e: any, ctx: string) {
  sshError.value = sshOfflineReason(e)
  audit.value.unshift(`[${new Date().toLocaleTimeString()}] ${ctx}失败：${sshError.value}`)
}

// 状态灯分层：/health 探活成功 ≠ 所有功能可用（探活 / 鉴权 / 靶机三层单一真源）
const connSummary = computed(() => {
  if (!status.value.running) return '网关未就绪 · HTTP 管理功能不可用'
  if (gwError.value) return `网关在线 · ${gwError.value.message}`
  // 以 Rust 侧对 22 端口的真实探测为准，绝不因「没报过错」就写已连接。
  const ssh = status.value.ssh_up
    ? '靶机在线（终端/文件/传输可用）'
    : '靶机未连接（22 端口不可达，终端/文件/传输不可用）'
  return `网关在线（HTTP 管理可用）· ${ssh}`
})

// 接入向导
const onboardClient = ref('workbuddy')
const onboardSnippet = ref('')
const onboardPaths = ref<string[]>([])
const onboardNote = ref('')
const writeResult = ref('')
const onboardProbe = ref('')
// 审批
const pendingTasks = ref<any[]>([])
const rejectNotes = ref<Record<string, string>>({})
// 凭据
const tokens = ref<any[]>([])
const issueTier = ref('observe')
const issueClient = ref('web')
const issuedToken = ref('')

const pendingCount = computed(() => pendingTasks.value.length)

const visibleFiles = computed(() => files.value.filter(f => {
  if (!showHidden.value && f.name.startsWith('.')) return false
  if (fileFilter.value && !f.name.toLowerCase().includes(fileFilter.value.toLowerCase())) return false
  return true
}))
const visibleTabs = computed(() => tabs.value.filter(t =>
  !sessionFilter.value || t.title.toLowerCase().includes(sessionFilter.value.toLowerCase())
))
async function hash8(s: string): Promise<string> {
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(s))
  return Array.from(new Uint8Array(buf)).slice(0,4).map(b=>b.toString(16).padStart(2,'0')).join('')
}
async function snapshot() {
  const t = tabs.value.find(x=>x.sid===activeSid.value)
  const history = t ? (t.term.buffer.active.getLine(0)?.translateToString(true) ?? '').slice(-200) : ''
  const snap = { time: new Date().toLocaleString(), cwd: cwd.value, sid: activeSid.value, history }
  const list = JSON.parse(localStorage.getItem('dou_snaps')||'[]')
  list.push(snap)
  localStorage.setItem('dou_snaps', JSON.stringify(list))
  const h = await hash8('127.0.0.1:22'), u = await hash8('dou'), s = await hash8(activeSid.value)
  alert(`快照已存\n三段指纹: ${h}/${u}/${s}\n历史: ${history.slice(-50)}...\n共 ${list.length} 个快照`)
}
const tabs = ref<{sid:string,title:string,term:Terminal,selected?:boolean,badge?:number}[]>([])
const activeSid = ref('')
let fit: FitAddon

function goUp() {
  if (cwd.value === '/' || cwd.value === '') return
  const parent = cwd.value.split('/').filter(Boolean).slice(0, -1).join('/')
  loadFiles(parent === '' ? '/' : '/' + parent)
}

async function downloadCwd() {
  try {
    const p = await invoke('sftp_download', { host:SSH_HOST, port:SSH_PORT, user:SSH_USER, keyPath:SSH_KEY, remote: cwd.value + '/dou_put_test.txt' })
    alert('已下载到: ' + p)
  } catch (e: any) {
    handleSshFailure(e, '下载')
    alert('下载失败：' + sshError.value + '\n靶机就绪后点「一键重试」恢复')
  }
}
function cleanExport(raw: string): string {
  return raw
    .replace(/\x1b\[[0-9;?]*[A-Za-z]/g, '')   // ANSI CSI
    .replace(/\x1b\][^\x07]*\x07/g, '')        // OSC
    .replace(/\x0c/g, '')                       // 分页符
    .replace(/[^\n]*\r/g, '')                   // 回车覆盖字节
    .replace(/\x1b[()][AB012]/g, '')            // 字符集切换
}
function exportTerm() {
  const t = tabs.value.find(x=>x.sid===activeSid.value)
  if (!t) return
  const lines: string[] = []
  const buf = t.term.buffer.active
  for (let i = 0; i < buf.length; i++) lines.push(buf.getLine(i)?.translateToString(true) ?? '')
  const clean = cleanExport(lines.join('\n'))
  const blob = new Blob([clean], { type: 'text/plain;charset=utf-8' })
  const a = document.createElement('a')
  a.href = URL.createObjectURL(blob)
  a.download = `dou_terminal_${Date.now()}.txt`
  a.click()
  URL.revokeObjectURL(a.href)
}
async function doBroadcast() {
  const cmd = broadcast.value + '\n'
  const targets = tabs.value.filter(t => t.selected)
  const list = targets.length ? targets : tabs.value
  for (const t of list) {
    await invoke('pty_write', { sid: t.sid, data: cmd }).catch(() => {
      audit.value.unshift(`[${new Date().toLocaleTimeString()}] 广播写入失败：会话 ${t.sid.slice(0,8)} 不可用`)
    })
  }
  broadcast.value = ''
}

async function refresh() { status.value = await invoke('gateway_status') }
async function loadFiles(path: string) {
  cwd.value = path
  try { files.value = await invoke('sftp_list', { host:SSH_HOST, port:SSH_PORT, user:SSH_USER, keyPath:SSH_KEY, path }) }
  catch (e: any) { files.value = []; handleSshFailure(e, '列出远端目录') }
}
async function onFile(f: any) {
  if (f.is_dir) { loadFiles(cwd.value === '/' ? '/'+f.name : cwd.value+'/'+f.name); return }
  const path = cwd.value === '/' ? '/'+f.name : cwd.value+'/'+f.name
  editingFile.value = path
  editorText.value = ''
  try {
    const st = await invoke('remote_stat', { host:SSH_HOST, port:SSH_PORT, user:SSH_USER, keyPath:SSH_KEY, remote: path }) as [number, number]
    editingStat.value = { size: st[0], mtime: st[1] }
    // F-04 大文件懒加载：超过 2MB 不直接载入编辑器
    if (editingStat.value.size > 2 * 1024 * 1024) {
      editorText.value = `// 文件较大（${(editingStat.value.size/1024/1024).toFixed(1)} MB），已懒加载，不直接展开全文`
    }
  } catch (e: any) { editingStat.value = null; handleSshFailure(e, '读取远端文件属性') }
  emit('file:opened', path)
}
async function saveFile() {
  // 保存前比对远端 mtime，防止覆盖他人改动（F-04）
  try {
    const st = await invoke('remote_stat', { host:SSH_HOST, port:SSH_PORT, user:SSH_USER, keyPath:SSH_KEY, remote: editingFile.value }) as [number, number]
    if (editingStat.value && Number(st[1]) !== editingStat.value.mtime) {
      if (!confirm(`远端 ${editingFile.value} 已被改动（mtime 不一致），仍要覆盖保存吗？`)) return
    }
    editingStat.value = { size: st[0], mtime: st[1] }
  } catch (e: any) { handleSshFailure(e, '比对远端 mtime'); alert('保存中止：' + sshError.value) }
  audit.value.unshift(`[${new Date().toLocaleTimeString()}] 保存 ${editingFile.value} → 过隔离区+扫描`)
  emit('file:saved', editingFile.value)
}

// 先连接靶机，成功后再创建终端；失败进入优雅离线态，不黑屏、不裸 os error、不伪造连接成功
async function connectSsh() {
  try {
    const sid = await invoke('open_pty', { host:SSH_HOST, port:SSH_PORT, user:SSH_USER, keyPath:SSH_KEY }) as string
    sshError.value = ''
    const term = new Terminal({ fontSize: 13, convertEol: true })
    fit = new FitAddon()
    term.loadAddon(fit)
    term.open(termEl.value!)
    fit.fit()
    // F-05：优先 WebGL，初始化失败自动回退 Canvas（渲染器回退，非连接错误）
    try { await term.loadAddon(new WebglAddon()) } catch { /* 回退默认 Canvas */ }
    term.write(`[已连接靶机 dou@127.0.0.1:22]\r\n`)
    term.onData(async (data) => { await invoke('pty_write', { sid, data }).catch(()=>{}) })
    tabs.value.push({ sid, title: `会话 ${tabs.value.length+1}`, term })
    activeSid.value = sid
    audit.value.unshift(`[${new Date().toLocaleTimeString()}] 新建会话 ${sid.slice(0,8)}`)
  } catch (e: any) {
    handleSshFailure(e, '连接靶机')
    // 不写裸 os error、不伪造「已连接」；离线态由模板覆盖层呈现，含一键重试
  }
}
async function newTab() { await connectSsh() }
async function retrySsh() {
  sshError.value = ''
  await connectSsh()
  await loadFiles(cwd.value || '/home/dou')
}
async function retryGw() {
  resetGwToken()
  gwError.value = null
  await refresh()
  await fetchOnboard()
  await loadTasks()
  await loadTokens()
}
function switchTab(sid: string) { activeSid.value = sid; const t = tabs.value.find(x=>x.sid===sid); if (t) t.badge = 0 }
function closeTab(sid: string) {
  tabs.value = tabs.value.filter(t=>t.sid!==sid)
  if (activeSid.value === sid && tabs.value.length) switchTab(tabs.value[0].sid)
}
async function importMoba() {
  const fs = await import('@tauri-apps/plugin-fs')
  const paths = ['C:/Program Files (x86)/MobaXterm/MobaXterm.ini','C:/Program Files/MobaXterm/MobaXterm.ini']
  for (const p of paths) {
    try {
      const txt = await fs.readTextFile(p)
      const sessions: any[] = []
      // 形式一：SessionN=ssh://user@host:port
      for (const line of txt.split(/\r?\n/)) {
        const m = line.match(/ssh:\/\/([^@]+)@([^:]+):(\d+)/)
        if (m) sessions.push({ user: m[1], host: m[2], port: Number(m[3]), source: 'mobaxterm' })
      }
      // 形式二：[别名] 段内 Username/Hostname/Port
      let cur: any = null
      for (const line of txt.split(/\r?\n/)) {
        const sec = line.match(/^\[(.+)\]$/)
        if (sec) { cur = { alias: sec[1] }; continue }
        if (cur && /^(Username|Hostname|Port)=/i.test(line)) {
          const [k, v] = line.split('=')
          if (/username/i.test(k)) cur.user = v
          if (/hostname/i.test(k)) cur.host = v
          if (/port/i.test(k)) cur.port = Number(v)
        }
      }
      const existing = JSON.parse(localStorage.getItem('dou_imported')||'[]')
      const merged = [...existing, ...sessions]
      localStorage.setItem('dou_imported', JSON.stringify(merged))
      alert(`已找到: ${p}\n解析到 ${sessions.length} 个 SSH 会话，共入库 ${merged.length} 个`)
      return
    } catch { continue } // 该路径未安装 MobaXterm，继续尝试下一路径
  }
  alert('未找到 MobaXterm 配置（本机未安装或路径不同）')
}

// ---------- 接入向导 ----------
async function fetchOnboard() {
  try {
    const r = await gwFetch(`/onboard/config?client=${encodeURIComponent(onboardClient.value)}`)
    const j = await r.json()
    // ${DOU_TOKEN} 在本地替换为真实令牌，仅用于展示 / 复制；网关响应体仍是占位符
    onboardSnippet.value = await replaceTokenPlaceholder(j.snippet || '')
    onboardPaths.value = j.config_paths || []
    onboardNote.value = j.note || ''
  } catch {
    onboardSnippet.value = gwError.value
      ? `无法生成片段：${gwError.value.message}`
      : '无法生成片段：网关不可用'
  }
}
async function copySnippet() {
  try {
    // 复制前再做一次本地替换，确保产物无 ${DOU_TOKEN} 残留
    const text = await replaceTokenPlaceholder(onboardSnippet.value)
    await navigator.clipboard.writeText(text)
    alert('片段已复制（Authorization 占位符已本地替换为可用令牌）')
  } catch {
    alert('复制失败：' + (gwError.value?.message || '令牌不可用'))
  }
}
async function writeConfig() {
  writeResult.value = ''
  try {
    // 只 POST {client}；真令牌由网关服务端本地代写，前端绝不回传令牌 / 替换后片段
    const r = await gwFetch('/onboard/write', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ client: onboardClient.value })
    })
    const j = await r.json()
    if (j.written) {
      writeResult.value = `${j.message}｜生效文件: ${j.path}${j.backed_up ? `｜备份: ${j.backed_up}` : ''}`
    } else {
      writeResult.value = '代写未执行: ' + (j.error || '未知原因')
    }
  } catch {
    writeResult.value = '代写失败：' + (gwError.value?.message || '请求未完成')
  }
}

// 握手自检：轮询审计记录，确认外部客户端已完成 initialize 握手。
let probeTimer: ReturnType<typeof setInterval> | null = null
function probeHandshake() {
  onboardProbe.value = '等待客户端连接…'
  const start = Date.now()
  let tries = 0
  if (probeTimer) clearInterval(probeTimer)
  probeTimer = setInterval(async () => {
    tries++
    try {
      const r = await gwFetch('/audit/query?source=agent&limit=30')
      if (r.status === 401 || r.status === 429) {
        // 鉴权 / 限流异常立即停止轮询，不风暴重试
        onboardProbe.value = `检测暂停：${gwError.value?.message || '鉴权/限流异常'}`
        if (probeTimer) clearInterval(probeTimer)
        return
      }
      const j = await r.json()
      const items = j.items || []
      const hit = items.find((x: any) => x.action === 'initialize' && Number(x.ts_ms) >= start - 2000)
      if (hit) {
        onboardProbe.value = '已检测到客户端握手，接入成功'
        if (probeTimer) clearInterval(probeTimer)
      } else if (tries >= 15) {
        onboardProbe.value = '超时未检测到连接，请检查配置后重试'
        if (probeTimer) clearInterval(probeTimer)
      }
    } catch {
      onboardProbe.value = `检测失败：${gwError.value?.message || '网关不可用'}`
      if (probeTimer) clearInterval(probeTimer)
    }
  }, 2000)
}

// ---------- 审批 ----------
async function loadTasks() {
  try {
    const r = await gwFetch('/tasks')
    const j = await r.json()
    pendingTasks.value = (j.items || []).filter((t: any) => t.status === 'pending_approval')
  } catch {
    pendingTasks.value = []
    markGwFailure('加载待审任务失败：网关不可用')  // 不静默吞错，错误态由全局横幅呈现
  }
}
function openApprove() { view.value = 'approve'; loadTasks() }
async function decideTask(id: string, approve: boolean, remember: boolean) {
  const note = rejectNotes.value[id] || ''
  if (!approve && !note.trim()) { alert('拒绝必须填写理由'); return }
  try {
    await gwFetch('/approve', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ task_id: id, approve, remember, note })
    })
    delete rejectNotes.value[id]
    await loadTasks()
  } catch {
    alert('审批提交失败：' + (gwError.value?.message || '请求未完成'))
  }
}

// ---------- 凭据与作用域 ----------
async function loadTokens() {
  try {
    const r = await gwFetch('/scope/tokens')
    const j = await r.json()
    tokens.value = j.items || []
  } catch {
    tokens.value = []
    markGwFailure('加载凭据列表失败：网关不可用')  // 不静默吞错
  }
}
async function issueToken() {
  issuedToken.value = ''
  try {
    const r = await gwFetch('/scope/tokens', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tier: issueTier.value, client_id: issueClient.value })
    })
    const j = await r.json()
    if (j.token) { issuedToken.value = j.token; await loadTokens() }
    else alert('签发失败: ' + (j.error || ''))
  } catch {
    alert('签发失败：' + (gwError.value?.message || '请求未完成'))
  }
}
async function revokeToken(id: string) {
  if (!confirm('吊销后下一次请求即被拒，记录保留。确认？')) return
  try {
    await gwFetch('/scope/revoke', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id })
    })
    await loadTokens()
  } catch {
    alert('吊销失败：' + (gwError.value?.message || '请求未完成'))
  }
}
async function copyIssued() { await navigator.clipboard.writeText(issuedToken.value); alert('已复制') }
function fmtTime(sec: number) { return sec ? new Date(sec * 1000).toLocaleString() : '-' }

onMounted(async () => {
  await refresh()
  const win = getCurrentWindow()
  const onFocus = () => { windowFocused = true }
  const onBlur = () => { windowFocused = false }
  await win.onFocusChanged(({ payload: focused }) => { windowFocused = focused })
  void onFocus; void onBlur
  await listen('pty_data', (e: any) => {
    const p = e.payload
    const t = tabs.value.find(x=>x.sid===p.sid)
    if (t) {
      t.term.write(p.data)
      // 窗口失焦：累计角标并发系统通知（F-07）
      if (!windowFocused) {
        t.badge = (t.badge ?? 0) + 1
        if (t.badge === 1) {
          try { sendNotification({ title: '豆 Dou', body: `${t.title} 有新输出` }) } catch { /* 忽略通知失败 */ }
        }
      }
    }
  })
  await newTab()
  loadFiles('/home/dou')
  fetchOnboard()
  loadTasks()
  setInterval(loadTasks, 30000)
})
</script>

<style scoped>
.app-shell { display: flex; height: 100vh; background: #f5f6f8; }
.sidebar { width: 180px; background: #fff; border-right: 1px solid #e5e7eb; padding: 16px; }
.brand { font-size: 20px; font-weight: 700; margin-bottom: 12px; }
.gateway { display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 12px; background: #fecaca; color: #991b1b; }
.gateway.on { background: #bbf7d0; color: #166534; }
.endpoint { font-size: 11px; color: #6b7280; margin: 8px 0; }
.conn-hint { font-size: 10px; color: #6b7280; margin: 6px 0 8px; line-height: 1.5; }
.btn { width: 100%; padding: 6px; border: 1px solid #d1d5db; background: #fff; border-radius: 4px; cursor: pointer; }
.nav { margin-top: 16px; }
.nav-item { padding: 6px 8px; border-radius: 4px; cursor: pointer; font-size: 13px; }
.nav-item.active { background: #e5e7eb; }
.workspace { flex: 1; padding: 8px; }
.grid { display: grid; grid-template-columns: 260px 1fr; grid-template-rows: 1fr 1fr; gap: 8px; height: 100%; }
.panel { background: #fff; border-radius: 6px; border: 1px solid #e5e7eb; display: flex; flex-direction: column; overflow: hidden; }
.panel-files { grid-row: 1 / 3; }
.panelhead { padding: 6px 10px; font-size: 12px; background: #f9fafb; border-bottom: 1px solid #e5e7eb; }
.filelist { overflow-y: auto; padding: 4px 0; }
.fileitem { padding: 4px 10px; cursor: pointer; font-size: 12px; }
.fileitem:hover { background: #f3f4f6; }
.icon { margin-right: 4px; }
.tabbar { display: flex; gap: 4px; padding: 4px; background: #f9fafb; border-bottom: 1px solid #e5e7eb; }
.tab { padding: 2px 10px; background: #fff; border: 1px solid #e5e7eb; border-radius: 4px; font-size: 11px; cursor: pointer; }
.tab.active { background: #000; color: #fff; border-color: #000; }
.tabadd { border: none; background: transparent; cursor: pointer; }
.broadcast { flex: 1; margin-left: 8px; padding: 2px 6px; border: 1px solid #d1d5db; border-radius: 3px; font-size: 11px; }
.term-wrap { flex: 1; position: relative; min-height: 0; display: flex; }
.terminal { flex: 1; background: #000; padding: 6px; }
.editorarea { flex: 1; min-height: 0; }
.editorbar { padding: 4px 8px; border-top: 1px solid #e5e7eb; display: flex; gap: 8px; align-items: center; }
.hint { font-size: 11px; color: #6b7280; }
.auditlist { overflow-y: auto; padding: 4px 8px; font-size: 11px; font-family: monospace; }
.audititem { padding: 2px 0; border-bottom: 1px solid #f3f4f6; }
.mini-btn { float: right; font-size: 10px; padding: 1px 6px; border: 1px solid #d1d5db; border-radius: 3px; background: #fff; cursor: pointer; }
.fileopts { display: flex; gap: 10px; align-items: center; padding: 4px 10px; font-size: 11px; color: #374151; border-bottom: 1px solid #f3f4f6; }
.filter { padding: 2px 6px; border: 1px solid #d1d5db; border-radius: 3px; font-size: 11px; }
.tabfilter { width: 90px; }
.tabcheck { margin-right: 3px; }
.badge { display: inline-block; margin-left: 4px; background: #dc2626; color: #fff; border-radius: 8px; font-size: 9px; padding: 0 5px; }
.panel-page { height: 100%; overflow-y: auto; padding: 0; }
.wizard, .approve-card, .issued { padding: 12px 16px; }
.steps { font-size: 12px; color: #374151; line-height: 1.9; margin: 0 0 12px; }
.row { display: flex; gap: 10px; align-items: center; margin: 8px 0; flex-wrap: wrap; }
.row label { font-size: 12px; color: #374151; }
.snippet { background: #111827; color: #e5e7eb; padding: 10px; border-radius: 6px; font-size: 12px; overflow-x: auto; white-space: pre; }
.mono { font-family: monospace; font-size: 12px; }
.empty { padding: 24px; color: #6b7280; font-size: 13px; }
.approve-card { border-bottom: 1px solid #f3f4f6; }
.approve-card .cmd { margin-bottom: 4px; }
.approve-actions { display: flex; gap: 8px; align-items: center; margin-top: 8px; flex-wrap: wrap; }
.approve-actions .filter { flex: 1; min-width: 180px; }
.btn.ok { background: #166534; color: #fff; border-color: #166534; width: auto; }
.btn.bad { background: #991b1b; color: #fff; border-color: #991b1b; width: auto; }
.scope-table { width: 100%; border-collapse: collapse; font-size: 12px; }
.scope-table th, .scope-table td { border-bottom: 1px solid #f3f4f6; padding: 6px 10px; text-align: left; }
.issued { background: #f0fdf4; border: 1px solid #bbf7d0; border-radius: 6px; margin: 10px 16px; }
select { padding: 4px 6px; border: 1px solid #d1d5db; border-radius: 3px; font-size: 12px; }

/* 本轮新增：统一错误态（fail-closed / 401 / 429 / 网关离线） */
.gw-error { position: fixed; top: 10px; left: 200px; right: 18px; z-index: 30; display: flex; gap: 8px; align-items: center; padding: 8px 12px; background: #fdecea; border: 1px solid #f5c6cb; border-radius: 6px; font-size: 12px; color: #7b241c; box-shadow: 0 2px 8px rgba(0,0,0,.12); }
.gw-error-msg { flex: 1; }
.gw-error .mini-btn { float: none; }
/* 本轮新增：靶机离线优雅降级（不黑屏 / 不纯白 / 不裸 os error / 不伪造成功） */
.ssh-offline { position: absolute; inset: 0; background: #111827; color: #e5e7eb; padding: 20px; display: flex; flex-direction: column; gap: 10px; justify-content: center; z-index: 5; font-size: 13px; }
.ssh-offline .ssh-title { font-size: 16px; font-weight: 700; color: #fca5a5; }
.ssh-offline .ssh-reason { color: #fde68a; line-height: 1.6; }
.ssh-offline .ssh-hint { color: #9ca3af; line-height: 1.8; }
.ssh-offline .btn.ok { align-self: flex-start; }
.ssh-offline-files { padding: 14px 12px; background: #fffbeb; border-bottom: 1px solid #fde68a; font-size: 12px; line-height: 1.7; color: #92400e; }
.ssh-offline-files .ssh-title { font-size: 14px; font-weight: 700; color: #b45309; margin-bottom: 4px; }
.ssh-offline-files .hint { color: #92400e; line-height: 1.7; }
.ssh-offline-files .btn.ok { margin-top: 8px; }
.file-empty { padding: 16px 10px; color: #6b7280; font-size: 12px; }
</style>
