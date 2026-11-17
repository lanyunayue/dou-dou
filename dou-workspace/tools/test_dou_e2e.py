#!/usr/bin/env python3
"""test_dou_e2e.py - Dou gateway end-to-end verification asset (QA / Yan).

Purpose
-------
Baseline + final-regression checker for the "1->2->3 make it truly usable" round.
With the master token injected AT RUNTIME, it proves the gateway's management
endpoints work end-to-end (no 401/403/429, no plaintext master-token echo),
that /health stays exempt, and that the SSH target restriction is reported
honestly (RESTRICTED != PASS, never counted into N/M).

Token injection (NEVER hardcoded)
---------------------------------
  1. DOU_TOKEN environment variable (QA injection channel), if non-empty.
  2. Otherwise %LOCALAPPDATA%/dou/token.txt, using the SAME fallback chain as
     the gateway auth.rs:14-20: LOCALAPPDATA -> USERPROFILE -> HOME -> "."
     then join("dou").join("token.txt").
The token is held only in a local variable: never printed, never written to
disk, never placed in a URL/query/body, and every response body is redacted
before it reaches stdout.

Run
---
    set DOU_TOKEN=<runtime-token> && python tools\\test_dou_e2e.py
    python tools\\test_dou_e2e.py                 # reads token.txt
Optional: DOU_BASE (default http://127.0.0.1:7419).

Endpoints (9 = 6 GET + 3 POST)
------------------------------
    GET  /onboard/config?client=workbuddy   GET  /audit/query?source=agent&limit=30
    GET  /tasks                              GET  /scope/tokens
    GET  /console                            GET  /audit/export
    POST /approve                            POST /scope/tokens   POST /scope/revoke
POST uses an empty business body ({}): 400/500 are accepted, 401/403/429 are
forbidden. Note: POST /scope/tokens legitimately issues a NEW device credential
(gateway default for empty body); that is NOT a master-token echo and is
redacted from stdout. It is a known, auditable side effect - revoke after use.

Assertions (one PASS/FAIL line each)
------------------------------------
  * /health: no Authorization header, 200, exact body {"status":"ok"}
  * per endpoint: status + "no master token literal in body"
  * /tasks: pending-task count readable and reported
  * /onboard/config: no full master token + ${DOU_TOKEN} placeholder preserved
  * 22/tcp: RESTRICTED: SSH target unavailable (informational, NOT in N/M)

Exit codes
----------
  0  all HTTP/E2E assertions passed (N/M passed, full green)
  1  at least one assertion FAILED
  2  NOT RUNNING / environment not ready (gateway unreachable OR token missing).
     A third state: neither forged PASS nor false FAIL. The gateway is NEVER
     started by this script (hard boundary).

Boundaries honored: no source/gateway edits, no long-running service, no
wsl.exe, no bypass, no forged pass. On an offline gateway the script degrades
gracefully to NOT RUNNING instead of crashing or faking green.
"""

import json
import os
import pathlib
import re
import socket
import sys
import urllib.error
import urllib.request

BASE = os.environ.get("DOU_BASE", "http://127.0.0.1:7419")
SSH_HOST, SSH_PORT = "127.0.0.1", 22
TIMEOUT = 5

# Loopback MUST bypass any system HTTP proxy. This environment sets
# HTTP_PROXY/HTTPS_PROXY=http://127.0.0.1:52668; urllib.request.urlopen would
# forward 127.0.0.1:7419 to that proxy and get a 502 "upstream connect failed"
# instead of a real network error - which misreports "gateway down" as a pile
# of FAILs. An opener with an empty ProxyHandler connects DIRECTLY to loopback.
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))

GET_ENDPOINTS = [
    "/onboard/config?client=workbuddy",
    "/audit/query?source=agent&limit=30",
    "/tasks",
    "/scope/tokens",
    "/console",
    "/audit/export",
]
POST_ENDPOINTS = [
    "/approve",
    "/scope/tokens",
    "/scope/revoke",
]
FORBIDDEN = (401, 403, 429)
EMPTY_BODY = b"{}"  # empty business body; 400/500 acceptable

# Any token-looking literal (master OR freshly issued device) is redacted before
# printing. This regex literal itself does NOT match the gate regex
# dou_ak_[0-9a-f]{24,} (it is followed by '[', not by 24+ hex chars).
TOKEN_LITERAL = re.compile(r"dou_ak_[0-9A-Za-z]+")


class TokenError(Exception):
    """Raised when no runtime token can be obtained (fail-closed)."""


def load_token() -> tuple[str, str]:
    """Runtime injection only. Returns (token, source_label). Never hardcodes."""
    env = os.environ.get("DOU_TOKEN", "").strip()
    if env:
        return env, "DOU_TOKEN env var"
    base = (
        os.environ.get("LOCALAPPDATA")
        or os.environ.get("USERPROFILE")
        or os.environ.get("HOME")
        or "."
    )
    path = pathlib.Path(base) / "dou" / "token.txt"
    try:
        t = path.read_text(encoding="utf-8").strip()
    except OSError as e:
        raise TokenError(f"cannot read {path}: {e}") from e
    if not t:
        raise TokenError(f"{path} is empty")
    return t, f"token.txt (base={base})"


def redact(s: str, token: str) -> str:
    """Never let ANY token literal reach stdout."""
    if token:
        s = s.replace(token, "[REDACTED]")
    return TOKEN_LITERAL.sub("[REDACTED]", s)


def call(method: str, path: str, token: str | None, body: bytes | None = None):
    """Returns (status, text). status is None on network error (text=reason)."""
    headers: dict[str, str] = {}
    if body is not None:
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(BASE + path, data=body, method=method, headers=headers)
    try:
        with OPENER.open(req, timeout=TIMEOUT) as r:
            return r.status, r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace")
    except OSError as e:
        return None, str(e)


def probe_ssh() -> bool:
    """True only if 127.0.0.1:22 accepts a TCP connection. No SSH handshake."""
    s = socket.socket()
    s.settimeout(TIMEOUT)
    try:
        return s.connect_ex((SSH_HOST, SSH_PORT)) == 0
    finally:
        s.close()


def main() -> int:
    print("=== dou gateway E2E verification (QA / Yan) ===")
    print(f"BASE = {BASE}")

    # --- A. Token: runtime injection, fail-closed, value never printed ---
    try:
        token, source = load_token()
    except TokenError as e:
        print(f"ERROR  token unavailable: {e}")
        print("NOT RUNNING: no token -> no unauthenticated request sent (fail-closed)")
        return 2
    print(f"INFO  token source: {source} | value=[REDACTED] | len={len(token)}")

    # --- B. SSH target: informational ONLY, excluded from N/M (restricted != pass) ---
    if probe_ssh():
        print(f"INFO  SSH target {SSH_HOST}:{SSH_PORT} reachable "
              "(informational only, NOT asserted as a pass)")
    else:
        print(f"RESTRICTED: SSH target unavailable ({SSH_HOST}:{SSH_PORT} CLOSED) "
              "- restricted != pass, excluded from N/M")

    checks: list[tuple[str, bool, str]] = []

    def check(name: str, ok: bool, detail: str = "") -> None:
        checks.append((name, bool(ok), detail))
        line = f"{'PASS' if ok else 'FAIL'}  {name}"
        if detail:
            line += f"  |  {detail}"
        print(line)

    # --- C. Gateway reachability via exempt /health (NO Authorization header) ---
    status, health_body = call("GET", "/health", None)
    if status is None:
        print(f"NOT RUNNING: gateway not reachable on {BASE} "
              f"({redact(health_body, token)})")
        print("NOT RUNNING: gateway NOT started by this script (hard boundary); "
              "HTTP assertions were NOT executed.")
        print("NOT RUNNING: neither pass nor fail - re-run after the gateway is up "
              "(final regression).")
        return 2

    # AC-08 / V11: /health exempt, minimal exact body, no auth, no token echo
    ok = (
        status == 200
        and health_body.strip() == '{"status":"ok"}'
        and token not in health_body
    )
    check('/health exempt, no auth, body={"status":"ok"}', ok,
          f"status={status} body={redact(health_body.strip()[:80], token)}")

    # --- D. 9 endpoints: status + no-master-token-leak, one PASS/FAIL line each ---
    responses: dict[str, tuple[int | None, str]] = {}

    for path in GET_ENDPOINTS:
        st, body = call("GET", path, token)
        responses[path] = (st, body)
        check(f"GET  {path}  status", st == 200 and st not in FORBIDDEN,
              f"status={st} (expect 200, forbid {FORBIDDEN})")
        check(f"GET  {path}  no-master-token-leak", st is not None and token not in body,
              f"body={redact(body[:80], token)}")

    for path in POST_ENDPOINTS:
        st, body = call("POST", path, token, body=EMPTY_BODY)
        responses[path] = (st, body)
        check(f"POST {path}  status", st is not None and st not in FORBIDDEN,
              f"status={st} (empty body: 400/500 ok, forbid {FORBIDDEN})")
        check(f"POST {path}  no-master-token-leak", st is not None and token not in body,
              f"body={redact(body[:80], token)}")

    # --- E1. /tasks: read pending tasks and report count (AC-03 / V2) ---
    st, body = responses.get("/tasks", (None, ""))
    count = None
    ok = False
    try:
        obj = json.loads(body)
        count = obj.get("count")
        ok = st == 200 and isinstance(count, int) and isinstance(obj.get("items"), list)
    except (ValueError, TypeError):
        ok = False
    check("/tasks pending count readable", ok,
          f"count={count} (pending tasks visible to client)")
    print(f"INFO  /tasks count = {count}")

    # --- E2. /onboard/config: no full master token + placeholder preserved (V4/V5) ---
    cfg_path = "/onboard/config?client=workbuddy"
    st, body = responses.get(cfg_path, (None, ""))
    check("/onboard/config no full master token", st == 200 and token not in body,
          f"status={st} body={redact(body[:120], token)}")
    check("/onboard/config ${DOU_TOKEN} placeholder preserved",
          "${DOU_TOKEN}" in body,
          "gateway keeps the placeholder; client substitutes locally")

    # --- F. Summary ---
    passed = sum(1 for _, ok, _ in checks if ok)
    total = len(checks)
    print(f"\nresult: {passed}/{total} passed")
    failed = [name for name, ok, _ in checks if not ok]
    if failed:
        print("FAILED: " + " | ".join(failed))
        return 1
    print("ALL GREEN (HTTP/E2E assertions). SSH target remains RESTRICTED/INFO - "
          "restricted != pass; AC-05 stays NOT TESTED until a real target exists.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
