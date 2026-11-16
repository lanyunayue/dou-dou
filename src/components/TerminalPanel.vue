<template>
  <div class="panel panel-term">
    <div class="tabbar">
      <input v-model="sessionFilter" class="filter tabfilter" placeholder="检索会话" />
      <div v-for="t in visibleTabs" :key="t.sid" class="tab" :class="{active: t.sid===activeSid}" @click="switchTab(t.sid)">
        <input type="checkbox" class="tabcheck" v-model="t.selected" @click.stop />
        {{t.title}}<span v-if="t.badge" class="badge">{{t.badge}}</span>
        <span class="close" @click.stop="closeTab(t.sid)">×</span>
      </div>
      <button class="tabadd" @click="newTab">+</button>
      <button class="mini-btn" @click="exportTerm">导出</button>
      <input v-model="broadcast" class="broadcast" placeholder="广播到勾选→" @keyup.enter="doBroadcast" />
    </div>
    <div class="term-wrap">
      <div :ref="setTermEl" class="terminal"></div>
      <!-- 优雅离线态：不黑屏 / 不纯白 / 不裸 os error / 不伪造连接成功 -->
      <OfflineState v-if="sshError" variant="terminal" />
    </div>
  </div>
</template>

<script setup lang="ts">
import OfflineState from './OfflineState.vue'
import {
  termEl, activeSid, sessionFilter, broadcast, visibleTabs,
  newTab, switchTab, closeTab, exportTerm, doBroadcast,
} from '../core/useTerminal'
import { sshError } from '../core/useSsh'
import 'xterm/css/xterm.css'

// 终端 DOM 挂载点：xterm 需要真实节点才能 open()，这里把节点交给终端 composable
function setTermEl(el: unknown) {
  termEl.value = (el as HTMLDivElement | null) ?? undefined
}
</script>
