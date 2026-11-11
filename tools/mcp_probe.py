#!/usr/bin/env python3
"""MCP 网关端到端自检脚本。

用途：不依赖任何 Agent 客户端，直接按 MCP Streamable HTTP 规范打网关，
验证“鉴权 → 协议握手 → 工具清单 → 策略判定 → 审计落盘”这条链路是否成立。

用法：
    python mcp_probe.py                 # 打默认 127.0.0.1:7419
    python mcp_probe.py --base http://127.0.0.1:7419/mcp
    python mcp_probe.py --expect-unauth  # 额外验证错令牌返回 401

退出码：0 全通过；1 有失败项。
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import socket
import sys
import urllib.error
import urllib.request

PROTOCOL_VERSION = "2025-06-18"


def default_token() -> str:
    env = os.environ.get("DOU_DEV_TOKEN", "").strip()
    if env:
        return env
    base = os.environ.get("LOCALAPPDATA", "")
    p = pathlib.Path(base) / "dou" / "token.txt"
    if p.exists():
        return p.read_text(encoding="utf-8").strip()
    return ""


class Client:
    def __init__(self, base: str, token: str) -> None:
        self.base = base
        self.token = token
        self.sid: str | None = None
        self._id = 0

    def _next_id(self) -> int:
        self._id += 1
        return self._id

    def post(self, payload: dict, *, auth: bool = True, expect_status: int | None = None):
        data = json.dumps(payload).encode("utf-8")
        headers = {
            "Content-Type": "application/json",
            "Accept": "application/json, text/event-stream",
        }
        if auth and self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        if self.sid:
            headers["Mcp-Session-Id"] = self.sid
        req = urllib.request.Request(self.base, data=data, headers=headers, method="POST")
        try:
            with urllib.request.urlopen(req, timeout=10) as resp:
                body = resp.read().decode("utf-8", "replace")
                if self.sid is None:
                    self.sid = resp.headers.get("Mcp-Session-Id")
                return resp.status, body
        except urllib.error.HTTPError as e:
            return e.code, e.read().decode("utf-8", "replace")
        except urllib.error.URLError as e:
            return None, f"URLError: {e.reason}"

    def rpc(self, method: str, params: dict | None = None, *, notify: bool = False):
        payload: dict = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            payload["params"] = params
        if not notify:
            payload["id"] = self._next_id()
        return self.post(payload)

    def call_tool(self, name: str, args: dict):
        return self.rpc("tools/call", {"name": name, "arguments": args})


def tool_text(body: str) -> str:
    try:
        obj = json.loads(body)
    except json.JSONDecodeError:
        return body[:200]
    result = obj.get("result") or {}
    content = result.get("content") or []
    if content and isinstance(content[0], dict):
        return content[0].get("text", "")
    return json.dumps(obj, ensure_ascii=False)[:200]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", default=os.environ.get("DOU_BASE", "http://127.0.0.1:7419/mcp"))
    ap.add_argument("--token", default=default_token())
    args = ap.parse_args()

    checks: list[tuple[str, bool, str]] = []

    def check(name: str, ok: bool, detail: str = "") -> None:
        checks.append((name, ok, detail))
        print(f"{'PASS' if ok else 'FAIL'}  {name}" + (f"  ·  {detail}" if detail else ""))

    # 0. 端口可达
    host_port = args.base.split("//", 1)[-1].split("/", 1)[0]
    host, _, port = host_port.partition(":")
    try:
        with socket.create_connection((host, int(port or 80)), timeout=2):
            pass
        check("回环端口可达", True, host_port)
    except OSError as e:
        print(f"FAIL  回环端口不可达 · {e}（网关是否已启动？）")
        return 1

    c = Client(args.base, args.token)

    # 1. initialize
    status, body = c.rpc(
        "initialize",
        {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "dou-probe", "version": "0.1.0"},
        },
    )
    ok = status == 200 and '"serverInfo"' in body
    detail = ""
    if ok:
        obj = json.loads(body)
        detail = f"session={c.sid or '-'} version={obj['result'].get('protocolVersion')}"
    check("initialize 握手", ok, detail)

    # 2. notifications/initialized （无 id，规范要求不回包）
    status, _ = c.rpc("notifications/initialized", notify=True)
    check("notifications/initialized 返回 202", status == 202, f"status={status}")

    # 3. tools/list
    status, body = c.rpc("tools/list")
    names: list[str] = []
    if status == 200:
        obj = json.loads(body)
        names = [t["name"] for t in obj.get("result", {}).get("tools", [])]
    expected = {"dou.session_list", "dou.exec", "dou.read", "dou.put", "dou.get"}
    check("tools/list 五个工具齐全", set(names) == expected, ",".join(names))

    # 4. 只读命令 → auto（用跨平台命令，避免 Windows 无 ls）
    status, body = c.call_tool("dou.exec", {"session": "s1", "command": "whoami"})
    text = tool_text(body)
    check("只读命令被判为放行", '"status": "executed_local_dev"' in text or '"status":"executed_local_dev"' in text, text[:120])

    # 5. 变更命令 → pending
    status, body = c.call_tool("dou.exec", {"session": "s1", "command": "rm -f out.txt"})
    check("变更命令进入待审批", "pending_approval" in tool_text(body), tool_text(body)[:120])

    # 6. 命令夹带 → 绝不能被整体放行（判待审或阻断都算正确）
    status, body = c.call_tool("dou.exec", {"session": "s1", "command": "ls; rm -f out.txt"})
    t6 = tool_text(body)
    check("只读+变更夹带未被自动放行", ("pending_approval" in t6 or "blocked" in t6), t6[:120])

    # 7. 危险命令 → blocked
    status, body = c.call_tool("dou.exec", {"session": "s1", "command": "sudo rm -rf /"})
    check("提权删除被阻断", "blocked" in tool_text(body), tool_text(body)[:120])

    # 8. 错令牌 → 401
    bad = Client(args.base, "dou_ak_deadbeef")
    status, _ = bad.rpc("tools/list")
    check("错令牌被拒", status == 401, f"status={status}")

    # 9. 审计落盘
    base = os.environ.get("LOCALAPPDATA", "")
    audit = pathlib.Path(base) / "dou" / "audit.jsonl"
    if audit.exists():
        lines = sum(1 for _ in audit.open(encoding="utf-8"))
        check("审计文件可写且非空", lines > 0, f"{lines} 条 · {audit}")
    else:
        check("审计文件可写且非空", False, "audit.jsonl 不存在")

    failed = [c for c in checks if not c[1]]
    print()
    print(f"结果：{len(checks) - len(failed)}/{len(checks)} 通过")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
