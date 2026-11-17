# 模拟任务验证报告 — 通过 dou 网关操作真实外部服务器

> 时间：2026-11-17 ｜ 结果：**7/7 全通过** ｜ 可复跑：`python tools/run_simtask.py`
> 目的：不靠裸工具（paramiko），而是让用户实际使用的 **dou 软件本身**去连外部服务器、执行受治理任务，验证真实链路。

## 1. 拓扑与模式

```
Python 编排器(run_simtask.py)
   │ subprocess.Popen 启动，自管理生命周期
   ▼
dou-gateway.exe（真实远程模式：dev=false）
   │ DOU_SSH=demo@test.rebex.net:22  DOU_SSH_PASS=password
   │ russh 真实连接 + TOFU 主机密钥记录（首次记录/与记录一致）
   ▼
test.rebex.net:22（外部公网 SSH/SFTP 服务器）
```

- 网关**默认 dev=false**——命令不会在本机兜底执行，必须真连远端；这是关键，保证结果真实。
- 鉴权：用 `%LOCALAPPDATA%\dou\token.txt` 的主令牌（master），`/mcp` 走 Bearer。
- 端口 `127.0.0.1:7419`（回环，符合网关 `is_loopback` 硬约束）。

## 2. 实测结果（7/7）

| # | 检查 | 结果 | 关键证据 |
|---|---|---|---|
| 1 | MCP initialize | ✅ | serverInfo dou-gateway 0.2.0 |
| 2 | MCP tools/list | ✅ | 5 工具：session_list/exec/read/put/get |
| 3 | dou.session_list | ✅ | `default` 会话 status=connected, user=demo |
| 4 | dou.exec(pwd) | ✅ | `status=executed_remote`, host=`demo@test.rebex.net`, stdout=`/\n`, exit 0 |
| 5 | dou.exec(whoami) | ✅ | `executed_remote`, stdout=`demo\n`, exit 0 |
| 6 | dou.read 越界路径 | ✅ | `/etc/passwd` 不放行（远端 Path not found / 策略拒绝），符合安全边界 |
| 7 | 审计链 | ✅ | `/audit/query` 查到 connect/exec 事件，命令带 `all segments read-only` 判定 |

网关启动日志确认：`ssh demo@test.rebex.net:22 … ok (via password · ssh-ed25519:AAAAC3NzaC1lYwNcYlE · 首次记录)`——真实 russh 握手 + TOFU。

## 3. 过程中验证到的 dou 内部机制（非缺陷，是安全特性）

- **A5 会话租约（30min TTL）**：反复启动后 default 会话命中"会话租约已超过 30 分钟，请重新配对接入"——真实状态机行为。`run_simtask.py` 每轮清理 `lease_state` 中过期的 default 行获得干净窗口。
- **B3 路径闸门**：相对路径被 `path_scope_denied`（"仅允许绝对路径"）；绝对路径越过允许根（`/home/demo`、`/tmp/`）也被拒。外部服务器上 `/etc/passwd` 不存在时返回 `E_SSH_UNKNOWN`，未泄漏任何内容。
- **策略判定**：`pwd`/`whoami` 判为 `all segments read-only` 放行；命令在远端执行后写 `exec_result` 审计 + 快照快照去重判断（duplicate=false）。
- **限流/鉴权**：错误令牌会被记为失败并可能触发 throttle；主令牌（与服务端 token 完全一致）才走 master 通道。

## 4. 已知边界（诚实记录）

- `test.rebex.net` 的 SFTP 是**受限命令集**（`cat` 不支持、目录多为只读空目录），适合验证连接/认证/TOFU/审计，不适合完整文件读写回放。
- 完整 shell 用例（自由读写、symlink 逃逸、监控条 `/proc` 数据）需用项目自带的本地回环靶机 `tools/ssh-loopback/dou-ssh-server`（Git Bash 真 shell）或自建 Linux 靶机（Oracle 免费 ARM）。

## 5. 复现命令

```bash
# 前置：网关已编译到 D:\dou-target\debug\dou-gateway.exe
python tools/run_simtask.py
# 期望：7/7 通过，网关自动启动/停止，无残留进程
```
