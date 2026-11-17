"""端到端探测：确认 dou.exec 确实经 SSH 在远端执行。"""
import json, os, pathlib, urllib.request

TOKEN = pathlib.Path(os.environ["LOCALAPPDATA"]) / "dou" / "token.txt"
token = TOKEN.read_text(encoding="utf-8").strip()
URL = "http://127.0.0.1:7419/mcp"


def rpc(method, params=None, rid=1):
    body = json.dumps({"jsonrpc": "2.0", "id": rid, "method": method,
                       **({"params": params} if params else {})}).encode()
    req = urllib.request.Request(URL, data=body, headers={
        "Content-Type": "application/json",
        "Authorization": f"Bearer {token}",
    })
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read().decode())


out = []
out.append(("initialize", rpc("initialize", {}, 1).get("result", {}).get("serverInfo")))

r = rpc("tools/call", {"name": "dou.session_list", "arguments": {}}, 2)
out.append(("session_list", r["result"]["content"][0]["text"]))

r = rpc("tools/call", {"name": "dou.exec", "arguments": {
    "session": "default", "command": "cat /proc/version"}}, 3)
out.append(("exec_proc_version", r["result"]["content"][0]["text"]))

r = rpc("tools/call", {"name": "dou.exec", "arguments": {
    "session": "default", "command": "ls /home/dou"}}, 4)
out.append(("exec_ls_home", r["result"]["content"][0]["text"]))

r = rpc("tools/call", {"name": "dou.exec", "arguments": {
    "session": "default", "command": "ls; rm -rf ~"}}, 5)
out.append(("exec_chained", r["result"]["content"][0]["text"]))

for k, v in out:
    print(f"### {k}\n{v}\n")
