# CURRENT-BASELINE · 豆 Dou 唯一现状基线

> 性质：**本项目唯一有效的现状基线**。其他一切文档（`ROADMAP.md` R1、`PLAN-V2.md` R2、`GAP-REPORT.md`、`docs/handover/00~10`）自本文创建起一律视为**历史态 / 目标态陈述**，其"已完成"结论不作为基线；冲突处以本文为准。
> 维护规则：每完成一项验证或一笔提交，就地更新本文对应行；每行结论必须能指向一条**可复跑的命令**或**一笔 commit**。
> 标记口径：✅ 本机实测（有命令/日志）｜ 📄 文档声称（未独立复验）｜ ⏸ 未验证。
> 创建：2026-11-16（接手会话）。

---

## 1. 仓库与真源

| 项 | 状态 | 指向 |
|---|---|---|
| 客户端真源 | ✅ `dou-handover-20261113-2139\dou-app`，git 干净 | HEAD `906642d`（2026-11-16 MobaXterm 三能力：密码认证兜底/本地 Shell 降级标签/主机监控条） |
| 网关真源 | ✅ `dou-handover-20261113-2139\dou-workspace`（`crates/dou-gateway`），git 干净 | HEAD `bc29000`（2026-11-16 本会话 S3 网关支撑：/policy/rules+/snapshots+audit 判定过滤） |
| 废弃镜像 | `dou-workspace/app/` 已标 DEPRECATED，不在其上开发 | commit `67be637` |
| 回滚点 | 🔴 `_backup-20261113-2350`（恢复 git 前快照，207 文件）勿动 | 磁盘体检报告 |
| 仓库外旧副本 | ⚠️ 根目录 `dou-app\`、`dou-workspace\` 为**旧版本非 git 副本**（2026-11-16 17:51~18:10 落盘，来历未记录），不作为任何依据 | 本会话哈希比对：15 抽 13 与仓库不一致且更旧 |
| 疑似过期产物 | 根目录 `豆仔-app.exe`（11-16 12:24，早于白屏修复提交）不作为验收依据 | 文件时间戳 vs commit 5850289 |

## 2. 环境与工具链

| 项 | 状态 | 指向 |
|---|---|---|
| Rust | ✅ 1.100.0；客户端 MSVC pin（`rust-toolchain.toml`），网关 GNU（`cargo +gnu`） | `verify.sh` §4 |
| Node | ✅ v22.16.0（`verify.sh` §4 实测；README 所写 22.22.2 为历史态） | `verify.sh` §4 |
| 构建产物目录 | `CARGO_TARGET_DIR=D:\dou-target`（客户端 MSVC）；网关用 workspace 自身 target | dou-app README |
| **bash 调用（新坑）** | ✅ 本机 PATH 的 `bash` 指向 `WindowsApps\bash.exe`（WSL 启动器），**WSL2/Hyper-V 未装不可用**；`verify.sh` 必须用 Git Bash 全路径：`& "C:\Program Files\Git\bin\bash.exe" verify.sh` | 2026-11-16 本会话实测（首次误用 WSL bash 报 `HCS_E_HYPERV_NOT_INSTALLED`） |
| PowerShell 5.1 | 脚本注释一律 ASCII 或文件带 UTF-8 BOM（无 BOM 中文注释会被按 GBK 误解码破坏解析） | 终版报告 §4.4 |
| MSVC 必要性 | GNU 目标 `cargo build` 必撞 `ld.exe: export ordinal too large`，不可省 | 终版报告 §4.1 |
| C 盘 | ✅ 48.94 GiB 可用（11-15 清理 297/301）；🔴 勿动清单 10/10 完好 | 磁盘与工程体检报告-20261115 |
| 224 服务器 | 🔴 关机 containment 中（root 挖矿木马）：不连、不恢复凭据、不推数据 | 接下来计划 §7 红线 1 |

## 3. 验证矩阵（当前可复跑）

| 检查 | 命令 | 结果 | 判定 |
|---|---|---|---|
| 交接包全量 | `& "C:\Program Files\Git\bin\bash.exe" verify.sh`（在包根目录） | 28 项判据 | ✅ 2026-11-16 复跑：25 PASS + 1 FAIL（rustfmt 回归）→ 修复后 **26 PASS** → B1/D2 后 **27 PASS** → D1 后 **28 PASS / 0 FAIL**（step 10 分层检查并入） |
| 环境自检 | `powershell -ExecutionPolicy Bypass -File .\scripts\env-check.ps1`（dou-app 内） | 21/21 ALL OK | ✅ 2026-11-14 |
| 客户端构建 | `cargo build`（dou-app/src-tauri） | 0 error | ✅ 2026-11-16（verify §5） |
| 客户端单测 | `cargo test --lib` | 14 passed（vault 11 + monitor 3） | ✅ 2026-11-16（verify §5） |
| 客户端静态 | `cargo clippy --all-targets` / `cargo fmt --check` | 0 警告 / 0 差异（fmt 回归已修） | ✅ 2026-11-16 |
| 网关测试 | `cargo test -p dou-gateway`（本机默认工具链即 GNU） | 54 passed | ✅ 2026-11-16 本会话实测（36→44→50（B1）→54（S3 网关支撑），S3 前端批次后回归 54/54 零回归） |
| 前端类型 | `vue-tsc --noEmit` | 0 error（含 spec 文件） | ✅ 2026-11-16（verify §6，D2 后含 vitest 测试文件） |
| 前端构建 | `npm run build`（前置分层检查 + vue-tsc + vite） | 781 模块 | ✅ 2026-11-16 本会话 S3 各批次均通过 |
| 前端单测 | `npm run test`（dou-app） | 16 passed（useApproval 12 + useRequestGuard 4） | ✅ 2026-11-16 本会话（verify §9） |
| 命令契约 | `python tools/verify-invoke.py` | 25 条无缺失 | ✅ 2026-11-16（verify §7） |
| SSH 主线 | `cargo run --bin dou-ssh-selfcheck` | 10/10（TOFU、公钥认证、分块传输、断点续传、密钥变更拒绝） | ✅ 2026-11-16（verify §8） |
| **真窗口** | `npm run tauri dev` | 窗口存在（1320×1000、标题「豆Dou」）+ 内容区非白屏 + 无崩溃 | ✅ 2026-11-16 本会话实测（S3 精修版截图 `d:\dou\app-window-s3-final.png`：暗色标题栏无白断层、三栏布局/侧栏分组/四态空态渲染正常；靶机未连接空态属功能性异常非渲染异常） |

### §3 追记（2026-11-16 第二轮）

- rustfmt 回归（`build.rs` 长行，commit 5850289 引入）已修：`cargo fmt --check` → 0 差异。
- A4 前端「断连置顶 + 人工终止入口」已实现（`useApproval.ts` / `ApprovalPanel.vue`），vue-tsc 与真窗口验证见追记轮次。

### §3 追记（2026-11-16 第三轮：D1 分层重构）

- src 重排为分层结构（git mv 保留历史，commit `3f1ddfc`）：`shared`(types) / `events`(useEventBus) / `adapters`(useGateway 唯一 IPC 出口) / `core`(useSsh/useApproval/useCredentials/useHostKeys/useOnboard/useEditor/useFileTree/useTerminal/useTransferQueue) / `commands`(useRecovery/useShell/useWorkspace) / `views`(App.vue)，components 独立成层。
- 反向引用构建期可检出（commit `ad32180`）：`tools/check-layers.mjs` 按「只允许向高层依赖」归层检查，链入 `npm run build` 与 verify.sh step 10；自测负例（core→commands）确认 exit 1。
- 同步项：vitest coverage exclude 改 `src/shared/types.ts`；verify.sh step 9 覆盖率过滤由 `composables` 路径改为 `src/(core|adapters|commands)/`（口径不变，同一批文件）。
- 复验：vue-tsc 0 error / vitest 12/12 / npm run build 通过 / verify.sh **28 PASS / 0 FAIL**。

### §3 追记（2026-11-16 第四轮：S3 前端形态与设计系统闭环）

- **S3-4a 设计 token 层**（dou-app `f6b7eac`）：`src/shared/tokens.ts`（颜色/字体/字号/几何/动效，CSS 与语法高亮双端同源）+ `app.css` 磷光绿终端风全量 token 化（保留全部历史类名）；无障碍：侧栏 div→button 化 + `aria-current="page"`、正文 14px、主交互 44px、`:focus-visible`、`prefers-reduced-motion` 降级、960px 单列降级；4 处亮色残留收敛为 ok-text/warn-text 语义类。
- **S3-2 健壮性**（dou-app `31da434`）：`shared/feedback.ts` notify/askConfirm 唯一出口（裸调用兼容 node 测试桩与 WebView 双环境，全仓 38 处收敛）；`core/useRequestGuard` requestId 防过期竞态（loadFiles 首个接入）+ 4 例单测；`components/StateShell` 四态容器（role/aria-live）；`components/ErrorBoundary` 渲染错误边界接入 App.vue 视图区。
- **S3-1 五页面**（dou-app `f9a7119`）：连接管理（3 步内新增主机，原「凭据保险库」整页并入）/ 策略配置（/policy/rules 三档只读分组）/ 审计中心（时间/来源/判定三维检索 + 所见即所得 CSV 导出 + 审计链完整性常驻）/ 实验详情（/snapshots 指纹聚合 runs 徽章）/ 设置（网关状态 + 快照/导入动作侧栏迁入）；侧栏分组「工作区/管理/凭据与安全」13 项同级可达。
- **S3-3 编辑器**（dou-app `c3ff73b`）：`dou-terminal` 主题（色值取自 douTokens）；py/yaml/md 内建 Monarch 按需接入 + json/log/conf 自定义 tokenizer，六类高亮零新增 worker；`core/editorLanguage` 扩展名映射；C4 重复指纹：EditorPanel 打开 py/sh 查 /snapshots 命中即显「跑过 N 次」横幅，`core/useNav` 全局 view 状态，结果目录跳转（loadFiles+navigateTo）双入口（编辑器横幅 / 实验详情页）。
- **S3 网关支撑**（dou-workspace `bc29000`，本轮之前提交）：`/policy/rules` + `/snapshots` 端点、audit query/export 判定过滤（所见即所得导出）、快照重跑聚合（id 带毫秒后缀，同指纹各占一行 GROUP BY）；store/snapshot 4 例新单测。
- **复验**：verify.sh **28 PASS / 0 FAIL**（Git Bash 全路径）；`cargo test -p dou-gateway` 54/54 零回归；前端 16/16；分层 0 违规。

### §3 追记（2026-11-16 第五轮：S3 真窗口视觉复核 + 精致化闭环）

- **真窗口截屏工具**：`d:\dou\scripts\capture-window.ps1`（按进程名 app 枚举窗口句柄截屏，纯 ASCII 规避 PS5.1 GBK 误解码；PS5.1 不支持 `0u` 字面量需写 `[uint32]0`）。
- **第一轮复核**（截图 `app-window-s3.png`）：暗色终端风渲染无异常、无白屏/错位/双渲染，四态空态正确；给出 4 项精致化建议（侧栏对比度 / 离线提示重复 / 保存按钮可点击感 / 标题栏暗色）。
- **精致化批次**（dou-app `c5808ae`）：① `tauri.conf.json` 加 `"theme": "dark"`（标题栏暗色，实测无白色断层）；② `--dou-dim`/`douTokens.dim` 双端同步 `#8ca296→#9fb4a8`（侧栏次级文本与编辑器注释/行号同步提亮，Monaco 主题引用 douTokens 自动跟随）；③ `.editorbar .btn` 绿色实心主操作视觉（与「一键重试」同语言）；④ 编辑器空态 `:disabled="!editingFile"` + `.editorbar .btn:disabled` 中性色（未打开文件时保存不可点）；⑤ OfflineState files 变体压缩为「标题+真实原因+重试」两行式，消除与终端 overlay 的长句重复。
- **设计决策**：files 变体保留 sshError 独立一行（不并入标题行）——sshError 是 fail-closed 的真实原因展示，230px 窄面板下单行会截断，两行式为信息密度与可读性的平衡点。
- **复验**：`npm run build`（分层 43 文件 0 违规 + vue-tsc + vite）通过；前端单测 16/16；第二轮复核（截图 `app-window-s3-refined.png` / `app-window-s3-final.png`）确认四项建议全部落地、空态禁用色正确。

### §3 追记（2026-11-16 第六轮：MobaXterm 三能力对齐）

- **能力 1 密码认证兜底**（dou-app `906642d`）：`ssh.rs` 拆出公共拨号层 `ssh_dial`，新增 `ssh_connect_with_password`；密码经 `Zeroizing<String>` 全程内存驻留（不落盘、不写日志）；`lib.rs open_pty` 新增 `password: Option<String>` 入参（空→None 走密钥）；保险库 `VaultTarget` 加 `auth` 字段（`serde(default)` 兼容旧库），密码模式目标只存地址与认证方式，`vault_test_connect` 密码分支用后即弃。前端：TerminalPanel 工具条认证方式下拉 + 密码框（内存 ref），ConnectionsPanel 密码模式入库与「密码连接」内联表单（提交后立即清空）。
- **能力 2 本地 Shell 降级标签**：`pty.rs open_local_pty` spawn 本机 PowerShell（UTF-8 管道模式、kill_on_drop），stdin/stdout/stderr 桥接既有 `pty_data`/`pty_write` 通道；前端「本地 Shell」按钮，靶机不可达时终端仍可用（真窗口实测：标签激活、`PS C:\Users\PC>` 提示符正常、Rust 日志 `[pty] open_local_pty (powershell)`）。
- **能力 3 主机监控条（无 agent）**：新增 `src-tauri/src/monitor.rs`（5s 周期，单次 exec 内嵌两次 `/proc/stat` 采样求 CPU、`/proc/meminfo` 内存、`/proc/net/dev` 字节差求速率），`host_stats`/`host_stats_off` 事件驱动前端薄监控条（CPU/内存迷你仪表 + 网速 + 运行时长，无会话自动隐藏）；russh 0.65 `Handle` 不可 Clone（`channel_open_session` 仅需 `&self`），pty 与监控以 `Arc<DouHandle>` 共享会话句柄（Drop 仅打日志不断连，会话存活由 Channel 保持）。
- **顺带修复**：多标签共用单容器导致终端纵向堆叠——`showActiveTerm` 按 activeSid 切换 xterm element 显隐并重排 fit（随标签保存 FitAddon）。
- **环境边界**：本机 127.0.0.1:22 无 sshd 监听且私钥不存在，密码认证的「连接成功」全链路与监控条真实数据留待靶机就绪后逐屏验收（S1-2）；解析逻辑由 monitor 3 例单测 + 回环自校验 10/10 覆盖。
- **复验**：verify.sh **28 PASS / 0 FAIL**（`cargo test --lib` 14/14、clippy 0 警告、fmt 0 差异、vue-tsc 0 error、前端 16/16、分层 0 违规、命令契约无缺失）；新增 UIA 点击工具 `d:\dou\scripts\uia-click.ps1`（UIAutomation 按名称点按钮，配 capture-window.ps1 构成真窗口交互验证链）。

## 4. 功能工单现状（S0→S4 视角，详见 接下来计划-20261115.md §3）

| 工单 | 状态 | 指向 |
|---|---|---|
| S0-1 29 项改动收口 | ✅ 完成 | 11-16 16:50 四笔 commit，两仓 `git status` 干净 |
| S0-3 C 盘清理剩余 4 项 | ⏸ 可选不阻塞（DxcCache 4.99G / SoftwareDistribution 0.78G / sdkusage.csv 0.63G / npm-cache 0.91G） | 体检报告 |
| S1-1 真窗口启动 | ✅ 本会话实测 | §3 真窗口行（截图 app-window-20261116-postfix.png） |
| S1-2 逐屏交互验收 | ⏸ 需用户在机器前（AC-01/02/03/06/09） | 接下来计划 §3 |
| A1 会话锁 / A2 长命令任务化 / A3 审批超时 | ✅ 已提交 | commit 2f47bf4；E2E 📄 WorkBuddy（blocked/path_scope_denied 用例过） |
| A4 审批卡片四要素 | ✅ 已提交（后端+前端展示） | commit 7b7dce3 |
| A4 前端断连置顶 + 人工终止 | ✅ 本会话提交（3e1a31a）；D2 单测又修复其「失联窗口判定」死代码 bug（18ff118） | `useApproval.ts`（classifyOrphans 参数化） |
| A5 配额/租约持久化 | ✅ 本会话提交 | dou-workspace `856aba6`（写穿三接线 + 启动恢复）+ fmt 归一 `118c1e2` |
| B1 私钥本地加密库 | ✅ 本会话提交（全量判据满足：密文+盐落库 / 无明文读接口 / 落库检索无明文） | dou-workspace `76b8457`（vault.rs，50/50） |
| B2 指纹变更二次确认 | ✅ 本会话闭环（网关 /hostkey/list+forget + 前端终端/网关两库遗忘 + mismatch 指引） | dou-workspace `1cf8643` + dou-app `4cd89dc` |
| B3 逃逸加固 | ✅ 本会话提交（SFTP REALPATH 协议级 symlink 真实解析，三通道统一闸门，put 拒写穿） | dou-workspace `16723a9` |
| D2 覆盖率门槛入脚本 | ✅ 观察期达成（vitest+coverage-v8 已入 verify.sh step 9，WARN 判定；门槛 核心≥70%/全量≥40% 当前 11.1%/11%，未达标不 FAIL） | dou-app `18ff118` |
| D1 分层目录重构 | ✅ 本会话提交（分层重排 `3f1ddfc` + 依赖检查器 `ad32180`，反向引用构建期可检出，verify 28 PASS） | dou-app `3f1ddfc`/`ad32180`（tools/check-layers.mjs） |
| S3 五页面/四态/设计 token/无障碍 | ✅ 本会话闭环（网关支撑 `bc29000` + 前端 `f6b7eac`/`31da434`/`f9a7119`/`c3ff73b`；判据：侧栏同级可达 13 项、连接管理 3 步新增主机、审计三维检索导出、六类高亮、C4 可跳转） | §3 追记第四轮 |
| S3 视觉精致化（复核驱动） | ✅ 本会话提交（`c5808ae`：暗色标题栏/对比度提亮/保存主操作视觉/空态禁用/离线提示减重复） | §3 追记第五轮 |
| S3-验证 真窗口视觉复核 | ✅ 本会话闭环（两轮截屏复核 + 精致化批次落地 + 设计决策登记；截图 `app-window-s3-final.png`） | §3 追记第五轮 |
| MobaXterm 三能力（密码兜底/本地 Shell/监控条） | ✅ 本会话提交（dou-app `906642d`：Rust 三层 + 前端接线 + 真窗口验证本地 Shell 与监控空态；密码全链路待靶机就绪） | §3 追记第六轮 |
| S4 打包/内测 | 🔴 移出计划（用户指示 2026-11-16：先产品后打包；签名/自动更新/i18n 同步暂缓） | docs/PRODUCT-RECTIFICATION.md §5 |
| 产品整改工单 R1-R11（P0 首屏可用性 3 项 / P1 露馅点 5 项 / P2 信任叙事 3 项） | ⏸ 未开始（来源：2026-11-16 真实用户视角体验复核 + 三能力闭环后的产品判断；执行顺序 R1→R5→R3→R2→R6→R8→R4→R7→R9/R10→R11，每单含可复跑验收标准） | docs/PRODUCT-RECTIFICATION.md |

## 5. 网关安全硬约束（🔴 不改不放松）

`/mcp` 单一 Bearer 解析点在 `authz.rs`；`EXEMPT_PATHS ≤ 1`；真实令牌字面量 0；32/32 路由鉴权；7/7 防爆破。为"让请求通"而改松网关的方案一律否决（07 PRD §9.2 历史态继承，约束本身仍有效）。

## 6. 历史文档口径声明（已在其文件头部标注）

- `docs/ROADMAP.md`（R1）· `docs/PLAN-V2.md`（R2）· `docs/GAP-REPORT.md` · `docs/handover/07-增量PRD-让豆真能用.md` → 顶部已加「历史态，以 CURRENT-BASELINE 为准」横幅。
- 其冲突点（如 ROADMAP 进度记录称 11-10「五工具全通」vs 任务表全 ○）不再逐条仲裁：一切以本文 §3/§4 实测行为准。
