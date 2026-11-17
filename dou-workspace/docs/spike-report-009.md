# 豆 Dou · Tauri 桌面壳窗口跑通（spike-report-009）

> 日期：2026-11-10
> 范围：阶段一第一个里程碑——Tauri 桌面壳窗口编译并启动。

## 一、这一轮突破

1. **MSVC 工具链解锁 Tauri 链接**：GNU 工具链 `ld.exe: export ordinal too large: 92875` 是 binutils 已知问题；切到 `stable-x86_64-pc-windows-msvc` 后链接成功。
2. **Tauri 窗口启动**：`app.exe` 12.9MB，窗口标题"豆 Dou"，前后端打通（gateway_status command）。
3. **前端骨架**：左侧栏（品牌/网关状态/导航）+ 右侧终端占位（xterm.js 待接入）。

## 二、技术栈确认

| 层 | 技术 | 状态 |
|---|---|---|
| 桌面壳 | Tauri 2.x | ✅ MSVC 编译通过 |
| 前端 | Vue 3 + TypeScript + Vite | ✅ 脚手架在 `E:\dou-app\` |
| Rust 核心 | russh 网关（独立进程） | ✅ 7419 端口 |
| 前后端桥 | tauri::command + invoke | ✅ gateway_status 通 |

## 三、当前窗口状态

- 窗口标题：豆 Dou
- 左侧栏：品牌 / 网关在线状态 / endpoint / 导航（终端/文件/审计）
- 右侧：深色终端占位（xterm.js 待接入）
- Rust command：`gateway_status()` → 调 127.0.0.1:7419/health

## 四、下一步（阶段一剩余）

1. 接 xterm.js 终端渲染（F-05）
2. 接 SFTP 树状浏览（F-02）
3. 接 monaco 编辑器（F-04）
4. 多会话标签（F-01）
5. 网关进程随 Tauri 窗口启动（不再手动启）

## 五、项目现状总览

| 模块 | 状态 |
|---|---|
| MCP 协议（5 工具） | ✅ 10/10 自检 |
| SSH 传输（russh 公钥+TOFU） | ✅ WSL dou@127.0.0.1:22 |
| SFTP 三工具 | ✅ sha256 一致 |
| 三档策略 + 审批闭环 | ✅ 控制台一键批准 |
| 双视图脱敏 | ✅ 6 类正则 |
| 隔离区 ClamAV 扫描 | ✅ clamav-wsl 真扫 |
| 审计链 + CSV 导出 | ✅ chain_ok |
| 路径范围 | ✅ 拒绝敏感路径 |
| 后台任务化 | ✅ background=true |
| Tauri 桌面壳窗口 | ✅ MSVC 编译启动 |
| xterm.js 终端 | ❌ 待接入 |
| SFTP 树 / 编辑器 | ❌ 待接入 |
| SQLite 替换 JSONL | ❌ rusqlite 已加未迁移 |
| 三档凭据 | ❌ 未做 |
