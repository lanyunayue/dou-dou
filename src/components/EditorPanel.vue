<template>
  <div class="panel panel-editor">
    <div class="panelhead">✏️ {{editingFile || '未打开文件'}}</div>
    <VueMonacoEditor class="editorarea" v-model:value="editorText" language="plaintext" theme="vs-dark" />
    <div class="editorbar">
      <button class="btn" @click="saveFile">保存</button>
      <span class="hint">保存前比对远端 mtime；走 dou.put 过隔离区+扫描</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { VueMonacoEditor } from '@guolao/vue-monaco-editor'
import { editingFile, editorText, saveFile } from '../composables/useEditor'
</script>
