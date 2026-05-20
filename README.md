# edge-camera-pipeline

Hybrid RTSP + MQTT + Wasm camera pipeline for Humanoid edge devices.

## Architecture
- Raspberry Pi 4 (edge-2, edge-3) → Logitech C922
  - RTSP video stream
  - WasmEdge metadata processor (timestamping, events)
  - MQTT publishing
- Jetson Orin Nano → DeepStream consumption

## Folders
- `wasm-modules/`     → Reusable Wasm binaries
- `hosts/pi4-c922/`   → Host binary for Pi 4 + C922
- `deployment/`       → Systemd services, scripts
- `docs/`            → Architecture & setup guides
