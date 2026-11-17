# -*- coding: utf-8 -*-
"""经豆 Dou 网关 MCP 链路对公开只读靶机 test.rebex.net 做只读冒烟。
不做任何写操作；验证 Agent 通道真实走远端：session_list / exec 只读 / read 示例文件。"""
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


def text(r):
    return r["result"]["content"][0]["text"]


print("### initialize:", rpc("initialize", {}, 1).get("result", {}).get("serverInfo"))

print("\n### dou.session_list")
print(text(rpc("tools/call", {"name": "dou.session_list", "arguments": {}}, 2)))

print("\n### dou.exec whoami")
print(text(rpc("tools/call", {"name": "dou.exec", "arguments": {
    "session": "default", "command": "whoami"}}, 3)))

print("\n### dou.exec uname -a")
print(text(rpc("tools/call", {"name": "dou.exec", "arguments": {
    "session": "default", "command": "uname -a"}}, 4)))

print("\n### dou.exec ls /pub/example")
print(text(rpc("tools/call", {"name": "dou.exec", "arguments": {
    "session": "default", "command": "ls -1p /pub/example"}}, 5)))

print("\n### dou.read /readme.txt (家目录根下)")
print(text(rpc("tools/call", {"name": "dou.read", "arguments": {
    "path": "/readme.txt"}}, 6)))

print("\n### dou.read /pub/example/readme.txt (家目录外共享目录)")
print(text(rpc("tools/call", {"name": "dou.read", "arguments": {
    "path": "/pub/example/readme.txt"}}, 7)))
