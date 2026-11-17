# 豆 Dou · 阶段一 xterm 终端组件接入（spike-report-010）

> 日期：2026-11-10
> 范围：Tauri 桌面壳接 xterm.js 终端组件。

## 一、这一轮

1. **xterm.js 接入**：`@xterm/xterm` 装好，Vue 组件里初始化 Terminal，深色主题（#1b1b19 底）
2. **窗口效果**：左侧栏（品牌/网关状态/导航）+ 右侧 xterm 终端，显示"豆 Dou 终端已就绪"+ 命令行提示符
3. **前后端桥**：`gateway_status` command 调 127.0.0.1:7419/health，终端里显示网关在线状态

## 二、当前窗口能力

| 区域 | 内容 |
|---|---|
| 左侧栏 | 豆 Dou 品牌 / 网关在线状态 / endpoint / 刷新按钮 / 导航（终端/文件/审计） |
| 右侧终端 | xterm.js 渲染，深色主题，13px Cascadia Code，启动欢迎语 + 命令行提示符 |

## 三、项目里程碑总览

| 里程碑 | 状态 |
|---|---|
| MCP 协议层（5 工具） | ✅ 10/10 |
| SSH 传输 + 主机密钥 TOFU | ✅ |
| SFTP 三工具（read/put/get） | ✅ sha256 一致 |
| 三档策略 + 审批闭环 | ✅ 控制台一键批准 |
| 双视图脱敏（6 类正则） | ✅ 实测全中 |
| 隔离区 ClamAV 真扫描 | ✅ clamav-wsl |
| 审计链 + CSV 导出 | ✅ chain_ok |
| 路径范围 + 后台任务化 | ✅ |
| Tauri 桌面壳窗口 | ✅ MSVC 编译启动 |
| xterm.js 终端组件 | ✅ 渲染欢迎语 |
| 终端接 SSH pty | ❌ 下一步 |
| SFTP 树 / monaco 编辑器 | ❌ |
| 多会话标签 | ❌ |
| SQLite / 三档凭据 | ❌ |

## 四、下一步

1. xterm 接 SSH pty：Tauri Rust 端开 russh channel，把数据桥接到 xterm
2. SFTP 树状浏览（左侧文件面板）
3. monaco 编辑器（双击文件打开）
4. 多会话标签（顶部 tab）
