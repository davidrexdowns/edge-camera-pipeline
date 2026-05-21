use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::{Child, Command};
use tracing::info;

use crate::config::{AppConfig, StreamPreset};

/// Manages the GStreamer publisher process (pushes H.264 to local MediaMTX).
pub struct CameraController;

impl CameraController {
    /// Spawn gst-launch publishing to rtsp://127.0.0.1:<port>/<path>.
    /// MediaMTX must already be running on the Pi.
    pub async fn start_stream(config: &AppConfig) -> Result<Child> {
        let preset: StreamPreset = config.stream_preset();
        let path: String = config.rtsp.path.trim().trim_start_matches('/').to_string();
        let publish_url: String = format!(
            "rtsp://127.0.0.1:{}/{}",
            config.rtsp.listen_port, path
        );
        let pipeline: String = format!(
            "gst-launch-1.0 -e v4l2src device={} ! video/x-raw,width={},height={},framerate={}/1 ! videoconvert ! x264enc tune=zerolatency speed-preset=ultrafast bitrate={} key-int-max={} ! h264parse ! rtph264pay config-interval=1 pt=96 ! rtspclientsink location={}",
            config.rtsp.video_device,
            preset.width,
            preset.height,
            preset.fps,
            preset.bitrate_kbps,
            preset.fps,
            publish_url
        );
        info!(
            device = %config.device.id,
            device_type = %config.device.device_type.as_str(),
            width = preset.width,
            height = preset.height,
            fps = preset.fps,
            publish_url = %publish_url,
            "starting GStreamer RTSP publisher"
        );
        let child: Child = Command::new("sh")
            .arg("-lc")
            .arg(&pipeline)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawn gst-launch-1.0")?;
        Ok(child)
    }

    /// Stop the GStreamer process gracefully (SIGTERM via kill).
    pub async fn stop_stream(child: &mut Child) -> Result<()> {
        if let Some(pid) = child.id() {
            info!(pid, "stopping GStreamer");
        }
        let _ = child.start_kill();
        let _ = child.wait().await.context("wait for GStreamer exit")?;
        Ok(())
    }
}
