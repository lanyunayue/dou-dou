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
      <nav class="nav" aria-label="主导航">
        <div class="nav-group">工作区</div>
        <button v-for="n in navs" :key="n.key" type="button" class="nav-item" :class="{active: view===n.key}" :aria-current="view===n.key ? 'page' : undefined" @click="view=n.key">{{n.label}}</button>
        <div class="nav-group">管理</div>
        <button type="button" class="nav-item" :class="{active: view==='connections'}" :aria-current="view==='connections' ? 'page' : undefined" @click="view='connections'">🔗 连接管理</button>
        <button type="button" class="nav-item" :class="{active: view==='policy'}" :aria-current="view==='policy' ? 'page' : undefined" @click="view='policy'">📜 策略配置</button>
        <button type="button" class="nav-item" :class="{active: view==='audit'}" :aria-current="view==='audit' ? 'page' : undefined" @click="view='audit'">🧾 审计中心</button>
        <button type="button" class="nav-item" :class="{active: view==='experiments'}" :aria-current="view==='experiments' ? 'page' : undefined" @click="view='experiments'">🧪 实验详情</button>
        <button type="button" class="nav-item" :class="{active: view==='settings'}" :aria-current="view==='settings' ? 'page' : undefined" @click="view='settings'">⚙ 设置</button>
        <div class="nav-group">凭据与安全</div>
        <button type="button" class="nav-item" :class="{active: view==='onboard'}" :aria-current="view==='onboard' ? 'page' : undefined" @click="view='onboard'">🔌 接入向导</button>
        <button type="button" class="nav-item" :class="{active: view==='approve'}" :aria-current="view==='approve' ? 'page' : undefined" @click="openApprove">✅ 审批<span v-if="pendingCount" class="badge">{{pendingCount}}</span></button>
        <button type="button" class="nav-item" :class="{active: view==='scope'}" :aria-current="view==='scope' ? 'page' : undefined" @click="view='scope'; loadTokens()">🔑 凭据与作用域</button>
        <button type="button" class="nav-item" :class="{active: view==='transfer'}" :aria-current="view==='transfer' ? 'page' : undefined" @click="view='transfer'; loadTq()">📤 传输队列</button>
        <button type="button" class="nav-item" :class="{active: view==='hostkeys'}" :aria-current="view==='hostkeys' ? 'page' : undefined" @click="view='hostkeys'; loadHostKeys(); loadGatewayHostKeys()">🛡 主机密钥</button>
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

      </ErrorBoundary>

    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
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

import { status, connSummary, refresh } from '../adapters/useGateway'
import { pendingCount, loadTasks, startApprovalPolling } from '../core/useApproval'
import { loadTokens } from '../core/useCredentials'
import { loadTq } from '../core/useTransferQueue'
import { loadHostKeys, loadGatewayHostKeys } from '../core/useHostKeys'
import { fetchOnboard } from '../core/useOnboard'
import { newTab, registerTerminalListener, registerWindowFocusListener } from '../core/useTerminal'
import { registerTqListener } from '../core/useTransferQueue'
import { registerHostKeyListener } from '../core/useHostKeys'
import { loadFiles } from '../core/useFileTree'
import { view as useNavView } from '../core/useNav'

const navs = [{key:'term',label:'终端'},{key:'files',label:'文件'}]
// 视图状态在 core/useNav（全局），供页面内跳转（如 C4 结果目录导航）复用
const view = useNavView

// 审批入口：切视图的同时拉取待审任务
function openApprove() { view.value = 'approve'; loadTasks() }

// ---------- 启动编排 ----------
// 事件订阅在应用级注册（应用生命周期内常驻），避免跨视图切换丢事件；卸载时统一解绑。
// vault_status 由 ConnectionsPanel 自行订阅（页面级状态，随页面生命周期）。
const unlisteners: UnlistenFn[] = []
let pollTimer: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
  await refresh()
  unlisteners.push(await registerWindowFocusListener())
  unlisteners.push(await registerTerminalListener())
  unlisteners.push(await registerHostKeyListener())
  unlisteners.push(await registerTqListener())

  await newTab()
  loadFiles('/home/dou')
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
