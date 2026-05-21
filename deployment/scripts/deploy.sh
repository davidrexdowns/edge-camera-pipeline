#!/usr/bin/env bash
# Deploy pi_camera_host to a DietPi node over SSH.
#
# Usage:
#   ./deployment/scripts/deploy.sh pi@100.103.x.x
#   ./deployment/scripts/deploy.sh pi@edge-pi4 --target aarch64-unknown-linux-gnu
#
set -euo pipefail

REMOTE_HOST="${1:-}"
BUILD_TARGET="${2:-}"

if [[ -z "${REMOTE_HOST}" ]]; then
  echo "Usage: $0 <user@netbird-ip> [cargo-target-triple]"
  echo "Example: $0 pi@100.103.99.20 aarch64-unknown-linux-gnu"
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
INSTALL_DIR="/opt/edge-camera-pipeline"
CONFIG_DIR="/etc/edge-camera-pipeline"
BINARY_NAME="pi_camera_host"

cd "${REPO_ROOT}"

echo "==> Building release binary"
if [[ -n "${BUILD_TARGET}" ]]; then
  rustup target add "${BUILD_TARGET}" 2>/dev/null || true
  cargo build --release -p pi-camera-host --target "${BUILD_TARGET}"
  BIN_PATH="${REPO_ROOT}/target/${BUILD_TARGET}/release/${BINARY_NAME}"
else
  cargo build --release -p pi-camera-host
  BIN_PATH="${REPO_ROOT}/target/release/${BINARY_NAME}"
fi

if [[ ! -f "${BIN_PATH}" ]]; then
  echo "Binary not found: ${BIN_PATH}"
  exit 1
fi

echo "==> Preparing remote directories"
ssh "${REMOTE_HOST}" "sudo mkdir -p ${INSTALL_DIR}/bin ${CONFIG_DIR}"

echo "==> Copying binary"
scp "${BIN_PATH}" "${REMOTE_HOST}:/tmp/${BINARY_NAME}"
ssh "${REMOTE_HOST}" "sudo mv /tmp/${BINARY_NAME} ${INSTALL_DIR}/bin/${BINARY_NAME} && sudo chmod +x ${INSTALL_DIR}/bin/${BINARY_NAME}"

echo "==> Installing config (only if missing)"
ssh "${REMOTE_HOST}" "test -f ${CONFIG_DIR}/config.toml || sudo cp -n ${INSTALL_DIR}/config.example.toml ${CONFIG_DIR}/config.toml 2>/dev/null || true"
scp "${REPO_ROOT}/config.example.toml" "${REMOTE_HOST}:/tmp/config.example.toml"
ssh "${REMOTE_HOST}" "sudo cp /tmp/config.example.toml ${INSTALL_DIR}/config.example.toml"
ssh "${REMOTE_HOST}" "if [[ ! -f ${CONFIG_DIR}/config.toml ]]; then sudo cp ${INSTALL_DIR}/config.example.toml ${CONFIG_DIR}/config.toml; echo 'Created ${CONFIG_DIR}/config.toml — EDIT BEFORE STARTING'; fi"

echo "==> Installing systemd unit"
scp "${REPO_ROOT}/deployment/systemd/pi-camera-host.service" "${REMOTE_HOST}:/tmp/pi-camera-host.service"
ssh "${REMOTE_HOST}" "sudo mv /tmp/pi-camera-host.service /etc/systemd/system/edge-camera-host.service && sudo systemctl daemon-reload"

echo "==> Enable and restart service"
ssh "${REMOTE_HOST}" "sudo systemctl enable edge-camera-host.service && sudo systemctl restart edge-camera-host.service || true"

echo "Done. On device:"
echo "  sudo nano ${CONFIG_DIR}/config.toml"
echo "  sudo journalctl -u edge-camera-host.service -f"
