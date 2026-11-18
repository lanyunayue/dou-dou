<template>
  <div class="panel panel-files" :class="{ 'drag-over': dragActive }" @dragover.prevent @drop.prevent>
    <div class="panelhead">
      📁 {{cwd}}
      <button class="mini-btn" @click="goUp">↑ 上级</button>
      <button class="mini-btn" @click="downloadCwd">下载</button>
    </div>
    <div class="fileopts">
      <label><input type="checkbox" v-model="showHidden" /> 隐藏文件</label>
      <input v-model="fileFilter" class="filter" placeholder="过滤文件名" />
    </div>
    <div class="filelist">
      <OfflineState v-if="filesError" variant="files" />
      <template v-else>
        <div v-for="f in visibleFiles" :key="f.name" class="fileitem" @click="onFile(f)">
          <span class="icon">{{f.is_dir?'📂':'📄'}}</span><span>{{f.name}}</span>
        </div>
        <div v-if="!visibleFiles.length" class="file-empty">目录为空或过滤条件无匹配</div>
      </template>
    </div>
    <div v-if="dragActive" class="drop-hint">松开鼠标，上传到 {{cwd}}</div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import OfflineState from './OfflineState.vue'
import { cwd, showHidden, fileFilter, visibleFiles, filesError, dragActive, loadFiles, goUp, downloadCwd, onFile, registerDropListener } from '../core/useFileTree'
import { activeTarget } from '../core/useSsh'

let dropUnlisten: UnlistenFn | null = null

// R1：启动不再自动连内测目标——已连接时进入文件视图按需加载当前目录
onMounted(async () => {
  if (activeTarget.value) loadFiles(cwd.value)
  dropUnlisten = await registerDropListener()
})

onUnmounted(() => { dropUnlisten?.(); dropUnlisten = null })
</script>

<style scoped>
.panel-files.drag-over {
  outline: 2px dashed var(--dou-accent, #4ea17a);
  outline-offset: -4px;
}
.drop-hint {
  position: absolute;
  inset: auto 8px 8px 8px;
  padding: 10px;
  text-align: center;
  border-radius: 6px;
  background: var(--dou-panel, #16211d);
  color: var(--dou-accent, #4ea17a);
  font-size: 13px;
  pointer-events: none;
}
</style>
