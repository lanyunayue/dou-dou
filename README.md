# 豆 Dou · 桌面客户端（Tauri 2 + Vue 3 + Rust）

> 接手速查。工具链已固定为 **MSVC**，见 `rust-toolchain.toml`。

## 一、环境（已配置好，无需重做）

| 项 | 位置 / 版本 |
|---|---|
| Rust 工具链 | `stable-x86_64-pc-windows-msvc`（rustc 1.99.1），由 `rust-toolchain.toml` 锁定 |
| C++ 编译器 | Visual Studio 2026 Community，`.../VC/Tools/MSVC/14.50.35717` |
| Windows SDK | `10.0.26100.0` |
| Node / npm | 见 `node_modules`，Vue 3 + Vite 8 + TypeScript 6 |
| 编辑器 | VS Code 1.145.2，`D:\Microsoft VS Code`（PATH 中 `code` 可直接用） |
| 已装扩展 | rust-analyzer 0.3.3113 / Vue.volar 3.3.14 / tauri-vscode 0.2.9 / cpptools 1.35.6 |
| 依赖缓存 | 全部离线可用（`CARGO_NET_OFFLINE=true`） |
| 构建产物 | `src-tauri/target/`（前端 `dist/`） |

先跑一次自检：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\env-check.ps1
```

## 二、常用命令

```powershell
.\scripts\dev.ps1 check      # cargo check + 前端类型检查（改完代码先跑这个）
.\scripts\dev.ps1 build      # 只编译 Rust 后端
.\scripts\dev.ps1 dev        # 整应用热重载（npm run tauri dev）
.\scripts\dev.ps1 release    # 出发布包（npm run tauri build）
.\scripts\dev.ps1 ssh-test   # SSH 回环端到端自校验（10 项）
.\scripts\dev.ps1 fmt        # cargo fmt + clippy
```

等价原始命令（VS Code 里 Ctrl+Shift+B 已配好）：

```bash
cd src-tauri && cargo check --offline
cd src-tauri && cargo build --offline
npm run tauri dev
npm run tauri build
cd E:/思考/dou/tools/ssh-loopback && cargo run --offline --bin dou-ssh-selfcheck
```

## 三、目录地图

| 路径 | 内容 |
|---|---|
| `src/` | Vue 3 前端（`App.vue` 及各面板） |
| `src-tauri/src/lib.rs` | Tauri 命令总装配 + 状态注册（`#[tauri::command]` 只在这里出现一次） |
| `src-tauri/src/ssh.rs` | 共享 SSH 内核：主机密钥 TOFU / known_hosts / 公钥认证连接 |
| `src-tauri/src/vault.rs` | 凭据保险库：PBKDF2 + AES-256-GCM（**不含** `#[tauri::command]`，由 lib.rs 包装） |
| `src-tauri/src/transfer_queue.rs` | 传输队列 / 分块续传 / 文件夹同步 |
| `src-tauri/src/transfer.rs` `pty.rs` `sftp.rs` | 传输、交互终端、SFTP |
| `E:/思考/dou/tools/ssh-loopback/` | 无外部服务器时的本地回环 SSH 靶机 + 自校验 |
| `E:/思考/dou/docs/handover/` | 接手上下文包 / PRD / 架构设计 / 工单清单 |
| `E:/思考/dou/crates/dou-gateway/` | 独立网关 workspace（**仍用 GNU 工具链**，见下） |

## 四、两个容易踩的坑

1. **两套工具链并存**：本客户端用 MSVC；网关 workspace（`E:/思考/dou`）仍在
   `.cargo/config.toml` 里锁了 MinGW GNU 的 linker/ar，`scripts/build.ps1` 显式用
   `+stable-x86_64-pc-windows-gnu`。**不要**把那份 GNU 配置复制进本项目。
2. **`crate-type` 已恢复官方三件套**（`staticlib` / `cdylib` / `rlib`）：早期 GNU 工具链下
   `cdylib` 会生成 `app_lib.dll` 并触发 `ld.exe: export ordinal too large`，当时靠移除绕过；
   现固定 MSVC（`link.exe`）后已实测恢复成功，`app_lib.dll` 正常产出，坑已根治。
   若哪天被迫换回 GNU 工具链，需重新移除 `cdylib`。

其它已修过的历史坑（改代码时留意）：
- `#[tauri::command]` 只能在最终包装层出现一次，`vault.rs` 里的已全部去除。
- russh 0.65：`client::connect` 返回 `Handle<H>`；`ChannelMsg::Eof` 是单元变体，
  `ExitStatus { exit_status }` 是结构体变体；`exec/data/eof/wait` 均为异步。
- `SystemRandom::fill()` 需要 `use ring::rand::SecureRandom;`。
- 传输命令 `dd if=... bs=65536 skip=N count=1`、`cat >`/`cat >>` 依赖远端 Unix 工具，
  回环靶机走 PortableGit 的 bash，路径用正斜杠。

## 五、调试

VS Code（推荐，已配好）：装齐 `.vscode/extensions.json` 推荐的 4 个扩展，
F5 选「调试 dou.exe (MSVC)」（`cppvsdbg` 复用 VS 2026 调试引擎）。

打开项目：

```powershell
code E:\dou-app
```

Visual Studio 2026 Community：「文件 → 打开 → 文件夹」选 `E:\dou-app`，把
`src-tauri` 当普通文本/Rust 目录编辑；Rust 编译仍走 `cargo`，VS 主要用作编辑器与
原生调试器。Tauri 是 cargo 工程，不是 `.sln`，不要试图生成解决方案文件。

## 六、受治理远程操作主线验证状态

本地回环自校验 `dou-ssh-selfcheck` 共 **10 项全部通过**：主机密钥首次/重复/变更拒绝、
公钥认证、上传分块、断点续传、下载分块、同步 push / 删除镜像 / pull。
证据目录：`%TEMP%\dou_loopback_test\`（含 `selfcheck_report.txt`）。

真实外部 Linux 靶机仍待接入：设置环境变量 `DOU_SSH=user@host:port` 与
`DOU_SSH_KEY=<私钥路径>` 即可，缺省回退到 `dou@127.0.0.1:22`。

## 七、接手体检结论（2026-11-13）

完整报告见 `E:\思考\dou\docs\handover\05-接手体检报告.md`。**构建、类型检查、静态分析
全部 0 error**。已处理与待处理如下：

| 优先级 | 问题 | 状态 |
|---|---|---|
| P0 | 保险库锁定态可绕过：`vault_list_targets` / `vault_remove_target` 未校验 unlocked | ✅ **已修**（新增 `ensure_unlocked` 闸门 + 2 个回归测试） |
| P0 | 明文私钥落盘后不回收（`vault_lock` 只清内存密钥） | ✅ **已修**（私钥只在内存中短暂存在，用后即清；新增 `ssh_connect_with_key` 内存私钥入口 + `vault_test_connect`；启动/锁定时清理 `keys/` 遗留明文） |
| P1 | 保险库文件损坏被静默吞，误报「未初始化」 | ✅ **已修**（`read_vault` 区分「不存在 / 损坏 / 正常」；损坏时 `vault_init` 拒绝覆盖原文件） |
| P1 | KDF 迭代 200k 偏低（OWASP 建议 ≥600k） | ✅ **已修**（新建保险库写 600k；迭代次数存进 `vault.json`，老文件按 200k 兼容解锁） |
| P1 | `.gitignore` 吞掉 VS Code 配置 | ✅ **已修** |
| P1 | 项目尚未 git 化 | ✅ **已修**（commit `00b8171`，tag `handover-baseline`） |
| P2 | 前端 942 行单文件 `App.vue`，24 处 invoke | ⬜ 待处理 |
| P2 | 5 个真死代码命令（已被 `tq_enqueue` 取代） | ⬜ 待处理 |
| P2 | clippy 26 warning / rustfmt 31 处差异 | ⬜ 待处理 |

## 八、版本底子（git）

仓库已初始化（分支 `main`），基线打 `handover-baseline` tag。

```bash
cd E:\dou-app
git log --oneline          # 查看历史
git status                 # 应始终干净
git tag -l                 # handover-baseline
git revert <commit>        # 单个提交可独立回滚（已实测）
```

- `.gitignore` 已排除全部敏感与产物：`token.txt` / `audit.db` / `known_hosts.json` /
  `vault.json` / `quarantine/` / 各类私钥 / `node_modules` / `dist` / `target/`。
- `.gitattributes` 统一仓库内换行为 LF（消除 Windows CRLF 噪音）。
- `.vscode/{settings,tasks,launch}.json` 随仓库走，协作方开箱即用。

⚠️ 提交前请确认没有 `git add -A` 误带敏感文件；架构师工单对首个 commit 有明确红线
（不得含 `token.txt` / `audit.db`）。

体检命令：

```bash
cd src-tauri && cargo fmt --check && cargo clippy --offline --all-targets
cd E:/dou-app && npx vue-tsc --noEmit
bash E:/思考/dou/tools/check-contract.sh    # 前后端命令契约核对
```
