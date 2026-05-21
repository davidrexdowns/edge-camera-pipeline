use anyhow::Result;
use tokio::process::Child;
use tracing::warn;

use crate::camera::CameraController;
use crate::config::AppConfig;

/// Runtime state for one edge node (streaming + metadata sequence).
pub struct NodeState {
    pub is_streaming: bool,
    pub gst_child: Option<Child>,
    pub metadata_sequence: u64,
}

impl NodeState {
    pub fn new() -> Self {
        Self {
            is_streaming: false,
            gst_child: None,
            metadata_sequence: 0,
        }
    }

    pub async fn start_stream(&mut self, config: &AppConfig) -> Result<()> {
        if self.is_streaming {
            return Ok(());
        }
        if !config.rtsp.enabled {
            warn!("rtsp.enabled=false; ignoring start");
            return Ok(());
        }
        let child: Child = CameraController::start_stream(config).await?;
        self.gst_child = Some(child);
        self.is_streaming = true;
        Ok(())
    }

    pub async fn stop_stream(&mut self) -> Result<()> {
        if !self.is_streaming {
            return Ok(());
        }
        if let Some(mut child) = self.gst_child.take() {
            CameraController::stop_stream(&mut child).await?;
        }
        self.is_streaming = false;
        Ok(())
    }

    pub fn next_metadata_sequence(&mut self) -> u64 {
        self.metadata_sequence = self.metadata_sequence.wrapping_add(1);
        self.metadata_sequence
    }
}
