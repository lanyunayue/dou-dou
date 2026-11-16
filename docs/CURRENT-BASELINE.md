# CURRENT-BASELINE · 豆 Dou 唯一现状基线

> 性质：**本项目唯一有效的现状基线**。其他一切文档（`ROADMAP.md` R1、`PLAN-V2.md` R2、`GAP-REPORT.md`、`docs/handover/00~10`）自本文创建起一律视为**历史态 / 目标态陈述**，其"已完成"结论不作为基线；冲突处以本文为准。
> 维护规则：每完成一项验证或一笔提交，就地更新本文对应行；每行结论必须能指向一条**可复跑的命令**或**一笔 commit**。
> 标记口径：✅ 本机实测（有命令/日志）｜ 📄 文档声称（未独立复验）｜ ⏸ 未验证。
> 创建：2026-11-16（接手会话）。

---

## 1. 仓库与真源

| 项 | 状态 | 指向 |
|---|---|---|
| 客户端真源 | ✅ `dou-handover-20261113-2139\dou-app`，git 干净 | HEAD `18ff118`（2026-11-16 本会话 D2 覆盖率门槛+useApproval 单测） |
| 网关真源 | ✅ `dou-handover-20261113-2139\dou-workspace`（`crates/dou-gateway`），git 干净 | HEAD `76b8457`（2026-11-16 本会话 B1 私钥本地加密库） |
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
| 交接包全量 | `& "C:\Program Files\Git\bin\bash.exe" verify.sh`（在包根目录） | 27 项判据 | ✅ 2026-11-16 复跑：25 PASS + 1 FAIL（rustfmt 回归）→ 修复后 **26 PASS** → 本会话（含 B1/D2 后）**27 PASS / 0 FAIL** |
| 环境自检 | `powershell -ExecutionPolicy Bypass -File .\scripts\env-check.ps1`（dou-app 内） | 21/21 ALL OK | ✅ 2026-11-14 |
| 客户端构建 | `cargo build`（dou-app/src-tauri） | 0 error | ✅ 2026-11-16（verify §5） |
| 客户端单测 | `cargo test --lib` | 11 passed | ✅ 2026-11-16（verify §5） |
| 客户端静态 | `cargo clippy --all-targets` / `cargo fmt --check` | 0 警告 / 0 差异（fmt 回归已修） | ✅ 2026-11-16 |
| 网关测试 | `cargo test -p dou-gateway`（本机默认工具链即 GNU） | 50 passed（A5/B2/B3/B1 单测并入） | ✅ 2026-11-16 本会话实测（36→44→50，B1 后 50/50） |
| 前端类型 | `vue-tsc --noEmit` | 0 error（含 spec 文件） | ✅ 2026-11-16（verify §6，D2 后含 vitest 测试文件） |
| 前端构建 | `npm run build` | 73 模块 | ✅ 2026-11-14；A4/D2 改动后待复跑 |
| 前端单测 | `npm run test`（dou-app） | 12 passed（useApproval 首批） | ✅ 2026-11-16 本会话（verify §9） |
| 命令契约 | `python tools/verify-invoke.py` | 25 条无缺失 | ✅ 2026-11-16（verify §7） |
| SSH 主线 | `cargo run --bin dou-ssh-selfcheck` | 10/10（TOFU、公钥认证、分块传输、断点续传、密钥变更拒绝） | ✅ 2026-11-16（verify §8） |
| **真窗口** | `npm run tauri dev` | 窗口存在（1320×1000、标题「豆Dou」）+ 内容区非白屏 + 无崩溃 | ✅ 2026-11-16 本会话实测（截图 `d:\dou\app-window-20261116-postfix.png`；靶机未连接空态属功能性异常非渲染异常） |

### §3 追记（2026-11-16 第二轮）

- rustfmt 回归（`build.rs` 长行，commit 5850289 引入）已修：`cargo fmt --check` → 0 差异。
- A4 前端「断连置顶 + 人工终止入口」已实现（`useApproval.ts` / `ApprovalPanel.vue`），vue-tsc 与真窗口验证见追记轮次。

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
| D1 分层目录重构 | ⏸ 未开工（S2 唯一剩余项） | PLAN-V2 W-29 |
| S3 五页面/四态/设计 token/无障碍 | ⏸ 未开工（可与 S2 并行） | PLAN-V2 R2 |
| S4 打包/内测 | ⏸ 等外部条件（证书/靶机/样本/课题组），不承诺 | 接下来计划 §3 |

## 5. 网关安全硬约束（🔴 不改不放松）

`/mcp` 单一 Bearer 解析点在 `authz.rs`；`EXEMPT_PATHS ≤ 1`；真实令牌字面量 0；32/32 路由鉴权；7/7 防爆破。为"让请求通"而改松网关的方案一律否决（07 PRD §9.2 历史态继承，约束本身仍有效）。

## 6. 历史文档口径声明（已在其文件头部标注）

- `docs/ROADMAP.md`（R1）· `docs/PLAN-V2.md`（R2）· `docs/GAP-REPORT.md` · `docs/handover/07-增量PRD-让豆真能用.md` → 顶部已加「历史态，以 CURRENT-BASELINE 为准」横幅。
- 其冲突点（如 ROADMAP 进度记录称 11-10「五工具全通」vs 任务表全 ○）不再逐条仲裁：一切以本文 §3/§4 实测行为准。
