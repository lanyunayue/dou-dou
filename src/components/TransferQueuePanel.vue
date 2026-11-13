<template>
  <div class="panel panel-page">
    <div class="panelhead">📤 传输队列（断点续传 / 暂停 / 同步）</div>
    <div class="row">
      <select v-model="tqType"><option value="download">下载</option><option value="upload">上传</option></select>
      <input v-model="tqRemote" class="filter" placeholder="远端路径 /home/dou/a.txt" />
      <input v-model="tqLocal" class="filter" placeholder="本地路径 C:/Users/mrhua/Downloads/a.txt" />
      <button class="btn ok" @click="enqueueTq">加入队列</button>
    </div>
    <div class="row">
      <span class="hint">同步：</span>
      <input v-model="syncLocal" class="filter" placeholder="本地目录" />
      <input v-model="syncRemote" class="filter" placeholder="远端目录" />
      <select v-model="syncMode"><option value="push">push 镜像</option><option value="pull">pull 镜像</option><option value="update">update 仅更优</option></select>
      <button class="btn ok" @click="doSync">同步</button>
      <button class="mini-btn" @click="tqClear">清理已完成</button>
    </div>
    <div v-if="!tqTasks.length" class="empty">队列为空</div>
    <div v-for="t in tqTasks" :key="t.id" class="tq-card">
      <div class="mono">{{ t.kind }} · {{ t.label }}</div>
      <div class="tq-bar"><div class="tq-fill" :style="{width: (t.total? Math.min(100, t.done/t.total*100):0)+'%'}"></div></div>
      <div class="hint">{{ t.status }} · {{ fmtBytes(t.done) }} / {{ fmtBytes(t.total) }}</div>
      <div class="tq-actions">
        <button class="mini-btn" v-if="t.status==='Queued'||t.status==='Active'" @click="tqPause(t.id)">暂停</button>
        <button class="mini-btn" v-if="t.status==='Paused'" @click="tqResume(t.id)">继续</button>
        <button class="mini-btn" v-if="t.status!=='Done'&&t.status!=='Failed'&&t.status!=='Canceled'" @click="tqCancel(t.id)">取消</button>
        <span v-if="t.error" class="tq-err">{{ t.error }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  tqTasks, tqRemote, tqLocal, tqType, syncLocal, syncRemote, syncMode,
  fmtBytes, enqueueTq, doSync, tqClear, tqPause, tqResume, tqCancel,
} from '../composables/useTransferQueue'
</script>
