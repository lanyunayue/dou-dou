<template>
  <div class="panel panel-term" @keydown="onTermKeydown">
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
      <!-- R14 终端查找条：Ctrl+F 打开，Enter 下一个 / Shift+Enter 上一个 / Esc 关闭 -->
      <div v-if="searchVisible" class="search-bar">
        <input ref="searchInput" v-model="searchQuery" class="search-input" placeholder="查找终端内容"
          @keyup.enter="findNextMatch" @keyup.shift.enter="findPrevMatch" @keyup.esc="closeSearch" />
        <button type="button" class="mini-btn" title="上一个（Shift+Enter）" @click="findPrevMatch">↑</button>
        <button type="button" class="mini-btn" title="下一个（Enter）" @click="findNextMatch">↓</button>
        <button type="button" class="mini-btn" title="关闭（Esc）" @click="closeSearch">×</button>
      </div>
      <!-- R13 终端右键菜单：复制/粘贴/全选/清屏（与溢出菜单同一设计语言） -->
      <div v-if="termCtxMenu.visible" class="term-ctx-mask" @click="termCtxClose" @contextmenu.prevent="termCtxClose"></div>
      <div v-if="termCtxMenu.visible" class="term-ctx" :style="{ left: termCtxMenu.x + 'px', top: termCtxMenu.y + 'px' }">
        <button type="button" @click="termCtxCopy">复制</button>
        <button type="button" @click="termCtxPaste">粘贴</button>
        <button type="button" @click="termCtxSelectAll">全选</button>
        <button type="button" @click="termCtxClear">清屏</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, nextTick } from 'vue'
import OfflineState from './OfflineState.vue'
import ConnectForm from './ConnectForm.vue'
import {
  termEl, activeSid, sessionFilter, broadcast, visibleTabs, connectFormVisible as formVisible,
  toggleConnectForm, openLocalTab, switchTab, closeTab, nextTab, prevTab, closeActiveTab,
  searchVisible, searchQuery, openSearch, closeSearch, findNextMatch, findPrevMatch,
  exportTerm, doBroadcast,
  selectedCount, showSessionFilter, tabs,
  lastSessions, restorableCount, restoringSessions, restoreEntryVisible, restoreLastSessions,
  termCtxMenu, termCtxClose, termCtxCopy, termCtxPaste, termCtxSelectAll, termCtxClear,
} from '../core/useTerminal'
import { sshError } from '../core/useSsh'
import { hostStats, monitorUnsupported } from '../core/useMonitor'
import { memPercent, meterClass, fmtKbps, fmtUptime } from '../shared/monitorFmt'
import 'xterm/css/xterm.css'

// R6 溢出菜单开关（点按钮切换，点遮罩关闭）
const menuOpen = ref(false)
// R14 查找输入框：打开后自动聚焦并全选，便于直接键入
const searchInput = ref<HTMLInputElement>()

async function focusSearch() {
  await nextTick()
  searchInput.value?.focus()
  searchInput.value?.select()
}

// 终端 DOM 挂载点：xterm 需要真实节点才能 open()，这里把节点交给终端 composable
function setTermEl(el: unknown) {
  termEl.value = (el as HTMLDivElement | null) ?? undefined
}

// R15 标签快捷键：Ctrl+Tab 下一个 / Ctrl+Shift+Tab 上一个 / Ctrl+Shift+W 关闭当前。
// 关闭刻意用 Shift+W：Ctrl+W 是 shell/readline 的删词快捷键，让位给终端输入，不抢占。
function onTermKeydown(ev: KeyboardEvent) {
  if (!ev.ctrlKey) return
  if (ev.key === 'Tab' && !ev.shiftKey) { ev.preventDefault(); nextTab(); return }
  if (ev.key === 'Tab' && ev.shiftKey) { ev.preventDefault(); prevTab(); return }
  if (ev.shiftKey && ev.key.toLowerCase() === 'w') { ev.preventDefault(); closeActiveTab(); return }
  if (!ev.shiftKey && ev.key.toLowerCase() === 'f') { ev.preventDefault(); openSearch(); void focusSearch() }
}

// 监控条展示：内存占比与分档色（<60 正常 / 60-85 偏高 / ≥85 告警），格式化纯函数见 shared/monitorFmt
const memPct = computed(() =>
  hostStats.value ? memPercent(hostStats.value.mem_used_mb, hostStats.value.mem_total_mb) : 0
)
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
/* R13 终端右键菜单：复用溢出菜单的设计语言（panel/border/radius/fs token 同源） */
.term-ctx {
  position: absolute;
  z-index: 30;
  min-width: 128px;
  background: var(--dou-panel);
  border: 1px solid var(--dou-border-strong);
  border-radius: var(--dou-radius-sm);
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
  display: flex;
  flex-direction: column;
  padding: 4px;
}
.term-ctx button {
  border: none;
  background: none;
  color: var(--dou-text);
  font: inherit;
  font-size: var(--dou-fs-xs);
  text-align: left;
  padding: 7px 10px;
  cursor: pointer;
  border-radius: var(--dou-radius-sm);
  white-space: nowrap;
}
.term-ctx button:hover { background: var(--dou-panel-alt); color: var(--dou-accent-strong); }
.term-ctx-mask { position: fixed; inset: 0; z-index: 25; }
/* R14 终端查找条：浮在终端区右上角，复用菜单/输入框设计语言 */
.search-bar {
  position: absolute;
  top: 8px;
  right: 10px;
  z-index: 28;
  display: flex;
  gap: 6px;
  align-items: center;
  padding: 6px 8px;
  background: var(--dou-panel);
  border: 1px solid var(--dou-border-strong);
  border-radius: var(--dou-radius-sm);
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
}
.search-input {
  width: 180px;
  height: 26px;
  padding: 0 8px;
  background: var(--dou-panel-alt);
  border: 1px solid var(--dou-border);
  border-radius: var(--dou-radius-sm);
  color: var(--dou-text);
  font: inherit;
  font-size: var(--dou-fs-xs);
}
.search-input:focus { outline: none; border-color: var(--dou-accent); }
</style>
