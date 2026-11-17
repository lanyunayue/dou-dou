# -*- coding: utf-8 -*-
"""自管理网关生命周期 + 通过 dou 网关 MCP 跑模拟任务（真实远程 test.rebex.net）。

用 subprocess 启动 dou-gateway（避免后台 shell 结束回收进程），等端口就绪后
调用 simulate_task 的 MCP 流程，最后 terminate 网关。
"""
import os, sys, time, socket, subprocess, argparse

import pathlib, sqlite3, time
GW = r"D:\dou-target\debug\dou-gateway.exe"
CWD = r"D:\dou\dou-handover-20261113-2139\dou-workspace"
HOST, PORT = "127.0.0.1", 7419
# 使用网关自身 token 文件中的主令牌（master），确保 /mcp 鉴权通过
TOKEN = pathlib.Path(r"C:\Users\PC\AppData\Local\dou\token.txt").read_text(encoding="utf-8").strip()

def wait_port(host, port, timeout=40):
    t0 = time.time()
    while time.time() - t0 < timeout:
        try:
            with socket.create_connection((host, port), timeout=1):
                return True
        except OSError:
            time.sleep(0.5)
    return False

def main():
    env = os.environ.copy()
    env["DOU_SSH"] = "demo@test.rebex.net:22"
    env["DOU_SSH_PASS"] = "password"
    # 不设 DOU_DEV_TOKEN：网关从 token.txt 读取主令牌，与调用方 TOKEN 一致
    env.pop("DOU_DEV_TOKEN", None)
    env["CARGO_TARGET_DIR"] = r"D:\dou-target"
    # 测试环境清理：删除可能过期的 default 会话租约（A5 30min TTL），确保本次获得新窗口
    try:
        db = r"C:\Users\PC\AppData\Local\dou\audit.db"
        con = sqlite3.connect(db)
        con.execute("delete from lease_state where session_id='default'")
        con.commit(); con.close()
    except Exception:
        pass
    log = open(r"D:\dou\_gateway_managed.txt", "w", encoding="utf-8")
    print("[simtask] starting dou-gateway (real remote mode) ...")
    proc = subprocess.Popen([GW], cwd=CWD, env=env, stdout=log, stderr=subprocess.STDOUT)
    try:
        if not wait_port(HOST, PORT):
            print("[simtask] gateway failed to listen")
            return 2
        print(f"[simtask] gateway listening on {HOST}:{PORT}")
        # 跑 MCP 任务（复用同目录 simulate_task 的逻辑）
        sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
        import simulate_task as st
        st.main_with(base=f"http://{HOST}:{PORT}", token=TOKEN)
        return 0
    finally:
        proc.terminate()
        try: proc.wait(timeout=8)
        except Exception:
            proc.kill(); proc.wait(timeout=5)
        log.close()
        # 兜底：确保无残留网关进程占用端口
        try:
            subprocess.run(["taskkill", "/F", "/PID", str(proc.pid)],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
        except Exception:
            pass
        print("[simtask] gateway stopped")

if __name__ == "__main__":
    sys.exit(main())
