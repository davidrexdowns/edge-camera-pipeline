use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, Outgoing, QoS};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::config::AppConfig;
use crate::metadata::{self, MetadataMessage};
use crate::state::NodeState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CommandMessage {
    Start,
    Stop,
    Status,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusMessage {
    pub device_id: String,
    pub device_type: String,
    pub is_streaming: bool,
    pub stream_url: String,
    pub unix_millis: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegisterMessage {
    pub device_id: String,
    pub device_type: String,
    pub unix_millis: u128,
    pub capabilities: Vec<String>,
}

pub struct MqttClient {
    pub client: AsyncClient,
    pub qos: QoS,
}

pub struct MqttConnection {
    pub client: MqttClient,
    pub event_loop: EventLoop,
}

impl MqttConnection {
    pub fn connect(config: &AppConfig) -> Result<Self> {
        let client_id: String = format!("edge-camera-{}", config.device.id.trim());
        let mut options: MqttOptions = MqttOptions::new(
            client_id,
            config.mqtt.jetson_netbird_ip.trim(),
            config.mqtt.port,
        );
        if !config.mqtt.username.trim().is_empty() {
            options.set_credentials(
                config.mqtt.username.clone(),
                config.mqtt.password.clone(),
            );
        }
        options.set_keep_alive(Duration::from_secs(config.mqtt.keep_alive_seconds));
        let (async_client, event_loop) = AsyncClient::new(options, 64);
        let qos: QoS = map_qos(config.mqtt.qos);
        Ok(Self {
            client: MqttClient {
                client: async_client,
                qos,
            },
            event_loop,
        })
    }
}

impl MqttClient {

    pub async fn subscribe_commands(&self, config: &AppConfig) -> Result<()> {
        let topics = config.build_topics();
        self.client
            .subscribe(topics.cmd, self.qos)
            .await
            .context("subscribe cmd topic")?;
        Ok(())
    }

    pub async fn publish_register(&self, config: &AppConfig) -> Result<()> {
        let topics = config.build_topics();
        let unix_millis: u128 = metadata::now_unix_millis();
        let payload: RegisterMessage = RegisterMessage {
            device_id: config.device.id.clone(),
            device_type: config.device.device_type.as_str().to_string(),
            unix_millis,
            capabilities: vec!["rtsp".to_string(), "metadata".to_string()],
        };
        self.publish_json(&topics.register, &payload, true).await
    }

    pub async fn publish_status(
        &self,
        config: &AppConfig,
        state: &Arc<Mutex<NodeState>>,
        stream_url: &str,
    ) -> Result<()> {
        let topics = config.build_topics();
        let is_streaming: bool = state.lock().await.is_streaming;
        let payload: StatusMessage = StatusMessage {
            device_id: config.device.id.clone(),
            device_type: config.device.device_type.as_str().to_string(),
            is_streaming,
            stream_url: stream_url.to_string(),
            unix_millis: metadata::now_unix_millis(),
        };
        self.publish_json(&topics.status, &payload, true).await
    }

    pub async fn publish_metadata(
        &self,
        config: &AppConfig,
        stream_url: &str,
        sequence: u64,
    ) -> Result<()> {
        let topics = config.build_topics();
        let unix_millis: u128 = metadata::now_unix_millis();
        let payload: MetadataMessage =
            metadata::build_metadata_message(config, stream_url, sequence, unix_millis);
        self.publish_json(&topics.metadata, &payload, false).await
    }

    async fn publish_json(
        &self,
        topic: &str,
        payload: &impl Serialize,
        retain: bool,
    ) -> Result<()> {
        let bytes: Vec<u8> = serde_json::to_vec(payload).context("encode mqtt json")?;
        self.client
            .publish(topic, self.qos, retain, bytes)
            .await
            .with_context(|| format!("publish to {topic}"))?;
        Ok(())
    }
}

pub fn map_qos(value: u8) -> QoS {
    match value {
        0 => QoS::AtMostOnce,
        2 => QoS::ExactlyOnce,
        _ => QoS::AtLeastOnce,
    }
}

/// Handle incoming MQTT command messages.
pub async fn run_command_loop(
    mut event_loop: EventLoop,
    mqtt: Arc<MqttClient>,
    config: AppConfig,
    state: Arc<Mutex<NodeState>>,
    stream_url: String,
) -> Result<()> {
    let cmd_topic: String = config.build_topics().cmd;
    loop {
        let event: Event = event_loop.poll().await.context("mqtt poll")?;
        match event {
            Event::Incoming(Incoming::Publish(publish)) => {
                if publish.topic != cmd_topic {
                    continue;
                }
                let command: CommandMessage = match serde_json::from_slice(&publish.payload) {
                    Ok(value) => value,
                    Err(err) => {
                        warn!(error = %err, "invalid command json");
                        continue;
                    }
                };
                match command {
                    CommandMessage::Start => {
                        info!("mqtt command: start");
                        let mut locked = state.lock().await;
                        locked.start_stream(&config).await?;
                        drop(locked);
                        mqtt.publish_status(&config, &state, &stream_url).await?;
                    }
                    CommandMessage::Stop => {
                        info!("mqtt command: stop");
                        let mut locked = state.lock().await;
                        locked.stop_stream().await?;
                        drop(locked);
                        mqtt.publish_status(&config, &state, &stream_url).await?;
                    }
                    CommandMessage::Status => {
                        debug!("mqtt command: status");
                        mqtt.publish_status(&config, &state, &stream_url).await?;
                    }
                }
            }
            Event::Outgoing(Outgoing::PingReq) => {
                debug!("mqtt keepalive ping");
            }
            _ => {}
        }
    }
}
