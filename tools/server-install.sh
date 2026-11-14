#!/usr/bin/env bash
# Kontinuum on a Linux server (Ubuntu or Debian, x64 or Arm): streams on its own, with no window
# and no sound card, as a service that starts with the machine and restarts if it stops.
#
#   curl -fsSL https://raw.githubusercontent.com/Almaper/Kontinuum/main/tools/server-install.sh | bash
#
# At the end it shows the address and the password of the control page: open it on a phone,
# paste the stream key in Settings > Stream, and save. Running the script again updates
# Kontinuum to the latest release (the settings stay).
set -euo pipefail

TZ_WANTED="${KNT_TIMEZONE:-Europe/Madrid}"

case "$(uname -m)" in
  x86_64 | amd64) target=linux-x64 ;;
  aarch64 | arm64) target=linux-arm64 ;;
  *) echo "Kontinuum: this machine ($(uname -m)) is not supported." >&2; exit 1 ;;
esac

echo "== The tools: ffmpeg (for the stream) and the sound library"
sudo apt-get update -y
sudo apt-get install -y ffmpeg curl ca-certificates
sudo apt-get install -y libasound2t64 2>/dev/null || sudo apt-get install -y libasound2

echo "== The local time ($TZ_WANTED), for the schedule and the sky"
sudo timedatectl set-timezone "$TZ_WANTED" || true

echo "== Kontinuum, the latest release for $target"
url=$(curl -fsSL https://api.github.com/repos/Almaper/Kontinuum/releases/latest \
  | grep -o "https://[^\"]*-$target\.tar\.gz" | head -n1)
if [ -z "$url" ]; then
  echo "Kontinuum: no $target download in the latest release." >&2
  exit 1
fi
dir=/opt/kontinuum
sudo mkdir -p "$dir"
curl -fsSL "$url" | sudo tar xz -C "$dir" --strip-components=1
echo "   $(basename "$url") in $dir"

conf="$HOME/.config/Kontinuum/server.toml"
mkdir -p "$(dirname "$conf")"

echo "== The service"
sudo tee /etc/systemd/system/kontinuum.service > /dev/null <<UNIT
[Unit]
Description=Kontinuum: generative ambient, streamed live
After=network-online.target
Wants=network-online.target

[Service]
User=$USER
Environment=HOME=$HOME
ExecStart=$dir/Kontinuum --server $conf
Restart=always
RestartSec=20

[Install]
WantedBy=multi-user.target
UNIT
sudo systemctl daemon-reload
sudo systemctl enable kontinuum > /dev/null 2>&1

echo "== The control page's port (8080) in the server's firewall"
if command -v iptables > /dev/null; then
  if ! sudo iptables -C INPUT -p tcp --dport 8080 -j ACCEPT 2> /dev/null; then
    sudo iptables -I INPUT -p tcp --dport 8080 -j ACCEPT
  fi
  if command -v netfilter-persistent > /dev/null; then
    sudo netfilter-persistent save > /dev/null 2>&1 || true
  fi
fi
if command -v ufw > /dev/null && sudo ufw status | grep -q active; then
  sudo ufw allow 8080/tcp > /dev/null
fi

sudo systemctl restart kontinuum
for _ in 1 2 3 4 5 6 7 8 9 10; do [ -f "$conf" ] && break; sleep 1; done
ip=$(curl -fsS --max-time 5 https://api.ipify.org || hostname -I | awk '{print $1}')
pw=$(grep -m1 '^password' "$conf" | cut -d'"' -f2)
cat <<NEXT

Kontinuum is installed and running.

  The control page:  http://$ip:8080
  Its password:      $pw

On Oracle Cloud, open the port once in the console: the instance > its subnet > the
security list > Add ingress rule: source 0.0.0.0/0, TCP, destination port 8080.

Then, on the page: Settings > Stream > paste your stream key > Save and apply.
What it does, here:  journalctl -u kontinuum -f
NEXT
