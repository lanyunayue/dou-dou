# 豆 Dou · 网关侧 M3 完整收口（spike-report-008）

> 日期：2026-11-10
> 范围：B 方案网关侧全部完成，M3 验证点闭环。

## 一、网关侧能力清单（全部实测通过）

| 能力 | 实现 | 验收 |
|---|---|---|
| MCP 协议 | 5 工具（session_list/exec/read/put/get） | 10/10 自检 |
| 三档策略 | auto 放行 / pending 审批 / blocked 阻断 | rm 触发 pending，rm -rf / 阻断 |
| 审批闭环 | 控制台一键批准 → tokio::spawn 后台执行 → done | exit=0 实测 |
| 双视图脱敏 | 6 类正则（私钥/AWS Key/Bearer/JWT/内网IP/.ssh路径） | 实测全中 |
| 隔离区扫描 | WSL clamav 真扫描 | `scan_engine=clamav-wsl, OK` |
| 路径范围 | 拒绝 /etc/shadow、.ssh、/root 等 | path_scope_denied 实测 |
| 后台任务化 | dou.exec background=true 转 task 句柄 | `background_task → done` |
| 审计链 | 链式哈希 prev_hash/hash | `/audit/verify` chain_ok:true |
| 审计导出 | CSV 六列 | `/audit/export` |
| 审计查询 | 最近 20 条 | `/audit` |
| SSH 传输 | russh 公钥认证 + 主机密钥 TOFU | WSL dou@127.0.0.1:22 通 |
| SFTP 三工具 | read/put/get sha256 一致 | 4/4 通过 |

## 二、M3 验证点对照

计划书 5.2：用 WorkBuddy/Codex 实际接入，跑通执行+下载+审查+审计闭环。

| 验证点 | 状态 |
|---|---|
| 执行 | ✅ auto/pending/blocked 三档 |
| 下载 | ✅ dou.get 落隔离区 |
| 审查 | ✅ clamav-wsl 真扫描 |
| 审计 | ✅ 链式哈希 + CSV 导出 |

## 三、Tauri 桌面壳状态

- 脚手架在 `E:\dou-app\`（Vue3 + TS + Tauri 2.x）
- **链接阻塞**：GNU 工具链 `ld.exe: export ordinal too large: 92875`
- 解锁：装 MSVC Build Tools 切 `stable-x86_64-pc-windows-msvc`

## 四、仍缺（诚实清单）

1. Tauri 窗口未出（链接阻塞，需 MSVC）
2. SQLite 替换 JSONL（rusqlite 已加，未迁移）
3. 三档凭据（配对码 60s / 会话租约 30min）
4. 多会话、SFTP 树、编辑器、终端渲染（阶段一工作量）

## 五、下一步

1. 装 MSVC Build Tools（winget 之前 1602 UAC 失败，需手动确认）
2. 切 MSVC 工具链编译 Tauri 窗口
3. 接 xterm.js 终端 + monaco 编辑器
4. 审计迁 SQLite
