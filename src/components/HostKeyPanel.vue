<template>
  <div class="panel panel-page">
    <div class="panelhead">🛡 主机密钥（known_hosts · 防中间人）</div>
    <div class="hint">首次连接某主机密钥会被记录（TOFU）；之后若密钥变更将拒绝连接（MITM 防护）。终端与网关各有一套信任库，遗忘仅在人工核实对方确属合法换钥后进行。</div>

    <div class="panelhead" style="margin-top:12px">终端信任库</div>
    <div v-if="!hostKeys.length" class="empty">暂无已记录主机密钥</div>
    <div v-for="h in hostKeys" :key="h.host+h.port" class="approve-card">
      <div class="mono">{{ h.host }}:{{ h.port }}</div>
      <div class="hint">SHA256: {{ h.fingerprint }}</div>
      <div class="approve-actions"><button class="btn bad" @click="forgetHost(h.host,h.port)">遗忘（密钥轮换后重认）</button></div>
    </div>

    <div class="panelhead" style="margin-top:12px">网关信任库（dou-gateway）</div>
    <div class="hint">MCP 命令通道使用。指纹变更时网关会硬性拒绝，此处提供人工核实后的遗忘出口。</div>
    <div v-if="!gatewayHostKeys.length" class="empty">网关信任库为空（或网关未就绪）</div>
    <div v-for="g in gatewayHostKeys" :key="g.addr" class="approve-card">
      <div class="mono">{{ g.addr }}</div>
      <div class="hint">主机公钥 {{ g.key }}</div>
      <div class="approve-actions"><button class="btn bad" @click="forgetGatewayHost(g.addr)">遗忘（密钥轮换后重认）</button></div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { hostKeys, gatewayHostKeys, forgetHost, forgetGatewayHost } from '../composables/useHostKeys'
</script>
