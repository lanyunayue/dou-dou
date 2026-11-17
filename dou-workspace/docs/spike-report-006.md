# 豆 Dou · M3 网关侧 + Tauri 脚手架收口（spike-report-006）

> 日期：2026-11-10
> 范围：B 方案连续推进——网关侧补完 M3 闭环；Tauri 桌面壳搭脚手架。

## 一、这一轮新增

### 网关侧
1. **审批闭环**（`tasks.rs`）：pending 命令创建 task，控制台一键批准/拒绝，后台 tokio::spawn 执行，结果回写 task map。
2. **双视图脱敏**（`sanitize.rs`）：6 类正则——私钥块 / AWS Key / Bearer / JWT / 内网 IP / .ssh 路径。代理视图遮蔽，人视图保留。
3. **隔离区扫描**（`scan.rs`）：dou.get 落隔离区后调 clamscan；未装则诚实降级为"仅记录哈希，未做特征扫描"，不假装扫了。感染即删。
4. **路径范围**（`tools.rs::check_remote_path`）：拒绝 /etc/shadow、/etc/sudoers、.ssh、/root 等；只允许 /home/dou/、/tmp/、少数系统白名单。
5. **审计 CSV 导出**：`/audit/export` 六列 CSV。
6. **控制台**：`/console` 加待审任务列表 + 批准/拒绝按钮。

### Tauri 桌面壳
- 脚手架已生成：`E:\dou-app\`（Vue3 + TS + Tauri 2.x）
- npm install 完成（54 包）
- 源码在 `E:\思考\dou\app\`，编译副本在 `E:\dou-app\`（绕开中文路径 windres 乱码）
- **链接阻塞**：GNU 工具链下 `ld.exe: export ordinal too large: 92875`，binutils 已知问题。需 MSVC Build Tools 或升级 binutils 才能链接出窗口二进制。

## 二、端到端验收（全部本机实测）

| 项 | 结果 |
|---|---|
| MCP 自检 | 10/10 通过 |
| SFTP 三工具 | 4/4 通过（read hostname/bashrc、put/get sha256 一致） |
| 审批闭环 | rm → task_id → 批准 → done exit=0 |
| 脱敏 | 192.168.1.10 → [REDACTED:IP]，AKIA... → [REDACTED:AWS KEY]，Bearer → [REDACTED] |
| 路径范围 | 读 /etc/shadow → `path_scope_denied` |
| 审计链 | `/audit/verify` → `chain_ok:true` |
| 隔离区扫描 | `scan_engine=none`（诚实降级，未假装扫描） |

## 三、对照计划书 M3（第 20 周）

| 验证点 | 状态 |
|---|---|
| 执行（dou.exec） | ✅ auto 放行 / pending 审批 / blocked 阻断 |
| 下载（dou.get 落隔离区） | ✅ sha256 命名 + 扫描钩子 |
| 审查（隔离区静态扫描） | ⚠️ 接口在，clamscan 未装，降级诚实记录 |
| 审计（链式哈希 + 导出） | ✅ chain_ok + CSV |
| 审批（人在控制台批/拒） | ✅ 一键批准后台执行 |
| 脱敏（代理视图遮蔽） | ✅ 6 类正则 |
| 路径范围 | ✅ 拒绝敏感路径 |

## 四、仍缺的（诚实清单）

1. **Tauri 桌面壳链接**：GNU 工具链 export ordinal 问题，需 MSVC。
2. **ClamAV 真扫描**：本机未装。
3. **SQLite 替换 JSONL**：计划书要求，现在还是 JSONL。
4. **三档凭据**：配对码 60s 一次性、会话租约 30min 没做；现在只有一个永久设备令牌。
5. **长命令任务化**：>30s 自动转 task 句柄没做（现在只有 pending 才进 task）。
6. **多会话、SFTP 树、编辑器、终端渲染**：阶段一工作量，Tauri 壳还没出窗口。

## 五、下一步

1. 装 MSVC Build Tools（或用 `rustup default stable-x86_64-pc-windows-msvc`）解锁 Tauri 链接。
2. 真装 ClamAV（WSL 里 `apt install clamav`）跑一次真扫描。
3. 把 JSONL 审计迁到 SQLite（rusqlite bundled）。
4. Tauri 窗口出来后接 xterm.js 终端 + monaco 编辑器。
