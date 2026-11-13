<template>
  <!-- 优雅离线态：不黑屏 / 不纯白 / 不裸 os error / 不伪造连接成功 -->
  <div v-if="variant === 'files'" class="ssh-offline-files">
    <div class="ssh-title">⚠ 未连接靶机</div>
    <div class="hint">{{ sshError }}</div>
    <div class="hint">文件树 / 终端 / 传输暂不可用；接入向导 / 审批 / 凭据 / 审计等 HTTP 管理功能不受影响。</div>
    <div class="hint">本机 127.0.0.1:22 不可达（wsl.exe 被安全策略禁用 / Windows OpenSSH 未安装 / 无外部靶机）。靶机就绪后点「一键重试」恢复，不会伪造连接成功。</div>
    <button class="btn ok" @click="retrySsh">一键重试</button>
  </div>
  <div v-else class="ssh-offline">
    <div class="ssh-title">⚠ 未连接靶机</div>
    <div class="ssh-reason">{{ sshError }}</div>
    <div class="ssh-hint">
      本机 127.0.0.1:22 不可达（wsl.exe 被安全策略禁用 / Windows OpenSSH 未安装 / 无外部靶机）。<br/>
      终端、文件、传输功能暂不可用；HTTP 管理功能（接入向导 / 审批 / 凭据 / 审计）照常可用。<br/>
      靶机就绪（22 端口可达）后点「一键重试」恢复，不会伪造连接成功。
    </div>
    <button class="btn ok" @click="retrySsh">一键重试</button>
  </div>
</template>

<script setup lang="ts">
import { sshError } from '../composables/useSsh'
import { retrySsh } from '../composables/useWorkspace'

// variant='files'   → 文件树内的浅色离线提示块
// variant='terminal'→ 终端区的深色全屏覆盖层
defineProps<{ variant: 'files' | 'terminal' }>()
</script>
