/**
 * 设计 token：JS 侧单一代码事实源。
 *
 * 与 src/styles/app.css 中的 CSS 变量（--dou-*）一一对应。
 * CSS 负责运行时消费；本文件供 JS 侧需要色值/字号的场景使用
 * （如 Monaco 主题定义、canvas 绘制、语法高亮映射）。
 * 修改色值时两侧必须同步，否则编辑器与界面会出现配色漂移。
 */

export const douTokens = {
  color: {
    bg: '#0a0f0d',
    panel: '#101614',
    panelAlt: '#151d1a',
    border: '#223029',
    borderStrong: '#35473d',
    text: '#d6e2d9',
    dim: '#9fb4a8',
    accent: '#5ce8a4',
    accentStrong: '#7df0bb',
    ok: '#4ade80',
    okText: '#6ee7b7',
    warn: '#f0b429',
    warnText: '#f5c66b',
    danger: '#f87171',
    dangerText: '#fca5a5',
    info: '#7cc4f0',
    /** verdict 语义色：allow=绿 / deny=红 / ask=琥珀 */
    verdictAllow: '#4ade80',
    verdictDeny: '#f87171',
    verdictAsk: '#f0b429',
    /** 深色填充底（用于徽章、横幅的底色） */
    okBg: '#0e2a1c',
    warnBg: '#2b2410',
    dangerBg: '#2b1414',
  },
  font: {
    mono: '"Cascadia Code", "Cascadia Mono", "JetBrains Mono", Consolas, monospace',
    ui: '"Cascadia Code", "Cascadia Mono", Consolas, "Segoe UI", "Microsoft YaHei", sans-serif',
  },
  size: {
    /** 最小字号（badge、时间戳） */
    xs: '12px',
    sm: '13px',
    /** 正文与交互文本下限 */
    body: '14px',
    lg: '16px',
    xl: '20px',
  },
  radius: {
    sm: '4px',
    md: '8px',
  },
  /** 主交互元素触控目标下限 */
  touch: '44px',
  duration: {
    fast: '120ms',
    normal: '200ms',
  },
} as const

export type DouTokens = typeof douTokens
