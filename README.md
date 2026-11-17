# 豆 Dou

> 本地优先的 SSH 客户端 + 审批审计网关。对标 MobaXterm 的核心价值链：**连上主机 → 用终端 → 管文件 → 看得见主机状态**，差异化在「密码不落盘、数据不出机器」的本地优先 + 审批审计安全链路。

## 🔴 唯一真源（换 AI / 换机必读）

**本仓库 `lanyunayue/dou-dou` 是本项目唯一权威真源。** 换 AI、换电脑、重新开会话时：

```bash
git clone https://github.com/lanyunayue/dou-dou.git
cd dou-dou
```

**不要**在别处新建骨架、复制副本或"从头重建"——历史上多次因此产生好几个互相冲突的版本。本地任何名为 `dou-app`、`dou-workspace` 的散落副本、旧 `handover` 快照、`_backup-*` 目录，都不是真源，一律以本仓库为准。

## 目录结构

| 路径 | 内容 | 技术栈 |
|---|---|---|
| `dou-app/` | 桌面客户端工作树（含完整 git 历史） | Tauri 2 + Vue 3 + TypeScript + Rust |
| `dou-workspace/` | 网关与文档主仓库（含完整 git 历史） | Rust (`crates/dou-gateway`) + docs + tools |

两个子目录各自保留原有 git 历史与 tag（`handover-baseline`）。本仓库用 `git subtree` 聚合，便于一次性 clone。

## 开发环境

| 项 | 版本 / 说明 |
|---|---|
| Rust | 1.100.0；客户端 MSVC（`rust-toolchain.toml` pin）；网关 GNU（`cargo +gnu`） |
| Node | v22.16.0+ |
| 构建缓存 | `CARGO_TARGET_DIR` 建议指向独立盘（如 `D:\dou-target`），删除只影响编译速度 |
| 前端 | `npm ci` 安装依赖 |

## 一键验证

在 `dou-workspace/` 同级（原 handover 包根）运行：

```bash
"C:\Program Files\Git\bin\bash.exe" verify.sh    # Windows；28 项判据
```

常用单项验证（在 `dou-app/` 内）：
```bash
npm run build          # 分层检查 + vue-tsc + vite
npm run test           # vitest 单测
cargo test --lib       # Rust 单测
cargo clippy --all-targets
cargo fmt --check
```

> 环境坑：Windows PATH 里的 `bash.exe` 可能指向 WSL 启动器（未装 WSL 会报 `HCS_E_HYPERV_NOT_INSTALLED`），必须用 Git Bash 全路径。

## 安全红线（不可放宽）

- `/mcp` 鉴权单一 Bearer 解析点；`EXEMPT_PATHS ≤ 1`；仓库内真实令牌字面量为 0。
- 私钥本地加密库（AES-256-GCM + PBKDF2），无明文读接口。
- B3 逃逸加固：SFTP REALPATH 协议级 symlink 真实解析，三通道统一闸门。
- 密码只驻留内存、不落盘、不写日志。

## 文档

- `dou-workspace/docs/CURRENT-BASELINE.md` — 唯一现状基线（每项结论指向可复跑命令或 commit）
- `dou-workspace/docs/竞品对比与交接计划-20261117.md` — 当前有效计划（R12-R18 + P3）
- `dou-workspace/docs/PRODUCT-RECTIFICATION.md` — R1-R11 产品整改执行记录
- `dou-workspace/docs/新机重建指南.md` — 换机恢复步骤

## 状态

活跃开发中。当前进度以 `CURRENT-BASELINE.md` 为准。
