#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="/opt/edge-camera-pipeline"
CONFIG_DIR="/etc/edge-camera-pipeline"

echo "Installing edge-camera-pipeline to ${PROJECT_DIR}"
sudo mkdir -p "${PROJECT_DIR}/bin" "${PROJECT_DIR}/wasm" "${CONFIG_DIR}"

echo "Copying binary"
sudo cp "./hosts/pi4-c922/target/release/pi4_c922_host" "${PROJECT_DIR}/bin/"

echo "Copying wasm module (optional)"
if [[ -f "./target/wasm32-wasip1/release/camera_processor.wasm" ]]; then
  sudo cp "./target/wasm32-wasip1/release/camera_processor.wasm" "${PROJECT_DIR}/wasm/camera_processor.wasm"
elif [[ -f "./target/wasm32-wasi/release/camera_processor.wasm" ]]; then
  sudo cp "./target/wasm32-wasi/release/camera_processor.wasm" "${PROJECT_DIR}/wasm/camera_processor.wasm"
else
  echo "Wasm module not found; skipping. (Build it under wasm-modules/camera-processor)"
fi

if [[ ! -f "${CONFIG_DIR}/pi4-c922.toml" ]]; then
  echo "Installing default config template"
  sudo cp "./config/pi4-c922.example.toml" "${CONFIG_DIR}/pi4-c922.toml"
  echo "IMPORTANT: edit ${CONFIG_DIR}/pi4-c922.toml (set mqtt.host and rtsp.advertise_host)"
fi

echo "Installing systemd service"
sudo cp "./deployment/systemd/pi4-c922.service" /etc/systemd/system/edge-camera-pi4-c922.service
sudo systemctl daemon-reload
sudo systemctl enable --now edge-camera-pi4-c922.service

echo "Done. Check logs with:"
echo "  sudo journalctl -u edge-camera-pi4-c922.service -f"

