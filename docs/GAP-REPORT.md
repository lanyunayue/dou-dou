# 差距对照报告（对照《豆项目计划书_第五版_投资版》）

更新时间：2026-11-10
口径：以计划书原文为准逐条对照；状态分 ✅ 已落地并编译/接口验证、🟡 部分落地（骨架或降级路径）、⛔ 受限待办（环境或深度项，记录原因与验收口径）。

## 一、P0 主链路

| 计划书条目 | 现状 | 状态 | 证据 |
| --- | --- | --- | --- |
| F-01 多会话标签，10+ 并行 | 顶部 tab 栏，每会话独立 pty，支持勾选分组 | ✅ | app/src/App.vue tabs / visibleTabs |
| F-02 SFTP 树状浏览、隐藏文件开关 | 列目录、上级跳转、隐藏文件过滤、文件名过滤 | ✅ | App.vue visibleFiles / goUp；sftp.rs |
| F-03 递归传输、断点续传、失败定位 | 递归下载(tar)、递归上传(tar)、分块下载、断点续传(offset 追加)、远端 stat | ✅ | transfer.rs download_dir/upload_dir/download_resume/download_chunked/remote_stat |
| F-04 编辑器懒加载与 mtime 比对 | >2MB 不载入全文；保存前比对远端 mtime，冲突二次确认 | ✅ | App.vue onFile/saveFile；remote_stat |
| F-05 终端渲染 Canvas/WebGL 可选 | 优先 WebGLAddon，初始化失败回退默认 Canvas | ✅ | App.vue newTab |
| F-06 长连接保护：关窗不断、tmux 接管 | 关窗改为隐藏不断连；pty 经 `tmux new-session -A -s dou` 接管，重开可 attach | ✅ | lib.rs CloseRequested；pty.rs tmux exec |
| F-07 输出变更系统通知 | 窗口失焦时累计 tab 角标并弹系统通知，切 tab 清角标 | ✅ | App.vue pty_data / onFocusChanged |
| F-08 导出净化 | 去 ANSI CSI/OSC、分页符 \x0c、回车覆盖 \r、字符集切换；落本地 .txt | ✅ | App.vue cleanExport/exportTerm |
| F-09 分组广播 | 勾选会话定向广播；未勾选则发全部 | ✅ | App.vue doBroadcast |
| F-10 MobaXterm 会话导入 | 解析 ssh:// 链接与段内 Username/Hostname/Port，入库 localStorage | ✅ | App.vue importMoba |

## 二、网关审查与快照（P1）

| 计划书条目 | 现状 | 状态 | 证据 |
| --- | --- | --- | --- |
| 六张数据表（project/host/credential/session/snapshot/policy_rule） | SQLite 建表、upsert/query、内置规则种子 | ✅ | store.rs init_schema/seed_builtin_rules |
| 三段指纹：命令哈希/参数规范化串/数据集路径哈希 | 命令内容、参数规范化、数据集路径分别哈希后拼接再 sha256；历史命中返回 duplicate+prior | ✅ | snapshot.rs fingerprint/record |
| 上下文采集 cwd/pid/GPU/日志尾部 | exec 后补采 probe 命令，解析入 Context | ✅ | snapshot.rs probe_command/Context；tools.rs exec |
| 四类处置：放行/告警/阻断/待审 | Disposition 四态；特征库+脚本规则(13 条混淆/反弹)双检；无 clamscan 降级脚本规则 | ✅ | scan.rs Disposition；tools.rs get |
| 双视图脱敏 6 类正则 | 私钥块/AWS Key/Bearer/JWT/内网 IP/.ssh 与敏感路径 | ✅ | sanitize.rs 默认规则 |
| 脱敏规则可配置 | 读 `%LOCALAPPDATA%\dou\sanitize_rules.json` 追加自定义正则，缺失/非法不阻断 | ✅ | sanitize.rs load_extra_rules |
| 审计按时间/来源检索 | `/audit/query?since_ms&source&limit` | ✅ | audit.rs query；main.rs audit_query |

## 三、凭据、作用域与授权模型（4.8）

| 计划书条目 | 现状 | 状态 | 证据 |
| --- | --- | --- | --- |
| 一级 配对码 6 位 60s 一次性 | 签发与校验，过期失效 | ✅ | auth.rs issue_pairing_code/check_pairing |
| 二级 设备令牌 dou_ak_ 默认 24h | 签发返回明文一次，库内只存哈希；列表/吊销 | ✅ | scope.rs issue_token/verify_token/list_active |
| 三级 会话租约 单次任务最长 30min | exec 入口按会话记录租约起点，超 LEASE_SECS(1800s) 拒并写 lease_expired 审计；租约存内存，网关重启重置 | ✅ | auth.rs LEASE_SECS；tools.rs exec 租约段；main.rs leases |
| 七维作用域默认值 | 绑定客户端/主机(空=全禁)/工具/路径/命令模式/审批策略/配额(24h·200 次·1 并发) | ✅ | scope.rs default_scope；表17 对照 |
| 凭据只存哈希、明文签发时显示一次 | 库内 hash_token，明文仅签发响应 | ✅ | scope.rs |
| 吊销写时间戳不删记录 | revoke_token 写 revoked_at；校验一看时间戳即拒 | ✅ | scope.rs revoke_token/verify_token |
| 三档授权观察/执行/变更 | 档位与作用域生成；变更档审批不可关闭（后端拒 remember、前端禁用按钮） | ✅ | scope.rs；tasks.rs；App.vue |
| 作用域在调用点强制校验 | tools/call 入口按凭据作用域校验工具白名单；read/get/put 校验路径前缀；exec 校验只读命令模式；配额按凭据在内存累计调用次数与并发数，超 max_calls/concurrency 拒并写审计。实测 observe 令牌调 exec 被拒、session_list 放行、吊销后 401。计数存内存，网关重启后重置（持久化配额计数属后续） | ✅ | tools.rs call/read/put/get/exec；scope.rs；main.rs handle_mcp/quota |

## 四、接入向导（4.7）

| 计划书条目 | 现状 | 状态 | 证据 |
| --- | --- | --- | --- |
| 六类客户端配置事实与格式 | WorkBuddy/Codex/Claude Code/Claude Desktop/VS Code/Cursor；JSON vs TOML、顶层键差异 | ✅ | main.rs onboard_config |
| 五步向导：开通道/选客户端/签发凭据/写入/握手自检 | 前端向导面板含步骤说明；片段生成；"检测客户端连接"轮询审计记录，命中 initialize 即判定握手成功，超时 30s 提示 | ✅ | App.vue probeHandshake；main.rs audit_query |
| 代写三约束：只追加不覆盖、写前备份+解析校验+失败回滚、回报生效文件 | JSON 类：备份 .bak、合并条目、写后再解析校验、失败自动回滚、回报 path/备份/是否覆盖 | ✅ | main.rs onboard_write |
| 逐级降级：代写→复制片段→注册命令 | TOML(Codex) 不代写，降级复制片段；片段复制已接 | ✅ | onboard_write / copySnippet |

## 五、审批与可见性（4.9）

| 计划书条目 | 现状 | 状态 | 证据 |
| --- | --- | --- | --- |
| 审批卡片三按钮：一次允许/本次任务内允许/拒绝 | 三按钮；remember 标志落任务；拒绝理由必填并入审计 | ✅ | tasks.rs decide；main.rs approve_task；App.vue decideTask |
| 拒绝理由进审计 | 拒绝无理由返回 400；通过后写 approve/reject 事件含 note | ✅ | approve_task |
| 变更档不可关闭审批 | 任务记录携带 policy；change 档下后端拒绝 remember=true，前端禁用"本次任务内允许"按钮并提示 | ✅ | tasks.rs decide；tools.rs create_pending；App.vue 审批卡片 |
| 长命令 >30s 转任务句柄 | 后台模式建任务句柄；超时挂起语义已建 | 🟡 | tools.rs create_pending；30s 阈值判定未单独计时 |
| 本地事件总线 | 前端 mitt 替代总线；网关审计/任务事件可订阅 | ✅ | App.vue bus；tasks.rs |

## 六、性能指标（表 14）实测结果

可在本机直接测的项已实测（`python tools/benchmark.py`）：

| 指标 | 目标 | 实测 | 状态 |
| --- | --- | --- | --- |
| 代理请求开销 | P95 ≤ 200ms | P50 8.4ms / P95 31.7ms | ✅ 达标 |
| 审计检索 | 10 万条 1s 内 | 19.9ms（当前既有样本；10 万条口径需灌样后复测） | 🟡 样本量不足 |
| 快照/审计写入 | 单次 ≤50ms | 内存库折算 ≈0ms/条 | ✅ 达标 |
| 冷启动时间 | ≤3s，10 次中位数 | 待打包实测 | ⛔ 需 release 包 |
| 安装包体积 | ≤30MB | 待打包实测 | ⛔ 需 release 包 |
| 常驻内存 | ≤200MB，8h 增长 <15% | 待长跑实测 | ⛔ 需长跑采样 |
| 目录上传 | 1000 文件 90s 内 | 待靶机实测 | ⛔ 需内网靶机 |
| 大日志打开 | 500MB 3s 可滚动 | 待靶机实测 | ⛔ 需大文件样本 |
| 扫描耗时 | 100MB 5s 内 | 待靶机实测 | ⛔ 需特征库+大文件 |

## 七、受限待办（不在本轮伪造完成）

1. **私钥本地加密库（F-21）**：当前私钥按文件路径直接读取；尚未做密钥派生 + 字段级加密存储。代理侧不接触私钥的隔离要求已满足（代理经网关），本机落盘加密属加固项。
2. **指标六项**：需出 release 包、接内网靶机与大样本后按表 14 方法复测，当前不填目标值。
3. **GUI 截图回归**：2026-11-11 dev 壳子首屏渲染复核通过，导航/文件面板/向导/审批/凭据/Moba/快照齐全，无白屏崩溃；窗口默认 1320×1000（最小 960×600）。网关在线状态依赖 Tauri invoke 命令，浏览器 dev 环境显示“离线”属桥接缺失；逐屏交互仍需在 app.exe 内复测。实测明细见 `docs/TEST-REPORT.md`。
4. **本机 WSL2 SSH 靶机连通性（环境阻塞，非代码缺口）**：本轮排查 7419 占用与 22 拒绝。已排除工具链（GNU/MSVC 各自复现一致）、Tauri 运行时、Russh、防火墙出站规则、tokio/标准库差异——对照实测 Rust 连本机回环 127.0.0.1:7419 正常（3.7ms），仅到 WSL 靶机（127.0.0.1:22 / WSL 虚拟 IP:22）被拒或超时；同一时刻 .NET 连接行为也间歇抖动。结论为本机 WSL2↔Windows 端口转发在当前远程控制/VPN 环境下不稳定。壳子 pty/sftp 与网关 SSH 通道的代码与编译均已就绪，需在稳定内网靶机或干净网络环境复测终端建连、SFTP 列目录与上传下载。
5. **配额/租约计数持久化**：当前调用次数、并发数、租约起点存内存，网关重启即重置；持久化计数属后续项。

## 八、对照计划书新识别缺口（2026-11-11 复核）

### 8.1 半完成（骨架已在，未闭环）

| 计划书条目 | 现状 | 缺口 | 状态 |
| --- | --- | --- | --- |
| 长命令 >30s 转任务句柄（4.9） | 后台任务句柄已建 | 30s 阈值自动计时判定未做，需超时自动移交句柄并持续推事件 | 🟡 |
| 审计检索 10 万条（表14） | 既有样本 19.9ms | 未灌 10 万条大样本复测检索口径 | 🟡 |
| 配额/租约计数持久化（4.8） | 调用次数、并发、租约起点存内存 | 网关重启即重置，需持久化计数与租约 | 🟡 |

### 8.2 P0/P1 范围内明确未做

| 计划书条目 | 现状 | 缺口 |
| --- | --- | --- |
| 会话锁与人机争抢（3.2） | 未实现 | 谁先占用谁持有、人工优先、代理请求排队或被拒，界面说明原因 |
| MCP 进度通知 / 任务扩展（4.9 表19） | 仅本地事件总线 | 单次调用内递增进度通知；任务句柄轮询状态通道未接 |
| 审批超时（表20） | 未实现 | 写入类 5 分钟无响应视为拒绝，任务挂起而非失败 |
| 私钥本地加密库 / 凭据字段级加密（F-21、4.4 credential） | 私钥按文件路径直读 | 密钥派生 + 密文 + 盐 + 本机解锁；代理不接触私钥的隔离已满足 |
| 五个独立页面（3.4） | 现有终端/文件/向导/审批/凭据视图，快照为弹窗 | 缺连接管理、策略配置、审计中心（前端入口）、实验详情页、设置页 |
| 主机指纹变更二次确认（4.4 host、流程一） | TOFU 已落 known_hosts | 指纹变更后的二次确认锁定流程待补 |
| 断连置顶提示（4.9） | tmux 接管已做 | “客户端已断开”任务置顶与人工终止入口未见 |

### 8.3 工程规范与质量门禁（6.x）

| 计划书条目 | 现状 | 缺口 |
| --- | --- | --- |
| 单元/集成测试与覆盖率（6.1、6.8） | 未见测试代码 | 核心层 ≥70%、视图层 ≥40%；脱敏规则专项回归用例集 |
| 分层目录（6.5） | Rust 扁平、前端单 App.vue | core/adapters/commands/events、views/components/stores/shared 目录与单向依赖 |
| 四态与请求标识（6.7） | 未系统封装 | 空/加载/错误/降级态；异步 requestId 比对、过期丢弃；错误边界 |
| 更新器与离线包校验（4.2、R-12） | 未接 | Tauri 更新器、离线包哈希/签名校验、可关闭开关 |

### 8.4 P2 后续功能（计划书定位后续迭代，非首发阻断）

| 编号 | 功能 | 现状 |
| --- | --- | --- |
| F-30 | 插件体系：独立进程、不依赖本机 npm、安装失败可回滚 | 未做 |
| F-31 | 可选远端钩子（自愿部署轻量脚本，不默认装/不自动推） | 未做 |
| F-32 | 多机任务看板：多台机器任务与 GPU 占用 | 未做 |
| F-33 | 动态沙箱观察：待审文件隔离容器短跑，记录外联与写入 | 未做 |
| F-34 | 会话录制与回放 | 未做 |

### 8.5 环境 / 实测待办（非代码缺口）

- 性能六项：冷启动、包体、常驻内存、目录上传(1000 文件 90s)、大日志(500MB 3s)、扫描耗时(100MB 5s)，需 release 包、内网靶机与大样本复测。
- SSH 靶机直连回归：本机 WSL2↔Windows 转发在远程/VPN 环境间歇抖动，终端建连、SFTP 列目录、上传下载待稳定环境复测（见第七节第 4 条）。
- 兼容性矩阵：三平台 × 六发行版 × 三 Shell 的字符/编码/分页符/路径权限回归。

### 8.6 商业后段（本期不阻断）

- 版本功能墙：免费版审计保留 30 天；专业版长期保留、在线规则库更新、多机看板；团队版共享策略模板与统一审计导出。

## 九、构建与自测命令

- 网关（GNU 工具链）：`cargo +stable-x86_64-pc-windows-gnu build`，产物 `E:\dou-target\debug\dou-gateway.exe`
- Tauri 壳（MSVC 工具链）：`cargo +stable-x86_64-pc-windows-msvc build`（src-tauri），前端 `npm run dev`
- 基准：`python tools/benchmark.py --runs 30`
- 后端自测脚本：`tools/probe_ssh.py / probe_sftp.py / probe_approve.py / mcp_probe.py`
