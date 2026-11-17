# 豆 Dou · 阶段一窗口完整启动（spike-report-011）

> 日期：2026-11-10
> 范围：Tauri 窗口启动时自动拉起网关，xterm 终端组件渲染，前后端打通。

## 一、这一轮

1. **网关自启**：Tauri 窗口启动时 `ensure_gateway_started()` spawn dou-gateway.exe，等 800ms 让它绑定端口
2. **验证**：启动 app.exe 后，进程列表同时有 `app` 和 `dou-gateway`，用户双击窗口即完整可用
3. **xterm.js 终端**：深色主题，13px Cascadia Code，欢迎语 + 命令行提示符

## 二、用户现在的体验

双击 `E:\dou-app\src-tauri\target\debug\app.exe`：
- 窗口标题"豆 Dou"
- 左侧栏显示网关在线状态（自动拉起后在线）
- 右侧 xterm 终端显示欢迎语和提示符
- 网关在后台跑，MCP endpoint 在 127.0.0.1:7419

## 三、项目能力总览

| 能力 | 状态 |
|---|---|
| MCP 5 工具 | ✅ 10/10 自检 |
| SSH + SFTP | ✅ sha256 一致 |
| 三档策略 + 审批 | ✅ 控制台一键批准 |
| 脱敏 + 隔离区扫描 | ✅ clamav-wsl 真扫 |
| 审计链 + CSV | ✅ chain_ok |
| Tauri 窗口 | ✅ MSVC 编译 |
| xterm 终端组件 | ✅ 渲染 |
| 网关自启 | ✅ 窗口启动自动拉起 |
| 终端接 SSH pty | ❌ 下一步 |
| SFTP 树 / 编辑器 | ❌ |

## 四、下一步

1. xterm 接 SSH pty（Tauri Rust 端开 russh channel，数据桥接 xterm）
2. SFTP 树状浏览面板
3. monaco 编辑器
4. 多会话标签
