"""核对前端 invoke() 调用集与基线命令是否一致。

用法（在 dou-app/ 下执行）：
    python ../dou-workspace/tools/verify-invoke.py
    python ../dou-workspace/tools/verify-invoke.py D:/path/to/dou-app/src

不再硬编码任何机器的盘符：默认扫当前目录的 src/，也可传入路径覆盖。
"""
import pathlib, re, sys

root = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path("src")
if not root.is_dir():
    print(f"[ERROR] 扫描目录不存在: {root.resolve()}")
    print("请在 dou-app/ 下执行，或把 src/ 的绝对路径作为参数传入。")
    sys.exit(2)

pat = re.compile(r'invoke(?:<[^>]*>)?\(\s*["\']([a-z_0-9]+)["\']')
found = {}
for p in sorted(root.rglob("*")):
    if p.suffix in (".vue", ".ts"):
        txt = p.read_text(encoding="utf-8")
        for m in pat.finditer(txt):
            found.setdefault(m.group(1), []).append(f"{p.name}:{txt[:m.start()].count(chr(10))+1}")

# 基线 24 条：11-13 前端重构新增的保险库命令族，R8(c81c8aa) 起「自测连接」
# 入口被真会话连接（connectByTarget）取代，vault_test_connect 移出前端基线；
# Rust 侧保留该命令仅为兼容，不在前端基线内。
# 注：generate_handler![] 注册数与前端基线条数口径不同，部分命令前端不直接调用。
base = set("""gateway_request gateway_status gateway_token host_key_forget host_key_list
open_pty pty_write remote_stat sftp_download sftp_list sync_folders tq_cancel tq_clear
tq_enqueue tq_list tq_pause tq_resume vault_add_target vault_get_key vault_init
vault_list_targets vault_lock vault_remove_target vault_unlock""".split())

cur = set(found)
print(f"扫描目录: {root.resolve()}")
print(f"扫描到唯一命令数: {len(cur)}")
print(f"基线命令数: {len(base)}")
print()
print("缺失(基线有/现无):", sorted(base - cur) or "无")
print("新增(现有/基线无):", sorted(cur - base) or "无")
print()
print("=== 每个命令的调用位置 ===")
for k in sorted(found):
    print(f"  {k:<22} {len(found[k])}x  {', '.join(found[k])}")
print()
print("结论:", "✅ 完全一致" if base == cur else "❌ 有差异")
sys.exit(0 if base == cur else 1)
