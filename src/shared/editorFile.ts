/** 编辑器大文件判定与提示：纯函数，不碰 Tauri / 响应式状态，便于单测。 */

/** 大文件阈值：2MB，超过则懒加载不直接展开全文。 */
export const LARGE_FILE_LIMIT = 2 * 1024 * 1024

/** 是否超过大文件阈值。 */
export function isLargeFile(size: number): boolean {
  return size > LARGE_FILE_LIMIT
}

/** 大文件懒加载提示文案（MB，一位小数）。 */
export function largeFileNotice(size: number): string {
  return `// 文件较大（${(size / 1024 / 1024).toFixed(1)} MB），已懒加载，不直接展开全文`
}
