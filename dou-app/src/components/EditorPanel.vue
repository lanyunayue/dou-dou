<template>
  <div class="panel panel-editor">
    <div class="panelhead">✏️ {{editingFile || '未打开文件'}}</div>
    <!-- C4 重复指纹提示：命中历史快照时显示「跑过」与结果目录跳转 -->
    <div v-if="rerun" class="rerun-banner" role="status">
      <span>⚠ 该实验<template v-if="rerun.runs > 1">已跑过 {{ rerun.runs }} 次</template><template v-else>跑过一次</template>，结果在 <code class="mono">{{ rerun.result_dir }}</code></span>
      <button type="button" class="mini-btn" @click="openResult">查看结果目录</button>
    </div>
    <VueMonacoEditor class="editorarea" v-model:value="editorText" :language="language" theme="dou-terminal" />
    <div class="editorbar">
      <button class="btn" :disabled="!editingFile" @click="saveFile">保存</button>
      <span class="hint">保存前比对远端 mtime；走 dou.put 过隔离区+扫描 · 高亮：{{ language }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { VueMonacoEditor } from '@guolao/vue-monaco-editor'
import { editingFile, editorText, saveFile } from '../core/useEditor'
import { loadFiles } from '../core/useFileTree'
import { navigateTo } from '../core/useNav'
import { languageForFile } from '../core/editorLanguage'
import { useRequestGuard } from '../core/useRequestGuard'
import { gwFetch } from '../adapters/useGateway'

/** 编辑器语言随文件扩展名切换（S3-3 六类映射）。 */
const language = computed(() => languageForFile(editingFile.value || ''))

/** C4：文件路径命中历史快照（command / dataset_path 包含该路径）时给出重复提示。 */
interface SnapshotAgg {
  fingerprint: string
  command: string
  dataset_path: string
  result_dir: string
  runs: number
}
const rerun = ref<SnapshotAgg | null>(null)
const guard = useRequestGuard()

watch(editingFile, async (path) => {
  rerun.value = null
  if (!path || !/\.(py|sh|ipynb)$/i.test(path)) return
  const id = guard.begin()
  try {
    const r = await gwFetch('/snapshots')
    const j = await r.json()
    if (!guard.isCurrent(id)) return
    const list = (j.items ?? []) as SnapshotAgg[]
    const hit = list.find(
      (s) => s.command?.includes(path) || s.dataset_path?.includes(path),
    )
    if (guard.isCurrent(id)) rerun.value = hit ?? null
  } catch { /* 快照查询失败不打扰编辑主流程 */ }
})

/** 结果目录在远程主机上：跳转 = 文件树导航到该目录并回到工作区视图。 */
function openResult() {
  if (!rerun.value?.result_dir) return
  loadFiles(rerun.value.result_dir)
  navigateTo('term')
}
</script>

<style scoped>
.rerun-banner {
  display: flex;
  gap: 10px;
  align-items: center;
  flex-wrap: wrap;
  padding: 8px 12px;
  background: var(--dou-warn-bg);
  border-bottom: 1px solid var(--dou-warn);
  color: var(--dou-warn-text);
  font-size: var(--dou-fs-sm);
}
.rerun-banner code { word-break: break-all; }
.rerun-banner .mini-btn { float: none; }
</style>
