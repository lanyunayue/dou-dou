#!/usr/bin/env python3
"""W-15 unified auth: negative-case matrix over all 14 REST endpoints.

Run against a live gateway:
    set DOU_TOKEN=<token> && python tools\\test_auth_matrix.py

Scenarios: missing token / wrong token / master token / device token on M-only.
Each wrong-token attempt uses a DISTINCT value so the throttle counter does
not turn a 401 into a 429 (that is tested separately in test_throttle.py).
"""

import json
import os
import pathlib
import sys
import urllib.error
import urllib.request

BASE = os.environ.get("DOU_BASE", "http://127.0.0.1:7419")
EXEMPT = {"/health"}
M_ONLY = {
    "/approve",
    "/audit/export",
    "/onboard/write",
    "/onboard/config",
    "/console",
    "/scope/tokens",
    "/scope/revoke",
}
ENDPOINTS = [
    ("POST", "/mcp"),
    ("GET", "/health"),
    ("GET", "/audit"),
    ("GET", "/audit/query"),
    ("GET", "/audit/verify"),
    ("GET", "/audit/export"),
    ("GET", "/tasks"),
    ("POST", "/approve"),
    ("GET", "/scope/tokens"),
    ("POST", "/scope/revoke"),
    ("GET", "/onboard/config"),
    ("POST", "/onboard/write"),
    ("GET", "/console"),
]


def load_token() -> str:
    t = os.environ.get("DOU_TOKEN", "").strip() or os.environ.get("DOU_DEV_TOKEN", "").strip()
    if t:
        return t
    base = os.environ.get("LOCALAPPDATA") or os.environ.get("HOME") or "."
    try:
        return (pathlib.Path(base) / "dou" / "token.txt").read_text(encoding="utf-8").strip()
    except OSError:
        print("!! set DOU_TOKEN or ensure %LOCALAPPDATA%\\dou\\token.txt exists", file=sys.stderr)
        sys.exit(2)


def call(method: str, path: str, token: str | None, payload: dict | None = None):
    data = json.dumps(payload or {}).encode("utf-8") if method == "POST" else None
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(BASE + path, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=5) as r:
            return r.status, r.read().decode("utf-8", "replace"), dict(r.headers)
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace"), dict(e.headers)
    except OSError as e:
        return None, str(e), {}


def main() -> int:
    master = load_token()
    checks = []

    def check(name, ok, detail=""):
        checks.append((name, ok, detail))
        print(f"{'PASS' if ok else 'FAIL'}  {name}" + (f"  ·  {detail}" if detail else ""))

    # 0. gateway reachable
    status, _, _ = call("GET", "/health", None)
    if status != 200:
        print(f"FAIL  gateway not reachable on {BASE}")
        return 1

    # 1. /health is the ONLY exempt path, and returns minimal info
    status, body, _ = call("GET", "/health", None)
    ok = status == 200 and "status" in body and "token" not in body and "version" not in body
    check("/health exempt + minimal body", ok, f"{status} {body[:80]}")

    # 2. every other endpoint rejects missing and wrong tokens
    for i, (method, path) in enumerate(ENDPOINTS):
        if path in EXEMPT:
            continue
        payload = {"jsonrpc": "2.0", "id": 1, "method": "ping"} if path == "/mcp" else {}
        s_missing, _, _ = call(method, path, None, payload)
        distinct_wrong = f"dou_ak_wrong{i:04d}deadbeef"
        s_wrong, _, _ = call(method, path, distinct_wrong, payload)
        # All anonymous requests share ONE throttle subject (empty presented
        # value), so after the failure threshold they legitimately return 429
        # instead of 401. Both are rejections. The wrong-token probe uses a
        # distinct subject per endpoint and therefore must stay 401.
        check(
            f"{method} {path} rejects anonymous/wrong",
            s_missing in (401, 429) and s_wrong == 401,
            f"missing={s_missing} wrong={s_wrong}",
        )

    # 3. master token gets through everywhere (400/500 from empty body is fine, never 401/403)
    for method, path in ENDPOINTS:
        if path == "/mcp":
            payload = {"jsonrpc": "2.0", "id": 1, "method": "ping"}
        elif path == "/onboard/write":
            # codex deliberately refuses config write -> 400, NO external side effect
            payload = {"client": "codex"}
        else:
            payload = {}
        status, _, _ = call(method, path, master, payload)
        check(f"{method} {path} accepts master", status not in (None, 401, 403), f"status={status}")

    # 4. device token cannot reach M-only endpoints
    status, body, _ = call(
        "POST", "/scope/tokens", master, {"tier": "observe", "client_id": "auth-matrix"}
    )
    device = None
    try:
        device = json.loads(body).get("token")
    except ValueError:
        pass
    if not device:
        check("device token issued", False, body[:120])
    else:
        check("device token issued", True)
        for method, path in [
            ("GET", "/audit/export"),
            ("GET", "/console"),
            ("GET", "/onboard/config"),
            ("GET", "/scope/tokens"),
        ]:
            status, _, _ = call(method, path, device)
            check(f"M-only {path} blocks device token", status == 403, f"status={status}")
        # device token IS allowed on /mcp
        status, _, _ = call(
            "POST", "/mcp", device, {"jsonrpc": "2.0", "id": 1, "method": "ping"}
        )
        check("device token allowed on /mcp", status == 200, f"status={status}")

    failed = [c for c in checks if not c[1]]
    print(f"\nresult: {len(checks) - len(failed)}/{len(checks)} passed")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
