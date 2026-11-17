# 公共测试靶机（免注册，豆 Dou 可直接连接）

> 性质：无需注册、无需信用卡、无需用户介入的**真实外部 SSH/SFTP 服务器**，用于在没有自建靶机时验证豆 Dou 的连接链路。
> 实测时间：2026-11-17，6/6 全通过。复跑脚本：`dou-workspace/tools/test_public_ssh.py`。

## 主选：test.rebex.net（Rebex 官方公开测试服务器）

| 项 | 值 |
|---|---|
| 主机 | `test.rebex.net`（实测 DNS → 194.108.117.16） |
| 端口 | `22` |
| 认证方式 | 密码 |
| 用户名 | `demo` |
| 密码 | `password` |
| 主机密钥指纹（TOFU） | `ssh-ed25519 SHA256:d7Te2DHmvBNSWJNBWik2KbDTjmWtYHe2bvXTMM9lVg4` |
| 支持协议 | SSH / SFTP / FTP / FTPS / IMAP / POP3 等 |

### 实测通过项（6/6）
1. DNS 解析
2. TCP + SSH 握手 + 密码认证
3. 主机密钥指纹（TOFU）读取——填进豆 Dou 首连确认即可对上
4. SSH keepalive 通道存活（对应 R12 会话保活）
5. SFTP 列目录（`/` → `pub/`、`readme.txt`）
6. SFTP 下载只读抽样（`/pub/example/readme.txt`，379 字节）

### 豆 Dou 里怎么填
- 新建连接：主机 `test.rebex.net`，端口 `22`，用户 `demo`，认证方式选**密码**，密码 `password`。
- 首次连接会弹 TOFU 指纹确认，核对上面那串 `SHA256:d7Te2...` 一致即可。
- 连上后可在终端执行只读命令、用文件面板浏览 SFTP（`/pub/example/` 下有示例文件）。

### 限制（务必知悉）
- **只读**：不能上传/删除/改名（拒绝写操作是预期行为，不是 bug）。
- **共享 + 限流**：仅供冒烟测试，不要接 CI 或高频轮询；用完及时断开，否则会触发 `usage quota exceeded`。
- **无 root、无 shell 自由执行**：测不了密码认证动线的底层配置、`/proc` 监控条真实数据、B3 symlink 逃逸等需要 root/真实 Linux 的用例。
- 远程依赖外网，离线环境不可用。

## 备选 A：项目内置的本地回环 SSH 靶机（离线、全功能）

`dou-workspace/tools/ssh-loopback` 提供 `dou-ssh-server`，专为"无外部 Linux 靶机时验证豆 Dou 远程主线"而写：

```bash
cd dou-workspace/tools/ssh-loopback
cargo build --bin dou-ssh-server --release
./target/release/dou-ssh-server --bind 127.0.0.1:2222
```
- 默认 `127.0.0.1:2222`，用 Git Bash 提供真实 shell，支持公钥认证、TOFU、SFTP、断点续传。
- `cargo run --bin dou-ssh-selfcheck` 自带 10/10 回环自校验（verify.sh 第 8 节）。
- 优点：完全离线、可写、可测 symlink/传输；缺点：不是公网真实服务器。

## 备选 B：免费云主机（需用户本人注册）

Oracle Cloud Always Free（ARM 2核12G 永久免费）是最接近真实生产环境的靶机，但注册需信用卡身份验证 + 手机验证，AI 无法代劳；且 2026 年 6 月后额度缩水、注册常遇 out of capacity。详见对话记录，此处不展开。

## 安全红线
- 新靶机一律用独立密钥/凭据，**绝不复用 224/221 服务器的任何凭据或 known_hosts**（224 处于 root 挖矿木马 containment）。
- 公共测试服务器只做只读冒烟；不要在公共服务器上存放任何真实数据/令牌。
