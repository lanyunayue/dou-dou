/**
 * 终端外观偏好纯函数（字号档位与解析），无副作用、可独立单测。
 * 持久化读写在 composable 层完成，本模块只负责把任意输入归一到合法档位。
 */

/** 默认终端字号（与历史硬编码一致）。 */
export const DEFAULT_FONT_SIZE = 13

/** 可选字号档位（px）。 */
export const FONT_SIZE_OPTIONS = [11, 12, 13, 14, 16, 18] as const

/**
 * 把任意输入归一到合法字号档位：
 * - 数字且落在档位内 → 取该值；
 * - 否则回退默认字号。
 */
export function clampFontSize(value: unknown): number {
  const n = typeof value === 'number' ? value : Number(value)
  if (!Number.isFinite(n)) return DEFAULT_FONT_SIZE
  const rounded = Math.round(n)
  return (FONT_SIZE_OPTIONS as readonly number[]).includes(rounded) ? rounded : DEFAULT_FONT_SIZE
}
