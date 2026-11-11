# 尖刀实测记录 001：网关协议层与策略层

- 日期：2026-11-10
- 范围：计划书 4.5（MCP 工具集）、4.8（凭据与授权）、4.9（审批语义）、4.10（接入边界）中的**协议层与策略层**
- 不在范围：终端渲染、SFTP、编辑器、russh 真实 SSH 通道、隔离区与静态扫描

## 一、结论先行

| 待验证项 | 结论 | 证据 |
| --- | --- | --- |
| MCP Streamable HTTP 协议可实现 | **成立** | initialize / notifications / tools/list / tools/call 全通，10/10 自检 |
| 只绑回环、非回环拒绝启动 | **成立** | 启动即校验 bind 地址 |
| Bearer 令牌鉴权可用 | **成立** | 错令牌返回 401 |
| 五工具集边界可控 | **成立** | tools/list 只返回五个，无多余能力 |
| 三档策略判定可用 | **成立（有缺口）** | 只读放行 / 变更待审 / 危险阻断，按命令段判；路径越界未校验 |
| 审计可落盘 | **成立** | JSONL 只追加，12 条记录可复现 |
| 私钥代签可行 | **未验证** | 需 russh + 真实 sshd |
| 客户端 task 扩展支持度 | **未验证** | 本机未装 WorkBuddy / Codex，需真机接入 |

一句话：**协议层不是风险，策略层有可修补的缺口，真正的未知仍是 SSH 代签。**

## 二、环境实测（本机 Windows）

| 项 | 实测前 | 处置 | 结果 |
| --- | --- | --- | --- |
| Rust | 无 | `winget install Rustlang.Rustup` | 已装（rustup 1.29.3） |
| MSVC 链接器 | 无 | `winget install VCTools` | **失败**，退出码 1602——当前进程为 Medium 完整性，UAC 无法在后台确认 |
| MinGW 替代 | 无 | `winget install BrechtSanders.WinLibs.POSIX.UCRT --scope user` | 成功，纯用户范围，不需要管理员 |
| WebView2 | 157.0.4341.42 | — | 已存在（Tauri 阶段可省一次安装） |
| Node | v22.23.2 / npm 10.9.8 | — | 可用于 Tauri 前端 |
| WSL2 | Ubuntu-22.04（Stopped） | — | 可作下一步 sshd 测试环境 |
| Docker | 29.6.1 | — | 可用 `linuxserver/openssh-server` 起测试机 |
| OpenSSH 客户端 | 9.5p2 | — | 可对照原生 ssh 行为 |

**这一条要记进计划书**：Windows 上从零到能编译 Rust，如果拿不到管理员权限，MSVC 路线会卡死；MinGW 路线可行但会引入两个坑（见下）。这对"单人 + 无 IT 支持"的实际处境是重要事实。

## 三、踩到的三个坑（都会复现在别人机器上）

### 坑 1：UAC 挡住 VS Build Tools

后台安装返回 1602（用户取消）。不是参数错，是非管理员进程无法确认 UAC。
**影响**：官方推荐的 MSVC 工具链在无管理员场景下不可用。
**处置**：先走 MinGW，把 MSVC 作为"拿到管理员权限后再切"的可选项。

### 坑 2：gnu 工具链缺 dlltool

`windows-sys` 编译时报：
```
error: error calling dlltool 'dlltool.exe': program not found
```
rustup 的 gnu 工具链自带 `rust-mingw`，但**不带 binutils 的 dlltool**。
**处置**：装 MinGW（含 dlltool/gcc/ar），并在构建时显式加入 PATH。

### 坑 3：中文路径把 dlltool 打断（这条最关键）

项目目录是 `E:\思考\dou`。编译报：
```
dlltool.exe: Can't create .lib file: E:\˼��\dou\target\...\ntdll.dll_imports.lib: No such file or directory
```
原因是 rustc 以 UTF-8 传路径，MinGW 的 dlltool 按 ANSI 代码页解释，`思考` 变成 `˼��`，目录找不到。

**处置**：`.cargo/config.toml` 里把 `target-dir` 指到纯 ASCII 路径 `E:/dou-target`。源文件留在中文目录不受影响——只有传给链接器的产物路径必须 ASCII。

**这个坑对产品本身也是提醒**：中文用户名、中文目录在中国用户里很常见。将来打包 Tauri、调用外部工具（ClamAV、签名工具）时，同类路径编码问题会反复出现。应当作为兼容矩阵的一行提前测，而不是等用户报。

## 四、实现与验证

### 4.1 编译

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 48.87s
```
一次通过，仅一个 `dead_code` 警告（已清理）。

### 4.2 单元测试（4/4）

```
test policy::tests::read_only_is_auto ... ok
test policy::tests::mutating_is_pending ... ok
test policy::tests::chained_smuggling_is_caught ... ok
test policy::tests::destructive_is_blocked ... ok
```

**测试抓到的真问题**：初版用整串子串匹配，`ls; rm -rf ~` 会因为包含 `rm -rf ~` 直接被判阻断，而不是按段判成待审；同时 `ls; rm -f out.txt` 这类只读前缀 + 变更后缀会被整串放行。改成分段判定后两条都正确。这正是计划书 4.10 表里"命令不做字符串匹配而做结构解析"那条的实际价值，不是理论洁癖。

### 4.3 端到端自检（10/10）

```
PASS  回环端口可达  ·  127.0.0.1:7419
PASS  initialize 握手  ·  session=t18e... version=2025-06-18
PASS  notifications/initialized 返回 202
PASS  tools/list 五个工具齐全
PASS  只读命令被判为放行  ·  exit_code=0, stdout="ty-zx\\mrhua"
PASS  变更命令进入待审批  ·  segment `rm -f out.txt` not in whitelist
PASS  只读+变更夹带未被自动放行
PASS  提权删除被阻断  ·  matched block pattern `rm -rf /`
PASS  错令牌被拒  ·  status=401
PASS  审计文件可写且非空  ·  12 条
```

自检脚本不依赖任何 Agent 客户端，任何人 clone 后跑一条命令就能复现。

## 五、当前实现暴露的缺口（下一步必须补）

1. **路径越界未校验**：`cat /etc/shadow` 现在判为只读放行。计划书 4.10 要求"路径做真实路径校验"，目前只有命令层判定。
2. **令牌未绑定设备、未实现吊销时间戳**：现在一枚令牌可用到进程结束。
3. **dev 模式的 exec 在本机跑**：Windows 下 `ls` 这类 Unix 命令会失败并回显 GBK 乱码。这不是缺陷，但说明"跨平台执行语义"必须由真实 SSH 通道承担，不能靠本地 shell 兜底。
4. **长命令未任务化**：`tool_timeout_sec` 之外没有 task 句柄，重复执行风险仍在。

## 六、下一步（按风险排序）

| 优先级 | 动作 | 依赖 | 判定标准 |
| --- | --- | --- | --- |
| P0 | russh 接真实 sshd（Docker 起 linuxserver/openssh-server） | Docker 已可用 | 能连、能 exec、能读到 stdout/exit code |
| P0 | 私钥代签验证 | 同上 | 私钥只在本地库，代理由网关签名，代理侧拿不到原文 |
| P1 | 路径真实路径校验 | 无 | `cat /etc/shadow`、符号链接逃逸被拒 |
| P1 | WorkBuddy 真机接入 | 需安装客户端 | 点信任后 tools/list 成功，且记录是否支持 task 扩展 |
| P2 | 令牌吊销与设备绑定 | 无 | 吊销后下一次请求即被拒 |

---

**给计划书的修正建议**：4.2 技术栈应补一行"Windows 构建前置：MSVC 或 MinGW，中文路径需 target-dir 隔离"。附录 D 的兼容矩阵应加"中文用户名/中文安装目录"一行。
