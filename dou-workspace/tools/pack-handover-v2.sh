#!/usr/bin/env bash
# 【历史脚本 / DEPRECATED】v2 是 git 化打包实验版，仍含旧机 /e/ 路径，换机不可直接跑。
# 现行打包请用同目录 pack-handover.sh（已参数化，支持 DOU_ROOT / DOU_APP_DIR 等覆盖）。
# 把豆 Dou 项目的全部成果打包成换机可用的压缩包。
#
# 【为什么不用 zip -r 直接打整个目录】
#   本机的 zip 不支持 --exclude-directory，-x 模式对顶层目录（node_modules/、target/）
#   匹配不可靠，实测会把 .git 一起丢掉，且扫描耗时不可控。
#   故改为 git 原生工具组合：
#     1. git archive  → HEAD 工作树快照（天然只含 git 跟踪的文件，
#                        自动排除 node_modules/target/dist 与全部敏感文件）
#     2. git bundle   → 完整 git 历史 + 全部分支 + tag（换机后可完整还原仓库）
#   两者合起来 = 既能直接跑代码，又能恢复全部版本历史。
#
# 【设计要点】
#   - 完整历史：dou-app 4 分支 + dou-workspace main，均带 handover-baseline tag
#   - 零敏感文件：token.txt / audit.db / known_hosts.json / vault.json / 私钥 一律不入包
#   - 可重建：node_modules（npm ci）、target（cargo build）、dist（npm run build）
#   - 附换机重建指南与一键验证脚本
set -euo pipefail

STAMP="$(date +%Y%m%d-%H%M)"
OUT_DIR="/e/dou-handover"
OUT="$OUT_DIR/dou-handover-$STAMP.zip"
STAGE="$OUT_DIR/.stage-$STAMP"

APP_SRC="/e/dou-app"
WS_SRC="/e/思考/dou"

echo "[pack] 输出: $OUT"
rm -rf "$STAGE"; mkdir -p "$STAGE"

# ---------- 1. 客户端仓库 ----------
echo "[pack] 客户端仓库 dou-app ..."
mkdir -p "$STAGE/dou-app"
( cd "$APP_SRC" && git archive --format=tar HEAD ) | ( cd "$STAGE/dou-app" && tar -xf - )
( cd "$APP_SRC" && git bundle create "$STAGE/dou-app/history.bundle" --all >/dev/null )
echo "[pack]   工作树 $(find "$STAGE/dou-app" -type f | wc -l) 文件 + history.bundle"

# ---------- 2. 主仓库 ----------
echo "[pack] 主仓库 dou-workspace ..."
mkdir -p "$STAGE/dou-workspace"
( cd "$WS_SRC" && git archive --format=tar HEAD ) | ( cd "$STAGE/dou-workspace" && tar -xf - )
( cd "$WS_SRC" && git bundle create "$STAGE/dou-workspace/history.bundle" --all >/dev/null )
echo "[pack]   工作树 $(find "$STAGE/dou-workspace" -type f | wc -l) 文件 + history.bundle"

# ---------- 3. 指南与脚本 ----------
cp "$WS_SRC/tools/新机重建指南.md" "$STAGE/新机重建指南.md"
cp "$WS_SRC/tools/verify.sh"           "$STAGE/verify.sh"

# ---------- 4. 清单 ----------
cat > "$STAGE/MANIFEST.txt" <<EOF
豆 Dou 项目换机打包清单
生成时间: $(date '+%Y-%m-%d %H:%M:%S')

【目录结构】
  dou-app/          桌面客户端工作树（Tauri 2 + Vue 3 + Rust）
    history.bundle  完整 git 历史（4 个分支 + handover-baseline tag）
  dou-workspace/    主仓库工作树（网关 crates/dou-gateway + docs/handover + tools）
    history.bundle  完整 git 历史（main + handover-baseline tag）
  新机重建指南.md    换机后如何恢复开发环境（先读这个）
  verify.sh         一键验证脚本

【如何恢复 git 仓库】
  cd dou-app
  git init && git fetch history.bundle 'refs/*:refs/*' && git checkout main
  git tag -l            # 应看到 handover-baseline
  git branch -a         # 应看到 4 个分支

【已排除（可一键重建）】
  node_modules/  →  npm ci
  target/        →  cargo build --offline
  dist/          →  npm run build
  src-tauri/gen/ →  cargo 自动生成

【已排除（敏感，请自行安全转移）】
  token.txt  audit.db  known_hosts.json  vault.json
  quarantine/  *.key  *.pem  id_rsa  id_ed25519
  运行时数据在 %LOCALAPPDATA%\\dou\\ ，需单独加密转移。
  注：known_hosts.json 不建议拷贝，换机后重新学习主机密钥更安全（TOFU）。

【本轮成果】
  客户端 dou-app（main，tag handover-baseline）：
    cargo build --offline       0 error
    cargo test --offline --lib  11 passed / 0 failed
    cargo clippy --all-targets  0 warning / 0 error
    cargo fmt --check           0 差异
    vue-tsc --noEmit            0 error
    回环自校验                   10/10 通过
  已修安全缺陷：保险库锁定态绕过(P0)、明文私钥落盘(P0)、
                文件损坏被吞(P1)、KDF 迭代 200k→600k(P1)
  已完成重构：App.vue 942→170 行，拆为 10 组件 + 14 composable
EOF

# ---------- 5. 压缩 ----------
echo "[pack] 压缩中 ..."
rm -f "$OUT"
( cd "$STAGE" && zip -r -q "$OUT" . )
rm -rf "$STAGE"

echo "[pack] 完成: $OUT"
ls -la "$OUT"
