import pathlib, re, sys

root = pathlib.Path("E:/dou-app/src")
pat = re.compile(r'invoke(?:<[^>]*>)?\(\s*["\']([a-z_0-9]+)["\']')
found = {}
for p in root.rglob("*"):
    if p.suffix in (".vue", ".ts"):
        txt = p.read_text(encoding="utf-8")
        for m in pat.finditer(txt):
            found.setdefault(m.group(1), []).append(f"{p.name}:{txt[:m.start()].count(chr(10))+1}")

base = set("""gateway_request gateway_status gateway_token host_key_forget host_key_list
open_pty pty_write remote_stat sftp_download sftp_list sync_folders tq_cancel tq_clear
tq_enqueue tq_list tq_pause tq_resume vault_add_target vault_get_key vault_init
vault_list_targets vault_lock vault_remove_target vault_unlock""".split())

cur = set(found)
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
