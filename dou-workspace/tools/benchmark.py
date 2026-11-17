#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
性能指标实测（计划书表 14）。

可在本机直接测的项：代理请求开销(P95)、快照/审计写入耗时、审计检索耗时。
需打包或靶机环境的项（冷启动、包体、常驻内存、目录上传、大日志、扫描耗时）
在报告里给出测量口径与当前状态，不在此脚本伪造数值。

用法：
    python tools/benchmark.py [--gateway http://127.0.0.1:7419] [--runs 50]
"""
import argparse
import json
import os
import sqlite3
import statistics
import time
import urllib.request
from pathlib import Path

REPORT = []


def record(metric, target, measured, method, status, note=""):
    REPORT.append({
        "metric": metric, "target": target, "measured": measured,
        "method": method, "status": status, "note": note,
    })


def http_post(url, body, token=None, timeout=10):
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(url, data=data, method="POST")
    req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    t0 = time.perf_counter()
    with urllib.request.urlopen(req, timeout=timeout) as r:
        r.read()
    return (time.perf_counter() - t0) * 1000.0


def http_get(url, timeout=10):
    t0 = time.perf_counter()
    with urllib.request.urlopen(url, timeout=timeout) as r:
        r.read()
    return (time.perf_counter() - t0) * 1000.0


def load_token():
    base = Path(os.environ.get("LOCALAPPDATA", ".")) / "dou"
    tok = base / "token.txt"
    if tok.exists():
        return tok.read_text(encoding="utf-8").strip()
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--gateway", default="http://127.0.0.1:7419")
    ap.add_argument("--runs", type=int, default=50)
    args = ap.parse_args()

    token = load_token()

    # 1. 代理请求开销：本地往返 P95（不含远端命令执行）
    try:
        lat = []
        payload = {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}
        for _ in range(args.runs):
            lat.append(http_post(f"{args.gateway}/mcp", payload, token))
        lat.sort()
        p50 = statistics.median(lat)
        p95 = lat[int(len(lat) * 0.95) - 1]
        record("代理请求开销", "P95 ≤ 200 ms", f"P50={p50:.1f}ms P95={p95:.1f}ms",
               "循环 tools/list 本地往返，不含远端执行", "实测" if p95 < 200 else "超标")
    except Exception as e:
        record("代理请求开销", "P95 ≤ 200 ms", "网关不可达", "tools/list 往返", "阻塞", str(e))

    # 2. 审计检索：现有库按时间/来源筛选耗时
    try:
        ms = http_get(f"{args.gateway}/audit/query?limit=200")
        record("审计检索", "10 万条 1 s 内返回", f"{ms:.1f}ms（当前库样本量见 note）",
               "GET /audit/query?limit=200，本地库非冷启动", "实测",
               "10 万条口径需先灌入样本；当前为既有记录")
    except Exception as e:
        record("审计检索", "10 万条 1 s 内返回", "失败", "/audit/query", "阻塞", str(e))

    # 3. 快照/审计写入：内存 SQLite 同构表测单条 insert 耗时（不碰真实审计链，避免破坏哈希）
    try:
        mem = sqlite3.connect(":memory:")
        mem.execute(
            "CREATE TABLE audit (ts_ms TEXT, trace_id TEXT, source TEXT, action TEXT,"
            " target TEXT, verdict TEXT, detail TEXT, prev_hash TEXT, hash TEXT)"
        )
        t0 = time.perf_counter()
        for i in range(100):
            mem.execute(
                "INSERT INTO audit (ts_ms, trace_id, source, action, target, verdict, detail, prev_hash, hash)"
                " VALUES (?,?,?,?,?,?,?,?,?)",
                (str(int(time.time() * 1000)), f"b{i}", "bench", "bench", "-", "auto", "{}", "p", "h"),
            )
        mem.commit()
        write_ms = (time.perf_counter() - t0) * 10.0  # 折算单条
        record("快照/审计写入", "单次 ≤ 50 ms 不阻塞会话", f"{write_ms:.2f}ms/条",
               "内存库 100 条 insert 折算单条；真实库写路径含哈希链", "实测" if write_ms < 50 else "超标")
        mem.close()
    except Exception as e:
        record("快照/审计写入", "单次 ≤ 50 ms", "失败", "SQLite insert", "阻塞", str(e))

    # 4–9. 需打包/靶机环境，给出口径，不伪造数值
    record("冷启动时间", "≤ 3 s", "待打包实测", "进程启动到主窗口可交互，10 次中位数", "待实测")
    record("安装包体积", "≤ 30 MB", "待打包实测", "Windows 安装包压缩后大小", "待实测")
    record("常驻内存", "≤ 200 MB，8h 增长 < 15%", "待长跑实测", "开 3 会话+文件树，每小时采样", "待实测")
    record("目录上传", "1000 文件 90 s 内", "待靶机实测", "100Mbps 内网，平均单文件 200KB", "待实测")
    record("大日志打开", "500 MB 3 s 内可滚动", "待靶机实测", "打开后立即滚到底并回滚", "待实测")
    record("扫描耗时", "100 MB 5 s 内出结果", "待靶机实测", "特征库全量加载后单文件扫描", "待实测")

    print(json.dumps(REPORT, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
