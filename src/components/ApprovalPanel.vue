<template>
  <div class="panel panel-page">
    <div class="panelhead">✅ 待审批任务（{{ pendingCount }}）</div>
    <div v-if="!pendingTasks.length" class="empty">没有待审任务</div>
    <div v-for="t in pendingTasks" :key="t.id" class="approve-card">
      <div class="mono cmd">{{ t.command }}</div>
      <div class="hint">原因：{{ t.reason }}</div>
      <div class="hint meta">
        <span v-if="t.impact">预计影响：{{ t.impact }}</span>
        <span v-if="t.target_path">目标路径：{{ t.target_path }}</span>
        <span v-if="t.history_count > 0">历史同类 {{ t.history_count }} 次</span>
      </div>
      <div class="approve-actions">
        <button class="btn ok" @click="decideTask(t.id, true, false)">一次允许</button>
        <button class="btn ok" :disabled="t.policy==='change'" :title="t.policy==='change' ? '变更档不可关闭审批' : ''" @click="decideTask(t.id, true, true)">本次任务内允许</button>
        <input v-model="rejectNotes[t.id]" class="filter" placeholder="拒绝理由（必填，将留痕）" />
        <button class="btn bad" @click="decideTask(t.id, false, false)">拒绝</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { pendingTasks, rejectNotes, pendingCount, decideTask } from '../composables/useApproval'
</script>

<style scoped>
.approve-card .meta { display: flex; gap: 12px; flex-wrap: wrap; margin-top: 2px; }
.approve-card .meta span { color: var(--color-text-secondary, #6b6b64); }
</style>
