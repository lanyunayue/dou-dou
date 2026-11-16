# 豆 Dou · Gateway 尖刀版

本地回环 MCP 网关：让 WorkBuddy、Codex 这类代理以**受限、可审计**的方式连服务器，
而不是把私钥直接交给它。

当前阶段：**技术尖刀（第 2–4 周）**。目标是尽快证伪或证成"接入层"这条唯一的差异化假设，
不包含终端渲染、SFTP 界面、编辑器。

---

## 已实现（能跑的部分）

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| 只绑回环地址 | 已实现 | 非回环 bind 直接拒绝启动 |
| Bearer 令牌鉴权 | 已实现 | `dou_ak_` 前缀，定长比较，令牌落盘一次 |
| MCP Streamable HTTP | 已实现 | `initialize` / `notifications/initialized` / `tools/list` / `tools/call` / `ping` |
| 五工具集 | 已实现 | 与计划书表 13 一致，不多开一个 |
| 三档策略判定 | 已实现 | 只读放行 / 变更待审 / 危险阻断，**按命令段判**而非整串匹配 |
| 审计 JSONL | 已实现 | 只追加，每次判定都落一条 |
| SSH 通道 | **已实现** | russh 0.65（ring 后端），密钥认证优先；命令真在远端执行 |
| 主机密钥校验 | **已实现** | TOFU 记录公钥全文到 `known_hosts.json`，逐字比对；不匹配 `E_HOSTKEY_MISMATCH`、文件损坏 `E_KNOWN_HOSTS_CORRUPT` |
| 隔离区与静态扫描 | **已实现（基础）** | `scan.rs` 三级降级扫描；本轮未改其逻辑 |

## 构建前置（Windows）

本机实测：**没有 MSVC 链接器，且当前进程不是管理员**，所以走 MinGW 路线。

```powershell
# 一次性准备（不需要管理员权限）
winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
winget install --id BrechtSanders.WinLibs.POSIX.UCRT -e --scope user --accept-package-agreements --accept-source-agreements
```

两个必须知道的坑（详见 `docs/spike-report-001.md`）：

1. MinGW 的 `dlltool.exe` 必须能通过 PATH 找到，否则 `windows-sys` 编译失败。`scripts/build.ps1` 已自动注入。
2. 项目在中文路径下会让 dlltool 把路径解码成乱码，构建产物因此被指到 `D:/dou-target`（见 `.cargo/config.toml`）。

## 运行

```powershell
# 构建（走脚本，自动处理 PATH）
.\scripts\build.ps1 build

# 启动（默认 127.0.0.1:7419，dev 模式允许本地执行只读命令）
$env:DOU_DEV_MODE="1"; D:\dou-target\debug\dou-gateway.exe
```

启动后终端会打印一次 `dou_ak_...` 令牌，客户端配置里要用它。

## 连服务器（SSH）

私钥只在网关本地读取，MCP 侧永远拿不到凭据——这是本项目唯一不可替代的地方。

```powershell
$env:DOU_SSH     = "dou@127.0.0.1:22"                            # user@host:port
$env:DOU_SSH_KEY = "$env:LOCALAPPDATA\dou\keys\id_ed25519"       # 优先用密钥
# 仅临时联调才用口令（与"私钥不出本地"矛盾，正式版会关掉）
# $env:DOU_SSH_PASS = "..."
D:\dou-target\debug\dou-gateway.exe
```

成功时启动日志会出现：

```
[dou-gateway]  ├ ssh    dou@127.0.0.1:22 … ok (dou@127.0.0.1:22 via publickey)
```

此后 `dou.exec` 走真实通道。验证是否真在远端：

```powershell
python tools\probe_ssh.py     # cat /proc/version 应返回 Linux 内核版本
```

本机靶机用 WSL 自建（Docker Hub 在本机网络下不可达，别试）：

```powershell
wsl -d Ubuntu-22.04 -u root --exec sh -c "systemctl start ssh; sleep 3600"
```

`sleep 3600` 不能省——WSL 实例一退出，`127.0.0.1:22` 立刻变拒绝连接。

### 连接失败的错误码

启动日志与 `dou.exec` 结果共用同一套码，看到码就能定位：

| 码 | 含义 | 先查什么 |
|---|---|---|
| `E_CONN_REFUSED` | 目标拒绝连接 | sshd 没起 / 端口不对 |
| `E_CONN_TIMEOUT` | 超时 | 网络、防火墙、墙外 |
| `E_AUTH_FAILED` | 认证被拒 | 公钥没进 `authorized_keys` / 口令错 |
| `E_KEY_LOAD` | 私钥读不了 | 路径、格式、私钥带口令 |
| `E_HOSTKEY_MISMATCH` | 主机公钥变了 | 换机器了？还是中间人？ |
| `E_KNOWN_HOSTS_CORRUPT` | known_hosts 损坏 | 修复或删除后重新确认 |
| `E_UNSUPPORTED_ALGO` | 算法不支持 | ring 后端不解析 RSA 私钥，换 ed25519 |

## 自检

```powershell
python tools\mcp_probe.py
```


不依赖任何 Agent 客户端，按 MCP 规范直接打网关，覆盖九项：握手、通知、工具清单、
只读放行、变更待审、夹带拦截、危险阻断、错令牌 401、审计落盘。

## 接入 Agent 客户端（等装好客户端后做）

WorkBuddy → `~/.workbuddy/mcp.json`：

```json
{
  "mcpServers": {
    "dou": {
      "type": "http",
      "url": "http://127.0.0.1:7419/mcp",
      "headers": { "Authorization": "Bearer <你的令牌>" }
    }
  }
}
```

Codex CLI → `~/.codex/config.toml`：

```toml
[mcp_servers.dou]
url = "http://127.0.0.1:7419/mcp"
http_headers = { Authorization = "Bearer <你的令牌>" }
tool_timeout_sec = 300
```

VS Code 的顶层键是 `servers` 不是 `mcpServers`，复制到别处会直接失效。

## 目录

```
dou/
  crates/dou-gateway/      网关本体
    src/main.rs            启动、路由、MCP 方法分发
    src/auth.rs            令牌签发与校验
    src/policy.rs          命令分段与三档判定
    src/tools.rs           五工具定义与执行
    src/ssh.rs             russh 适配器（建连、认证、exec）
    src/hostkey.rs         主机密钥 TOFU（known_hosts）
    src/errors.rs          SSH 错误码与可读提示
    src/audit.rs           JSONL 审计
    src/rpc.rs             JSON-RPC 响应构造
  tools/mcp_probe.py       端到端自检（协议与策略）
  tools/probe_ssh.py       SSH 链路端到端自检
  docs/                    架构决策与实测记录
```

## 已知限制（不要当成 bug）

1. `exec` 在 **没有 SSH 会话** 且 dev 模式时执行的是本机命令，不代表已连上远端服务器；有会话则一律走远端。
2. 设备令牌已支持吊销时间戳（`scope.rs::revoke_token`）；设备绑定仍未做。
3. 路径校验为字符串前缀匹配、仅覆盖 `read/put/get`；`dou.exec` 不做路径校验，仍是已知弱项。
4. 长命令未任务化：超时后远端进程会游离，`tool_timeout_sec` 之外的重复执行风险仍在。
5. **主机密钥**：首次连接记录公钥全文到 `%LOCALAPPDATA%\dou\known_hosts.json`，之后逐字比对；不一致即拒连（`E_HOSTKEY_MISMATCH`），文件损坏亦拒连（`E_KNOWN_HOSTS_CORRUPT`，防止通过破坏文件绕过校验）。展示形式是公钥截断，**不是** OpenSSH 的 `SHA256:` 指纹。
6. 只读白名单已扩到 60+ 命令（`echo` / `whoami` / `uname` / `hostname` 均为 Auto），不再是瓶颈。
7. 编译前必须先停掉运行中的 `dou-gateway`，否则 `failed to remove dou-gateway.exe (os error 5)`；`scripts/gate.ps1` 与 `build.ps1` 已自动处理。

---

## 本轮升级：Windows 11 兼容 + 风控 + 状态/鉴权重构（2026-11-12）

> 设计与验证记录见 `docs/handover/04`~`06`；兼容矩阵见 `docs/WINDOWS-COMPAT.md`。

### 行为变更（务必注意）

1. **所有 REST 端点需要 Bearer**，仅 `GET /health` 豁免且只返回 `{"status":"ok"}`。此前 13 条管理端点裸奔，现由 `authz::require_auth` 单层中间件统一收口。
2. **`DOU_DEV_MODE` 默认关闭**：不显式设 `=1` 时不再执行本机命令兜底。
3. **启动日志、控制台、接入向导不再回吐完整令牌**：改为 `dou_ak_****abcd` 掩码，`/onboard/config` 片段用 `${DOU_TOKEN}` 占位；真实令牌只由 `/onboard/write` 在本地代写。
4. **仓库已 git 化**：`.gitignore` 排除凭据/审计/产物/工作区数据；全仓真实令牌字面量已清零；`tools/*.py` 改为 `DOU_TOKEN` 运行时注入。

### 新增能力

- **防暴力破解**：滑动窗口失败计数 + 指数退避临时锁定（429 + `Retry-After`），状态落 `auth_throttle_state` 表、重启不丢；错令牌按自身哈希自指，无法冻结合法主体。阈值由 `DOU_THROTTLE_*` 配置。
- **凭据加固**：OS CSPRNG 替换 xorshift；`sha256(盐‖明文)` 替换 `DefaultHasher`；`subtle` 恒定时间比较；`/approve`、`/scope/*`、`/onboard/*`、`/console`、`/audit/export` 为 M-only。
- **W-11 统一状态模型**：`DbPool` 单一连接归口；新增 `task_state` / `quota_state` / `lease_state` / `session_meta` / `auth_throttle_state` 五表；任务写穿、重启恢复；时间单位统一 epoch 毫秒（旧库幂等迁移）。`sessions` 含 `russh::client::Handle`，仍只作运行时句柄缓存，重启需重连。
- **Windows 兼容**：GNU / MSVC 双工具链均编译通过；`build.ps1` 自动探测 MinGW 并支持 `-Toolchain`；新增 `scripts/gate.ps1`（构建+测试+clippy 一键）；`.gitattributes` 统一换行；脚本用纯 ASCII 规避 PowerShell 5.1 编码问题。

### 自检

```powershell
.\scripts\gate.ps1
$env:DOU_DEV_MODE="1"; D:\dou-target\debug\dou-gateway.exe
# 另一个终端，令牌从本地 token.txt 获取后注入
$env:DOU_TOKEN="<运行时令牌>"
python tools\mcp_probe.py
python tools\test_auth_matrix.py
python tools\test_throttle.py
```
