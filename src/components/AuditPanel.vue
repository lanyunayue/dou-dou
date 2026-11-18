<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { gwFetch } from '../adapters/useGateway'
import { notify } from '../shared/feedback'
import { verdictClass } from '../shared/verdict'
import { useRequestGuard } from '../core/useRequestGuard'
import StateShell from './StateShell.vue'

/**
 * S3-1 审计中心。判据：按时间/来源/判定检索，导出与检索共用参数（所见即所得）。
 * 数据源 /audit/query 与 /audit/export，均支持 since_ms / source / verdict。
 */
interface AuditItem {
  ts_ms?: string
  ts?: string
  source?: string
  action?: string
  target?: string
  verdict?: string
  trace_id?: string
  [k: string]: unknown
}

const shell = ref<'loading' | 'empty' | 'error' | 'ready'>('loading')
const errorText = ref('')
const items = ref<AuditItem[]>([])
const chainOk = ref<boolean | null>(null)
const entries = ref(0)
const guard = useRequestGuard()

const since = ref('all')          // all | 1h | 24h | 7d
const source = ref('')            // 空 = 全部（动态候选）
const verdict = ref('')           // 空 = 全部（动态候选）
const sources = ref<string[]>([])
const verdicts = ref<string[]>([])

const sinceOptions = [
  { key: '1h', label: '近 1 小时', ms: 3600e3 },
  { key: '24h', label: '近 24 小时', ms: 24 * 3600e3 },
  { key: '7d', label: '近 7 天', ms: 7 * 24 * 3600e3 },
  { key: 'all', label: '全部时间', ms: 0 },
]

function sinceMs(): string {
  const opt = sinceOptions.find(o => o.key === since.value)
  return opt && opt.ms ? String(Date.now() - opt.ms) : ''
}

function queryString(): string {
  const p = new URLSearchParams()
  const s = sinceMs(); if (s) p.set('since_ms', s)
  if (source.value) p.set('source', source.value)
  if (verdict.value) p.set('verdict', verdict.value)
  p.set('limit', '300')
  return p.toString()
}

async function load() {
  const id = guard.begin()
  shell.value = 'loading'
  try {
    const r = await gwFetch('/audit/query?' + queryString())
    const j = await r.json()
    if (!guard.isCurrent(id)) return
    items.value = j.items ?? []
    // 从当前结果动态提取来源/判定候选（避免硬编码 verdict 词表漂移）
    sources.value = [...new Set(items.value.map(i => i.source).filter(Boolean))] as string[]
    verdicts.value = [...new Set(items.value.map(i => i.verdict).filter(Boolean))] as string[]
    shell.value = items.value.length ? 'ready' : 'empty'
  } catch (e: any) {
    if (!guard.isCurrent(id)) return
    errorText.value = '审计检索失败：' + (e?.message ?? String(e))
    shell.value = 'error'
  }
}

async function loadChain() {
  try {
    const r = await gwFetch('/audit/verify')
    const j = await r.json()
    chainOk.value = j.chain_ok
    entries.value = j.entries ?? 0
  } catch { chainOk.value = null }
}

/** 导出：与当前检索同一套过滤参数，所见即所得。 */
async function exportCsv() {
  try {
    const r = await gwFetch('/audit/export?' + queryString())
    const text = await r.text()
    if (!r.ok) { notify(`导出失败：网关状态 ${r.status}`); return }
    const blob = new Blob([text], { type: 'text/csv;charset=utf-8' })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `dou-audit-${new Date().toISOString().slice(0, 19).replace(/[T:]/g, '-')}.csv`
    a.click()
    URL.revokeObjectURL(a.href)
  } catch (e: any) { notify('导出失败：' + (e?.message ?? String(e))) }
}

function fmtTime(item: AuditItem): string {
  const raw = item.ts_ms ?? item.ts ?? ''
  const n = Number(raw)
  if (!raw || !Number.isFinite(n) || n <= 0) return String(raw || '-')
  return new Date(n).toLocaleString('zh-CN', { hour12: false })
}

const countText = computed(() => `共 ${items.value.length} 条（最多显示 300 条）`)

onMounted(() => { load(); loadChain() })
</script>

<template>
  <div class="audit-page">
    <div class="fileopts" role="group" aria-label="审计过滤器">
      <label>时间
        <select v-model="since" @change="load" aria-label="时间范围">
          <option v-for="o in sinceOptions" :key="o.key" :value="o.key">{{ o.label }}</option>
        </select>
      </label>
      <label>来源
        <select v-model="source" @change="load" aria-label="来源">
          <option value="">全部</option>
          <option v-for="s in sources" :key="s" :value="s">{{ s }}</option>
        </select>
      </label>
      <label>判定
        <select v-model="verdict" @change="load" aria-label="判定">
          <option value="">全部</option>
          <option v-for="v in verdicts" :key="v" :value="v">{{ v }}</option>
        </select>
      </label>
      <span class="hint">{{ countText }}</span>
      <button type="button" class="mini-btn" @click="load">重查</button>
      <button type="button" class="mini-btn" @click="exportCsv">导出 CSV</button>
    </div>

    <div class="chain-line hint" :class="chainOk === false ? 'v-deny' : chainOk === true ? 'v-allow' : ''">
      审计链：{{ chainOk === null ? '未知' : chainOk ? `完整（${entries} 条）` : '已断裂！存在被篡改风险' }}
    </div>

    <StateShell :state="shell" :error-text="errorText" empty-text="当前过滤条件下无审计记录" @retry="load">
      <div class="auditlist" role="table" aria-label="审计记录">
        <div v-for="(it, i) in items" :key="it.trace_id ?? i" class="audititem" role="row">
          <span class="a-time">{{ fmtTime(it) }}</span>
          <span class="a-src">{{ it.source || '-' }}</span>
          <span class="a-act">{{ it.action || '-' }}</span>
          <span class="a-tgt" :title="String(it.target ?? '')">{{ it.target || '-' }}</span>
          <span class="a-verdict" :class="verdictClass(it.verdict)">{{ it.verdict || '-' }}</span>
        </div>
      </div>
    </StateShell>
  </div>
</template>

<style scoped>
.audit-page { height: 100%; display: flex; flex-direction: column; min-height: 0; }
.chain-line { padding: 8px 14px; border-bottom: 1px solid var(--dou-border); }
.a-time { display: inline-block; min-width: 150px; color: var(--dou-dim); }
.a-src { display: inline-block; min-width: 70px; color: var(--dou-info); }
.a-act { display: inline-block; min-width: 150px; }
.a-tgt { color: var(--dou-dim); overflow: hidden; text-overflow: ellipsis; max-width: 32%; display: inline-block; vertical-align: bottom; }
.a-verdict { display: inline-block; min-width: 64px; text-align: center; border-radius: 999px; padding: 0 8px; margin-left: 8px; }
.v-allow { color: var(--dou-ok-text); background: var(--dou-ok-bg); }
.v-deny { color: var(--dou-danger-text); background: var(--dou-danger-bg); }
.v-ask { color: var(--dou-warn-text); background: var(--dou-warn-bg); }
</style>
