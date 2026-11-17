# -*- coding: utf-8 -*-
"""真实外部 SSH/SFTP 连通性验证：test.rebex.net (demo/password)。
验证豆 Dou 将要走的链路：外网可达 -> 主机密钥指纹(TOFU) -> 密码认证 -> SFTP 列目录/下载 -> keepalive。
只读公开测试服务器，不做任何写操作。"""
import sys, hashlib, base64, socket
import paramiko

HOST = "test.rebex.net"
PORT = 22
USER = "demo"
PASS = "password"
results = []

def rec(name, ok, detail=""):
    results.append((name, ok, detail))
    print(("[PASS] " if ok else "[FAIL] ") + name + (" :: " + detail if detail else ""))

# 1) DNS + TCP 可达
try:
    ip = socket.gethostbyname(HOST)
    rec("DNS 解析", True, f"{HOST} -> {ip}")
except Exception as e:
    rec("DNS 解析", False, str(e)); ip = None

client = paramiko.SSHClient()
client.set_missing_host_key_policy(paramiko.AutoAddPolicy())  # 等价 TOFU：首次接受并记录
try:
    client.connect(HOST, port=PORT, username=USER, password=PASS,
                   timeout=20, allow_agent=False, look_for_keys=False,
                   banner_timeout=20, auth_timeout=20)
    rec("TCP+SSH 握手+密码认证", True, f"{USER}@{HOST}:{PORT}")

    # 2) 主机密钥指纹（TOFU 展示用）
    pk = client.get_transport().get_remote_server_key()
    raw = pk.asbytes()
    sha256 = base64.b64encode(hashlib.sha256(raw).digest()).decode().rstrip("=")
    rec("主机密钥指纹(TOFU)", True, f"{pk.get_name()} SHA256:{sha256}")

    # 3) keepalive（对应 R12 会话保活机制）
    tr = client.get_transport()
    try:
        tr.send_ignore(1234)  # paramiko 此版本要求 int
        rec("SSH keepalive 通道存活", tr.is_active())
    except Exception as ke:
        rec("SSH keepalive 通道存活", tr.is_active(), f"(send_ignore 兼容:{ke})")

    # 4) SFTP 列目录（对应文件面板）
    sftp = client.open_sftp()
    try:
        entries = sftp.listdir("/")
        rec("SFTP 列目录", True, f"/ 共 {len(entries)} 项: {entries[:10]}")
    except Exception as e:
        rec("SFTP 列目录", False, str(e))

    # 5) SFTP 下载一个小文件（对应传输队列/下载）
    downloaded = False
    for cand in ["/pub/example/readme.txt", "/readme.txt", "/pub/example/WinFormClient.png"]:
        try:
            st = sftp.stat(cand)
            got = sftp.file(cand, "rb").read(64)
            rec("SFTP 下载(只读抽样)", True, f"{cand} size={st.st_size} 首64字节={got[:32]!r}")
            downloaded = True
            break
        except Exception:
            continue
    if not downloaded:
        rec("SFTP 下载(只读抽样)", False, "常见示例文件路径均不可读（服务器目录结构可能不同）")
    sftp.close()
    client.close()
except Exception as e:
    rec("TCP+SSH 握手+密码认证", False, f"{type(e).__name__}: {e}")

passed = sum(1 for _, ok, _ in results if ok)
print(f"\n===== 结果：{passed}/{len(results)} 通过 =====")
sys.exit(0 if passed == len(results) else 1)
