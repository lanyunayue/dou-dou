<template>
  <div class="toast-host" aria-live="polite">
    <div v-for="t in toasts" :key="t.id" class="toast" :class="`toast-${t.kind}`" role="status">
      <span class="toast-icon">{{ icon(t.kind) }}</span>
      <span class="toast-msg">{{ t.message }}</span>
      <button type="button" class="toast-close" :aria-label="'关闭提示'" @click="dismissToast(t.id)">×</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { toasts, dismissToast, type ToastKind } from '../shared/feedback'

function icon(kind: ToastKind): string {
  if (kind === 'success') return '✓'
  if (kind === 'warn') return '⚠'
  if (kind === 'error') return '✕'
  return 'ℹ'
}
</script>

<style scoped>
.toast-host {
  position: fixed;
  top: 12px;
  right: 12px;
  z-index: 60;
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-width: 360px;
  pointer-events: none;
}
.toast {
  pointer-events: auto;
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 9px 12px;
  background: var(--dou-panel);
  border: 1px solid var(--dou-border-strong);
  border-left-width: 3px;
  border-radius: var(--dou-radius-md);
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
  font-size: var(--dou-fs-sm);
  color: var(--dou-text);
}
.toast-icon { flex: none; line-height: 1.4; }
.toast-msg { flex: 1; white-space: pre-wrap; word-break: break-word; line-height: 1.45; }
.toast-close {
  flex: none;
  border: none;
  background: none;
  color: var(--dou-dim);
  font-size: var(--dou-fs-lg);
  line-height: 1;
  cursor: pointer;
  padding: 0 2px;
}
.toast-close:hover { color: var(--dou-text); }
.toast-info { border-left-color: var(--dou-info); }
.toast-info .toast-icon { color: var(--dou-info); }
.toast-success { border-left-color: var(--dou-ok); }
.toast-success .toast-icon { color: var(--dou-ok); }
.toast-warn { border-left-color: var(--dou-warn); }
.toast-warn .toast-icon { color: var(--dou-warn); }
.toast-error { border-left-color: var(--dou-danger); }
.toast-error .toast-icon { color: var(--dou-danger); }
</style>
