import json, os, pathlib, urllib.request, tempfile, datetime

TOKEN = pathlib.Path(os.environ["LOCALAPPDATA"]) / "dou" / "token.txt"
token = TOKEN.read_text(encoding="utf-8").strip()
URL = "http://127.0.0.1:7419/mcp"

def rpc(method, params=None, rid=1):
    body = json.dumps({"jsonrpc":"2.0","id":rid,"method":method,
        **({"params": params} if params else {})}).encode()
    req = urllib.request.Request(URL, data=body, headers={
        "Content-Type":"application/json","Authorization":f"Bearer {token}"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read().decode())

def text(r):
    return r["result"]["content"][0]["text"]

print("### init:", rpc("initialize", {}, 1).get("result",{}).get("serverInfo"))

print("\n### dou.read /etc/hostname")
print(text(rpc("tools/call", {"name":"dou.read","arguments":{"path":"/etc/hostname"}}, 2)))

print("\n### dou.read /home/dou/.bashrc 1-3")
print(text(rpc("tools/call", {"name":"dou.read","arguments":{"path":"/home/dou/.bashrc","start_line":1,"end_line":3}}, 3)))

local = os.path.join(tempfile.gettempdir(), "dou_put_test.txt")
open(local,"w",encoding="utf-8").write("hello from dou at " + datetime.datetime.now().isoformat())
print("\n### dou.put -> /home/dou/dou_put_test.txt")
print(text(rpc("tools/call", {"name":"dou.put","arguments":{"remote_path":"/home/dou/dou_put_test.txt","local_ref":local}}, 4)))

print("\n### dou.get <- /home/dou/dou_put_test.txt")
print(text(rpc("tools/call", {"name":"dou.get","arguments":{"remote_path":"/home/dou/dou_put_test.txt"}}, 5)))
