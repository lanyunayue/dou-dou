<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">豆 Dou</div>
      <div class="gateway" :class="{ on: status.running }">
        {{ status.running ? '网关 在线' : '网关 离线' }}
      </div>
      <div class="endpoint">{{ status.endpoint }}</div>
      <!-- R3：网关状态与远程主机状态拆分各自成行，未连接时给出动作入口 -->
      <div class="host-status" :class="{ bad: !activeTarget }">
        <template v-if="activeTarget">远程主机 {{ activeTarget.user }}@{{ activeTarget.host }}（已连接）</template>
        <template v-else>
          远程主机：未连接
          <button class="linklike" type="button" @click="gotoConnect">新建连接</button>
          <button class="linklike" type="button" @click="openLocal">打开本地 Shell</button>
        </template>
      </div>
      <button class="btn" @click="refresh">刷新</button>
      <!-- R2 信息架构：默认 5 项自明入口（≤7），内部功能收进「高级」可折叠分组；每项 title 一句话 -->
      <nav class="nav" aria-label="主导航">
        <div class="nav-group">工作区</div>
        <button v-for="n in mainNavs" :key="n.key" type="button" class="nav-item" :class="{active: view===n.key}" :title="n.tip" :aria-current="view===n.key ? 'page' : undefined" @click="n.click ? n.click() : (view=n.key)">{{n.label}}</button>
        <div class="nav-group">日常</div>
        <button v-for="n in dailyNavs" :key="n.key" type="button" class="nav-item" :class="{active: view===n.key}" :title="n.tip" :aria-current="view===n.key ? 'page' : undefined" @click="n.click ? n.click() : (view=n.key)">{{n.label}}</button>
        <button type="button" class="nav-group nav-toggle" :title="advancedOpen ? '收起内部功能' : '展开内部功能：审批/策略/审计等'" :aria-expanded="advancedOpen" @click="advancedOpen=!advancedOpen">
          高级 {{ advancedOpen ? '▾' : '▸' }}
        </button>
        <template v-if="advancedOpen">
          <button v-for="n in advancedNavs" :key="n.key" type="button" class="nav-item" :class="{active: view===n.key}" :title="n.tip" :aria-current="view===n.key ? 'page' : undefined" @click="n.click ? n.click() : (view=n.key)">
            {{n.label}}<span v-if="n.badge" class="badge">{{n.badge}}</span>
          </button>
        </template>
      </nav>
    </aside>
    <div class="workspace">
      <!-- 统一可见错误态：fail-closed / 401 / 429 / 网关离线，含一键重试 -->
      <ErrorBanner />

      <!-- 渲染错误边界：单视图崩溃不拖垮应用外壳 -->
      <ErrorBoundary>

      <!-- 四区：左上=文件树，右上=终端，左下=编辑器 -->
      <div v-if="view==='term'||view==='files'" class="grid">
        <FileTreePanel />
        <TerminalPanel />
        <EditorPanel />
      </div>

      <!-- 接入向导 -->
      <OnboardPanel v-if="view==='onboard'" />

      <!-- 审批 -->
      <ApprovalPanel v-if="view==='approve'" />

      <!-- 凭据与作用域 -->
      <CredentialScopePanel v-if="view==='scope'" />

      <!-- 传输队列 -->
      <TransferQueuePanel v-if="view==='transfer'" />

      <!-- 主机密钥 -->
      <HostKeyPanel v-if="view==='hostkeys'" />

      <!-- S3-1 五页面 -->
      <ConnectionsPanel v-if="view==='connections'" class="panel-page" />
      <PolicyPanel v-if="view==='policy'" class="panel-page" />
      <AuditPanel v-if="view==='audit'" class="panel-page" />
      <ExperimentsPanel v-if="view==='experiments'" class="panel-page" />
      <SettingsPanel v-if="view==='settings'" class="panel-page" />
      <HelpPanel v-if="view==='help'" class="panel-page" />

      </ErrorBoundary>

    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'

import ErrorBanner from '../components/ErrorBanner.vue'
import ErrorBoundary from '../components/ErrorBoundary.vue'
import FileTreePanel from '../components/FileTreePanel.vue'
import TerminalPanel from '../components/TerminalPanel.vue'
import EditorPanel from '../components/EditorPanel.vue'
import OnboardPanel from '../components/OnboardPanel.vue'
import ApprovalPanel from '../components/ApprovalPanel.vue'
import CredentialScopePanel from '../components/CredentialScopePanel.vue'
import TransferQueuePanel from '../components/TransferQueuePanel.vue'
import HostKeyPanel from '../components/HostKeyPanel.vue'
import ConnectionsPanel from '../components/ConnectionsPanel.vue'
import PolicyPanel from '../components/PolicyPanel.vue'
import AuditPanel from '../components/AuditPanel.vue'
import ExperimentsPanel from '../components/ExperimentsPanel.vue'
import SettingsPanel from '../components/SettingsPanel.vue'
import HelpPanel from '../components/HelpPanel.vue'

import { status, refresh } from '../adapters/useGateway'
import { activeTarget } from '../core/useSsh'
import { showConnectForm, openLocalTab } from '../core/useTerminal'
import { pendingCount, loadTasks, startApprovalPolling } from '../core/useApproval'
import { loadTokens } from '../core/useCredentials'
import { loadTq } from '../core/useTransferQueue'
import { loadHostKeys, loadGatewayHostKeys } from '../core/useHostKeys'
import { fetchOnboard } from '../core/useOnboard'
import { registerTerminalListener, registerWindowFocusListener } from '../core/useTerminal'
import { registerMonitorListener } from '../core/useMonitor'
import { registerTqListener } from '../core/useTransferQueue'
import { registerHostKeyListener } from '../core/useHostKeys'
import { view as useNavView } from '../core/useNav'

// R2 信息架构：默认 5 项自明入口（工作区 2 + 日常 3），内部功能 8 项收进「高级」折叠分组
interface NavEntry { key: string; label: string; tip: string; click?: () => void; badge?: number }
const advancedOpen = ref(false)
const mainNavs: NavEntry[] = [
  { key: 'term', label: '终端', tip: '远程主机 SSH 会话与本地 Shell' },
  { key: 'files', label: '文件', tip: '浏览与编辑远程主机上的文件' },
]
const dailyNavs: NavEntry[] = [
  { key: 'connections', label: '🔗 连接管理', tip: '保存的主机档案，双击即可连接' },
  { key: 'settings', label: '⚙ 设置', tip: '界面与行为偏好' },
  { key: 'help', label: '❓ 帮助', tip: '上手指引与常见问题' },
]
// 审批徽章随待审数动态变化，收进高级分组后仍可见
const advancedNavs = computed<NavEntry[]>(() => [
  { key: 'onboard', label: '🔌 接入向导', tip: '生成接入代码片段与令牌说明' },
  { key: 'approve', label: '✅ 审批', tip: '高危操作的放行队列', click: openApprove, badge: pendingCount.value || undefined },
  { key: 'scope', label: '🔑 凭据与作用域', tip: '网关令牌与权限范围', click: () => { view.value = 'scope'; loadTokens() } },
  { key: 'transfer', label: '📤 传输队列', tip: '文件传输任务与进度', click: () => { view.value = 'transfer'; loadTq() } },
  { key: 'hostkeys', label: '🛡 主机密钥', tip: 'SSH 主机指纹核对与管理', click: () => { view.value = 'hostkeys'; loadHostKeys(); loadGatewayHostKeys() } },
  { key: 'policy', label: '📜 策略配置', tip: '命令白名单与访问策略' },
  { key: 'audit', label: '🧾 审计中心', tip: '操作日志检索与审计轨迹' },
  { key: 'experiments', label: '🧪 实验详情', tip: '内部实验工单进度' },
])
// 视图状态在 core/useNav（全局），供页面内跳转（如 C4 结果目录导航）复用
const view = useNavView

// 审批入口：切视图的同时拉取待审任务
function openApprove() { view.value = 'approve'; loadTasks() }

// R3 状态区动作：未连接时一键去新建连接 / 开本地 Shell
function gotoConnect() { view.value = 'term'; showConnectForm.value = true }
function openLocal() { openLocalTab() }

// ---------- 启动编排 ----------
// 事件订阅在应用级注册（应用生命周期内常驻），避免跨视图切换丢事件；卸载时统一解绑。
// vault_status 由 ConnectionsPanel 自行订阅（页面级状态，随页面生命周期）。
const unlisteners: UnlistenFn[] = []
let pollTimer: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
  await refresh()
  unlisteners.push(await registerWindowFocusListener())
  unlisteners.push(await registerTerminalListener())
  unlisteners.push(...await registerMonitorListener())
  unlisteners.push(await registerHostKeyListener())
  unlisteners.push(await registerTqListener())

  // R1：不再自动连内测默认目标——首屏呈现「新建连接」表单，由用户填写后连接
  fetchOnboard()
  loadTasks()
  loadTq()
  loadHostKeys()
  pollTimer = startApprovalPolling()
})

onUnmounted(() => {
  unlisteners.forEach(u => u())
  unlisteners.length = 0
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null }
})
</script>
