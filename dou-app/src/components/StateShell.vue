<script setup lang="ts">
/**
 * 四态容器：loading / empty / error / ready。
 *
 * S3 页面化后所有异步视图统一用它渲染四态，禁止「请求中无反馈 /
 * 失败静默 / 空数据白板」。error 态提供重试出口，ready 态渲染默认插槽。
 */

export type ShellState = 'loading' | 'empty' | 'error' | 'ready'

withDefaults(
  defineProps<{
    state: ShellState
    errorText?: string
    emptyText?: string
    loadingText?: string
  }>(),
  {
    errorText: '加载失败',
    emptyText: '暂无数据',
    loadingText: '加载中…',
  },
)

defineEmits<{ retry: [] }>()
</script>

<template>
  <div class="shell" :data-state="state">
    <div v-if="state === 'loading'" class="shell-loading" role="status" aria-live="polite">
      <span class="shell-spinner" aria-hidden="true"></span>
      <span>{{ loadingText }}</span>
    </div>

    <div v-else-if="state === 'empty'" class="shell-empty">
      <div>{{ emptyText }}</div>
      <slot name="empty-actions" />
    </div>

    <div v-else-if="state === 'error'" class="shell-error" role="alert">
      <div class="shell-error-text">{{ errorText }}</div>
      <button type="button" class="btn ok" @click="$emit('retry')">一键重试</button>
    </div>

    <div v-else class="shell-ready">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.shell { height: 100%; display: flex; flex-direction: column; min-height: 0; }
.shell-loading,
.shell-empty,
.shell-error {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 24px;
  font-size: var(--dou-fs-body);
}
.shell-loading { color: var(--dou-dim); }
.shell-empty { color: var(--dou-dim); }
.shell-error { color: var(--dou-danger-text); }
.shell-error-text { line-height: 1.7; word-break: break-all; max-width: 60ch; }
.shell-ready { flex: 1; min-height: 0; display: flex; flex-direction: column; }

/* 旋转指示器：受 prefers-reduced-motion 全局降级约束 */
.shell-spinner {
  width: 20px;
  height: 20px;
  border: 2px solid var(--dou-border-strong);
  border-top-color: var(--dou-accent);
  border-radius: 50%;
  animation: shell-spin 0.9s linear infinite;
}
@keyframes shell-spin { to { transform: rotate(360deg); } }
</style>
