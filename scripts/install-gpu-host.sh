#!/usr/bin/env bash
set -euo pipefail

HOST_NAME="io.colonist_assistant.gpu"
EXTENSION_ID="${1:-}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO="${CARGO:-${HOME}/.cargo/bin/cargo}"

if [[ ! "${EXTENSION_ID}" =~ ^[a-p]{32}$ ]]; then
  echo "usage: $0 <chrome-extension-id>" >&2
  echo "Find the 32-character ID at chrome://extensions with Developer mode enabled." >&2
  exit 2
fi

if [[ ! -x "${CARGO}" ]]; then
  CARGO="cargo"
fi

"${CARGO}" build \
  --manifest-path "${ROOT}/engine/Cargo.toml" \
  --release \
  -p colonist-catan-native-host

SOURCE="${ROOT}/engine/target/release/colonist-assistant-gpu"
DEST_DIR="${HOME}/.local/lib/colonist-assistant"
BINARY="${DEST_DIR}/colonist-assistant-gpu"
LAUNCHER="${DEST_DIR}/colonist-assistant-gpu-host"
MANIFEST_DIR="${HOME}/.config/google-chrome/NativeMessagingHosts"
MANIFEST="${MANIFEST_DIR}/${HOST_NAME}.json"

mkdir -p "${DEST_DIR}" "${MANIFEST_DIR}"
install -m 0755 "${SOURCE}" "${BINARY}"

printf '#!/usr/bin/env bash\nexec %q\n' "${BINARY}" > "${LAUNCHER}"
chmod 0755 "${LAUNCHER}"

cat > "${MANIFEST}" <<EOF
{
  "name": "${HOST_NAME}",
  "description": "Colonist Assistant CUDA strategist",
  "path": "${LAUNCHER}",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://${EXTENSION_ID}/"
  ]
}
EOF
chmod 0644 "${MANIFEST}"

echo "Installed ${HOST_NAME} for Chrome extension ${EXTENSION_ID}"
echo "Native host manifest: ${MANIFEST}"
