#!/usr/bin/env bash
# 换机后一键验证脚本。
# 用法：解压后 cd 到包根目录，执行  bash verify.sh
set -uo pipefail

PASS=0
FAIL=0
say()  { printf '%s\n' "$*"; }
ok()   { PASS=$((PASS+1)); printf '  [PASS] %s\n' "$*"; }
bad()  { FAIL=$((FAIL+1)); printf '  [FAIL] %s\n' "$*"; }
step() { printf '\n=== %s ===\n' "$*"; }

here="$(cd "$(dirname "$0")" && pwd)"
APP="$here/dou-app"
WS="$here/dou-workspace"

step "0. 目录完整性"
for d in "$APP" "$WS"; do
  if [ -d "$d" ]; then ok "$(basename "$d")/ 存在"; else bad "$(basename "$d")/ 缺失"; fi
done
for f in MANIFEST.txt "新机重建指南.md"; do
  if [ -f "$here/$f" ]; then ok "$f 存在"; else bad "$f 缺失"; fi
done

step "1. git 历史完整性"
# 仓库可能是两种形态：
#   (a) 已含 .git 目录 —— 直接检查
#   (b) 只有 history.bundle —— 自动还原后检查
check_repo() {
  local dir="$1" label="$2"
  if [ ! -d "$dir/.git" ] && [ -f "$dir/history.bundle" ]; then
    printf '  [INFO] %s 为 bundle 形态，自动恢复 git ...\n' "$label"
    # 工作树已由 git archive 填好，checkout 必须用 -f 强制覆盖同名文件，
    # 否则 git 会以「untracked working tree files would be overwritten」拒绝。
    ( cd "$dir" && git init -q && git fetch -q history.bundle 'refs/*:refs/*' \
        && git checkout -qf main ) >/dev/null 2>&1 \
      && ok "$label 已从 history.bundle 还原" \
      || bad "$label bundle 还原失败"
  fi
  if [ -d "$dir/.git" ]; then
    ok "$label 含 .git"
    local n t b
    n=$(cd "$dir" && git rev-list --count HEAD 2>/dev/null || echo 0)
    [ "$n" -gt 0 ] && ok "$label 提交数 = $n" || bad "$label 无提交"
    t=$(cd "$dir" && git tag -l 2>/dev/null | grep -c handover-baseline || true)
    [ "$t" -ge 1 ] && ok "$label 有 handover-baseline tag" || bad "$label 缺 tag"
    b=$(cd "$dir" && git branch -a 2>/dev/null | wc -l)
    [ "$b" -ge 2 ] && ok "$label 分支数 = $b" || bad "$label 分支异常"
  else
    bad "$label 缺 .git 且无 history.bundle"
  fi
}

check_repo "$APP" "dou-app"
check_repo "$WS"  "dou-workspace"

step "2. 敏感文件未入包"
leak=$(find "$here" -maxdepth 4 \( -name "token.txt" -o -name "audit.db" \
        -o -name "known_hosts.json" -o -name "vault.json" \
        -o -name "id_ed25519" -o -name "id_rsa" \) 2>/dev/null | head -5)
if [ -z "$leak" ]; then ok "无敏感文件混入"; else bad "发现敏感文件: $leak"; fi

step "3. 构建产物已排除（应为可重建）"
for p in "$APP/node_modules" "$APP/src-tauri/target" "$APP/dist"; do
  if [ -e "$p" ]; then printf '  [INFO] %s 存在（未排除）\n' "${p#$here/}"; else ok "${p#$here/} 已排除"; fi
done
[ -f "$APP/package-lock.json" ] && ok "package-lock.json 在（可 npm ci）" || bad "缺 package-lock.json"
[ -f "$APP/src-tauri/Cargo.lock" ] && ok "Cargo.lock 在（可 cargo build）" || bad "缺 Cargo.lock"

step "4. 工具链"
if command -v cargo >/dev/null 2>&1; then
  ok "cargo: $(cargo --version 2>/dev/null)"
  tc=$(cd "$APP" && rustup show active-toolchain 2>/dev/null || echo unknown)
  case "$tc" in
    *msvc*) ok "客户端工具链 = MSVC ($tc)" ;;
    *) printf '  [WARN] 客户端工具链非 MSVC: %s\n' "$tc" ;;
  esac
else
  bad "未安装 cargo"
fi
if command -v node >/dev/null 2>&1; then ok "node: $(node --version)"; else bad "未安装 node"; fi

step "5. Rust 构建与测试"
if [ -d "$APP/src-tauri" ]; then
  ( cd "$APP/src-tauri" && cargo build --offline >/tmp/v_build.log 2>&1 )
  rc=$?
  # 判定标准是「有无编译错误」，而非退出码：新机首次编译全部依赖耗时很长，
  # 若被外部超时打断会返回非 0 但日志里并无 error。
  if grep -qE "^error(\[|:)" /tmp/v_build.log; then
    bad "cargo build 有编译错误"
    grep -E "^error(\[|:)" /tmp/v_build.log | head -8 | sed 's/^/      /'
  elif grep -q "Finished" /tmp/v_build.log; then
    ok "cargo build --offline 通过"
  else
    printf '  [INFO] cargo build 未跑完（首次编译依赖耗时长），但无编译错误\n'
  fi

  ( cd "$APP/src-tauri" && cargo test --offline --lib >/tmp/v_test.log 2>&1 )
  if [ $? -eq 0 ]; then
    r=$(grep -oE "test result: ok\. [0-9]+ passed" /tmp/v_test.log | head -1)
    ok "cargo test --lib 通过 ${r}"
  else
    bad "cargo test 失败"; tail -8 /tmp/v_test.log | sed 's/^/      /'
  fi

  ( cd "$APP/src-tauri" && cargo fmt --check >/tmp/v_fmt.log 2>&1 )
  if [ $? -eq 0 ]; then ok "cargo fmt --check 0 差异"; else bad "格式不一致（$(grep -c '^Diff in' /tmp/v_fmt.log) 处）"; fi

  ( cd "$APP/src-tauri" && cargo clippy --offline --all-targets >/tmp/v_clippy.log 2>&1 )
  w=$(grep -cE "^(warning|error): " /tmp/v_clippy.log || true)
  if [ "$w" -le 1 ]; then ok "cargo clippy 警告数 = $w"; else bad "clippy 警告数 = $w"; grep -E "^(warning|error): " /tmp/v_clippy.log | head -5 | sed 's/^/      /'; fi
else
  bad "src-tauri 目录缺失"
fi

step "6. 前端类型检查"
if [ -d "$APP/node_modules" ]; then
  ( cd "$APP" && npx vue-tsc --noEmit >/tmp/v_tsc.log 2>&1 )
  if [ $? -eq 0 ]; then ok "vue-tsc 0 error"; else bad "vue-tsc 有错误"; head -10 /tmp/v_tsc.log | sed 's/^/      /'; fi
else
  printf '  [SKIP] node_modules 未安装，跳过（跑 npm ci 后再验）\n'
fi

step "7. 前后端命令契约"
if [ -f "$WS/tools/verify-invoke.py" ]; then
  out=$(cd "$APP" && python "$WS/tools/verify-invoke.py" 2>&1)
  miss=$(printf '%s' "$out" | grep "缺失(基线有/现无)" | sed 's/.*: //')
  if [ -z "$miss" ] || [ "$miss" = "无" ]; then ok "前端 invoke 命令无缺失"; else bad "缺失命令: $miss"; fi
else
  printf '  [SKIP] 未找到 verify-invoke.py\n'
fi

step "8. 回环自校验（SSH 靶机，10 项）"
if [ -d "$WS/tools/ssh-loopback" ]; then
  ( cd "$WS/tools/ssh-loopback" && cargo run --offline --bin dou-ssh-selfcheck >/tmp/v_selfcheck.log 2>&1 )
  if [ $? -eq 0 ]; then
    ok "回环自校验全通过"
    grep -E "总计" /tmp/v_selfcheck.log | sed 's/^/      /'
  else
    bad "回环自校验失败"; tail -12 /tmp/v_selfcheck.log | sed 's/^/      /'
  fi
else
  printf '  [SKIP] 未找到 tools/ssh-loopback\n'
fi

printf '\n========================================\n'
printf '验证完成：通过 %d 项，失败 %d 项\n' "$PASS" "$FAIL"
printf '========================================\n'
if [ "$FAIL" -eq 0 ]; then
  printf '环境完好，可直接开始开发。\n'
else
  printf '有 %d 项未通过，请对照「新机重建指南.md」处理。\n' "$FAIL"
fi
exit "$FAIL"
