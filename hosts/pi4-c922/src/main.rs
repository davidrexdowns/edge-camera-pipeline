use anyhow::Result;
use chrono::Utc;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::fs;
use tokio;

#[derive(Debug, Deserialize)]
struct Config {
    device_id: String,
    jetson_netbird_ip: String,
    mqtt_port: u16,
}

#[derive(Debug, Serialize)]
struct Metadata {
    camera_id: String,
    timestamp: String,
    frame_number: u64,
    status: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("✅ edge-camera-host started");

    // Load config
    let config: Config = toml::from_str(&fs::read_to_string("config.toml")
        .unwrap_or_else(|_| include_str!("../config.example.toml").to_string()))?;

    let mut mqttoptions = MqttOptions::new(
        &config.device_id,
        &config.jetson_netbird_ip,
        config.mqtt_port,
    );
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(10));

    let (mut client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    // Subscribe to commands
    client.subscribe(format!("cameras/{}/cmd", config.device_id), QoS::AtLeastOnce).await?;

    println!("Connected to Jetson. Device: {}", config.device_id);
    println!("Waiting for commands (start / stop)...");

    let mut running = false;
    let mut frame_number: u64 = 0;

    loop {
        let notification = eventloop.poll().await?;
        if let rumqttc::Event::Incoming(rumqttc::Packet::Publish(p)) = notification {
            let cmd = String::from_utf8_lossy(&p.payload).trim().to_string();
            println!("Received command: {}", cmd);

            match cmd.as_str() {
                "start" => {
                    if !running {
                        running = true;
                        println!("🚀 Starting camera stream");
                        // TODO: Start GStreamer RTSP here
                    }
                }
                "stop" => {
                    running = false;
                    println!("⛔ Stopping camera stream");
                }
                _ => println!("Unknown command: {}", cmd),
            }
        }

        // Send metadata while running
        if running {
            frame_number += 1;
            let metadata = Metadata {
                camera_id: config.device_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                frame_number,
                status: "live".to_string(),
            };

            if let Ok(json) = serde_json::to_string(&metadata) {
                client.publish(
                    format!("cameras/{}/metadata", config.device_id),
                    QoS::AtLeastOnce,
                    false,
                    json.as_bytes(),
                ).await?;
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(800)).await;
    }
}