#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project_root="$(cd -- "${script_dir}/../.." && pwd)"
pine_binary="${XDG_CACHE_HOME:-${HOME}/.cache}/pine-target/debug/pine-linux"
autostart_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/autostart"
template="${script_dir}/io.pinehq.Pine.desktop.in"
destination="${autostart_dir}/io.pinehq.Pine.desktop"

if [[ ! -x "${pine_binary}" ]]; then
  echo "Pine binary not found: ${pine_binary}" >&2
  echo "Build it with CARGO_TARGET_DIR=${pine_binary%/debug/pine-linux} cargo build -p pine-linux" >&2
  exit 1
fi

escape_sed_replacement() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//&/\\&}"
  value="${value//|/\\|}"
  printf '%s\n' "${value}"
}

mkdir -p "${autostart_dir}"
sed \
  -e "s|@PINE_BINARY@|$(escape_sed_replacement "${pine_binary}")|g" \
  -e "s|@PINE_PROJECT_ROOT@|$(escape_sed_replacement "${project_root}")|g" \
  "${template}" >"${destination}"
chmod 0644 "${destination}"

desktop-file-validate "${destination}"
echo "Installed Pine autostart entry: ${destination}"
