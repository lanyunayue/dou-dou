# 尖刀实测记录 002 — SSH 传输层接通

日期：2026-11-10
范围：`dou-gateway` 从"只能在本机跑只读命令"推进到"经真实 SSH 会话在远端执行，且凭据不出网关"。

---

## 一、结论

| 环节 | 状态 | 可复核证据 |
|---|---|---|
| 代理 → 网关（本地 MCP 回环） | 成立 | 记录 001，自检 10/10 |
| 网关 → 远端（SSH 密钥认证） | **成立（本轮新增）** | 启动日志 `ok (dou@127.0.0.1:22 via publickey)` |
| 命令真在远端执行 | **成立（本轮新增）** | `cat /proc/version` 返回 `Linux version 6.18.33.2-microsoft-standard-WSL2` |
| 策略判定在 SSH 路径上仍生效 | **成立（本轮新增）** | `ls; rm -rf ~` → `blocked`（未建立通道） |
| 审计留痕 | 成立 | `audit.jsonl` 31 条，`exec` 与 `exec_result` 成对，含 exit_code 与 trace_id |
| 私钥不出本地 | **结构成立** | 私钥在 `%LOCALAPPDATA%\dou\keys\id_ed25519`；MCP 五个工具的入参里没有任何凭据字段 |
| 主机密钥校验 | **未做** | `check_server_key` 目前无条件返回 `true` |

这条链路现在可以对外演示了：**代理发一条命令 → 网关判档 → 网关用本机私钥代连 → 远端执行 → 结果与判定一并写审计**。代理全程看不到任何地址凭据与私钥材料。

---

## 二、实测原始输出

启动（密钥认证）：

```
[dou-gateway]  │ token  <REDACTED_TOKEN>
[dou-gateway]  │ audit  C:\Users\mrhua\AppData\Local\dou\audit.jsonl
[dou-gateway]  │ dev    true
[dou-gateway]  ├ ssh    dou@127.0.0.1:22 … ok (dou@127.0.0.1:22 via publickey)
[dou-gateway]  └ bind   http://127.0.0.1:7419/mcp
```

经 MCP 发 `dou.exec`：

```json
// cat /proc/version
{"status":"executed_remote","host":"dou@127.0.0.1","exit_code":0,"timed_out":false,
 "stdout":"Linux version 6.18.33.2-microsoft-standard-WSL2 (root@f1bbfb02316b) (gcc (GCC) 13.2.0, GNU ld (GNU Binutils) 2.41) #1 SMP PREEMPT_DYNAMIC Thu Jun 18 21:54:43 UTC 2026\n",
 "stderr":""}

// ls; rm -rf ~
{"status":"blocked","reason":"matched block pattern `rm -rf ~`"}
```

`/proc/version` 在 Windows 上不存在，这条输出是"确实落在 Linux 里"的硬证据；而不是像 `hostname` 那样两边同名、证不了什么。

审计尾部（成对记录）：

```json
{"action":"exec","detail":{"command":"cat /proc/version","reason":"all segments read-only"},"verdict":"auto",...}
{"action":"exec_result","detail":{"command":"cat /proc/version","exit_code":0,"timed_out":false},"verdict":"done",...}
{"action":"exec","detail":{"command":"ls; rm -rf ~","reason":"matched block pattern `rm -rf ~`"},"verdict":"blocked",...}
```

单测：`4 passed; 0 failed`。

---

## 三、本轮踩的四个坑

1. **Docker Hub 拉不动** —— `docker run linuxserver/openssh-server` 在 `registry-1.docker.io:443` 超时。容器靶机这条路在本机网络下不通，直接放弃，改用 WSL 自建 sshd。这条要写进交付文档，否则下一个接手的人会再撞一次。
2. **WSL 实例一退出，端口就消失** —— 用 `wsl --exec` 起完 sshd 命令返回后，WSL 停止，Windows 侧 `127.0.0.1:22` 立刻变成"积极拒绝"。必须留一个前台常驻进程（这里用 `sleep 3600`）把实例顶住。同时发现 apt 安装时 systemd 已经把 sshd 拉到 **22 端口**（不是我以为的 2222），`-p 2222` 那次其实没生效。教训：起完服务要 `ss -ltn` 看**实际**监听，不要相信自己传的参数。
3. **目标 exe 被运行中的实例占用** —— `error: failed to remove file E:/dou-target\debug\dou-gateway.exe: 拒绝访问 (os error 5)`。改代码后编译前必须先 `Stop-Process dou-gateway`。这条会反复出现，值得写进构建脚本。
4. **关联函数写成自由函数** —— `E0599: no associated function named connect_key found for struct SshConnection`。修法是把调用改成 `ssh::connect_key(...)`，或把函数移进 `impl` 块；我选前者，保持"连接"与"执行"分层。

另有一个**编译期陷阱**值得记：russh 默认后端 `aws-lc-rs` 需要 CMake + MSVC 编 C 代码，在本机 MinGW 环境基本必挂。改成 `default-features = false, features = ["ring"]` 后，锁定包数从 153 降到 13，编译一次通过。

---

## 四、当前缺口（按风险排序）

| # | 缺口 | 后果 | 处理计划 |
|---|---|---|---|
| 1 | 主机密钥无条件信任，无 known_hosts / 无指纹确认 | 中间人可劫持；当前只连本机回环，风险暂时受控，一旦连真实服务器就是硬伤 | 下一轮接 TOFU：首次存指纹并要求 UI 确认，之后不匹配即拒连 |
| 2 | 只读白名单过窄（`echo`、`whoami`、`uname`、`hostname` 均判 pending） | 真实用户的日常只读命令会大量卡在审批，可用性差，容易被用户直接关掉策略 | 白名单补 `uname/id/wc/stat/date/which`，并升级为"命令+参数"判定 |
| 3 | 超时后远端进程游离 | 长命令（训练、编译）会留下孤儿进程，用户以为被取消了 | 按计划书 4.9 做任务化：返回 job id 而非阻塞等待 |
| 4 | 口令认证路径仍在 | 与"私钥不出本地"的卖点自相矛盾 | 正式版默认关闭，仅保留密钥 + Agent |
| 5 | 单会话串行（一把锁） | 恰好等于默认配额"1 并发"，但没有任何并发上限的可配置实现 | 配额层接入，按项目配置并发数 |
| 6 | 路径越界未校验（继承自 001） | `cat /etc/shadow` 会判只读放行，靠远端权限兜底 | 与白名单改造同批做 |

---

## 五、下一步

1. **known_hosts + 指纹确认**（上面第 1 条，风险最高，先做）
2. `dou.read` 接 SFTP（russh 自带 `sftp_client.rs` 示例，路径已确认可用）
3. 白名单与参数级判定重写（这是"策略"卖点的真正门槛，现在太粗）
4. `put`/`get` 的隔离区与哈希

---

## 附：复现步骤

```powershell
# 1. 靶机（WSL Ubuntu 22.04，root 免密进入）
wsl -d Ubuntu-22.04 -u root --exec sh -c "systemctl start ssh; sleep 3600"   # 必须前台常驻
Test-NetConnection 127.0.0.1 -Port 22                                        # 应为 True

# 2. 密钥
ssh-keygen -t ed25519 -f "$env:LOCALAPPDATA\dou\keys\id_ed25519" -N ''
# 公钥写入 /home/dou/.ssh/authorized_keys，chmod 700/600

# 3. 起网关
$env:DOU_SSH="dou@127.0.0.1:22"
$env:DOU_SSH_KEY="$env:LOCALAPPDATA\dou\keys\id_ed25519"
E:\dou-target\debug\dou-gateway.exe

# 4. 探测
python E:\思考\dou\tools\probe_ssh.py
```
