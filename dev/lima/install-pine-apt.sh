#!/usr/bin/env bash
set -euo pipefail

# Add the Pine Desktop APT repository and install the released pine package.
# Intended to run inside the pine-gnome Lima guest; safe to re-run.

base_url="https://pinehq.github.io/pine-desktop/apt"
architecture="$(dpkg --print-architecture)"
keyring="/usr/share/keyrings/pine.gpg"
sources_list="/etc/apt/sources.list.d/pine.list"

curl --proto '=https' --tlsv1.2 -fsSL "${base_url}/pubkey.gpg" |
  gpg --dearmor |
  sudo tee "${keyring}" >/dev/null

echo "deb [arch=${architecture} signed-by=${keyring}] ${base_url} stable main" |
  sudo tee "${sources_list}" >/dev/null

sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install --yes pine

dpkg-query --status pine >/dev/null
echo "Installed $(dpkg-query --show --showformat='${Package} ${Version} ${Architecture}' pine)"
