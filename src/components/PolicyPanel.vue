<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { gwFetch } from '../adapters/useGateway'
import { useRequestGuard } from '../core/useRequestGuard'
import StateShell from './StateShell.vue'

/**
 * S3-1 策略配置页：网关内置规则列表（observe/execute/change 档位，与后端
 * policy_rule.tier、凭据档位同一套取值；动作列 action 才是 allow/approve/deny）。
 * 规则为只读呈现（红线 2：不放宽网关安全判定），编辑通道不在此页提供。
 */
interface PolicyRule {
  id: string
  tier: string
  pattern: string
  action: string
  enabled: boolean
  builtin: boolean
}

const shell = ref<'loading' | 'empty' | 'error' | 'ready'>('loading')
const errorText = ref('')
const rules = ref<PolicyRule[]>([])
const guard = useRequestGuard()

const tierLabel: Record<string, string> = { observe: '观察只读', execute: '执行待审', change: '变更拒绝' }

async function load() {
  const id = guard.begin()
  shell.value = 'loading'
  try {
    const r = await gwFetch('/policy/rules')
    const j = await r.json()
    if (!guard.isCurrent(id)) return
    rules.value = j.items ?? []
    shell.value = rules.value.length ? 'ready' : 'empty'
  } catch (e: any) {
    if (!guard.isCurrent(id)) return
    errorText.value = '规则加载失败：' + (e?.message ?? String(e))
    shell.value = 'error'
  }
}

function byTier(tier: string) { return rules.value.filter(r => r.tier === tier) }

onMounted(load)
</script>

<template>
  <StateShell :state="shell" :error-text="errorText" empty-text="网关未返回任何策略规则" @retry="load">
    <div class="wizard" v-for="tier in ['observe', 'execute', 'change']" :key="tier">
      <h3 class="tier-title" :data-tier="tier">{{ tierLabel[tier] }}档 · {{ byTier(tier).length }} 条</h3>
      <table class="scope-table" v-if="byTier(tier).length">
        <thead><tr><th>模式</th><th>动作</th><th>状态</th><th>来源</th></tr></thead>
        <tbody>
          <tr v-for="r in byTier(tier)" :key="r.id">
            <td class="mono">{{ r.pattern }}</td>
            <td class="mono">{{ r.action }}</td>
            <td>
              <span :class="r.enabled ? 'ok-text' : 'hint'">{{ r.enabled ? '启用' : '停用' }}</span>
            </td>
            <td class="hint">{{ r.builtin ? '内置' : '自定义' }}</td>
          </tr>
        </tbody>
      </table>
      <div v-else class="hint" style="padding: 8px 0;">该档位暂无规则</div>
    </div>
  </StateShell>
</template>

<style scoped>
.tier-title { margin: 0 0 8px; font-size: var(--dou-fs-body); color: var(--dou-text); }
.tier-title::before { content: "§ "; color: var(--dou-accent); }
.tier-title[data-tier="change"] { color: var(--dou-danger-text); }
.tier-title[data-tier="execute"] { color: var(--dou-warn-text); }
.tier-title[data-tier="observe"] { color: var(--dou-ok-text); }
</style>
