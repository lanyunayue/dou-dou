/**
 * 文件扩展名 → Monaco language id（S3-3 六类映射：py/yaml/json/md/log/conf）。
 * 纯函数，不触碰 monaco 实例 —— token 注册与主题在 monaco-setup（entry 层）。
 */
export function languageForFile(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? ''
  switch (ext) {
    case 'py': return 'python'
    case 'yaml': case 'yml': return 'yaml'
    case 'json': return 'dou-json'
    case 'md': case 'markdown': return 'markdown'
    case 'log': case 'out': return 'dou-log'
    case 'conf': case 'cfg': case 'config': case 'ini': case 'env': case 'service': return 'dou-conf'
    default: return 'plaintext'
  }
}
