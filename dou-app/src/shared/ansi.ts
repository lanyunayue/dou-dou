/**
 * 终端文本清洗纯函数（剥离导出文本里的控制序列，无副作用）。
 */

/** 剥离 ANSI CSI、OSC、分页符、回车覆盖与字符集切换，保留可见文本与换行。 */
export function cleanExport(raw: string): string {
  return raw
    .replace(/\x1b\[[0-9;?]*[A-Za-z]/g, '') // ANSI CSI
    .replace(/\x1b\][^\x07]*\x07/g, '') // OSC
    .replace(/\x0c/g, '') // 分页符
    .replace(/[^\n]*\r/g, '') // 回车覆盖字节
    .replace(/\x1b[()][AB012]/g, '') // 字符集切换
}
