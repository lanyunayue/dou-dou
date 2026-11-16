<template>
  <div class="panel panel-files">
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
      <OfflineState v-if="sshError" variant="files" />
      <template v-else>
        <div v-for="f in visibleFiles" :key="f.name" class="fileitem" @click="onFile(f)">
          <span class="icon">{{f.is_dir?'📂':'📄'}}</span><span>{{f.name}}</span>
        </div>
        <div v-if="!visibleFiles.length" class="file-empty">目录为空或过滤条件无匹配</div>
      </template>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import OfflineState from './OfflineState.vue'
import { cwd, showHidden, fileFilter, visibleFiles, loadFiles, goUp, downloadCwd, onFile } from '../core/useFileTree'
import { activeTarget, sshError } from '../core/useSsh'

// R1：启动不再自动连内测目标——已连接时进入文件视图按需加载当前目录
onMounted(() => { if (activeTarget.value) loadFiles(cwd.value) })
</script>
