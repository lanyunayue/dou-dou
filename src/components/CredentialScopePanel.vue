<template>
  <div class="panel panel-page">
    <div class="panelhead">🔑 设备令牌与七维作用域</div>
    <div class="row">
      <label>档位</label>
      <select v-model="issueTier">
        <option value="observe">观察档（只读）</option>
        <option value="execute">执行档（白名单命令+上传）</option>
        <option value="change">变更档（不可关闭审批）</option>
      </select>
      <label>客户端标识</label>
      <input v-model="issueClient" class="filter" placeholder="client_id" />
      <button class="btn" @click="issueToken">签发令牌</button>
    </div>
    <div v-if="issueTier==='change'" class="hint warn-text">变更档为高风险档位：审批不可关闭，每个动作都需逐条人工确认，不可“任务内记住放行”。</div>
    <div v-if="issuedToken" class="issued">
      明文仅显示一次：<span class="mono">{{ issuedToken }}</span>
      <button class="mini-btn" @click="copyIssued">复制</button>
    </div>
    <table class="scope-table">
      <thead><tr><th>ID</th><th>档位</th><th>客户端</th><th>主机</th><th>工具</th><th>路径</th><th>过期</th><th>操作</th></tr></thead>
      <tbody>
        <tr v-for="t in tokens" :key="t.id">
          <td class="mono">{{ t.id.slice(0,10) }}</td>
          <td>{{ t.tier }}</td>
          <td>{{ t.client_id }}</td>
          <td>{{ (t.scope?.hosts||[]).join(',') || '全禁' }}</td>
          <td>{{ (t.scope?.tools||[]).join(',') }}</td>
          <td>{{ (t.scope?.paths||[]).join(',') }}</td>
          <td>{{ fmtTime(t.expires_at) }}</td>
          <td><button class="btn bad" @click="revokeToken(t.id)" :disabled="!!t.revoked_at">{{ t.revoked_at?'已吊销':'吊销' }}</button></td>
        </tr>
      </tbody>
    </table>
  </div>
</template>

<script setup lang="ts">
import { tokens, issueTier, issueClient, issuedToken, issueToken, revokeToken, copyIssued, fmtTime } from '../core/useCredentials'
</script>
