<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { status, connSummary, refresh, gwFetch } from '../adapters/useGateway'
import { snapshot, importMoba } from '../commands/useShell'
import { notify } from '../shared/feedback'
import StateShell from './StateShell.vue'

/**
 * S3-1 设置页：网关运行状态、健康探针、离线架构说明、工具动作聚合
 * （快照 / 导入 MobaXterm 从侧栏迁入，侧栏减负）。
 */
const shell = ref<'loading' | 'empty' | 'error' | 'ready'>('loading')
const errorText = ref('')
const health = ref('')
let alive = true

async function load() {
  shell.value = 'loading'
  try {
    await refresh()
    const r = await gwFetch('/health')
    const j = await r.json()
    health.value = j?.status ?? 'ok'
    if (alive) shell.value = 'ready'
  } catch (e: any) {
    if (!alive) return
    errorText.value = '网关状态获取失败：' + (e?.message ?? String(e))
    shell.value = 'error'
  }
}

async function runSnapshot() { try { await snapshot() } catch (e: any) { notify('快照失败：' + (e?.message ?? String(e))) } }
async function runImport() { try { await importMoba() } catch (e: any) { notify('导入失败：' + (e?.message ?? String(e))) } }

onMounted(async () => { await load() })
</script>

<template>
  <StateShell :state="shell" :error-text="errorText" @retry="load">
    <div class="wizard">
      <h3 class="sec-title">网关</h3>
      <div class="set-row"><span class="set-k">端点</span><code class="mono">{{ status.endpoint }}</code></div>
      <div class="set-row"><span class="set-k">状态</span><span :class="status.running ? 'ok-text' : 'warn-text'">{{ status.running ? '在线' : '离线' }}</span></div>
      <div class="set-row"><span class="set-k">健康探针</span><span class="mono">{{ health || '-' }}</span></div>
      <div class="set-row"><span class="set-k">连通性</span><span class="hint">{{ connSummary }}</span></div>
    </div>

    <div class="wizard">
      <h3 class="sec-title">工具动作</h3>
      <div class="row">
        <button type="button" class="btn ok" @click="runSnapshot">📸 采集实验快照</button>
        <button type="button" class="btn" @click="runImport">📥 导入 MobaXterm 会话</button>
      </div>
      <div class="hint">快照按「命令哈希 + 参数规范化 + 数据集路径」生成三段指纹，重复执行会在「实验详情」聚合提示。</div>
    </div>

    <div class="wizard">
      <h3 class="sec-title">离线架构说明</h3>
      <div class="hint arch-note">
        网关只监听 127.0.0.1 回环地址，不接受外部连接；所有远端操作经网关白名单与审计链约束。
        远程主机离线时终端/文件/传输优雅降级，HTTP 管理面（审计/策略/快照）不受影响。
        本应用无遥测、无 CDN 依赖、无网络升级通道。
      </div>
    </div>
  </StateShell>
</template>

<style scoped>
.sec-title { margin: 0 0 10px; font-size: var(--dou-fs-body); color: var(--dou-accent-strong); }
.sec-title::before { content: "// "; color: var(--dou-dim); font-weight: 400; }
.set-row { display: flex; gap: 12px; align-items: baseline; margin: 8px 0; flex-wrap: wrap; }
.set-k { color: var(--dou-dim); min-width: 72px; font-size: var(--dou-fs-sm); }
.arch-note { max-width: 64ch; }
.wizard { border-bottom: 1px solid var(--dou-border); }
.wizard:last-child { border-bottom: none; }
</style>
