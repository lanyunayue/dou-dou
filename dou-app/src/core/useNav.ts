import { ref } from 'vue'

/**
 * 应用视图状态：从 App.vue 提升到 core 层，
 * 供页面内跳转（如 C4 实验详情 → 文件树查看结果目录）。
 */
export const view = ref('term')

export function navigateTo(v: string) {
  view.value = v
}
