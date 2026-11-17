<script setup lang="ts">
import { ref, onErrorCaptured } from 'vue'

/**
 * 渲染错误边界：子组件渲染/生命周期异常在此拦截，
 * 展示降级 UI 与恢复按钮，避免单个视图崩溃拖垮整个应用外壳。
 * return false 阻断错误继续向顶层传播。
 */

const err = ref<{ message: string } | null>(null)

onErrorCaptured((e: unknown) => {
  err.value = { message: e instanceof Error ? e.message : String(e) }
  return false
})

function reset() {
  err.value = null
}
</script>

<template>
  <div v-if="err" class="eb" role="alert">
    <div class="eb-title">界面渲染出错（已拦截，网关与审计链路不受影响）</div>
    <pre class="eb-detail">{{ err.message }}</pre>
    <button type="button" class="btn ok" @click="reset">恢复视图</button>
  </div>
  <slot v-else />
</template>

<style scoped>
.eb {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 24px;
}
.eb-title { font-size: var(--dou-fs-lg); font-weight: 700; color: var(--dou-danger-text); }
.eb-detail {
  margin: 0;
  max-width: 72ch;
  max-height: 200px;
  overflow: auto;
  padding: 12px;
  background: var(--dou-panel-alt);
  border: 1px solid var(--dou-border);
  border-radius: var(--dou-radius-md);
  font-size: var(--dou-fs-sm);
  font-family: var(--dou-font-mono);
  color: var(--dou-warn-text);
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
