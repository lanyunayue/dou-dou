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
        <div class="nav-item" :class="{active: view==='transfer'}" @click="view='transfer'; loadTq()">📤 传输队列</div>
        <div class="nav-item" :class="{active: view==='vault'}" @click="view='vault'; vaultList()">🔐 凭据保险库</div>
        <div class="nav-item" :class="{active: view==='hostkeys'}" @click="view='hostkeys'; loadHostKeys(); loadGatewayHostKeys()">🛡 主机密钥</div>
        <div class="nav-item" @click="importMoba">📥 导入 MobaXterm</div>
        <div class="nav-item" @click="snapshot">📸 快照</div>
      </nav>
    </aside>
    <div class="workspace">
      <!-- 统一可见错误态：fail-closed / 401 / 429 / 网关离线，含一键重试 -->
      <ErrorBanner />

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

      <!-- 凭据保险库 -->
      <div v-if="view==='vault'" class="panel panel-page">
        <div class="panelhead">🔐 凭据保险库（主密码加密存储连接档案 + 私钥）</div>
        <div v-if="!vaultUnlocked" class="row">
          <input v-model="masterPw" type="password" class="filter" placeholder="主密码" />
          <button class="btn" @click="vaultInit">初始化</button>
          <button class="btn ok" @click="vaultUnlock">解锁</button>
          <span class="hint">首次需初始化；之后用主密码解锁。锁定时清除内存密钥；私钥明文不落盘，用后即清。</span>
        </div>
        <div v-else class="row">
          <span class="hint" style="color:#166534">已解锁</span>
          <button class="btn bad" @click="vaultLock">锁定</button>
        </div>
        <div v-if="vaultUnlocked" class="wizard">
          <div class="row">
            <input v-model="newName" class="filter" placeholder="名称" />
            <input v-model="newHost" class="filter" placeholder="host" />
            <input v-model="newPort" class="filter" placeholder="port" style="width:60px" />
            <input v-model="newUser" class="filter" placeholder="user" />
            <input v-model="newKey" class="filter" placeholder="私钥路径" />
            <button class="btn ok" @click="vaultAdd">加密入库</button>
          </div>
          <div v-if="!vaultTargets.length" class="empty">暂无目标</div>
          <div v-for="t in vaultTargets" :key="t.name" class="approve-card">
            <div class="mono">{{ t.name }} · {{ t.user }}@{{ t.host }}:{{ t.port }} {{ t.enc_key? '🔑 已加密':'—' }}</div>
            <div class="approve-actions">
              <button class="btn ok" v-if="t.enc_key" @click="vaultGetKey(t.name)">校验密钥</button>
              <button class="btn" v-if="t.enc_key" @click="vaultTestConnect(t.name)">用密钥连接</button>
              <button class="btn bad" @click="vaultRemove(t.name)">删除</button>
            </div>
          </div>
        </div>
      </div>

      <!-- 主机密钥 -->
      <HostKeyPanel v-if="view==='hostkeys'" />

    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

import ErrorBanner from './components/ErrorBanner.vue'
import FileTreePanel from './components/FileTreePanel.vue'
import TerminalPanel from './components/TerminalPanel.vue'
import EditorPanel from './components/EditorPanel.vue'
import OnboardPanel from './components/OnboardPanel.vue'
import ApprovalPanel from './components/ApprovalPanel.vue'
import CredentialScopePanel from './components/CredentialScopePanel.vue'
import TransferQueuePanel from './components/TransferQueuePanel.vue'
import HostKeyPanel from './components/HostKeyPanel.vue'

import { status, connSummary, refresh } from './composables/useGateway'
import { pendingCount, loadTasks, startApprovalPolling } from './composables/useApproval'
import { loadTokens } from './composables/useCredentials'
import { loadTq } from './composables/useTransferQueue'
import { loadHostKeys, loadGatewayHostKeys } from './composables/useHostKeys'
import { fetchOnboard } from './composables/useOnboard'
import { newTab, registerTerminalListener, registerWindowFocusListener } from './composables/useTerminal'
import { registerTqListener } from './composables/useTransferQueue'
import { registerHostKeyListener } from './composables/useHostKeys'
import { loadFiles } from './composables/useFileTree'
import { snapshot, importMoba } from './composables/useShell'
import { SSH_KEY } from './composables/useSsh'

const navs = [{key:'term',label:'终端'},{key:'files',label:'文件'}]
const view = ref('term')

// 审批入口：切视图的同时拉取待审任务
function openApprove() { view.value = 'approve'; loadTasks() }

// ---------- 凭据保险库 ----------
/** 保险库中某个目标私钥的**非敏感**信息（私钥明文不出后端、不落盘）。 */
interface VaultKeyInfo {
  algorithm: string
  fingerprint: string
}
const vaultUnlocked = ref(false)
const vaultTargets = ref<any[]>([])
const masterPw = ref('')
const newName = ref(''); const newHost = ref(''); const newPort = ref(22); const newUser = ref('dou'); const newKey = ref(SSH_KEY)
async function vaultInit() { try { await invoke('vault_init', { master: masterPw.value }); vaultUnlocked.value = true; await vaultList(); alert('保险库已初始化并解锁') } catch (e: any) { alert('初始化失败: ' + e) } }
async function vaultUnlock() { try { await invoke('vault_unlock', { master: masterPw.value }); vaultUnlocked.value = true; await vaultList() } catch (e: any) { alert('解锁失败: ' + e) } }
function vaultLock() { invoke('vault_lock'); vaultUnlocked.value = false; vaultTargets.value = [] }
async function vaultList() { try { vaultTargets.value = await invoke('vault_list_targets') } catch { vaultTargets.value = [] } }
async function vaultAdd() {
  try { await invoke('vault_add_target', { name: newName.value, host: newHost.value, port: Number(newPort.value), user: newUser.value, keyPath: newKey.value }); await vaultList(); alert('已加密入库') }
  catch (e: any) { alert('入库失败: ' + e) }
}
async function vaultGetKey(name: string) { try { const info = await invoke<VaultKeyInfo>('vault_get_key', { name }); alert('私钥已在内存中解密校验：\n算法 ' + info.algorithm + '\n指纹 ' + info.fingerprint + '\n（明文未落盘，用完即清）') } catch (e: any) { alert('取出失败: ' + e) } }
async function vaultTestConnect(name: string) { try { const msg = await invoke<string>('vault_test_connect', { name }); alert(msg) } catch (e: any) { alert('连接失败: ' + e) } }
async function vaultRemove(name: string) { try { await invoke('vault_remove_target', { name }); await vaultList() } catch (e: any) { alert('删除失败: ' + e) } }

// ---------- 启动编排 ----------
// 事件订阅在应用级注册（应用生命周期内常驻），避免跨视图切换丢事件；卸载时统一解绑。
const unlisteners: UnlistenFn[] = []
let pollTimer: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
  await refresh()
  unlisteners.push(await registerWindowFocusListener())
  unlisteners.push(await registerTerminalListener())
  unlisteners.push(await registerHostKeyListener())
  unlisteners.push(await registerTqListener())
  unlisteners.push(await listen('vault_status', (e: any) => { vaultUnlocked.value = e.payload; if (e.payload) vaultList() }))

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
