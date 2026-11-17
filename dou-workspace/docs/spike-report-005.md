# 豆 Dou · M3 网关侧收口报告（spike-report-005）

> 日期：2026-11-10
> 范围：按 B 方案，把网关侧补到 M3 验证点（执行 + 下载 + 审查 + 审计闭环），Tauri 桌面壳后续再做。

## 一、这一轮新交付的能力

| 模块 | 文件 | 说明 |
|---|---|---|
| 任务与审批闭环 | `src/tasks.rs` | TaskHub 内存表；pending → approved/rejected → running → done/failed；后台 tokio::spawn 执行 |
| 双视图脱敏 | `src/sanitize.rs` | 6 类正则遮蔽：私钥块 / AWS Key / Bearer / JWT / 内网 IP / .ssh 路径 |
| 隔离区扫描 | `src/scan.rs` | 调 clamscan 进程；未装则诚实降级为"仅记录哈希，不假装扫了" |
| HTTP 路由 | `src/main.rs` | 新增 `/tasks`（GET）、`/approve`（POST）、`/audit/export`（CSV） |
| 控制台 | `/console` | 加待审任务列表 + 批准/拒绝按钮，一键刷新 |
| dou.exec 改造 | `src/tools.rs` | pending 分支改为创建 task 并返回 task_id；auto 分支 stdout/stderr 过脱敏 |
| dou.get 改造 | `src/tools.rs` | 文件落隔离区后调 scan_file；感染即删；扫描结果写审计并返回给代理 |

## 二、端到端验收（全部本机实测）

### 2.1 MCP 自检（10/10）
- 端口可达、initialize 握手、notifications/initialized、tools/list 五工具齐全
- 只读命令放行（echo）
- 变更命令（rm）进入待审批并返回 task_id
- 只读+变更夹带不自动放行
- 提权（rm -rf /）阻断
- 错令牌 401
- 审计文件可写

### 2.2 审批闭环
- 发 `rm -f /tmp/dou_approve_test.txt` → 返回 `task_id=task_18e6a001b64a1fbc`
- POST `/approve {task_id, approve:true, note}` → 后台执行
- `/tasks` 查到：`status=done, exit_code=0`
- 审计 CSV 里同时记录 `agent exec pending_approval` 和 `human approve_exec allowed` 两条

### 2.3 脱敏
- 远端执行 `echo connect to 192.168.1.10 and key AKIAIOSFODNN7EXAMPLE and Bearer abc.def.ghi done`
- 返回给代理的 stdout：
  ```
  connect to [REDACTED:IP] and key [REDACTED:AWS KEY] and Bearer [REDACTED] done
  ```
- 人视图（控制台 / 审计原始记录）不脱敏——这是计划书 4.10 的双视图语义。

### 2.4 SFTP 三工具（4/4）
- read /etc/hostname → `TY-ZX`
- read /home/dou/.bashrc 前 3 行 → 正常
- put 44 字节 → sha256 `66096e5e...`
- get 回隔离区 → `scan_engine=none, scan_detail="本机未安装 clamscan；本文件仅记录哈希，未做特征扫描"`

### 2.5 审计链
- `/audit/verify` → `{"broken_index":0,"chain_ok":true}`
- `/audit/export` → CSV 含 ts_ms/source/action/target/verdict/trace_id 六列

## 三、对照计划书 M3（第 20 周）

| 验证点 | 计划书要求 | 现状 |
|---|---|---|
| 执行 | 代理能通过 dou.exec 跑命令 | ✅ 只读 auto 放行；变更 pending |
| 下载 | dou.get 落隔离区 | ✅ 隔离区 + sha256 命名 |
| 审查 | 隔离区静态扫描 | ⚠️ 接口在，本机未装 clamscan，降级诚实记录 |
| 审计 | 链式哈希 + 可导出 | ✅ 链完整 + CSV 导出 |
| 审批 | 人在终端批/拒 | ✅ 控制台一键批准/拒绝 |
| 脱敏 | 代理视图遮蔽密钥 | ✅ 6 类正则 |

## 四、仍缺的（诚实清单）

1. **ClamAV 真扫描**：本机没装，现在是降级路径。要真扫得装 clamav/clamwin 或在 WSL 里 `apt install clamav`。
2. **Tauri 桌面壳**：阶段一/二完全没动。现在是 axum + 一个静态 HTML 控制台。
3. **SQLite 替换 JSONL**：计划书 4.3 要求 SQLite，现在还是 JSONL 文件。
4. **三档凭据**：配对码（60s 一次性）、会话租约（30min）没做；现在只有一个永久设备令牌。
5. **七维作用域**：路径范围、工具清单、配额等没做。
6. **长命令任务化**：>30s 自动转任务句柄没做（现在只有 pending 才进任务）。
7. **多会话、SFTP 树、编辑器、终端渲染**：阶段一工作量，0 进度。

## 五、下一步

按 B 方案顺序，接下来做 Tauri 桌面壳（阶段一：终端 + SFTP 树 + 编辑器），同时把上面 2-7 项里 M3 闭环必需的补齐。
