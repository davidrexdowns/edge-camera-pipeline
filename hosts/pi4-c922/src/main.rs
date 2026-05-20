use anyhow::Result;
use chrono::Utc;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tokio;

#[derive(Debug, Deserialize)]
struct Config {
    device_id: String,
    mqtt: MqttConfig,
}

#[derive(Debug, Deserialize)]
struct MqttConfig {
    host: String,
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("✅ edge-camera-host started");

    // Load config later - for now hardcoded (we'll improve)
    let device_id = "edge-2-c922"; // change for edge-3
    let jetson_ip = "100.103.86.154"; // ← YOUR JETSON NETBIRD IP HERE

    let mut mqttoptions = MqttOptions::new(device_id, jetson_ip, 1883);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(10));

    let (mut client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    // Subscribe to commands
    client.subscribe(format!("cameras/{}/cmd", device_id), QoS::AtLeastOnce).await?;

    println!("Connected to Jetson MQTT. Waiting for commands...");

    let mut running = false;

    loop {
        let notification = eventloop.poll().await?;
        if let rumqttc::Event::Incoming(rumqttc::Packet::Publish(p)) = notification {
            let payload = String::from_utf8_lossy(&p.payload);
            println!("Command received: {}", payload);

            match payload.trim() {
                "start" => {
                    if !running {
                        running = true;
                        println!("Starting camera + RTSP stream");
                        // TODO: start GStreamer → MediaMTX
                    }
                }
                "stop" => {
                    running = false;
                    println!("Stopping camera stream");
                }
                _ => {}
            }
        }
    }
}