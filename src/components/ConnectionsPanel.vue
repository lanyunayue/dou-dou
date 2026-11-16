<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { SSH_KEY } from '../core/useSsh'
import { notify, askConfirm } from '../shared/feedback'
import { useRequestGuard } from '../core/useRequestGuard'
import StateShell from './StateShell.vue'

/**
 * S3-1 连接管理页。判据：3 步内新增主机 ——
 * 填 host/user（名称留空自动取 user@host）→ 点「加密入库」→ 列表出现。
 */
interface VaultTarget {
  name: string
  host: string
  port: number
  user: string
  /** 认证方式：'key'（默认）/ 'password'（密码不入库，连接时输入） */
  auth?: string
  key_path?: string
  enc_key?: string
}
interface VaultKeyInfo { algorithm: string; fingerprint: string }

const shell = ref<'loading' | 'empty' | 'error' | 'ready'>('loading')
const errorText = ref('')
const targets = ref<VaultTarget[]>([])
const hostKeys = ref<any[]>([])
const unlocked = ref(false)
const unlisteners: UnlistenFn[] = []
const guard = useRequestGuard()

const quickName = ref(''); const quickHost = ref(''); const quickPort = ref(22); const quickUser = ref('dou')
const quickAuth = ref<'key' | 'password'>('key')
const masterPw = ref('')
/** 密码模式目标的「本次连接密码」：只驻留内存，随输入/用完即清，不落盘 */
const pwFor = ref(''); const pwInput = ref('')

/** 保险库初始化/解锁（原「凭据保险库」页入口迁入此处）。 */
async function vaultInit() {
  try { await invoke('vault_init', { master: masterPw.value }); unlocked.value = true; masterPw.value = ''; await load(); notify('保险库已初始化并解锁') }
  catch (e: any) { notify('初始化失败: ' + e) }
}
async function vaultUnlock() {
  try { await invoke('vault_unlock', { master: masterPw.value }); unlocked.value = true; masterPw.value = ''; await load() }
  catch (e: any) { notify('解锁失败: ' + e) }
}
function vaultLock() {
  invoke('vault_lock'); unlocked.value = false; targets.value = []
}

async function load() {
  const id = guard.begin()
  shell.value = 'loading'
  try {
    const list = await invoke<VaultTarget[]>('vault_list_targets')
    const keys = await invoke<any[]>('host_key_list')
    if (!guard.isCurrent(id)) return
    targets.value = list
    hostKeys.value = keys
    shell.value = list.length ? 'ready' : 'empty'
  } catch (e: any) {
    if (!guard.isCurrent(id)) return
    errorText.value = '加载连接档案失败：' + (e?.message ?? String(e))
    shell.value = 'error'
  }
}

/** 3 步新增：填表 → 入库 → 完成（名称缺省 = user@host）。
 *  密码模式：目标只存地址与认证方式，密码不入库（连接时临时输入）。 */
async function quickAdd() {
  if (!quickHost.value.trim()) { notify('请填写主机地址'); return }
  const name = quickName.value.trim() || `${quickUser.value}@${quickHost.value.trim()}`
  try {
    await invoke('vault_add_target', {
      name, host: quickHost.value.trim(), port: Number(quickPort.value) || 22,
      user: quickUser.value.trim() || 'root',
      auth: quickAuth.value,
      keyPath: quickAuth.value === 'password' ? '' : SSH_KEY,
    })
    notify(quickAuth.value === 'password' ? `已入库（密码模式）：${name} —— 连接时输入密码` : `已加密入库：${name}`)
    quickName.value = ''; quickHost.value = ''
    await load()
  } catch (e: any) { notify('入库失败: ' + e) }
}

async function getKey(name: string) {
  try {
    const info = await invoke<VaultKeyInfo>('vault_get_key', { name })
    notify(`私钥已在内存中解密校验：\n算法 ${info.algorithm}\n指纹 ${info.fingerprint}\n（明文未落盘，用完即清）`)
  } catch (e: any) { notify('取出失败: ' + e) }
}

async function testConnect(name: string) {
  try { notify(await invoke<string>('vault_test_connect', { name })) }
  catch (e: any) { notify('连接失败: ' + e) }
}

/** 密码模式连接：密码仅存在于本次调用内存（Rust 侧 Zeroizing，用完即弃）。 */
async function testConnectPw(name: string) {
  if (!pwInput.value) { notify('请输入本次连接密码'); return }
  try {
    notify(await invoke<string>('vault_test_connect', { name, password: pwInput.value }))
    pwInput.value = ''; pwFor.value = ''
  } catch (e: any) { notify('连接失败: ' + e) }
}

function authLabel(t: VaultTarget): string {
  if (t.enc_key) return '🔑 已加密'
  return t.auth === 'password' ? '🔒 密码认证（连接时输入密码）' : '— 未配钥'
}

async function removeTarget(name: string) {
  if (!askConfirm(`删除连接档案「${name}」？该操作不可恢复。`)) return
  try { await invoke('vault_remove_target', { name }); await load() }
  catch (e: any) { notify('删除失败: ' + e) }
}

function fingerprintOf(addr: string): string {
  const k = hostKeys.value.find((h: any) => `${h.host}:${h.port}` === addr)
  return k?.fingerprint ? String(k.fingerprint).slice(0, 20) + '…' : '未记录（首次连接时确认）'
}

onMounted(async () => {
  await load()
  unlisteners.push(await listen('vault_status', (e: any) => { unlocked.value = e.payload }))
  try { unlocked.value = await invoke<boolean>('vault_status') } catch { unlocked.value = false }
})

onUnmounted(() => { unlisteners.forEach(u => u()); unlisteners.length = 0 })
</script>

<template>
  <StateShell :state="shell" :error-text="errorText" empty-text="暂无连接档案 —— 填写主机信息即可 3 步入库" @retry="load">
    <div class="wizard">
      <div class="row vault-state">
        <span class="hint" :class="unlocked ? 'ok-text' : 'warn-text'">
          {{ unlocked ? '保险库已解锁' : '保险库锁定中（新增主机需先解锁）' }}
        </span>
      </div>

      <!-- 保险库状态与解锁（私钥明文不落盘，锁定即清内存密钥） -->
      <div class="row vault-state">
        <template v-if="!unlocked">
          <input v-model="masterPw" type="password" class="filter" placeholder="主密码" aria-label="保险库主密码" @keyup.enter="vaultUnlock" />
          <button type="button" class="btn" @click="vaultInit">初始化</button>
          <button type="button" class="btn ok" @click="vaultUnlock">解锁</button>
          <span class="hint">首次使用先「初始化」；解锁后可新增主机。锁定时清除内存密钥。</span>
        </template>
        <template v-else>
          <span class="hint ok-text">保险库已解锁</span>
          <button type="button" class="btn bad" @click="vaultLock">锁定</button>
        </template>
      </div>

      <!-- 3 步快速添加 -->
      <div class="row" role="group" aria-label="快速添加主机">
        <input v-model="quickName" class="filter" placeholder="名称（可留空）" aria-label="连接名称" />
        <input v-model="quickHost" class="filter" placeholder="host，如 10.0.0.224" aria-label="主机地址" />
        <input v-model="quickPort" class="filter quick-port" placeholder="22" aria-label="端口" />
        <input v-model="quickUser" class="filter" placeholder="user" aria-label="用户名" />
        <select v-model="quickAuth" class="quick-auth" aria-label="认证方式">
          <option value="key">密钥认证</option>
          <option value="password">密码认证</option>
        </select>
        <button type="button" class="btn ok" @click="quickAdd">加密入库</button>
      </div>
      <div class="row hint-row">
        <span class="hint">{{ quickAuth === 'password' ? '密码模式：目标只存地址，密码不入库，连接时输入（仅驻留内存）' : '密钥模式：私钥加密入库，明文只在内存解密' }}</span>
      </div>

      <div v-for="t in targets" :key="t.name" class="approve-card">
        <div class="mono conn-line">
          <strong>{{ t.name }}</strong> · {{ t.user }}@{{ t.host }}:{{ t.port }}
          <span :class="t.enc_key ? 'ok-text' : (t.auth === 'password' ? 'ok-text' : 'hint')">{{ authLabel(t) }}</span>
        </div>
        <div class="hint">主机密钥：{{ fingerprintOf(`${t.host}:${t.port}`) }}</div>
        <div class="approve-actions">
          <button type="button" class="btn ok" v-if="t.enc_key" @click="getKey(t.name)">校验密钥</button>
          <button type="button" class="btn" v-if="t.enc_key" @click="testConnect(t.name)">用密钥连接</button>
          <button type="button" class="btn ok" v-if="t.auth === 'password' && !t.enc_key" @click="pwFor = pwFor === t.name ? '' : t.name; pwInput = ''">
            {{ pwFor === t.name ? '收起' : '密码连接' }}
          </button>
          <button type="button" class="btn bad" @click="removeTarget(t.name)">删除</button>
        </div>
        <!-- 密码模式：本次连接密码仅驻留内存，提交后立即清空 -->
        <div v-if="pwFor === t.name" class="row pw-row">
          <input type="password" v-model="pwInput" class="filter" placeholder="本次连接密码（仅内存，不落盘）"
                 aria-label="连接密码" autocomplete="off" @keyup.enter="testConnectPw(t.name)" />
          <button type="button" class="btn ok" @click="testConnectPw(t.name)">连接</button>
        </div>
      </div>
    </div>
  </StateShell>
</template>

<style scoped>
.quick-port { width: 72px; }
.quick-auth {
  height: 26px;
  background: var(--dou-panel);
  color: var(--dou-text);
  border: 1px solid var(--dou-border);
  border-radius: var(--dou-radius-sm, 4px);
  font-size: 12px;
  padding: 0 4px;
}
.hint-row { margin-top: -6px; margin-bottom: 8px; }
.pw-row { margin-top: 8px; }
.pw-row .filter { max-width: 260px; }
.conn-line { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
.vault-state { margin-top: 4px; }
</style>
