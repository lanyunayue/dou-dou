#!/usr/bin/env bash
# ============================================================
# 豆 Dou · Windows 侧 SSH 隧道脚本（Git Bash 运行）
# 作用：把本机 127.0.0.1:22 通过隧道映射到 Linux 靶机，使 豆 客户端
#       （写死的 127.0.0.1:22 / 用户 dou / 密钥 id_ed25519）直接连上 Linux。
# 原理：隧道外层用 id_ed25519 以 dou 身份登入靶机；内层把本地 22 转发到
#       靶机本机 sshd（默认 22）。客户端再连 127.0.0.1:22 即等效直连靶机。
#
# 用法（在 Git Bash 中）：
#   HOST=192.0.2.10 PORT=22 LP=22 bash start_dou_tunnel.sh
#   HOST   = Linux 靶机地址（IP 或域名），必填
#   PORT  = 从 Windows 到达靶机 sshd 的端口（外层），默认 22
#   LP    = 靶机本机 sshd 监听端口（内层转发目标），默认 22
#           （若靶机 sshd 改了端口，LP 填那个端口；PORT 与 LP 可不同，例如 NAT 映射）
# ============================================================
set -euo pipefail

HOST="${HOST:?请设置 HOST=Linux靶机地址}"
PORT="${PORT:-22}"
LP="${LP:-22}"
KEY="${DOU_SSH_KEY:-}"
if [ -z "$KEY" ]; then
  KEY="$(cygpath -u "${LOCALAPPDATA:?请设置 LOCALAPPDATA}")/dou/keys/id_ed25519"
fi

if [ ! -f "$KEY" ]; then
  echo "缺失客户端私钥: $KEY"; exit 1
fi

echo "[tunnel] 本地 127.0.0.1:22 -> $HOST:$LP (外层 $HOST:$PORT, 用户 dou, 密钥 id_ed25519)"
echo "[tunnel] 按 Ctrl-C 停止；保持此窗口运行，豆 才能连上靶机。"
exec ssh -i "$KEY" -N \
  -o StrictHostKeyChecking=accept-new \
  -o ServerAliveInterval=30 \
  -o ServerAliveCountMax=3 \
  -o ExitOnForwardFailure=yes \
  -L 127.0.0.1:22:localhost:"$LP" \
  -p "$PORT" dou@"$HOST"
