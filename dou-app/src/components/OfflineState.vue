<template>
  <div v-if="variant === 'files'" class="ssh-offline-files">
    <div class="ssh-title">⚠ 尚未连接远程主机 · 文件树暂不可用</div>
    <div class="hint">{{ err || '连接远程主机后即可浏览与编辑远端文件' }}</div>
    <button class="btn ok" @click="retrySsh">新建连接</button>
  </div>
  <div v-else class="ssh-offline">
    <div class="ssh-title">⚠ 连接失败</div>
    <div class="ssh-reason">{{ err }}</div>
    <div class="ssh-hint">
      请检查主机地址、端口与认证方式（密钥连不上可改密码认证）。<br/>
      也可以先打开「本地 Shell」继续手头工作；HTTP 管理功能不受影响。
    </div>
    <div class="ssh-actions">
      <button class="btn ok" @click="retrySsh">一键重试</button>
      <button class="btn" @click="openLocalTab">本地 Shell</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { sshError } from '../core/useSsh'
import { filesError } from '../core/useFileTree'
import { openLocalTab } from '../core/useTerminal'
import { retrySsh } from '../commands/useWorkspace'

// variant='files'   → 文件树内的浅色离线提示块（读文件树自己的错误态，不误伤终端）
// variant='terminal'→ 终端区的深色全屏覆盖层（连接级错误）
const props = defineProps<{ variant: 'files' | 'terminal' }>()
const err = computed(() => (props.variant === 'files' ? filesError.value : sshError.value))
</script>
