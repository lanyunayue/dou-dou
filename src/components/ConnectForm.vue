<template>
  <div class="connect-card" role="form" aria-label="新建连接">
    <h3 class="cc-title">新建连接</h3>
    <p class="cc-sub">连接一台 SSH 主机；密钥连不上时可用密码认证兜底</p>
    <div class="cc-row">
      <input v-model="host" class="filter cc-host" placeholder="主机地址，如 192.168.1.10" aria-label="主机地址" @keyup.enter="submit" />
      <input v-model="port" class="filter cc-port" placeholder="22" aria-label="端口" @keyup.enter="submit" />
      <input v-model="user" class="filter cc-user" placeholder="用户名" aria-label="用户名" @keyup.enter="submit" />
    </div>
    <div class="cc-row">
      <select v-model="auth" class="cc-auth" aria-label="认证方式">
        <option value="key">密钥认证</option>
        <option value="password">密码认证</option>
      </select>
      <input v-if="auth==='password'" v-model="password" type="password" class="filter cc-pw"
             placeholder="密码（仅驻留内存，不落盘）" aria-label="SSH 密码" autocomplete="off" @keyup.enter="submit" />
      <button type="button" class="btn ok cc-go" :disabled="connecting" @click="submit">{{ connecting ? '连接中…' : '连接' }}</button>
      <button v-if="closable" type="button" class="btn" @click="close">取消</button>
    </div>
    <div v-if="sshError" class="cc-error" role="alert">{{ sshError }}</div>
    <div v-else class="hint">{{ hint }}</div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { connectSsh, tabs, showConnectForm } from '../core/useTerminal'
import { sshError } from '../core/useSsh'

const host = ref('')
const port = ref('22')
const user = ref('')
const auth = ref<'key' | 'password'>('key')
const password = ref('')
const connecting = ref(false)

// 已有会话时表单可关闭；无会话时为常显首屏
const closable = computed(() => tabs.value.length > 0)
const hint = computed(() => auth.value === 'password'
  ? '密码模式：密码不落盘、不写日志，仅本次连接在内存使用'
  : '密钥模式：使用本机私钥（Rust 侧自动推导路径，也可在接入向导配置）')

async function submit() {
  connecting.value = true
  try {
    await connectSsh({
      host: host.value, port: Number(port.value) || 22, user: user.value,
      auth: auth.value, password: auth.value === 'password' ? password.value : undefined,
    })
    if (!sshError.value) password.value = ''
  } finally {
    connecting.value = false
  }
}

function close() {
  showConnectForm.value = false
}
</script>

<style scoped>
.connect-card {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  gap: 10px;
  background: var(--dou-bg, #0b0f0e);
  padding: 24px;
  z-index: 5;
}
.cc-title { margin: 0; font-size: 18px; color: var(--dou-text); }
.cc-sub { margin: 0; font-size: 13px; color: var(--dou-dim); }
.cc-row { display: flex; gap: 8px; flex-wrap: wrap; justify-content: center; }
.cc-host { width: 220px; }
.cc-port { width: 72px; }
.cc-user { width: 140px; }
.cc-auth {
  height: 30px;
  background: var(--dou-panel);
  color: var(--dou-text);
  border: 1px solid var(--dou-border);
  border-radius: 4px;
  font-size: 13px;
  padding: 0 6px;
}
.cc-pw { width: 220px; }
.cc-go { min-width: 88px; }
.cc-error { color: var(--dou-danger); font-size: 13px; max-width: 460px; text-align: center; }
</style>
