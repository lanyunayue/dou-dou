#!/usr/bin/env bash
# ============================================================
# 豆 Dou · Linux 靶机配置脚本（在靶机本机以 root 运行）
# 作用：创建 dou 用户、安装 tmux、部署 豆 客户端公钥、确保 sshd 允许公钥登录。
# 用法：
#   sudo bash setup_linux_target.sh '<一行公钥>'
# 公钥取自 Windows 端：ssh-keygen -y -P "" -f ~/AppData/Local/dou/keys/id_ed25519
# （已推导：ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJXM4W6IEEJ5eyG0TLfpRAtz1i6IaaR/RoqgZYMhbXlw dou-gateway-spike）
# ============================================================
set -euo pipefail

PUBKEY="${1:?用法: sudo bash setup_linux_target.sh '<公钥一行>'}"
USER=dou

echo "[1/4] 创建用户 $USER（若已存在则跳过）"
if id "$USER" >/dev/null 2>&1; then
  echo "  用户已存在"
else
  useradd -m -s /bin/bash "$USER"
fi

echo "[2/4] 安装 tmux（豆 客户端会话复用依赖）"
if command -v tmux >/dev/null 2>&1; then
  echo "  tmux 已安装: $(command -v tmux)"
else
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update -y && apt-get install -y tmux
  elif command -v dnf >/dev/null 2>&1; then
    dnf install -y tmux
  elif command -v yum >/dev/null 2>&1; then
    yum install -y tmux
  elif command -v apk >/dev/null 2>&1; then
    apk add tmux
  elif command -v zypper >/dev/null 2>&1; then
    zypper install -y tmux
  else
    echo "  无法自动安装 tmux，请手动安装后重跑"; exit 1
  fi
fi

echo "[3/4] 写入 $USER 的 authorized_keys"
mkdir -p /home/$USER/.ssh
echo "$PUBKEY" >> /home/$USER/.ssh/authorized_keys
chmod 700 /home/$USER/.ssh
chmod 600 /home/$USER/.ssh/authorized_keys
chown -R $USER:$USER /home/$USER/.ssh

echo "[4/4] 确保 sshd 允许公钥登录并 reload"
if grep -q '^PubkeyAuthentication' /etc/ssh/sshd_config; then
  sed -i 's/^PubkeyAuthentication.*/PubkeyAuthentication yes/' /etc/ssh/sshd_config
else
  echo 'PubkeyAuthentication yes' >> /etc/ssh/sshd_config
fi
# 若 sshd 端口非默认 22，请确认 /home/dou 仍可通过该端口密钥登录
systemctl reload ssh 2>/dev/null || service ssh reload 2>/dev/null || true

echo ""
echo "OK: 用户 $USER 就绪；tmux=$(command -v tmux)；authorized_keys 已追加 1 行。"
echo "请确认：从 Windows 能到达本机 sshd（安全组/防火墙放行 $USER 登录端口）。"
