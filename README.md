# edge-camera-pipeline

Hybrid **RTSP + MQTT + Wasm** camera pipeline for Humanoid edge devices.

### Architecture
- **edge-2 / edge-3** (Pi 4 + Logitech C922)
  - RTSP video stream (high quality)
  - WasmEdge metadata processor (precise timestamping + events)
  - MQTT publishing of metadata
- **david-jetson** → DeepStream consumes RTSP + MQTT metadata

