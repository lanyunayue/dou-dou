#!/usr/bin/env python3
"""Anti-brute-force pressure test (W-11 persistent state + W-15 throttle).

Run against a live gateway:
    set DOU_TOKEN=<token> && python tools\\test_throttle.py

Verifies:
  1. N consecutive failures from ONE subject trigger 429 with Retry-After.
  2. A legitimate master token stays usable during the attack (anti-DoS).
  3. The lock is persisted in SQLite auth_throttle_state (survives restart).
  4. A different wrong-token subject is NOT locked by the attacker's subject.
"""

import json
import os
import pathlib
import sqlite3
import sys
import time
import urllib.error
import urllib.request

BASE = os.environ.get("DOU_BASE", "http://127.0.0.1:7419")
MAX_FAILURES = int(os.environ.get("DOU_THROTTLE_MAX_FAILURES", "5"))
BAD_TOKEN = "dou_ak_bruteforce_probe_0001"


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


def call(path: str, token: str | None, payload=None):
    data = json.dumps(payload or {}).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(BASE + path, data=data, method="POST", headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=5) as r:
            return r.status, dict(r.headers), r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), e.read().decode("utf-8", "replace")
    except OSError as e:
        return None, {}, str(e)


def db_path() -> pathlib.Path:
    p = os.environ.get("DOU_DB_PATH")
    if p:
        return pathlib.Path(p)
    base = os.environ.get("LOCALAPPDATA") or os.environ.get("HOME") or "."
    return pathlib.Path(base) / "dou" / "audit.db"


def main() -> int:
    master = load_token()
    checks = []

    def check(name, ok, detail=""):
        checks.append((name, ok, detail))
        print(f"{'PASS' if ok else 'FAIL'}  {name}" + (f"  ·  {detail}" if detail else ""))

    ping = {"jsonrpc": "2.0", "id": 1, "method": "ping"}

    # gateway up?
    s, _, _ = call("/mcp", master, ping)
    if s != 200:
        print(f"FAIL  gateway not reachable (status={s})")
        return 1

    # 1. hammer one wrong-token subject
    statuses = []
    headers = {}
    for _ in range(MAX_FAILURES + 2):
        s, h, _ = call("/mcp", BAD_TOKEN, ping)
        statuses.append(s)
        headers = h
    check(
        "repeated failures eventually return 429",
        429 in statuses,
        f"statuses={statuses}",
    )
    has_retry = any(
        k.lower() == "retry-after" for k in headers
    ) or "Retry-After" in headers
    check("429 carries Retry-After", has_retry, f"headers={list(headers)[:8]}")

    # 2. anti-DoS: master still works while the bad subject is locked
    s, _, _ = call("/mcp", master, ping)
    check("legitimate master unaffected by attack", s == 200, f"status={s}")

    # 3. lock persisted in SQLite (restart-safe)
    try:
        conn = sqlite3.connect(str(db_path()))
        rows = conn.execute(
            "SELECT subject_key, failures, locked_until_ms FROM auth_throttle_state WHERE failures > 0"
        ).fetchall()
        conn.close()
        check(
            "throttle state persisted in SQLite",
            len(rows) > 0,
            f"{len(rows)} row(s)",
        )
        now_ms = int(time.time() * 1000)
        check(
            "lock timestamp is in the future",
            any(r[2] > now_ms for r in rows),
            f"now={now_ms}",
        )
    except sqlite3.Error as e:
        check("throttle state persisted in SQLite", False, str(e))

    # 4. a different wrong-token subject is not contaminated
    s, _, _ = call("/mcp", "dou_ak_other_subject_9999", ping)
    check("different subject not locked by attacker", s == 401, f"status={s}")

    # 5. /health never counts towards throttling
    req = urllib.request.Request(BASE + "/health")
    try:
        with urllib.request.urlopen(req, timeout=5) as r:
            hs = r.status
    except urllib.error.HTTPError as e:
        hs = e.code
    check("/health exempt from throttling", hs == 200, f"status={hs}")

    failed = [c for c in checks if not c[1]]
    print(f"\nresult: {len(checks) - len(failed)}/{len(checks)} passed")
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
