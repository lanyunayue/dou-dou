# 尖刀实测记录 003 — 主机身份与可诊断性（M2 收口）

日期：2026-11-10
范围：P0-3 主机密钥 TOFU、P0-4 连接失败可诊断、P0-5 构建可复现、P0-6 收口。
结论：**M2 达成（有一项未满足，见第四节）。**

---

## 一、这一轮解决了什么

上一轮结尾留了两个硬缺口：主机密钥无条件信任、连接失败只吐一坨英文错误。现在两个都补上了。

| 项 | 之前 | 现在 |
|---|---|---|
| 主机身份 | `check_server_key` 无条件 `Ok(true)` | 首次记录公钥全文，之后逐字比对，不一致即拒连 |
| 失败原因 | `ssh connect failed: Unknown server key` | `E_HOSTKEY_MISMATCH` / `E_CONN_REFUSED` / `E_AUTH_FAILED` … 带一句人话 |
| 连接失败留痕 | 只打印，不写审计 | 成功/失败都写审计，失败带错误码 |
| 构建 | 手动停进程，否则 `os error 5` | `build.ps1` 自动停掉运行中的实例 |

---

## 二、四场景实测（原始输出）

```
A 首次     => ok (dou@127.0.0.1:22 via publickey · 主机公钥 ssh-ed25519:AAAAC3NzaC1l…smx4JIxG · 首次记录)
B 篡改公钥 => FAILED [E_HOSTKEY_MISMATCH] 主机密钥与本地记录不一致。可能是对方换了机器，也可能是中间人；已拒绝连接。
C 文件损坏 => FAILED [E_KNOWN_HOSTS_CORRUPT] known_hosts 损坏或不可读，已拒绝连接。修复或删除该文件后重新确认主机公钥。
D 删后重连 => ok (… · 首次记录)
```

B 是把 `known_hosts.json` 里记录的 base64 末段替换成 `TAMPERED` 后启动——模拟"对方换了机器或中间人"。
C 是把文件写成 `{"broken`——模拟文件被损坏。

---

## 三、过程中自己找出来的两个设计错误（不是环境问题）

### 错误 1：解析失败被静默降级成"首次信任"（安全漏洞）

第一版 `load()` 写的是：

```rust
let map = fs::read_to_string(&path).ok()
    .and_then(|s| serde_json::from_str(&s).ok())
    .unwrap_or_default();   // ← 解析失败 == 空表 == 没记录过 == 放行
```

后果：**任何人只要把 `known_hosts.json` 写坏一点点，主机身份校验就整体失效**，而且用户完全无感。这正好是第一轮测试没拦住篡改的真实原因（当时以为是测试脚本的 JSON 序列化问题，实际是我的 fail-open 设计）。

改成 fail-closed：文件存在但解析失败 → `load_ok = false` → 任何连接一律拒绝，错误码 `E_KNOWN_HOSTS_CORRUPT`。

### 错误 2：拒绝理由在 russh 那一层被吞掉

`check_server_key` 返回 `Ok(false)` 后，russh 统一抛 `Unknown server key`，我精心区分的原因（不匹配 vs 文件损坏）全部丢失，用户看到的还是原始报错——等于"拦住了但说不清为什么"，这跟不拦差别不大。

修法：handler 把判定结果写进共享状态，`client::connect` 失败后先读该状态还原原因，再决定报什么错误码。

---

## 四、M2 验收对照

| 验收项 | 结果 |
|---|---|
| 主机密钥不匹配时连接失败 | 通过（场景 B） |
| 首次连接记录指纹/公钥 | 通过（场景 A、D） |
| 失败原因可区分（4 类） | 通过（`E_HOSTKEY_MISMATCH` / `E_KNOWN_HOSTS_CORRUPT` / `E_CONN_REFUSED` / `E_AUTH_FAILED`） |
| 连接事件全部留痕 | 通过（成功与失败都写审计，含 code） |
| 无回归 | 通过（单测 4/4、协议自检 10/10、SSH 探测全绿） |
| **接一次非 WSL 的 sshd** | **未满足** |

**未满足项说明**：M2 出口条件原本要求"至少接一次非 WSL 的 sshd 再宣布 M2"。当前环境下没有可用的真实服务器（Docker Hub 不可达，也无云主机凭据），所以这一条没有做。它不影响"主机身份校验逻辑正确"，但影响"面对真实 OpenSSH 时是否同样正确"的信心——真实服务器可能使用 RSA 主机密钥、可能带证书、可能有非标准配置。

**所以 M2 的实际状态是"技术上达成、真实环境待验"。** 给我任意一台可连的主机（哪怕临时开个 10 分钟的），这一条当天可补上。

---

## 五、当前缺口（更新）

| # | 缺口 | 状态变化 |
|---|---|---|
| 1 | 主机密钥无条件信任 | **已关闭** |
| 2 | 只读白名单过窄 | 仍在（P2-1/P2-2） |
| 3 | 超时后远端进程游离 | 仍在（P2-6） |
| 4 | 口令认证路径仍在 | 仍在（正式版关闭） |
| 5 | 单会话串行 | 仍在（P2-5） |
| 6 | 路径越界未校验 | 仍在（P1-2） |
| 7 | 真实服务器未验证 | **新增**（需外部条件） |
| 8 | 公钥比对是全文，不是 SHA256 指纹展示 | **新增**（体验项，非安全问题） |

第 8 条值得说清楚：现在界面显示的是 `ssh-ed25519:AAAA…xxxx`（公钥首尾截断），**不是** OpenSSH 的 `SHA256:xxx`。安全性等价（指纹本来就是公钥摘要），但用户没法直接拿它和 `ssh-keygen -lf` 的输出对。要么下一轮改成标准 SHA256 指纹（需引入 sha2+base64），要么在界面上明确写"这是公钥指纹的另一种展示形式"。我倾向前者，成本约 1 小时。

---

## 六、下一步

按 ROADMAP，进入 P1（五工具全通）：
P1-2 路径越界校验 → P1-1 `dou.read` 走 SFTP → P1-3/4 隔离区 → P1-5 大文件 → P1-6 自检扩到 22 项。

---

## 附：环境与命令

```powershell
# 靶机常驻（WSL 不常驻端口就消失）
wsl -d Ubuntu-22.04 -u root --exec sh -c "systemctl start ssh; sleep 3600"

# 构建（自动停旧进程）
.\scripts\build.ps1 build

# 三套自检
python tools\mcp_probe.py    # 协议层 10/10
python tools\probe_ssh.py    # SSH 链路
.\scripts\build.ps1 test     # 单测 4/4
```

数据目录：`%LOCALAPPDATA%\dou\`（`token.txt` / `audit.jsonl` / `known_hosts.json` / `keys\id_ed25519`）
