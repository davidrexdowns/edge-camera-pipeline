mod camera;
mod config;
mod metadata;
mod mqtt;
mod state;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::sync::Mutex;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::AppConfig;
use crate::mqtt::{run_command_loop, MqttClient, MqttConnection};
use crate::state::NodeState;

#[derive(Debug, Parser)]
#[command(name = "pi_camera_host", about = "DietPi edge camera node (MQTT + RTSP)")]
struct Cli {
    /// Path to config.toml on the device.
    #[arg(long, default_value = "/etc/edge-camera-pipeline/config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let cli: Cli = Cli::parse();
    let config: AppConfig = AppConfig::load_from_file(&cli.config)?;
    let stream_url: String = config.build_stream_url();
    info!(
        device_id = %config.device.id,
        device_type = %config.device.device_type.as_str(),
        broker = %config.mqtt.jetson_netbird_ip,
        stream_url = %stream_url,
        "pi-camera-host starting"
    );
    let connection: MqttConnection =
        MqttConnection::connect(&config).context("mqtt connect")?;
    let event_loop = connection.event_loop;
    let mqtt: MqttClient = connection.client;
    mqtt.subscribe_commands(&config).await?;
    mqtt.publish_register(&config).await?;
    let state: Arc<Mutex<NodeState>> = Arc::new(Mutex::new(NodeState::new()));
    mqtt.publish_status(&config, &state, &stream_url).await?;
    let mqtt_arc: Arc<MqttClient> = Arc::new(mqtt);
    let command_task = {
        let mqtt_clone = mqtt_arc.clone();
        let config_clone = config.clone();
        let state_clone = state.clone();
        let stream_url_clone = stream_url.clone();
        tokio::spawn(async move {
            run_command_loop(
                event_loop,
                mqtt_clone,
                config_clone,
                state_clone,
                stream_url_clone,
            )
            .await
        })
    };
    let metadata_task = {
        let mqtt_clone = mqtt_arc.clone();
        let config_clone = config.clone();
        let state_clone = state.clone();
        let stream_url_clone = stream_url.clone();
        tokio::spawn(async move {
            run_metadata_loop(mqtt_clone, config_clone, state_clone, stream_url_clone).await
        })
    };
    tokio::select! {
        result = command_task => {
            result??;
        }
        result = metadata_task => {
            result??;
        }
        _ = tokio::signal::ctrl_c() => {
            info!("shutdown signal received");
        }
    }
    let mut locked = state.lock().await;
    locked.stop_stream().await?;
    drop(locked);
    mqtt_arc
        .publish_status(&config, &state, &stream_url)
        .await?;
    info!("pi-camera-host stopped");
    Ok(())
}

fn init_logging() {
    let filter: EnvFilter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
        .init();
}

async fn run_metadata_loop(
    mqtt: Arc<MqttClient>,
    config: AppConfig,
    state: Arc<Mutex<NodeState>>,
    stream_url: String,
) -> Result<()> {
    let tick_ms: u64 = config.metadata.tick_millis.max(50);
    let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let sequence: Option<u64> = {
            let mut locked = state.lock().await;
            if !locked.is_streaming {
                None
            } else {
                Some(locked.next_metadata_sequence())
            }
        };
        let Some(sequence) = sequence else {
            continue;
        };
        if let Err(err) = mqtt
            .publish_metadata(&config, &stream_url, sequence)
            .await
        {
            warn!(error = %err, "metadata publish failed");
        }
    }
}
