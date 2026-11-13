# Xftp 竞品研究 & 豆 Dou 对比总结

> 研究来源：① `D:\xftp` 安装目录（可执行与 DLL 分析）；② `Xftp7_en.pdf` 官方手册；③ 豆 Dou 源码 `E:/dou-app/src-tauri/src/pty.rs` 与 `App.vue`。
> 日期：2026-11-12
> 目的：学习成熟 SFTP 客户端的工程化与产品能力，对照豆 Dou 找差距与可借鉴点。

---

## 0. 结论速览
- **Xftp（NetSarang）**：商业闭源，**纯 SFTP/FTP 文件传输客户端**（终端是兄弟产品 Xshell 的事，Xftp 专注传输）。
- **豆 Dou**：带「治理网关」的**智能体工作台**——终端 + 文件树 + 传输 + 审批/审计/接入向导 集成在同一壳里。
- 两者在「远程文件操作」上高度重叠，但**定位不同**：Xftp 深在*纯传输工程化*，豆 深在*受治理的远程操作*。
- **判断**：豆 不必去卷纯 FTP 功能，应借鉴 Xftp 的**传输工程化（队列/续传/同步）**与**安全底座（主机密钥/凭据保险库）**，守住自己「受治理」的差异化主线。

---

## 1. Xftp 模块架构（从 DLL 反推，对豆 有借鉴价值）

| 模块 | 作用 | 对豆 的启示 |
|------|------|-------------|
| `nssftp.dll` / `nsssh3.dll` / `nssock2.dll` / `crypto-50.dll` | 分层的网络/加密栈（SFTP·SSH·Socket·Crypto 各自独立） | 豆 当前 russh/sftp 调用散在 `pty.rs`/`sftp.rs`/`transfer.rs`；可抽成可独立测试的「连接与传输内核」 |
| `Xtransport.exe` | **后台传输引擎**，与 UI 解耦，支撑队列/暂停/续传/调度 | 豆 的 `download_resume` 仅是雏形；应升级为独立 `TransferService`，UI 只发指令 |
| `Xagent.exe` | **SSH 密钥代理**，缓存/解锁密钥，跨 Xshell/Xftp 互操作 | 豆 每次从磁盘读 `id_ed25519`；可加密钥代理避免重复解锁 |
| `CrashRpt.dll` / `CrashSender.exe` | 崩溃上报 | 豆 **零可观测性**，至少应补结构化日志 + 崩溃转储 |
| `LiveUpdate.exe` / `LiveUpdate.dat` | 自更新 | 豆 是 Tauri，可启用 built-in updater |
| `FNP_Act_Installer.dll` / `nslicense.dll` / `nsactivate.dll` | FlexNet 授权体系 | 豆 用自有网关令牌体系，无需照搬 |
| `fido2.dll` | FIDO2 / WebAuthn 硬件密钥 | 豆 仅单 ed25519 文件密钥，可补硬件密钥 |
| `SessionConverterXftp.exe` | 会话/配置迁移 | 豆 可加会话导入导出 |
| `Languages/` | **30 个本地化资源 DLL**（10 语言 × 3 模块：Xagent/Xftp/Xtransport，含简体中文 CHS） | 豆 中文优先；若出海，借鉴「按模块拆分的多语言资源」i18n 框架 |

---

## 2. Xftp 关键能力（来自官方手册）

**协议与连接**：SFTP(SSH2)、FTP、FTPS、SCP；**FXP**（服务器间直传）；**Kerberos**(MIT/SSPI)；被动/主动模式；多样代理；多用户；keep-alive；UTF-8。

**会话管理**：会话文件、快速连接栏（Quick Connect Bar）、会话快捷方式、链接栏、默认会话、URL 命令、自动重连上次会话、会话树、导入/导出。

**安全**：RSA/DSA/ECDSA/ED25519 公钥 + 密码 + 键盘交互认证；新密钥向导与导入导出；**PKCS#11**（硬件加密狗）；多种加密(AES/3DES/BLOWFISH/…)与 MAC(SHA1/MD5/…)；zlib 压缩；**主机密钥管理**（Host Keys 对话框 + SSH 安全警告）；**主密码保险库**（跨 Xmanager/Xshell/Xftp 统一加密会话凭据）。

**外观**：可停靠 UI、主题切换、多标签/多标签组、可定制工具栏。

**文件传输**：并发传输、暂停/续传、传输队列（状态 Ready/Transferring/Paused/Finished）、**文件夹同步**（双向/单向/镜像，按修改时间或大小比较）、图形化传输状态。

**文件管理**：**直接编辑远程文件**（本地编辑器改完自动回传）、重命名/删除/新建/执行/改权限、保持符号链接/权限。

**自动化**：**定时传输调度**（Scheduled Transfer，按时间收发）、传输队列批量操作。

**互操作**：与 Windows 资源管理器、Xshell、Xagent 互通。

---

## 3. 与豆 Dou 对比

### 3.1 相似点（重叠区）
- 都做 **SFTP 远程文件操作**：文件树浏览、上传/下载、终端/Shell。
- 都支持 **ED25519 公钥认证**（豆 用 `id_ed25519`；Xftp 支持 ED25519 并含密钥向导）。
- 都有 **会话/主机** 概念与本地文件管理。
- 都面向 **Windows**（Xftp 还覆盖 macOS；豆 用 Tauri 理论上跨平台）。

### 3.2 豆 有而 Xftp 无（差异化优势，要守住）
| 能力 | 说明 |
|------|------|
| **治理网关** | 统一 Bearer 鉴权、令牌不落前端、fail-closed、防爆破节流/锁定 |
| **审批工作流** | 命令白名单拦截 → 待审任务 + 拒绝理由留痕（`/tasks`、`audit.jsonl`） |
| **审计日志** | 全量操作留痕（auth_throttled / connect / deny …） |
| **接入向导** | 自动生成 MCP 配置片段（含 Bearer），客户端可复制/一键改写 |
| **七维作用域令牌** | 设备令牌 vs 主令牌的 M-only 端点隔离 |
| **内置终端 + 文件树 + 传输同壳** | Xftp 的终端是兄弟产品 Xshell，豆 一体化更顺 |
| **现代技术栈** | Rust + Tauri + Vue，体积小、跨平台潜力 |

### 3.3 Xftp 有而豆 无（能力差距，按价值排序）
| 能力 | 豆 现状 | 价值 |
|------|---------|------|
| **主机密钥校验** | `check_server_key` 直接 `Ok(true)`，**不校验** → MITM 风险 | 🔴 安全必改 |
| **传输队列 + 可视化状态 + 暂停/续传** | 仅有 `download_resume` 雏形 | 🔴 可用性必改 |
| **文件夹同步**（双向/单向/镜像） | 无 | 🟠 高频需求 |
| **主密码/本地凭据保险库** | 仅占位符化，无本地加密保险库 | 🟠 安全 |
| **直接编辑远程文件自动回传** | 有编辑器，是否自动回传待确认 | 🟠 |
| **定时传输调度** | 无 | 🟡 |
| **PKCS#11 / FIDO2 / Kerberos 认证** | 仅单 ed25519 文件密钥 | 🟡 |
| **会话导入导出 / 快速连接栏 / 多标签组 / 可停靠 UI / 主题** | 侧栏固定，UI 较朴素 | 🟡 |
| **FTP/FTPS/FXP 协议广度** | 仅 SFTP | 🟡 非主线 |

---

## 4. 安全专项：主机密钥校验（必改项）
豆 `src-tauri/src/pty.rs:28`：
```rust
async fn check_server_key(&mut self, _key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
    Ok(true)   // 盲目信任任何主机密钥 —— MITM 风险
}
```
Xftp 对应能力：Host Keys 对话框 + SSH 安全警告（首次连接指纹确认、known_hosts 等价）。
**建议**：实现主机密钥指纹确认 + 本地 known_hosts 持久化（`~/.dou/known_hosts`），首次连接弹窗让用户确认 SHA256 指纹，之后严格比对；拒绝变更（防中间人）。这是接外部 Linux 靶机前的必做安全项（否则隧道把流量送出去却不验服务端身份）。

---

## 5. 给豆 的学习清单（优先级）

### P0 —— 安全与可用性地基（接外部靶机前必做）
1. **主机密钥校验**：`known_hosts` + 首次指纹确认弹窗（防 MITM）。见 §4。
2. **传输工程化**：把 `sftp_*` / `transfer_*` / `download_resume` 抽成独立 `TransferService`（参考 Xtransport），UI 仅发指令，支持**队列 + 暂停 + 续传 + 图形化状态**。
3. **可观测性**：参考 `CrashRpt`，加结构化日志 + 崩溃转储（至少把 `eprintln!` 的 pty/transfer 错误落盘到审计目录）。

### P1 —— 差异化功能（拉开体验）
4. **文件夹同步**：双向/单向/镜像 + 按修改时间或大小比较（Xftp §8.3）。这是 Xftp 最高频亮点，豆 目前完全缺失。
5. **直接编辑自动回传**：确认/实现远程文件本地改完保存即回传。
6. **密钥代理 + 主密码保险库**（参考 Xagent + Master Password）：密钥/令牌本地加密缓存，跨会话解锁一次。
7. **SFTP 属性保持**：权限/符号链接/UTF-8（Xftp 基础能力，豆 补齐）。

### P2 —— 广度（按需）
8. FIDO2/PKCS#11/Kerberos 认证；定时传输调度；会话导入导出、快速连接栏、多标签组、主题；若出海则上多语言 i18n 框架（参考 Xftp 的模块级资源 DLL）。

---

## 6. 一句话建议
> Xftp 用 20+ 年把「纯文件传输」做到了工程化极致；豆 Dou 的护城河是「**受治理的远程操作**」（审批/审计/接入/风控）。不要在 FTP 功能广度上和它正面对卷，而是把它的**传输工程化（队列/续传/同步）**和**安全底座（主机密钥/凭据保险库）**搬过来，补强自己主线的「远程操作」可靠性。

---

## 附：证据索引
- 安装目录：`D:\xftp`（Xftp.exe / Xftpcl.exe / Xagent.exe / Xtransport.exe / nssftp.dll / nsssh3.dll / fido2.dll / FNP_Act_Installer.dll / CrashRpt.dll / LiveUpdate.exe / Languages/ 共 30 个本地化 DLL）
- 手册：`D:\xftp\Xftp7_en.pdf`（章节 2.3–2.9、3.1 Key Functions、5 会话管理、6 连接、7 安全、8 文件传输、9 文件管理）
- 豆 源码：`E:/dou-app/src-tauri/src/pty.rs:28`（主机密钥未校验）、`E:/dou-app/src/tauri/src/lib.rs`（open_pty/sftp_* 命令）、`E:/dou-app/src/App.vue`（SSH_HOST/USER/KEY 常量、文件树与编辑器）
