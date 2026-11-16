<template>
  <div class="panel panel-term">
    <!-- R6 工具条降噪：+ 主按钮 / 本地 Shell 次按钮 / 检索 ≥5 会话才出现 / 广播勾选 ≥1 才出现 / 导出进溢出菜单 -->
    <div class="tabbar">
      <div v-for="t in visibleTabs" :key="t.sid" class="tab" :class="{active: t.sid===activeSid}" @click="switchTab(t.sid)">
        <input type="checkbox" class="tabcheck" v-model="t.selected" @click.stop />
        {{t.title}}<span v-if="t.badge" class="badge">{{t.badge}}</span>
        <span class="close" @click.stop="closeTab(t.sid)">×</span>
      </div>
      <button class="tabadd primary" @click="toggleConnectForm" title="新建连接：主机 + 认证方式一气呵成">＋</button>
      <button class="mini-btn" @click="openLocalTab" title="本地 PowerShell，无需远程主机">本地 Shell</button>
      <input v-if="showSessionFilter" v-model="sessionFilter" class="filter tabfilter" placeholder="检索会话" />
      <input v-if="selectedCount" v-model="broadcast" class="broadcast" placeholder="同时输入到所选会话（回车发送）" @keyup.enter="doBroadcast" />
      <div v-if="tabs.length" class="overflow">
        <button class="mini-btn" title="更多操作" :aria-expanded="menuOpen" @click.stop="menuOpen=!menuOpen">⋯</button>
        <div v-if="menuOpen" class="overflow-menu">
          <button type="button" @click="exportTerm(); menuOpen=false">导出当前会话记录</button>
        </div>
        <div v-if="menuOpen" class="overflow-mask" @click="menuOpen=false"></div>
      </div>
    </div>
    <!-- R7：监控条（Linux 主机）或边界说明（非 Linux 主机），不再静默隐藏 -->
    <div v-if="hostStats" class="hostbar" role="status" aria-label="主机监控">
      <span class="hb-item">
        CPU {{ Math.round(hostStats.cpu_pct) }}%
        <span class="hb-meter"><span class="hb-fill" :class="meterClass(hostStats.cpu_pct)" :style="{ width: hostStats.cpu_pct + '%' }"></span></span>
      </span>
      <span class="hb-item">
        内存 {{ hostStats.mem_used_mb }}/{{ hostStats.mem_total_mb }} MB
        <span class="hb-meter"><span class="hb-fill" :class="meterClass(memPct)" :style="{ width: memPct + '%' }"></span></span>
      </span>
      <span class="hb-item">↓ {{ fmtKbps(hostStats.rx_kbps) }} · ↑ {{ fmtKbps(hostStats.tx_kbps) }}</span>
      <span class="hb-item hb-dim">已运行 {{ fmtUptime(hostStats.uptime_s) }}</span>
    </div>
    <!-- R7：非 Linux 主机的边界说明行（探测结果驱动，单次提示） -->
    <div v-else-if="monitorUnsupported" class="hostbar hostbar-note" role="status" aria-label="监控边界说明">
      ⓘ {{ monitorUnsupported }}
    </div>
    <!-- R11 会话恢复入口：上次退出有会话且当前无会话时出现（不抢占连接表单主视觉） -->
    <div v-if="restoreEntryVisible" class="restore-bar" role="status" aria-label="恢复上次会话">
      <span class="hint">
        上次退出时有 {{ lastSessions.length }} 个会话，其中 {{ restorableCount }} 个可自动恢复（密码认证会话需重新输入密码）
      </span>
      <button type="button" class="btn ok restore-btn" :disabled="restoringSessions" @click="restoreLastSessions">
        {{ restoringSessions ? '恢复中…' : '↺ 恢复上次会话' }}
      </button>
    </div>
    <div class="term-wrap">
      <div :ref="setTermEl" class="terminal" :style="{ visibility: formVisible ? 'hidden' : 'visible' }"></div>
      <!-- 优雅离线态：不黑屏 / 不纯白 / 不裸 os error / 不伪造连接成功 -->
      <OfflineState v-if="sshError && !formVisible" variant="terminal" />
      <!-- R1 新建连接表单：无会话时常显，连接成功自动收起 -->
      <ConnectForm v-if="formVisible" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import OfflineState from './OfflineState.vue'
import ConnectForm from './ConnectForm.vue'
import {
  termEl, activeSid, sessionFilter, broadcast, visibleTabs, connectFormVisible as formVisible,
  toggleConnectForm, openLocalTab, switchTab, closeTab, exportTerm, doBroadcast,
  selectedCount, showSessionFilter, tabs,
  lastSessions, restorableCount, restoringSessions, restoreEntryVisible, restoreLastSessions,
} from '../core/useTerminal'
import { sshError } from '../core/useSsh'
import { hostStats, monitorUnsupported } from '../core/useMonitor'
import 'xterm/css/xterm.css'

// R6 溢出菜单开关（点按钮切换，点遮罩关闭）
const menuOpen = ref(false)

// 终端 DOM 挂载点：xterm 需要真实节点才能 open()，这里把节点交给终端 composable
function setTermEl(el: unknown) {
  termEl.value = (el as HTMLDivElement | null) ?? undefined
}

// 监控条展示：内存占比与分档色（<60 正常 / 60-85 偏高 / ≥85 告警）
const memPct = computed(() =>
  hostStats.value ? Math.round(hostStats.value.mem_used_mb / Math.max(hostStats.value.mem_total_mb, 1) * 100) : 0
)
function meterClass(v: number) { return v >= 85 ? 'hot' : v >= 60 ? 'warm' : '' }
function fmtKbps(k: number) { return k >= 1024 ? (k / 1024).toFixed(1) + 'MB/s' : k.toFixed(1) + 'KB/s' }
function fmtUptime(s: number) {
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60)
  return h ? `${h}h${m}m` : `${m}m`
}
</script>

<style scoped>
.hostbar {
  display: flex;
  gap: 16px;
  align-items: center;
  flex-wrap: wrap;
  padding: 4px 10px;
  background: var(--dou-panel-alt);
  border-bottom: 1px solid var(--dou-border);
  font-size: 12px;
  color: var(--dou-text);
}
.hb-item { display: inline-flex; align-items: center; gap: 6px; }
.hb-dim { color: var(--dou-dim); margin-left: auto; }
.hb-meter {
  width: 72px;
  height: 6px;
  border-radius: 3px;
  background: var(--dou-border);
  overflow: hidden;
  display: inline-block;
}
.hb-fill {
  display: block;
  height: 100%;
  border-radius: 3px;
  background: var(--dou-accent);
  transition: width 200ms ease;
}
.hb-fill.warm { background: var(--dou-warn); }
.hb-fill.hot { background: var(--dou-danger); }
/* R7：非 Linux 主机的边界说明行（信息性，非告警） */
.hostbar-note { color: var(--dou-dim); font-size: 12px; }
/* R11：会话恢复入口（信息性一行，弱化于连接表单主视觉） */
.restore-bar {
  display: flex;
  gap: 12px;
  align-items: center;
  flex-wrap: wrap;
  padding: 6px 10px;
  background: var(--dou-panel-alt);
  border-bottom: 1px solid var(--dou-border);
}
.restore-btn { height: 24px; }
</style>
