# edge-camera-pipeline

Reliable **DietPi** edge camera nodes for a NetBird mesh: Pi 4 and Pi Zero 2 W run a single Rust binary that waits for MQTT commands, then publishes **RTSP** (GStreamer → MediaMTX) and **timestamped metadata**.

## Architecture

```text
Phone / Jetson controller
        │  MQTT (start / stop / status)
        ▼
┌─────────────────────────────┐
│  DietPi Pi (dumb node)      │
│  pi_camera_host             │
│    ├─ subscribe cmd         │
│    ├─ gst → MediaMTX :8554  │
│    └─ metadata ~200ms       │
└─────────────────────────────┘
        │  RTSP over NetBird
        ▼
Jetson (DeepStream / Cosmos consumers)
```

## Repo layout

```text
edge-camera-pipeline/
├── Cargo.toml                 # workspace
├── config.example.toml        # per-device template
├── hosts/pi-camera-host/      # main binary (modular Rust)
├── deployment/
│   ├── systemd/pi-camera-host.service
│   └── scripts/deploy.sh
└── README.md
```

## Prerequisites (each Pi)

On **DietPi**:

```bash
sudo apt update
sudo apt install -y gstreamer1.0-tools gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav
# MediaMTX (RTSP server) — install binary or package for your image
# Example: download from https://github.com/bluenviron/mediamtx/releases
```

- **NetBird** enrolled; note the Pi’s overlay IP for `rtsp.advertise_host`
- **MQTT broker** on Jetson (`jetson_netbird_ip` in config)
- Camera on `/dev/video0` (or change `rtsp.video_device`)

## Build (WSL or native)

```bash
cd edge-camera-pipeline
cargo build --release -p pi-camera-host
# Binary: target/release/pi_camera_host
```

### Cross-compile for ARM64 (Pi 4 / Pi Zero 2 W)

```bash
rustup target add aarch64-unknown-linux-gnu
sudo apt install -y gcc-aarch64-linux-gnu
cargo build --release -p pi-camera-host --target aarch64-unknown-linux-gnu
# Binary: target/aarch64-unknown-linux-gnu/release/pi_camera_host
```

The Rust binary is statically linked where possible (`lto`, `strip`). **GStreamer** remains a runtime dependency on the Pi.

## Configuration

Copy and edit on each device:

```bash
sudo mkdir -p /etc/edge-camera-pipeline
sudo cp config.example.toml /etc/edge-camera-pipeline/config.toml
sudo nano /etc/edge-camera-pipeline/config.toml
```

| Field | Purpose |
|--------|---------|
| `device.id` | Unique MQTT device id |
| `device.device_type` | `pi4` or `pi_zero_2w` (resolution presets) |
| `mqtt.jetson_netbird_ip` | Jetson NetBird IP (broker) |
| `rtsp.advertise_host` | **This Pi’s** NetBird IP for the public RTSP URL |
| `rtsp.path` | MediaMTX path (e.g. `pi4-c922-01`) |

### Device presets

| Type | Default resolution | FPS | Bitrate |
|------|-------------------|-----|---------|
| `pi4` | 1280×720 | 30 | 1500 kbps |
| `pi_zero_2w` | 640×480 | 15 | 800 kbps |

Override with `rtsp.width`, `rtsp.height`, `rtsp.fps`, `rtsp.bitrate_kbps`.

## MQTT topics

Prefix default: `edge-camera`

| Topic | Direction | Payload |
|-------|-----------|---------|
| `edge-camera/<id>/cmd` | Subscribe | `{"type":"start"}` / `stop` / `status` |
| `edge-camera/<id>/status` | Publish (retain) | streaming state + URL |
| `edge-camera/<id>/metadata` | Publish | timestamped JSON ~200ms while live |
| `edge-camera/<id>/register` | Publish (retain) | capabilities on connect |

### Test from Jetson

```bash
mosquitto_pub -h 127.0.0.1 -t edge-camera/pi4-c922-01/cmd -m '{"type":"start"}'
mosquitto_sub -h 127.0.0.1 -t 'edge-camera/pi4-c922-01/#'
```

## Deploy

```bash
chmod +x deployment/scripts/deploy.sh
./deployment/scripts/deploy.sh pi@100.103.x.x aarch64-unknown-linux-gnu
```

Manual install:

```bash
sudo install -m 755 target/aarch64-unknown-linux-gnu/release/pi_camera_host \
  /opt/edge-camera-pipeline/bin/pi_camera_host
sudo cp deployment/systemd/pi-camera-host.service /etc/systemd/system/edge-camera-host.service
sudo systemctl daemon-reload
sudo systemctl enable --now edge-camera-host.service
sudo journalctl -u edge-camera-host.service -f
```

## systemd

Service name: `edge-camera-host.service`  
Expects config at `/etc/edge-camera-pipeline/config.toml`.

MediaMTX should listen on `127.0.0.1:8554` before the host starts streaming. Add a `mediamtx.service` unit or start MediaMTX in your image.

## Development guidelines

- Prefer simplicity and reliability over clever abstractions
- `anyhow` for errors; `tracing` with UTC timestamps for logs
- One config file per device; no secrets in git
- Wasm metadata optional in a future release

## License

Private / project use — adjust as needed for `davidrexdowns/edge-camera-pipeline`.
