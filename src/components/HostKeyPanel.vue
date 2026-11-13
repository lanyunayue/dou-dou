<template>
  <div class="panel panel-page">
    <div class="panelhead">🛡 主机密钥（known_hosts · 防中间人）</div>
    <div class="hint">首次连接某主机密钥会被记录（TOFU）；之后若密钥变更将拒绝连接（MITM 防护）。</div>
    <div v-if="!hostKeys.length" class="empty">暂无已记录主机密钥</div>
    <div v-for="h in hostKeys" :key="h.host+h.port" class="approve-card">
      <div class="mono">{{ h.host }}:{{ h.port }}</div>
      <div class="hint">SHA256: {{ h.fingerprint }}</div>
      <div class="approve-actions"><button class="btn bad" @click="forgetHost(h.host,h.port)">遗忘（密钥轮换后重认）</button></div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { hostKeys, forgetHost } from '../composables/useHostKeys'
</script>
