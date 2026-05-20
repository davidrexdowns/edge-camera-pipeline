use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, Outgoing, QoS};
use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::MissedTickBehavior;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, default_value = "/etc/edge-camera-pipeline/pi4-c922.toml")]
    config: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct Config {
    device: DeviceConfig,
    mqtt: MqttConfig,
    rtsp: RtspConfig,
    wasm: WasmConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct DeviceConfig {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MqttConfig {
    host: String,
    port: u16,
    username: String,
    password: String,
    topic_prefix: String,
    qos: u8,
    keep_alive_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct RtspConfig {
    enabled: bool,
    listen_port: u16,
    path: String,
    advertise_host: String,
    video_device: String,
    width: u32,
    height: u32,
    fps: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct WasmConfig {
    enabled: bool,
    module_path: String,
    tick_millis: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum CommandMessage {
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "status")]
    Status,
}

#[derive(Debug, Clone, Serialize)]
struct StatusMessage {
    device_id: String,
    is_streaming: bool,
    stream_url: String,
    unix_millis: u128,
}

#[derive(Debug, Clone, Serialize)]
struct RegisterMessage {
    device_id: String,
    unix_millis: u128,
    capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MetadataInput {
    device_id: String,
    stream_url: String,
    sequence: u64,
    unix_millis: u128,
}

#[derive(Debug, Clone)]
struct Topics {
    cmd: String,
    status: String,
    metadata: String,
    register: String,
}

#[derive(Debug)]
struct AppState {
    is_streaming: bool,
    gst_child: Option<Child>,
    metadata_sequence: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();
    let args: Args = Args::parse();
    let config: Config = read_config(&args.config)?;
    info!("starting pi host");
    let topics: Topics = build_topics(&config);
    let qos: QoS = map_qos(config.mqtt.qos);
    let mqtt_options: MqttOptions = build_mqtt_options(&config)?;
    let (mqtt_client, event_loop) = AsyncClient::new(mqtt_options, 50);
    mqtt_client
        .subscribe(topics.cmd.clone(), qos)
        .await
        .context("subscribe cmd")?;
    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState {
        is_streaming: false,
        gst_child: None,
        metadata_sequence: 0,
    }));
    let stream_url: String = build_stream_url(&config);
    publish_register(&mqtt_client, qos, &topics, &config).await?;
    publish_status(&mqtt_client, qos, &topics, &config, &state, &stream_url).await?;
    let wasm_processor: Option<WasmProcessor> = if config.wasm.enabled {
        Some(WasmProcessor::new(&config.wasm.module_path))
    } else {
        None
    };
    let wasm_processor: Arc<Option<WasmProcessor>> = Arc::new(wasm_processor);
    let mqtt_task = tokio::spawn(run_mqtt_loop(
        event_loop,
        mqtt_client.clone(),
        qos,
        topics.clone(),
        config.clone(),
        state.clone(),
        stream_url.clone(),
    ));
    let metadata_task = tokio::spawn(run_metadata_loop(
        mqtt_client.clone(),
        qos,
        topics.clone(),
        config.clone(),
        state.clone(),
        stream_url.clone(),
        wasm_processor.clone(),
    ));
    let shutdown_task = tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("shutdown signal received");
    });
    tokio::select! {
        result = mqtt_task => { result??; }
        result = metadata_task => { result??; }
        _ = shutdown_task => {}
    }
    execute_stop_stream(&mqtt_client, qos, &topics, &config, &state, &stream_url).await?;
    publish_status(&mqtt_client, qos, &topics, &config, &state, &stream_url).await?;
    Ok(())
}

fn read_config(path: &PathBuf) -> Result<Config> {
    let raw: String = std::fs::read_to_string(path).with_context(|| format!("read config {path:?}"))?;
    let parsed: Config = toml::from_str(&raw).context("parse toml")?;
    Ok(parsed)
}

fn build_mqtt_options(config: &Config) -> Result<MqttOptions> {
    let client_id: String = format!("edge-camera-{}", config.device.id);
    let mut options: MqttOptions = MqttOptions::new(client_id, config.mqtt.host.clone(), config.mqtt.port);
    if !config.mqtt.username.trim().is_empty() {
        options.set_credentials(config.mqtt.username.clone(), config.mqtt.password.clone());
    }
    options.set_keep_alive(Duration::from_secs(config.mqtt.keep_alive_seconds));
    Ok(options)
}

fn map_qos(value: u8) -> QoS {
    match value {
        0 => QoS::AtMostOnce,
        2 => QoS::ExactlyOnce,
        _ => QoS::AtLeastOnce,
    }
}

fn build_topics(config: &Config) -> Topics {
    let prefix: String = config.mqtt.topic_prefix.trim().trim_end_matches('/').to_string();
    let device_id: &str = &config.device.id;
    Topics {
        cmd: format!("{prefix}/{device_id}/cmd"),
        status: format!("{prefix}/{device_id}/status"),
        metadata: format!("{prefix}/{device_id}/metadata"),
        register: format!("{prefix}/{device_id}/register"),
    }
}

fn build_stream_url(config: &Config) -> String {
    let host: String = config.rtsp.advertise_host.trim().to_string();
    let path: String = config.rtsp.path.trim().trim_start_matches('/').to_string();
    format!("rtsp://{}:{}/{}", host, config.rtsp.listen_port, path)
}

async fn publish_register(client: &AsyncClient, qos: QoS, topics: &Topics, config: &Config) -> Result<()> {
    let unix_millis: u128 = now_unix_millis()?;
    let payload: RegisterMessage = RegisterMessage {
        device_id: config.device.id.clone(),
        unix_millis,
        capabilities: vec!["rtsp".to_string(), "wasm-metadata".to_string()],
    };
    let encoded: Vec<u8> = serde_json::to_vec(&payload).context("encode register")?;
    client
        .publish(topics.register.clone(), qos, true, encoded)
        .await
        .context("publish register")?;
    Ok(())
}

async fn publish_status(
    client: &AsyncClient,
    qos: QoS,
    topics: &Topics,
    config: &Config,
    state: &Arc<Mutex<AppState>>,
    stream_url: &str,
) -> Result<()> {
    let unix_millis: u128 = now_unix_millis()?;
    let is_streaming: bool = state.lock().await.is_streaming;
    let payload: StatusMessage = StatusMessage {
        device_id: config.device.id.clone(),
        is_streaming,
        stream_url: stream_url.to_string(),
        unix_millis,
    };
    let encoded: Vec<u8> = serde_json::to_vec(&payload).context("encode status")?;
    client
        .publish(topics.status.clone(), qos, true, encoded)
        .await
        .context("publish status")?;
    Ok(())
}

async fn run_mqtt_loop(
    mut event_loop: EventLoop,
    client: AsyncClient,
    qos: QoS,
    topics: Topics,
    config: Config,
    state: Arc<Mutex<AppState>>,
    stream_url: String,
) -> Result<()> {
    loop {
        let event: Event = event_loop.poll().await.context("mqtt poll")?;
        match event {
            Event::Incoming(Incoming::Publish(publish)) => {
                if publish.topic != topics.cmd {
                    continue;
                }
                let parsed: CommandMessage = match serde_json::from_slice(&publish.payload) {
                    Ok(value) => value,
                    Err(err) => {
                        warn!("invalid command payload: {err}");
                        continue;
                    }
                };
                match parsed {
                    CommandMessage::Start => {
                        info!("command: start");
                        execute_start_stream(&client, qos, &topics, &config, &state, &stream_url).await?;
                        publish_status(&client, qos, &topics, &config, &state, &stream_url).await?;
                    }
                    CommandMessage::Stop => {
                        info!("command: stop");
                        execute_stop_stream(&client, qos, &topics, &config, &state, &stream_url).await?;
                        publish_status(&client, qos, &topics, &config, &state, &stream_url).await?;
                    }
                    CommandMessage::Status => {
                        debug!("command: status");
                        publish_status(&client, qos, &topics, &config, &state, &stream_url).await?;
                    }
                }
            }
            Event::Outgoing(Outgoing::PingReq) => {
                debug!("mqtt ping");
            }
            _ => {}
        }
    }
}

async fn run_metadata_loop(
    client: AsyncClient,
    qos: QoS,
    topics: Topics,
    config: Config,
    state: Arc<Mutex<AppState>>,
    stream_url: String,
    wasm_processor: Arc<Option<WasmProcessor>>,
) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_millis(config.wasm.tick_millis.max(50)));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let is_streaming: bool = state.lock().await.is_streaming;
        if !is_streaming {
            continue;
        }
        let unix_millis: u128 = now_unix_millis()?;
        let sequence: u64 = {
            let mut locked = state.lock().await;
            locked.metadata_sequence = locked.metadata_sequence.wrapping_add(1);
            locked.metadata_sequence
        };
        let input: MetadataInput = MetadataInput {
            device_id: config.device.id.clone(),
            stream_url: stream_url.clone(),
            sequence,
            unix_millis,
        };
        let payload: Vec<u8> = if let Some(processor) = wasm_processor.as_ref() {
            let input_json: Vec<u8> = serde_json::to_vec(&input).context("encode metadata input")?;
            match processor.process_json(&input_json).await {
                Ok(value) => value,
                Err(err) => {
                    warn!("wasm processing failed: {err}");
                    serde_json::to_vec(&input).context("encode metadata fallback")?
                }
            }
        } else {
            serde_json::to_vec(&input).context("encode metadata")?
        };
        client
            .publish(topics.metadata.clone(), qos, false, payload)
            .await
            .context("publish metadata")?;
    }
}

async fn execute_start_stream(
    client: &AsyncClient,
    qos: QoS,
    topics: &Topics,
    config: &Config,
    state: &Arc<Mutex<AppState>>,
    stream_url: &str,
) -> Result<()> {
    if !config.rtsp.enabled {
        warn!("rtsp disabled in config");
        return Ok(());
    }
    let mut locked = state.lock().await;
    if locked.is_streaming {
        return Ok(());
    }
    let child: Child = spawn_gstreamer_rtsp(config).context("spawn gstreamer")?;
    locked.gst_child = Some(child);
    locked.is_streaming = true;
    drop(locked);
    publish_status(client, qos, topics, config, state, stream_url).await?;
    Ok(())
}

async fn execute_stop_stream(
    _client: &AsyncClient,
    _qos: QoS,
    _topics: &Topics,
    _config: &Config,
    state: &Arc<Mutex<AppState>>,
    _stream_url: &str,
) -> Result<()> {
    let mut locked = state.lock().await;
    if !locked.is_streaming {
        return Ok(());
    }
    if let Some(mut child) = locked.gst_child.take() {
        if let Some(pid) = child.id() {
            info!("stopping gstreamer pid={pid}");
        }
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
    locked.is_streaming = false;
    Ok(())
}

fn spawn_gstreamer_rtsp(config: &Config) -> Result<Child> {
    let path: String = config.rtsp.path.trim().trim_start_matches('/').to_string();
    let publish_url: String = format!("rtsp://127.0.0.1:{}/{}", config.rtsp.listen_port, path);
    let launch: String = format!(
        "gst-launch-1.0 -q v4l2src device={} ! video/x-raw,width={},height={},framerate={}/1 ! videoconvert ! x264enc tune=zerolatency speed-preset=ultrafast bitrate=1500 key-int-max=30 ! rtph264pay config-interval=1 pt=96 ! rtspclientsink location={}",
        config.rtsp.video_device,
        config.rtsp.width,
        config.rtsp.height,
        config.rtsp.fps,
        publish_url
    );
    let mut command: Command = Command::new("sh");
    command
        .arg("-lc")
        .arg(launch)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let child: Child = command.spawn().context("spawn sh")?;
    Ok(child)
}

fn now_unix_millis() -> Result<u128> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| anyhow!("system time before epoch: {err}"))?;
    Ok(duration.as_millis())
}

#[derive(Debug, Clone)]
struct WasmProcessor {
    module_path: String,
}

impl WasmProcessor {
    fn new(module_path: &str) -> Self {
        Self {
            module_path: module_path.to_string(),
        }
    }

    async fn process_json(&self, input_json: &[u8]) -> Result<Vec<u8>> {
        let mut child = Command::new("wasmedge")
            .arg(&self.module_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawn wasmedge")?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input_json).await.context("write wasm stdin")?;
        }
        let output = child.wait_with_output().await.context("wait wasmedge")?;
        if !output.status.success() {
            return Err(anyhow!("wasmedge exit code {:?}", output.status.code()));
        }
        Ok(output.stdout)
    }
}
