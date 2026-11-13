#!/usr/bin/env bash
# 核对前后端 Tauri 命令契约一致性（供接手体检用）
set -u
cd /e/dou-app
OUT="E:/思考/dou/tools/.contract"
mkdir -p "$OUT"

# 前端：extract invoke("cmd_name"
python - "$OUT" <<'PY'
import re, pathlib, sys
out = pathlib.Path(sys.argv[1])
fe = pathlib.Path("src/App.vue").read_text(encoding="utf-8")
names = sorted(set(re.findall(r'invoke(?:<[^>]*>)?\(\s*["\']([a-z_0-9]+)["\']', fe)))
(out / "fe.txt").write_text("\n".join(names) + "\n", encoding="utf-8")
print("前端 invoke 命令数:", len(names))
print(" ".join(names))
PY

echo
python - "$OUT" <<'PY'
import re, pathlib, sys
out = pathlib.Path(sys.argv[1])
be_src = pathlib.Path("src-tauri/src/lib.rs").read_text(encoding="utf-8")
# #[tauri::command] 后面紧跟的 fn 名
names = sorted(set(re.findall(r'#\[tauri::command\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s+)?(?:async\s+)?fn\s+([a-z_0-9]+)', be_src)))
(out / "be.txt").write_text("\n".join(names) + "\n", encoding="utf-8")
print("Rust 导出命令数:", len(names))
print(" ".join(names))
PY

echo
echo "=== 前端调用了但 Rust 未导出（运行时必报错）==="
comm -23 "$OUT/fe.txt" "$OUT/be.txt" | tr '\n' ' '
echo
echo "=== Rust 导出但前端未调用（潜在死代码）==="
comm -13 "$OUT/fe.txt" "$OUT/be.txt" | tr '\n' ' '
echo
