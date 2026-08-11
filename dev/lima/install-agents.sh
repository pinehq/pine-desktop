#!/usr/bin/env bash
set -euo pipefail

export PATH="${HOME}/.local/bin:${PATH}"

if ! command -v node >/dev/null || ! node -e \
  'const [major, minor] = process.versions.node.split(".").map(Number); process.exit(major > 22 || (major === 22 && minor >= 19) ? 0 : 1)'
then
  echo "Pi requires Node.js 22.19 or newer." >&2
  echo "On Ubuntu 26.04: sudo apt-get install nodejs npm" >&2
  exit 1
fi

curl --proto '=https' --tlsv1.2 -fsSL \
  https://chatgpt.com/codex/install.sh | sh
curl --proto '=https' --tlsv1.2 -fsSL \
  https://claude.ai/install.sh | bash -s stable
curl --proto '=https' --tlsv1.2 -fsSL \
  https://pi.dev/install.sh | sh

hash -r
codex --version
claude --version
pi --version
