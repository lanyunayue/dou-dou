# 豆 Dou · M3 网关侧最终收口（spike-report-007）

> 日期：2026-11-10
> 范围：B 方案网关侧 M3 闭环——执行/下载/审查/审计全部真跑通。

## 一、这一轮补完的最后一块

**ClamAV 真扫描**：
- WSL 里 `apt install clamav` 装好（1.5.4）
- `scan.rs` 改三级降级：Windows clamscan → WSL clamscan → 诚实记录未扫描
- 实测：dou.get 落隔离区后，调 `wsl clamscan /mnt/c/...`，返回 `OK`
- `scan_engine=clamav-wsl`，不再是降级路径

## 二、M3 验证点逐项对照

| 计划书 M3（第 20 周） | 状态 | 证据 |
|---|---|---|
| 执行（dou.exec） | ✅ | auto 放行 / pending 审批 / blocked 阻断；MCP 10/10 |
| 下载（dou.get 落隔离区） | ✅ | sha256 命名 + 隔离区目录 |
| 审查（隔离区静态扫描） | ✅ | clamav-wsl 真扫，返回 OK |
| 审计（链式哈希 + 导出） | ✅ | `/audit/verify` chain_ok:true；`/audit/export` CSV |
| 审批（人在控制台批/拒） | ✅ | 一键批准 → 后台执行 → done exit=0 |
| 脱敏（代理视图遮蔽） | ✅ | 6 类正则，实测 IP/AWS Key/Bearer 全中 |
| 路径范围 | ✅ | 读 /etc/shadow → path_scope_denied |

## 三、端到端验收数据

- MCP 自检：**10/10 通过**
- SFTP 三工具：**4/4 通过**（read hostname/bashrc、put/get sha256 一致）
- 审批闭环：`rm → task_id → 批准 → done exit=0`
- 脱敏：`192.168.1.10 → [REDACTED:IP]`、`AKIA... → [REDACTED:AWS KEY]`、`Bearer → [REDACTED]`
- 隔离区扫描：`scan_engine=clamav-wsl, detail="...: OK"`
- 审计链：`chain_ok:true`

## 四、Tauri 桌面壳状态

- 脚手架：`E:\dou-app\`（Vue3 + TS + Tauri 2.x），npm install 完成
- 源码：`E:\思考\dou\app\`
- **链接阻塞**：GNU 工具链 `ld.exe: export ordinal too large: 92875`，binutils 已知问题
- 解锁路径：装 MSVC Build Tools 切 `stable-x86_64-pc-windows-msvc`，或升级 MinGW binutils

## 五、仍缺（诚实清单）

1. Tauri 桌面壳窗口未出（链接阻塞）
2. SQLite 替换 JSONL（rusqlite 已加，未迁移）
3. 三档凭据（配对码 60s / 会话租约 30min）
4. 长命令任务化（>30s 自动转 task 句柄）
5. 多会话、SFTP 树、编辑器、终端渲染（阶段一工作量）

## 六、下一步建议

1. 装 MSVC Build Tools 解锁 Tauri 链接（这是阶段一的硬阻塞）
2. Tauri 窗口出来后接 xterm.js 终端 + monaco 编辑器
3. 审计迁 SQLite（rusqlite bundled 已就绪）
