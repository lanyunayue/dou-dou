<template>
  <div class="panel panel-page">
    <div class="panelhead">✅ 待审批任务（{{ pendingCount }}）</div>
    <div v-if="gwDisconnected" class="orphan-banner live">
      ⚠️ 与网关失联中：待审任务状态可能过期，连接恢复后自动刷新。
    </div>
    <div v-else-if="orphanCount" class="orphan-banner">
      ⚠️ 有 {{ orphanCount }} 个任务在客户端断开期间保持挂起（置顶显示），可人工终止。
    </div>
    <div v-if="!pendingTasksView.length" class="empty">暂无待审任务 —— 高危操作需要放行时会出现在这里</div>
    <div v-for="t in pendingTasksView" :key="t.id" class="approve-card" :class="{ orphan: t.orphaned }">
      <div v-if="t.orphaned" class="orphan-badge">客户端已断开期间挂起</div>
      <div class="mono cmd">{{ t.command }}</div>
      <div class="hint">原因：{{ t.reason }}</div>
      <div class="hint meta">
        <span v-if="t.impact">预计影响：{{ t.impact }}</span>
        <span v-if="t.target_path">目标路径：{{ t.target_path }}</span>
        <span v-if="t.history_count > 0">历史同类 {{ t.history_count }} 次</span>
      </div>
      <div class="approve-actions">
        <button class="btn ok" :disabled="busyTask === t.id" @click="decideTask(t.id, true, false)">{{ busyTask === t.id ? '提交中…' : '一次允许' }}</button>
        <button class="btn ok" :disabled="t.policy==='change' || busyTask === t.id" :title="t.policy==='change' ? '变更档不可关闭审批' : ''" @click="decideTask(t.id, true, true)">{{ busyTask === t.id ? '提交中…' : '本次任务内允许' }}</button>
        <input v-model="rejectNotes[t.id]" class="filter" placeholder="拒绝理由（必填，将留痕）" :disabled="busyTask === t.id" />
        <button class="btn bad" :disabled="busyTask === t.id" @click="decideTask(t.id, false, false)">{{ busyTask === t.id ? '提交中…' : '拒绝' }}</button>
        <button class="btn" :disabled="busyTask === t.id" title="终止网关侧任务（仅对执行中/挂起任务生效）" @click="cancelTask(t.id)">{{ busyTask === t.id ? '终止中…' : '终止任务' }}</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { pendingTasksView, rejectNotes, pendingCount, gwDisconnected, busyTask, decideTask, cancelTask } from '../core/useApproval'

const orphanCount = computed(() => pendingTasksView.value.filter(t => t.orphaned).length)
</script>

<style scoped>
.approve-card .meta { display: flex; gap: 12px; flex-wrap: wrap; margin-top: 2px; }
.approve-card .meta span { color: var(--dou-dim); }
.approve-card.orphan { border-left: 3px solid var(--dou-warn); }
.orphan-banner { margin: 8px 10px 0; padding: 8px 10px; font-size: var(--dou-fs-xs); border: 1px solid var(--dou-warn); background: var(--dou-warn-bg); color: var(--dou-warn-text); border-radius: var(--dou-radius-sm); }
.orphan-banner.live { border-color: var(--dou-danger); background: var(--dou-danger-bg); color: var(--dou-danger-text); }
.orphan-badge { display: inline-block; margin-bottom: 4px; padding: 1px 6px; font-size: var(--dou-fs-xs); color: var(--dou-warn-text); background: var(--dou-warn-bg); border: 1px solid var(--dou-warn); border-radius: 8px; }
</style>
