#!/usr/bin/env bash
# Kontinuum on a Linux server (Ubuntu or Debian, x64 or Arm): streams on its own, with no window
# and no sound card, as a service that starts with the machine and restarts if it stops.
#
#   curl -fsSL https://raw.githubusercontent.com/Almaper/Kontinuum/main/tools/server-install.sh | bash
#
# Then paste your stream key in ~/.config/Kontinuum/server.toml and start it:
#   nano ~/.config/Kontinuum/server.toml
#   sudo systemctl restart kontinuum
# Running it again updates Kontinuum to the latest release (the settings stay).
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
if [ ! -f "$conf" ]; then
  "$dir/Kontinuum" --server "$conf" || true
fi

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

if grep -m1 '^key = ' "$conf" | grep -q '""'; then
  cat <<NEXT

Kontinuum is installed. Now:
  1. Paste your stream key (YouTube Studio > Go live > Stream > Stream key):
       nano $conf
     In [stream], write it between the quotes of key = "", then Ctrl+O, Enter, Ctrl+X.
  2. Start it:
       sudo systemctl restart kontinuum
  3. Watch what it does (Ctrl+C to stop watching; it keeps streaming):
       journalctl -u kontinuum -f
  The texts for YouTube (title, description, hashtags) are in
       $(dirname "$conf")/youtube-texts.txt
NEXT
else
  sudo systemctl restart kontinuum
  echo
  echo "Kontinuum is updated and streaming again. Watch it with: journalctl -u kontinuum -f"
fi
