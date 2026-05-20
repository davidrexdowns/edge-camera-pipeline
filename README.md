# edge-camera-pipeline

Simple, reliable, field-deployable edge camera pipeline:

- **NetBird**: secure overlay network (assumed already enrolled on devices)
- **Jetson**: central controller (Mosquitto broker + DeepStream/Cosmos consumers)
- **Pi worker**: Rust host daemon that:
  - auto-starts on boot (systemd)
  - registers to Jetson via MQTT
  - listens for `start/stop/status` commands
  - when started: launches an RTSP stream + publishes timestamped metadata (≈200ms)
- **Wasm module**: reusable logic for metadata shaping (WASI)

## Repo layout

```text
edge-camera-pipeline/
├── wasm-modules/camera-processor/     # Reusable Wasm logic (WASI)
├── hosts/pi4-c922/                    # Rust host daemon for Pi 4 + Logitech C922
├── deployment/
│   ├── systemd/                       # Service files
│   └── scripts/                       # deploy / update scripts
├── config/                            # Example config + topic conventions
└── README.md
```

## MQTT topic conventions

Default topic prefix is `edge-camera`.

For a device id like `pi4-c922-01`:

- Commands: `edge-camera/pi4-c922-01/cmd`
- Status: `edge-camera/pi4-c922-01/status`
- Metadata: `edge-camera/pi4-c922-01/metadata`
- Register: `edge-camera/pi4-c922-01/register`

Command payloads (JSON):

```json
{"type":"start"}
```

```json
{"type":"stop"}
```

```json
{"type":"status"}
```

## Build

### Pi host (native)

```bash
cd hosts/pi4-c922
cargo build --release
```

### Wasm module (WASI)

Requires Rust target `wasm32-wasip1` (preferred) or `wasm32-wasi` depending on your toolchain.

```bash
cd wasm-modules/camera-processor
rustup target add wasm32-wasip1 || true
cargo build --release --target wasm32-wasip1
ls -la ../../target/wasm32-wasip1/release/camera_processor.wasm
```

## Run on Pi (manual)

1) Install dependencies:

- Mosquitto is on Jetson (broker)
- On Pi: `wasmedge` must be installed (for now we embed via WasmEdge SDK; CLI optional)
- GStreamer is used for RTSP pipeline

2) Put config at:

`/etc/edge-camera-pipeline/pi4-c922.toml` (see `config/pi4-c922.example.toml`)

3) Run:

```bash
sudo ./target/release/pi4_c922_host --config /etc/edge-camera-pipeline/pi4-c922.toml
```

## systemd

See:

- `deployment/systemd/pi4-c922.service`
- `deployment/scripts/install_pi_service.sh`

## Notes / assumptions

- RTSP streaming assumes an RTSP server is running on the Pi (recommended: **mediamtx** / rtsp-simple-server). The Pi host spawns `gst-launch-1.0` to *publish* to `rtsp://127.0.0.1:8554/<path>`.
- Metadata is published every ~200ms, independent of video frames.
- This repo intentionally keeps the control plane simple (MQTT) and avoids Docker on Pis.

# edge-camera-pipeline

Hybrid **RTSP + MQTT + Wasm** camera pipeline for Humanoid edge devices.

### Architecture
- **edge-2 / edge-3** (Pi 4 + Logitech C922)
  - RTSP video stream (high quality)
  - WasmEdge metadata processor (precise timestamping + events)
  - MQTT publishing of metadata
- **david-jetson** → DeepStream consumes RTSP + MQTT metadata

