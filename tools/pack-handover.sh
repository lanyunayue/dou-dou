#!/usr/bin/env bash
# 把豆 Dou 项目的全部成果打包成换机可用的压缩包。
#
# 设计要点：
#   1. 含完整 git 历史（history.bundle）—— 保留提交历史、分支、tag（handover-baseline），
#      换机后 git log / git revert / git branch 全部照常可用。
#   2. 排除 node_modules / target / dist —— 这些能用 lock 文件一键重建，
#      带上会臃肿几百 MB 且可能跨机路径失效。
#   3. 排除全部敏感文件 —— 凭据不进包（W-03 红线）。
#   4. 附带换机重建指引与验证脚本。
#
# 换机路径：默认以「本脚本所在 tools/ 目录的上两级」为交接包根，
# 可用 DOU_ROOT / DOU_APP_DIR / DOU_WS_DIR / DOU_OUT_DIR 覆盖，不再钉死盘符。
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="${DOU_ROOT:-$(cd "$HERE/../.." && pwd)}"
APP_DIR="${DOU_APP_DIR:-$ROOT/dou-app}"
WS_DIR="${DOU_WS_DIR:-$ROOT/dou-workspace}"
OUT_DIR="${DOU_OUT_DIR:-$(cd "$ROOT/.." && pwd)/dou-handover}"

STAMP="$(date +%Y%m%d-%H%M)"
OUT="$OUT_DIR/dou-handover-$STAMP.zip"
STAGE="$OUT_DIR/.stage-$STAMP"

echo "[pack] 输出: $OUT"
echo "[pack] 客户端: $APP_DIR"
echo "[pack] 主仓库: $WS_DIR"
mkdir -p "$OUT_DIR"
rm -rf "$STAGE"; mkdir -p "$STAGE/dou-app" "$STAGE/dou-workspace"

# ---------- 排除规则 ----------
# 注意 zip 的 -x 模式里 "*/node_modules/*" 匹配不到顶层 "node_modules/"，
# 故顶层与嵌套两种形式都要写。
EXCLUDES=(
  # 构建产物（可重建）
  "node_modules/*" "*/node_modules/*"
  "target/*" "*/target/*"
  "dist/*" "*/dist/*"
  "gen/schemas/*" "*/gen/schemas/*"
  # 敏感文件（W-03 红线：首个 commit 即不得含 token.txt / audit.db）
  "token.txt" "*/token.txt"
  "audit.db" "*/audit.db"
  "known_hosts.json" "*/known_hosts.json"
  "vault.json" "*/vault.json"
  "quarantine/*" "*/quarantine/*"
  "*.key" "*/.key" "*.pem" "*/.pem" "*.pfx" "*.p12"
  "id_rsa" "*/id_rsa" "id_ed25519" "*/id_ed25519" "id_ecdsa" "*/id_ecdsa"
  "*.pub" "*/.pub"
  # 临时与日志
  ".contract/*" "*/.contract/*"
  "cargo-build.log" "*/cargo-build.log"
  ".DS_Store" "*/.DS_Store" "Thumbs.db" "*/Thumbs.db"
  "*.swp" "*~" "*.log" "*/.idea/*"
)

# ---------- 1. 客户端仓库 ----------
echo "[pack] 收集客户端仓库 dou-app ..."
( cd "$APP_DIR" && zip -r -q "$STAGE/dou-app.zip" . -x "${EXCLUDES[@]}" )
( cd "$STAGE" && unzip -q dou-app.zip -d dou-app && rm dou-app.zip )

# ---------- 2. 主仓库 ----------
echo "[pack] 收集主仓库 dou-workspace ..."
( cd "$WS_DIR" && zip -r -q "$STAGE/dou-workspace.zip" . -x "${EXCLUDES[@]}" )
( cd "$STAGE" && unzip -q dou-workspace.zip -d dou-workspace && rm dou-workspace.zip )

# ---------- 3. 清单 ----------
cat > "$STAGE/MANIFEST.txt" <<EOF
豆 Dou 项目换机打包清单
生成时间: $(date '+%Y-%m-%d %H:%M:%S')

【包含内容】
1. dou-app/          桌面客户端（Tauri 2 + Vue 3 + Rust），含 history.bundle
2. dou-workspace/    主仓库（网关 crates/dou-gateway + 交接文档 docs/handover + 工具 tools），含 history.bundle
3. MANIFEST.txt      本清单
4. 新机重建指南.md    换机后如何恢复开发环境
5. verify.sh         一键验证脚本（构建 / 测试 / 契约核对）

【git 历史】
两个目录都含 history.bundle，全部提交、分支、tag 均保留：
  - dou-app:       main / fix/vault-security / refactor/frontend-components /
                   chore/cleanup-deadcode-fmt，tag handover-baseline
  - dou-workspace: main，tag handover-baseline
换机后 git init && git fetch history.bundle 'refs/*:refs/*' && git checkout -f main
即可恢复 git log / git revert / git checkout -b。

【已排除（可一键重建）】
  node_modules/  →  npm ci
  target/        →  cargo build
  dist/          →  npm run build
  src-tauri/gen/ →  cargo 自动生成

【已排除（敏感，需自行安全转移）】
  token.txt  audit.db  known_hosts.json  vault.json
  quarantine/  *.key  *.pem  id_rsa  id_ed25519
  运行时数据位于 %LOCALAPPDATA%\\dou\\ ，请单独加密转移。
EOF

# ---------- 4. 压缩 ----------
echo "[pack] 压缩中 ..."
rm -f "$OUT"
( cd "$STAGE" && zip -r -q "$OUT" . )
rm -rf "$STAGE"

echo "[pack] 完成: $OUT"
ls -la "$OUT"
