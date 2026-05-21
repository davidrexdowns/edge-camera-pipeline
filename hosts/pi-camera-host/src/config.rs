use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Supported edge hardware profiles. Values in config.toml: "pi4" or "pi_zero_2w".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Pi4,
    PiZero2W,
}

impl DeviceType {
    pub fn as_str(self) -> &'static str {
        match self {
            DeviceType::Pi4 => "pi4",
            DeviceType::PiZero2W => "pi_zero_2w",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub device: DeviceSection,
    pub mqtt: MqttSection,
    pub rtsp: RtspSection,
    pub metadata: MetadataSection,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceSection {
    pub id: String,
    pub device_type: DeviceType,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MqttSection {
    /// Jetson (or broker) NetBird IP or hostname.
    pub jetson_netbird_ip: String,
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_topic_prefix")]
    pub topic_prefix: String,
    #[serde(default = "default_qos")]
    pub qos: u8,
    #[serde(default = "default_keep_alive")]
    pub keep_alive_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RtspSection {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// NetBird IP or DNS of *this* Pi — used in advertised stream URL for Jetson/clients.
    pub advertise_host: String,
    #[serde(default = "default_rtsp_port")]
    pub listen_port: u16,
    pub path: String,
    pub video_device: String,
    /// Optional overrides; if omitted, presets from device_type are used.
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<u32>,
    pub bitrate_kbps: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetadataSection {
    #[serde(default = "default_tick_ms")]
    pub tick_millis: u64,
}

fn default_topic_prefix() -> String {
    "edge-camera".to_string()
}

fn default_qos() -> u8 {
    1
}

fn default_keep_alive() -> u64 {
    30
}

fn default_true() -> bool {
    true
}

fn default_rtsp_port() -> u16 {
    8554
}

fn default_tick_ms() -> u64 {
    200
}

impl AppConfig {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let raw: String =
            std::fs::read_to_string(path).with_context(|| format!("read config {}", path.display()))?;
        let parsed: AppConfig = toml::from_str(&raw).context("parse config.toml")?;
        parsed.validate()?;
        Ok(parsed)
    }

    fn validate(&self) -> Result<()> {
        if self.device.id.trim().is_empty() {
            anyhow::bail!("device.id must not be empty");
        }
        if self.mqtt.jetson_netbird_ip.trim().is_empty() {
            anyhow::bail!("mqtt.jetson_netbird_ip must not be empty");
        }
        if self.rtsp.advertise_host.trim().is_empty() {
            anyhow::bail!("rtsp.advertise_host must not be empty (this Pi's NetBird IP)");
        }
        Ok(())
    }

    pub fn stream_preset(&self) -> StreamPreset {
        let (default_w, default_h, default_fps, default_bitrate) = match self.device.device_type {
            DeviceType::Pi4 => (1280, 720, 30, 1500),
            DeviceType::PiZero2W => (640, 480, 15, 800),
        };
        StreamPreset {
            width: self.rtsp.width.unwrap_or(default_w),
            height: self.rtsp.height.unwrap_or(default_h),
            fps: self.rtsp.fps.unwrap_or(default_fps),
            bitrate_kbps: self.rtsp.bitrate_kbps.unwrap_or(default_bitrate),
        }
    }

    pub fn build_stream_url(&self) -> String {
        let host: String = self.rtsp.advertise_host.trim().to_string();
        let path: String = self.rtsp.path.trim().trim_start_matches('/').to_string();
        format!("rtsp://{}:{}/{}", host, self.rtsp.listen_port, path)
    }

    pub fn build_topics(&self) -> MqttTopics {
        let prefix: String = self.mqtt.topic_prefix.trim().trim_end_matches('/').to_string();
        let device_id: &str = self.device.id.trim();
        MqttTopics {
            cmd: format!("{prefix}/{device_id}/cmd"),
            status: format!("{prefix}/{device_id}/status"),
            metadata: format!("{prefix}/{device_id}/metadata"),
            register: format!("{prefix}/{device_id}/register"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StreamPreset {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone)]
pub struct MqttTopics {
    pub cmd: String,
    pub status: String,
    pub metadata: String,
    pub register: String,
}
