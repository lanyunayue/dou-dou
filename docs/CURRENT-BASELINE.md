# CURRENT-BASELINE · 豆 Dou 唯一现状基线

> 性质：**本项目唯一有效的现状基线**。其他一切文档（`ROADMAP.md` R1、`PLAN-V2.md` R2、`GAP-REPORT.md`、`docs/handover/00~10`）自本文创建起一律视为**历史态 / 目标态陈述**，其"已完成"结论不作为基线；冲突处以本文为准。
> 维护规则：每完成一项验证或一笔提交，就地更新本文对应行；每行结论必须能指向一条**可复跑的命令**或**一笔 commit**。
> 标记口径：✅ 本机实测（有命令/日志）｜ 📄 文档声称（未独立复验）｜ ⏸ 未验证。
> 创建：2026-11-16（接手会话）。

---

## 1. 仓库与真源

| 项 | 状态 | 指向 |
|---|---|---|
| 客户端真源 | ✅ `dou-handover-20261113-2139\dou-app`，git 干净 | HEAD `c3ff73b`（2026-11-16 本会话 S3 全量：token 层/四态/五页面/语法高亮） |
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
| 客户端单测 | `cargo test --lib` | 11 passed | ✅ 2026-11-16（verify §5） |
| 客户端静态 | `cargo clippy --all-targets` / `cargo fmt --check` | 0 警告 / 0 差异（fmt 回归已修） | ✅ 2026-11-16 |
| 网关测试 | `cargo test -p dou-gateway`（本机默认工具链即 GNU） | 54 passed | ✅ 2026-11-16 本会话实测（36→44→50（B1）→54（S3 网关支撑），S3 前端批次后回归 54/54 零回归） |
| 前端类型 | `vue-tsc --noEmit` | 0 error（含 spec 文件） | ✅ 2026-11-16（verify §6，D2 后含 vitest 测试文件） |
| 前端构建 | `npm run build`（前置分层检查 + vue-tsc + vite） | 781 模块 | ✅ 2026-11-16 本会话 S3 各批次均通过 |
| 前端单测 | `npm run test`（dou-app） | 16 passed（useApproval 12 + useRequestGuard 4） | ✅ 2026-11-16 本会话（verify §9） |
| 命令契约 | `python tools/verify-invoke.py` | 25 条无缺失 | ✅ 2026-11-16（verify §7） |
| SSH 主线 | `cargo run --bin dou-ssh-selfcheck` | 10/10（TOFU、公钥认证、分块传输、断点续传、密钥变更拒绝） | ✅ 2026-11-16（verify §8） |
| **真窗口** | `npm run tauri dev` | 窗口存在（1320×1000、标题「豆Dou」）+ 内容区非白屏 + 无崩溃 | ✅ 2026-11-16 本会话实测（截图 `d:\dou\app-window-20261116-postfix.png`；靶机未连接空态属功能性异常非渲染异常） |

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
| S3-验证 真窗口视觉复核 | ⏸ 新暗色 UI 真窗口截图待跑（`npm run tauri dev` 后人工/工具截屏） | 本行 |
| S4 打包/内测 | ⏸ 等外部条件（证书/靶机/样本/课题组），不承诺 | 接下来计划 §3 |

## 5. 网关安全硬约束（🔴 不改不放松）

`/mcp` 单一 Bearer 解析点在 `authz.rs`；`EXEMPT_PATHS ≤ 1`；真实令牌字面量 0；32/32 路由鉴权；7/7 防爆破。为"让请求通"而改松网关的方案一律否决（07 PRD §9.2 历史态继承，约束本身仍有效）。

## 6. 历史文档口径声明（已在其文件头部标注）

- `docs/ROADMAP.md`（R1）· `docs/PLAN-V2.md`（R2）· `docs/GAP-REPORT.md` · `docs/handover/07-增量PRD-让豆真能用.md` → 顶部已加「历史态，以 CURRENT-BASELINE 为准」横幅。
- 其冲突点（如 ROADMAP 进度记录称 11-10「五工具全通」vs 任务表全 ○）不再逐条仲裁：一切以本文 §3/§4 实测行为准。
