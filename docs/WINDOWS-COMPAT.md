# Windows 兼容性说明（本轮实测）

> 版本：v1.0 · 2026-11-12
> 目标：**至少保障 Windows 11 x64 全链路可用**；Windows 10 x64 尽力兼容。

## 1. 兼容矩阵

| 平台 | 架构 | 状态 | 依据 |
|---|---|---|---|
| **Windows 11 25H2 (build 26200)** | x64 | ✅ **本机实测**（`cargo build` / `cargo test` / MCP 自检） | 注册表 `DisplayVersion=25H2`、`CurrentBuild=26200.9711` |
| Windows 11 24H2 (26100) | x64 | 🟡 同内核基线，无实机 → 受限·未实测 | 与 25H2 同为 NT 10.0.26100+ 内核 |
| Windows 11 23H2 (22631) | x64 | 🟡 无实机 → 受限·未实测 | 需外部 runner |
| Windows 11 22H2 (22621) | x64 | 🟡 无实机 → 受限·未实测 | 需外部 runner |
| Windows 10 22H2 (19045) | x64 | 🟡 尽力兼容，无实机 → 受限·未实测 | API 均为 Win10 已存在的子集 |
| Windows Server / ARM64 / 32 位 | — | ❌ 本轮明确不做 | 04 增量 PRD §4 |

> 注意：注册表 `ProductName` 在升级到 Win11 后仍可能显示 "Windows 10 Home China"，
> **不能据此判断系统版本**，应以 `CurrentBuild` / `DisplayVersion` 为准。
> Win10 最终 build 是 19045；26100=Win11 24H2，26200=Win11 25H2。

## 2. 本轮处理的兼容性问题

| # | 问题 | 处理 |
|---|---|---|
| 1 | 中文路径 `E:\思考\dou` 使 MinGW `dlltool` 按 ANSI 解码路径乱码 | 产物统一指向 ASCII 目录 `E:/dou-target`；`gate.ps1` / `build.ps1` 支持 `DOU_TARGET_DIR` 覆盖 |
| 2 | `cargo` / `rustc` 不在 PATH | 两个脚本自动注入 `%USERPROFILE%\.cargo\bin` 与 MinGW `mingw64\bin` |
| 3 | `.cargo/config.toml` 的 linker/ar 写死用户级 WinGet 绝对路径 | `build.ps1` 改为自动探测 `BrechtSanders.WinLibs.POSIX.UCRT_*`，不再依赖单一用户名 |
| 4 | PowerShell 5.1 把无 BOM 的 UTF-8 `.ps1` 当 ANSI 读，中文注释/输出乱码 | 所有 `.ps1` 脚本使用纯 ASCII；同时设置 `[Console]::OutputEncoding = UTF8` 以正确显示审计中文 |
| 5 | 控制台 GBK/UTF-8 混用导致审计中文乱码 | 探测脚本统一 `encode("utf-8")` / `decode("utf-8", errors="replace")`；HTTP 响应统一 `charset=utf-8` |
| 6 | `%LOCALAPPDATA%` 未设置时数据目录回退不一致 | `auth::data_dir()` 与 `state::db_path()` 统一三级回退 `LOCALAPPDATA → USERPROFILE → HOME → .` |
| 7 | 运行中的 `dou-gateway.exe` 锁住产物（`os error 5`） | 构建/门禁前自动 `Stop-Process dou-gateway` |
| 8 | GNU 与 MSVC 双工具链行为差异 | 两套 toolchain 均已安装；CI 设 GNU 必跑 job + MSVC 条件 job |
| 9 | 换行符跨平台漂移 | 新增 `.gitattributes`：源码/文档 LF，`.ps1`/`.bat` CRLF，二进制显式标记 |

## 3. 可复现命令

```powershell
# 全量质量门禁（构建 + 单测 + clippy，默认 GNU）
.\scripts\gate.ps1

# 切 MSVC
.\scripts\gate.ps1 -Toolchain stable-x86_64-pc-windows-msvc

# 仅构建
.\scripts\build.ps1 build

# 启动网关（dev 默认关闭；需显式开启本机命令兜底）
$env:DOU_DEV_MODE="1"
E:\dou-target\debug\dou-gateway.exe

# 端到端自检（另一个终端）
python tools\mcp_probe.py
python tools\test_auth_matrix.py
python tools\test_throttle.py
```

## 4. 已知限制（不伪造通过）

- 22H2 / 23H2 / 24H2 / Win10 无本机实机，只能标注「受限·未实测」，需 CI Windows runner 或多版本实机补测。
- `russh` 必须 `ring` 后端；`aws-lc-rs` 在 MinGW 下需 CMake+MSVC，本轮不切换。
- `run_local` 走 `cmd /C`，Unix 命令在 Windows 下仍会失败或 GBK 乱码；正式执行应走 SSH 远端会话。
- 业务代码未做 GUI 形态（Q4 桥接形态未拍板），本轮只覆盖网关进程、构建脚本、CI 与探测脚本。
