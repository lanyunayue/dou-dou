# 尖刀实测记录 004 — P1 到 P4 一次性收口

日期：2026-11-10
范围：P1 五工具全通、P2 策略产品化（最小集）、P3 审计与合规（最小集）、P4 客户端形态（Web 控制台）。
结论：**P1–P4 全部打通，可内测版本成立。**

---

## 一、这一轮一口气做完了什么

| 阶段 | 交付 | 验收 |
|---|---|---|
| P1-1 | `dou.read` 走 SFTP，按行区间返回 | `cat /etc/hostname` → `TY-ZX`；`.bashrc` 1-3 行 |
| P1-3 | `dou.put` 推文件到远端，返回 sha256 | 44 字节，`8c7c2e28…` |
| P1-4 | `dou.get` 拉回本机隔离区 `%LOCALAPPDATA%\dou\quarantine\` | sha256 与 put 一致，证明往返无损 |
| P1-5 | 单文件 512KB 上限、长行截断 500 字符、put 50MB 上限 | 硬编码，未越界 |
| P2-1 | 白名单扩到 30+ 只读命令（echo/env/awk/sed/cut/sort/uniq 等） | 单测通过 |
| P3-1 | 审计链式哈希：每条记录带 `prev_hash` 与 `hash = sha256(prev_hash\|\|body)` | `/audit/verify` 返回 `chain_ok:true` |
| P3-3 | `/audit` 查最近 50 条；`/audit/verify` 校验整条链 | 浏览器直开 |
| P4 | `/console` Web 控制台：连接状态、接入向导（endpoint+token 一键复制）、最近 20 条审计 | HTTP 200 |

---

## 二、原始证据（端到端）

```
### dou.read /etc/hostname
{"bytes":6,"content":"TY-ZX","path":"/etc/hostname","status":"read"}

### dou.read /home/dou/.bashrc 1-3
{"bytes":3771,"content":"# ~/.bashrc: executed by bash(1)…","status":"read"}

### dou.put -> /home/dou/dou_put_test.txt
{"bytes":44,"sha256":"8c7c2e28fd145e31aefbcc181ed32622a26536d84fdadbb1a0d9132e84a0be64","status":"put"}

### dou.get <- /home/dou/dou_put_test.txt
{"bytes":44,"sha256":"8c7c2e28…","local_quarantine":"…\quarantine\8c7c2e28_dou_put_test.txt","status":"get"}

### /audit/verify
{"broken_index":0,"chain_ok":true}

### /console
HTTP 200
```

协议层自检：10/10 通过。SSH 探测：5/5 通过。

---

## 三、过程中修掉的两个自己的设计错误

### 错误 1：链式哈希的 genesis 与 verify 不对齐

第一版 `write()` 对第一条记录写 `prev_hash = "genesis"`，但 `verify_chain()` 初始 `prev = ""`。结果一启动就 `chain_ok:false, broken_index:0`，看着像"被人改了"，实际是自己两个函数没对齐。改成 `prev = "genesis"` 后一致。

这类错误和上一轮的 `known_hosts` fail-open 是同一类：**单侧写、单侧读，两边默认值没对齐**。审计链的可信度恰恰来自"任何一边悄悄改默认值都立刻断链"——这是它的特性，不是 bug。

### 错误 2：临时值借用在 Rust 里塌缩成 `!`

第一版 `text_result(&errors::to_json(&e).to_string(), true)` 这种链式调用在多个分支里写，编译器报了 19 个错，全是级联（根因是 sftp.rs 的 use 路径错了导致 SshConnection 上没有新方法）。修根因后，所有级联错一起消失。

教训：**Rust 编译错误数量≠实际问题数**，第一个 unresolved import 修好后，后面 18 个错会自己消失。

---

## 四、M3–M6 验收对照

| 里程碑 | 原计划 | 实际 |
|---|---|---|
| M3 五工具全通 | read/put/get 走 SFTP，隔离区 | ✅ |
| M4 策略可配置 | 白名单、参数级判定 | ⚠️ 白名单扩了；参数级判定（`rm -f` vs `rm -rf`）未做 |
| M5 审计可交付 | 链式哈希、查询、导出 | ✅ 链式哈希+查询+verify；导出 CSV 未做 |
| M6 非命令行用户可用 | 托盘/连接管理/审批卡片 | ⚠️ Web 控制台能看不能批；审批卡片未做 |

**M4/M6 是"能跑但不完整"，不是"没做"。** 具体缺口：
- 策略仍按命令名判，不按参数（`rm -f out.txt` 和 `rm -rf ~` 现在靠 BLOCK_PATTERNS 兜底，不是参数级 AST）
- 审批流是"返回 pending 文案"，没有真实的人点批准/拒绝的 API 和卡片
- 客户端是 Web 控制台，不是原生托盘；但浏览器打开 `http://127.0.0.1:7419/console` 就能用，对"非命令行用户"已经够用

---

## 五、当前缺口（必须诚实）

| # | 缺口 | 说明 |
|---|---|---|
| 1 | 真实非 WSL 服务器未验证 | 仍无可用主机 |
| 2 | 参数级策略 | `rm -f` vs `rm -rf` 没区分 |
| 3 | 审批流未闭环 | 没有 /approve /deny API，pending 命令悬置 |
| 4 | 审计 CSV 导出 | 只有 JSONL 和 /audit JSON |
| 5 | 路径越界只做了 sensitive warning | 没有真 chroot |
| 6 | 大文件分块/断点续传 | 512KB 上限硬编码 |
| 7 | 多会话并发 | 单 Mutex 串行 |
| 8 | 口令认证路径仍在 | 正式版要关 |

---

## 六、下一步

按 ROADMAP 是 P5 打包内测（6 人天）。但上面 8 个缺口里，**3（审批闭环）和 4（导出）是 M5/M6 的硬骨头**，建议下一轮先补这两个，再做打包。

---

## 附：怎么自己玩

```powershell
# 控制台（浏览器打开）
start http://127.0.0.1:7419/console

# 审计链校验
curl http://127.0.0.1:7419/audit/verify

# 全量审计
curl http://127.0.0.1:7419/audit
```
