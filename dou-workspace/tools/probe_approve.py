"""审批闭环探测：令牌运行时注入，仓库内不得出现真实凭据。

用法：
    set DOU_TOKEN=<运行时令牌> && python tools\\probe_approve.py
    或：确保 %LOCALAPPDATA%\\dou\\token.txt 存在（网关启动时生成）

注意：统一鉴权上线后，/tasks、/approve、/audit/* 同样需要 Bearer，
本脚本所有 HTTP 请求一律携带 Authorization。
"""

import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

BASE = os.environ.get("DOU_BASE", "http://127.0.0.1:7419")


def load_token() -> str:
    t = os.environ.get("DOU_TOKEN", "").strip()
    if t:
        return t
    base = os.environ.get("LOCALAPPDATA") or os.environ.get("HOME") or "."
    try:
        t = (Path(base) / "dou" / "token.txt").read_text(encoding="utf-8").strip()
        if t.startswith("dou_ak_") and len(t) > 16:
            return t
    except OSError:
        pass
    print(
        "!! 未找到令牌：请设置 DOU_TOKEN 环境变量，"
        "或确认 %LOCALAPPDATA%\\dou\\token.txt 存在",
        file=sys.stderr,
    )
    sys.exit(2)


TOKEN = load_token()


def headers() -> dict:
    return {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {TOKEN}",
    }


def mcp(method, params=None, id=1):
    body = json.dumps({"jsonrpc": "2.0", "id": id, "method": method, "params": params or {}}).encode("utf-8")
    req = urllib.request.Request(BASE + "/mcp", data=body, headers=headers())
    return json.loads(urllib.request.urlopen(req, timeout=10).read().decode("utf-8"))


def http(path, method="GET", body=None):
    data = json.dumps(body).encode("utf-8") if body else None
    req = urllib.request.Request(BASE + path, data=data, method=method, headers=headers())
    try:
        return urllib.request.urlopen(req, timeout=10).read().decode("utf-8")
    except urllib.error.HTTPError as e:
        return f"HTTP {e.code}: {e.read().decode('utf-8', errors='replace')}"


print("== 1. 发一个 pending 命令（rm 应触发审批）==")
r = mcp("tools/call", {"name": "dou.exec", "arguments": {"command": "rm -f /tmp/dou_approve_test.txt", "session": "default"}}, id=2)
print(json.dumps(r, ensure_ascii=False, indent=2))

content = r.get("result", {}).get("content", [])
task_id = None
for c in content:
    try:
        d = json.loads(c.get("text", "{}"))
        if "task_id" in d:
            task_id = d["task_id"]
    except (ValueError, TypeError):
        pass
print(f"\ntask_id = {task_id}")

if not task_id:
    print("!! 没拿到 task_id，说明策略没把 rm 判为 pending")
    sys.exit(1)

print("\n== 2. 待审任务列表 ==")
print(http("/tasks"))

print("\n== 3. 批准 ==")
print(http("/approve", "POST", {"task_id": task_id, "approve": True, "note": "测试批准"}))

print("\n== 4. 等 3 秒看结果 ==")
time.sleep(3)
out = json.loads(http("/tasks"))
for t in out["items"]:
    print(f"  {t['id']}  status={t['status']}  exit={t.get('exit_code')}")
    if t.get("stdout"):
        print(f"    stdout: {t['stdout'][:200]}")
    if t.get("stderr"):
        print(f"    stderr: {t['stderr'][:200]}")

print("\n== 5. 审计 CSV 导出 ==")
print(http("/audit/export")[:500])

print("\n== 6. 审计链 verify ==")
print(http("/audit/verify"))
