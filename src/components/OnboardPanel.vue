<template>
  <div class="panel panel-page">
    <div class="panelhead">🔌 客户端接入向导（五步）</div>
    <div class="wizard">
      <ol class="steps">
        <li>开通道：网关只绑本机回环 127.0.0.1:7419</li>
        <li>选客户端：探测安装与配置文件</li>
        <li>签发凭据：默认观察档、24 小时</li>
        <li>写入：一键代写 / 复制片段 / 注册命令（逐级降级）</li>
        <li>握手自检：以收到首个工具清单请求为准</li>
      </ol>
      <div class="row">
        <label>客户端</label>
        <select v-model="onboardClient" @change="fetchOnboard">
          <option value="workbuddy">WorkBuddy</option>
          <option value="codex">Codex CLI</option>
          <option value="claude_code">Claude Code</option>
          <option value="claude_desktop">Claude Desktop</option>
          <option value="vscode">VS Code (Copilot)</option>
          <option value="cursor">Cursor</option>
        </select>
      </div>
      <div class="row">
        <label>配置路径</label>
        <span class="mono">{{ onboardPaths.join(' · ') }}</span>
      </div>
      <pre class="snippet">{{ onboardSnippet }}</pre>
      <div class="row">
        <button class="btn" @click="copySnippet">复制片段</button>
        <button class="btn ok" @click="writeConfig">一键代写配置</button>
        <span class="hint">{{ onboardNote }}</span>
      </div>
      <div v-if="writeResult" class="issued">{{ writeResult }}</div>
      <div class="row">
        <button class="btn" @click="probeHandshake">检测客户端连接</button>
        <span class="hint">{{ onboardProbe }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  onboardClient, onboardSnippet, onboardPaths, onboardNote, writeResult, onboardProbe,
  fetchOnboard, copySnippet, writeConfig, probeHandshake,
} from '../composables/useOnboard'
</script>
