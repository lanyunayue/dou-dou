<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { gwFetch } from '../adapters/useGateway'
import { notify } from '../shared/feedback'
import { loadFiles } from '../core/useFileTree'
import { navigateTo } from '../core/useNav'
import { useRequestGuard } from '../core/useRequestGuard'
import StateShell from './StateShell.vue'

/**
 * S3-1 实验详情页（C4）：快照指纹聚合。
 * 同一指纹重跑聚合为 runs；runs>1 即「跑过」，历史结果目录可复制可跳转。
 */
interface SnapshotAgg {
  fingerprint: string
  command: string
  dataset_path: string
  result_dir: string
  collected_at: number
  runs: number
}

const shell = ref<'loading' | 'empty' | 'error' | 'ready'>('loading')
const errorText = ref('')
const snaps = ref<SnapshotAgg[]>([])
const guard = useRequestGuard()

const rerunCount = computed(() => snaps.value.filter(s => s.runs > 1).length)

async function load() {
  const id = guard.begin()
  shell.value = 'loading'
  try {
    const r = await gwFetch('/snapshots')
    const j = await r.json()
    if (!guard.isCurrent(id)) return
    snaps.value = j.items ?? []
    shell.value = snaps.value.length ? 'ready' : 'empty'
  } catch (e: any) {
    if (!guard.isCurrent(id)) return
    errorText.value = '快照加载失败：' + (e?.message ?? String(e))
    shell.value = 'error'
  }
}

async function copyText(text: string, label: string) {
  try { await navigator.clipboard.writeText(text); notify(`${label}已复制`) }
  catch { notify('复制失败：剪贴板不可用') }
}

/** C4 跳转：结果目录在远端靶机 —— 文件树导航过去并切到工作区视图。 */
function openResultDir(dir: string) {
  if (!dir) return
  loadFiles(dir)
  navigateTo('term')
}

function fmtTime(ms: number): string {
  if (!ms) return '-'
  return new Date(ms).toLocaleString('zh-CN', { hour12: false })
}

onMounted(load)
</script>

<template>
  <div class="exp-page">
    <div class="fileopts" role="group" aria-label="实验概览">
      <span class="hint">{{ snaps.length }} 个实验指纹，其中 <span class="warn-text">{{ rerunCount }}</span> 个重复执行</span>
      <button type="button" class="mini-btn" @click="load">刷新</button>
    </div>

    <StateShell :state="shell" :error-text="errorText" empty-text="暂无实验快照 —— 执行「快照」动作后此处聚合显示指纹历史" @retry="load">
      <div class="wizard" v-for="s in snaps" :key="s.fingerprint">
        <div class="exp-head">
          <span class="mono fp">{{ s.fingerprint.slice(0, 12) }}</span>
          <span v-if="s.runs > 1" class="runs-badge warn-text" title="该指纹的历史执行次数">
            跑过 {{ s.runs }} 次
          </span>
          <span v-else class="runs-badge ok-text">首次</span>
          <span class="hint">{{ fmtTime(s.collected_at) }}</span>
        </div>
        <div class="exp-row"><span class="exp-k">命令</span><code class="mono">{{ s.command || '-' }}</code></div>
        <div class="exp-row"><span class="exp-k">数据集</span><code class="mono">{{ s.dataset_path || '-' }}</code></div>
        <div class="exp-row">
          <span class="exp-k">结果目录</span>
          <code class="mono">{{ s.result_dir || '-' }}</code>
          <button type="button" v-if="s.result_dir" class="mini-btn" @click="copyText(s.result_dir, '结果目录')">复制</button>
          <button type="button" v-if="s.result_dir" class="mini-btn" @click="openResultDir(s.result_dir)">在文件树打开</button>
        </div>
      </div>
    </StateShell>
  </div>
</template>

<style scoped>
.exp-page { height: 100%; overflow-y: auto; display: flex; flex-direction: column; min-height: 0; }
.exp-page .wizard { border-bottom: 1px solid var(--dou-border); }
.exp-head { display: flex; gap: 10px; align-items: center; margin-bottom: 8px; flex-wrap: wrap; }
.fp { color: var(--dou-accent-strong); font-weight: 700; font-size: var(--dou-fs-body); }
.runs-badge { border: 1px solid currentColor; border-radius: 999px; padding: 0 10px; font-size: var(--dou-fs-xs); }
.exp-row { display: flex; gap: 8px; align-items: baseline; margin: 4px 0; flex-wrap: wrap; }
.exp-k { color: var(--dou-dim); font-size: var(--dou-fs-sm); min-width: 56px; }
.exp-row code { word-break: break-all; }
</style>
