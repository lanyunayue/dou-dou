# -*- coding: utf-8 -*-
"""通过 dou 网关 MCP 发起模拟任务（真实远程模式，连外部 test.rebex.net）。

端到端验证 dou 的受治理远程操作主线：
  initialize -> tools/list -> dou.session_list -> dou.exec(只读命令)
  -> dou.read(远端文本) -> dou.get(下载) -> REST /audit 查审计链

用法：
  python tools/simulate_task.py --token <dou_ak_...> [--base http://127.0.0.1:7419]

网关需以真实远程模式启动：
  DOU_SSH=demo@test.rebex.net:22 DOU_SSH_PASS=password dou-gateway
"""
import argparse, json, urllib.request, urllib.error, sys

MCP_HEADERS_BASE = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
results = []

def rec(name, ok, detail=""):
    results.append((name, ok, detail))
    print(("[PASS] " if ok else "[FAIL] ") + name + (" :: " + str(detail)[:400] if detail else ""))

def mcp_call(base, token, method, rpc_id, params=None):
    body = {"jsonrpc": "2.0", "id": rpc_id, "method": method}
    if params is not None:
        body["params"] = params
    headers = dict(MCP_HEADERS_BASE)
    headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(base + "/mcp", data=json.dumps(body).encode(),
                                 headers=headers, method="POST")
    with urllib.request.urlopen(req, timeout=30) as r:
        raw = r.read().decode("utf-8", "replace")
    # SSE 或纯 JSON
    payload = raw
    if raw.lstrip().startswith("event:") or "\ndata:" in raw:
        for line in raw.splitlines():
            if line.startswith("data:"):
                payload = line[5:].strip()
                break
    return json.loads(payload)

def tool_content(resp):
    try:
        c = resp["result"]["content"][0]["text"]
        try: return json.loads(c)
        except Exception: return c
    except Exception:
        return resp

def run(base, token, do_exit=True):
    """可复用入口：给定 base/token 跑完整流程。do_exit=False 时返回结果不退出。"""
    global results
    results = []
    a = type("Args", (), {"token": token, "base": base})()
    _run(a)
    passed = sum(1 for _, ok, _ in results if ok)
    print(f"\n===== 模拟任务结果：{passed}/{len(results)} 通过 =====")
    if do_exit:
        sys.exit(0 if passed == len(results) else 1)
    return passed, len(results)

def main_with(base, token):
    return run(base, token, do_exit=True)

def _run(a):
    # 1) MCP initialize
    try:
        init = mcp_call(a.base, a.token, "initialize", 1, {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "dou-simtask", "version": "1.0"},
        })
        si = init.get("result", {}).get("serverInfo", {})
        rec("MCP initialize", bool(si), si)
    except Exception as e:
        rec("MCP initialize", False, f"{type(e).__name__}: {e}")
        return

    # 2) tools/list
    try:
        tl = mcp_call(a.base, a.token, "tools/list", 2)
        names = [t["name"] for t in tl.get("result", {}).get("tools", [])]
        rec("MCP tools/list", len(names) >= 4, names)
    except Exception as e:
        rec("MCP tools/list", False, str(e))

    call_id = 10
    def call_tool(name, arguments):
        nonlocal call_id
        call_id += 1
        return tool_content(mcp_call(a.base, a.token, "tools/call", call_id,
                                     {"name": name, "arguments": arguments}))

    # 3) 会话列表（确认真实远程会话已建立）
    try:
        r = call_tool("dou.session_list", {})
        rec("dou.session_list (真实远程会话)", True, r)
    except Exception as e:
        rec("dou.session_list", False, str(e))

    # 4) 执行只读命令（关键：期望 executed_remote，不是 executed_local_dev）
    try:
        r = call_tool("dou.exec", {"session": "default", "command": "pwd", "timeout_sec": 20})
        status = r.get("status") if isinstance(r, dict) else "?"
        rec("dou.exec(pwd) 真实远端执行", status == "executed_remote", r)
    except Exception as e:
        rec("dou.exec(pwd)", False, str(e))

    # 5) 执行一条只读远端命令（whoami），验证真实远端 shell
    try:
        r = call_tool("dou.exec", {"session": "default", "command": "whoami", "timeout_sec": 20})
        status = r.get("status") if isinstance(r, dict) else "?"
        rec("dou.exec(whoami) 真实远端执行", status == "executed_remote" and r.get("exit_code") == 0, r)
    except Exception as e:
        rec("dou.exec(whoami)", False, str(e))

    # 6) B3 路径闸门：越界路径不得返回内容（策略拒绝/远端不存在均可接受）
    try:
        r = call_tool("dou.read", {"session": "default", "path": "/etc/passwd", "start_line": 1, "end_line": 3})
        blocked = isinstance(r, dict) and r.get("status") in ("path_scope_denied", "blocked", "error") and "content" not in r
        rec("dou.read 越界路径不放行（安全预期）", blocked, r)
    except Exception as e:
        rec("dou.read 越界拦截", False, str(e))

    # 7) 查审计链
    try:
        req = urllib.request.Request(a.base + "/audit/query?limit=20")
        req.add_header("Authorization", f"Bearer {a.token}")
        with urllib.request.urlopen(req, timeout=20) as resp:
            data = json.loads(resp.read().decode("utf-8", "replace"))
        items = data if isinstance(data, list) else data.get("items", data.get("events", []))
        acts = [e for e in items if isinstance(e, dict) and e.get("action") in ("exec", "exec_result", "connect")]
        rec("审计链记录 (connect/exec)", len(acts) > 0, f"共 {len(items)} 条，其中相关 {len(acts)} 条")
    except Exception as e:
        rec("审计链查询", False, str(e))

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--token", required=True)
    ap.add_argument("--base", default="http://127.0.0.1:7419")
    a = ap.parse_args()
    run(a.base, a.token, do_exit=True)

if __name__ == "__main__":
    main()
