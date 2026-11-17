# 10-CORS 修复与最终验收报告（第 3 轮收尾）

> 文档类型：增量收尾 + 最终验收（接续 `07`/`08`/`09`）
> 日期：2026-11-12
> 主理人：齐活林（交付总监）· 团队：许清楚(PM)/高见远(架构)/寇豆码(工程)/严过关(QA)
> 关联提交：`65d695b`（令牌桥接+源码收编+离线重编）、`10d7d0d`（本轮 CORS 修复）

---

## 1. 本轮要解决的剩余问题

上一轮（commit `65d695b`）完成了令牌桥接、源码收编、离线重编，并跑通了后端 e2e（22/22）。但**真实窗口验收**时发现两个客户端层面的阻断，本轮回填：

| 编号 | 现象 | 严重度 | 是否阻断「真能用」 |
|------|------|--------|--------------------|
| BUG-R1 | 接入向导显示「网关离线：无法连接 127.0.0.1:7419」，但同一时刻带令牌的 HTTP 实测全部 200 | 高 | 是——HTTP 管理功能（接入向导/审批/审计）在 UI 上不可用 |
| BUG-R2 | 状态灯在靶机不可用时写「靶机已连接」，与 22 端口 CLOSED 矛盾，属谎报在线 | 中 | 是——状态口径与真实不可用相悖，误导用户 |

---

## 2. 根因分析（已定位，不臆测）

### 2.1 BUG-R1 根因：网关无 CORS 头 + WebView 跨域 + 系统代理劫持
- 网关 `crates/dou-gateway` 的响应**不含 `Access-Control-Allow-Origin`**（实测 `None`）。
- 前端 webview 运行在 `http://localhost:1420`（Vite devUrl），请求目标 `http://127.0.0.1:7419`——**跨域**（host 不同：`localhost` ≠ `127.0.0.1`）。
- 浏览器/CORS 策略拦截了跨域读取，`fetch` 抛错被翻译成「网关离线」。
- 此前 QA 还发现系统 `HTTP_PROXY` 会把回环请求误判成 502，加剧不可达假象。
- **令牌桥接（Bearer 注入）解决了 401，但没解决 CORS/代理**——这是 R1 在上一轮 22/22 后端绿、UI 却报错的根因。

### 2.2 BUG-R2 根因：状态推断逻辑错误
- 旧 `connSummary` 逻辑：`sshError` 为空即写「靶机已连接」。
- 实际上「没有报错」≠「已连接」：靶机从未连上、SSH 通道根本没建立时 `sshError` 也是空，于是凭空报「已连接」。

---

## 3. 修复方案（已落地，commit `10d7d0d`）

### 3.1 统一请求通道改为 Tauri IPC（绕开 CORS/代理）
- **新增 Rust command `gateway_request(path, method, body)`**（`src-tauri/src/lib.rs`）：
  - 令牌由 Rust 侧 `gateway_token()` 读取并注入 `Bearer`，**HTTP 由 `reqwest` 发出，且 `.no_proxy()`**。
  - 走 `http://127.0.0.1:7419`，与 webview 同源无关，**不受 CORS 与系统代理影响**，令牌也不进入前端 `fetch` 作用域。
- **前端 `gwFetch()` 改为唯一出口 `invoke('gateway_request', …)`**（`src/App.vue`）：
  - `fail-closed` 仍然生效——`gwToken()` 前置校验失败即中止，绝不发无令牌请求（避免再次 401→429）。
  - 仍保留 `ok/status/json()/text()` 的形态，调用点零改动（8 处全部经 `gwFetch`）。

### 3.2 状态灯真实探测靶机端口
- Rust 侧新增 `ssh_target_up()`：对 `127.0.0.1:22` 做 300ms 超时 TCP 探测。
- `gateway_status` 返回 `ssh_up` 字段；前端据此写「靶机在线 / 靶机未连接（22 端口不可达）」，**不再用「没报错=已连接」推断**。

### 3.3 靶机不可用时优雅降级（不黑屏/不谎报）
- 终端区、文件树在 SSH 不可用时显示明确提示：「⚠ 未连接靶机 · 连接被拒绝：目标 127.0.0.1:22 未监听（靶机未启动）」+ 说明 `wsl.exe` 被安全策略禁用 / Windows OpenSSH 未安装 / 无外部靶机 + **一键重试**按钮。
- HTTP 管理功能（接入向导/审批/搜索/审计）照常可用，提示文案明确区分。

> 涉及文件：`src-tauri/src/lib.rs`（+47/-2）、`src/App.vue`（+50/-22）。**网关侧 `crates/dou-gateway/**` 零改动**（已 `git diff` 核验）。全仓 `rg "dou_ak_[0-9a-f]{24,}"` 命中 0（无明文主令牌）。

---

## 4. 最终验收证据

### 4.1 机器可验回归（服务真实运行态下执行）
| 套件 | 结果 | 备注 |
|------|------|------|
| `tools/test_dou_e2e.py` | **22/22 PASS** | 9 端点无 401/403/429；`/tasks` 读到 4 条待审；无主令牌泄露；`${DOU_TOKEN}` 占位符保留供前端本地替换 |
| `tools/test_auth_matrix.py` | **32/32 PASS** | 设备令牌/M-only 端点鉴权矩阵全绿 |
| `tools/test_throttle.py` | **7/7 PASS** | 防爆破：429 带 Retry-After、合法主令牌不受影响、限速状态持久化 |
| `cargo test --offline` | **24/24 PASS** | Rust 单测（含脱敏 redacts_private_key / internal_ip） |
| `tools/mcp_probe.py` | **9/10** | 见 §4.3 受限说明 |

### 4.2 真实窗口截图证据（`E:/dou-test/`）
| 截图 | 证明内容 |
|------|----------|
| `v2-main.png` | 窗口渲染豆 Dou 客户端（非 Edge 错误页/白屏）；左侧状态灯「网关 在线」；审批角标=4（当时网关 pending=4） |
| `v2-status.png` | 状态文字逐字：「网关在线（HTTP 管理可用）· 靶机未连接（22 端口不可达，终端/文件/传输不可用）」——**无谎报** |
| `v2-onboard.png` / `v2-approve.png` | 接入向导显示配置片段（含 Bearer）、复制/改写/检测按钮可用——**R1 已修复，不再「网关离线」** |
| `v2-approve-correct.png` | 审批页「待审批任务 (4)」，4 条任务含命令/白名单拦截原因/一次允许·拒绝（带必填理由留痕） |
| `release-main.png` | **自包含 release exe 双击启动、未开 Vite** 即渲染客户端；状态灯「网关 在线」；审批角标与页内同为 8（网关 pending 已累计为 8，角标与标题共用同一 `pendingCount` 反应式值，始终同步） |
| `release-approve.png` | release 下「待审批任务 (8)」正常列出，功能可用、无错误 |

### 4.3 仍受限 / 未判通过项（如实标注，非缺陷）
- **SSH 靶机（步骤 1 原目标）仍不可达**：`127.0.0.1:22 CLOSED`；`wsl.exe` 被安全策略黑名单拦截且禁止绕过；Windows OpenSSH Server 未安装。终端/文件/传输属**优雅降级**而非「已连通」，不判通过。用户需手动安装 OpenSSH Server 或接外部靶机后「一键重试」恢复。
- **`mcp_probe` 第 10 项（9/10）**：只读命令 `ls` 因无已连接靶机会话 `s1` 返回 `pending_approval` 而非自动放行。该自动放行依赖活会话，本环境靶机不可用故走待审——与 §4.3 同源，属预期降级，非本轮引入回归（上一轮同为 9/10）。

---

## 5. 「真能用」判据对照（AC-01~10）
| 判据 | 状态 | 证据 |
|------|------|------|
| AC-01 窗口打开无白屏/错误页 | ✅ | `v2-main.png` |
| AC-02 状态灯与页面口径一致 | ✅ | `v2-status.png`（不再谎报） |
| AC-03 审批角标显示待审数 | ✅ | 角标=4，页内「待审批任务 (4)」 |
| AC-04 接入向导生成可用片段 | ✅ | `v2-onboard.png`（含 Bearer，可复制/改写） |
| AC-05 终端/文件树靶机可用时连上 | ⚠️ RESTRICTED | 22 CLOSED，走优雅降级（受限≠通过） |
| AC-06 靶机不可用时清晰提示 | ✅ | 「未连接靶机 + 原因 + 一键重试」 |
| AC-07 无 401/429 | ✅ | e2e 22/22、auth_matrix 32/32、throttle 7/7 |
| AC-08 令牌不泄露 | ✅ | 全仓扫描 0 命中；e2e no-master-token-leak 全 PASS |
| AC-09 防爆破生效 | ✅ | throttle 7/7 |
| AC-10 离线可构建 | ✅ | 全程 `--offline`，未联网下载依赖 |

---

## 6. 交付物与运行方式
- **源码**：已提交 `10d7d0d`（CORS 修复），同步收编进 `E:/思考/dou/app/`。
- **自包含 release exe**：**已构建并验证完成** —— `E:/dou-app/src-tauri/target/release/app.exe`（8.7MB，内嵌 dist）。
  - 验证：双击启动后网关 7419 自动拉起，且 **1420 端口保持 CLOSED**（证明前端来自内嵌 dist，无需 Vite）；窗口正常渲染客户端、审批/接入向导功能可用（见 `release-main.png` / `release-approve.png`）。
- **开发态运行**：`cd E:/dou-app && npm run dev`（起 Vite 1420）＋ 双击 debug `app.exe`（自动拉起网关 7419）。
- **生产态运行**：双击 release `app.exe` 即可（前端已内嵌，单文件自包含）。

## 7. 风险与回退
- 若用户环境无 WebView2 运行时，release exe 首次启动会提示安装（系统级，非本项目依赖）。
- 回退：保留 `E:/dou-app-backup-20261112` 母本；若 IPC 通道异常，可 `git revert 10d7d0d` 回到令牌桥接版（仍有 R1/R2，仅后端可用）。
- 仍需用户侧决策：是否安装 Windows OpenSSH Server（步骤 1 靶机替代路径）、是否保留 `E:/dou-app` 副本。
