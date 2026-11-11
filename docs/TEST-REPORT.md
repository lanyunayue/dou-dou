# 功能性测试记录

测试时间：2026-11-11
口径：本机可验证项以实际编译 / HTTP 探测 / 进程窗口测量为准；受本机网络与环境限制的项如实标注，不记为通过。

## 一、窗口尺寸

| 项 | 配置 / 实测 | 结果 |
| --- | --- | --- |
| 豆包主窗口尺寸 | 1320 × 1000（pid 21220 GetWindowRect） | 基准 |
| Tauri 默认窗口 | tauri.conf.json width 1320 / height 1000，minWidth 960 / minHeight 600 | ✅ |
| 实测窗口 | pid 22904，窗口矩形 1336×1039（含 DWM 边框与标题栏），客户区对应 1320×1000 | ✅ |
| 窗口标题 | `豆 Dou`，无乱码 | ✅ |

## 二、编译与类型检查回归

| 目标 | 工具链 | 结果 |
| --- | --- | --- |
| 前端 `vue-tsc --noEmit && vite build` | Node v22.23.2 | ✅ exit 0，dist 产物生成（主包 509.75 kB / gzip 138.84 kB） |
| 网关 `dou-gateway` | stable-x86_64-pc-windows-gnu | ✅ Finished dev profile，12 个 unused 警告（不阻塞） |
| Tauri 壳 `app` | stable-x86_64-pc-windows-msvc | ✅ Finished dev profile，5 个 dead_code 警告（不阻塞） |

## 三、网关 HTTP 端点（GET 本地控制台/只读）

| 端点 | 状态码 | 说明 |
| --- | --- | --- |
| `/health` | 200 | 返回 `ok` |
| `/audit/query?limit=5` | 200 | 返回审计记录 JSON |
| `/tasks` | 200 | 任务列表 |
| `/scope/tokens` | 200 | 活跃凭据列表 |
| `/onboard/config?client=workbuddy` | 200 | JSON 片段，top_key=mcpServers |
| `/onboard/config?client=codex` | 200 | TOML 片段 |
| `/onboard/config?client=claude_code` | 200 | JSON，mcpServers |
| `/onboard/config?client=claude_desktop` | 200 | JSON，mcpServers |
| `/onboard/config?client=vscode` | 200 | JSON，top_key=servers |
| `/onboard/config?client=cursor` | 200 | JSON，mcpServers |
| `/console` | 200 | 本地控制台 HTML |

## 四、鉴权与作用域闭环（MCP `/mcp`）

| 用例 | 预期 | 实测 |
| --- | --- | --- |
| 无 Authorization 调 initialize | 401 | ✅ 401 |
| 错误 Bearer 调 initialize | 401 | ✅ 401 |
| dev 兜底令牌调 initialize | 200 | ✅ 200，返回 protocolVersion/capabilities/serverInfo |
| dev 令牌 `tools/list` | 200 | ✅ 返回 5 个工具 |
| observe 令牌调 `dou.exec` | 作用域拒绝 | ✅ isError=true，返回“不在当前凭据作用域内” |
| observe 令牌调 `dou.session_list` | 白名单放行 | ✅ isError=false，返回空会话列表 |
| 吊销 observe 凭据 | 200 ok | ✅ `{"ok":true}` |
| 吊销后再调 `dou.session_list` | 401 | ✅ 401 |

## 五、审计与导出

| 用例 | 实测 |
| --- | --- |
| `/audit/verify` 审计链校验 | ✅ `chain_ok: true`，broken_index 0 |
| `/audit/export` CSV 导出 | ✅ 200，74 行记录 |

## 六、前端界面回归

- 壳子进程响应正常，首屏渲染无白屏、无崩溃。
- 左侧导航齐全：终端 / 文件 / 接入向导 / 审批 / 凭据与作用域 / 导入 MobaXterm / 快照。
- 文件面板：路径栏、上级跳转、隐藏文件开关、文件名过滤、会话检索、导出、分组广播入口均在。
- 终端区、编辑器区（未打开文件）正常挂载。

## 七、本轮未通过 / 待环境复测（不记为完成）

1. **SSH 靶机直连**：本机 WSL2↔Windows 端口转发在当前远程控制 / VPN 环境下间歇不通，终端建连、SFTP 列目录、上传下载未能在本机实测；代码与编译已就绪，需在稳定内网靶机或干净网络复测。详见 GAP-REPORT.md 第七节第 4 条。
2. **性能六项**（冷启动 / 包体 / 常驻内存 / 目录上传 / 大日志 / 扫描耗时）：需 release 包、内网靶机与大样本后按计划书表 14 复测。
3. **配额 / 租约计数持久化**：当前存内存，网关重启重置。
4. **私钥本地落盘加密**：加固项，未做字段级加密存储。

## 八、遗留警告（不阻塞，后续可清理）

- 网关：store.rs / tools.rs 若干未使用函数（list_rules、upsert_rule、close_session、not_implemented 等）。
- Tauri 壳：pty.rs `pty_write`、sftp.rs `SftpState` 等 dead_code；transfer.rs 两处 unused_mut。
