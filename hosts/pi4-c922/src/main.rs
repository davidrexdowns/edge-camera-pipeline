use anyhow::Result;
use chrono::Utc;
use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::process::Command;
use wasmedge_sdk::{config::{CommonConfigOptions, ConfigBuilder, HostRegistrationConfigOptions}, VmBuilder, WasmValue};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Pi4 C922 Host + Wasm Processor started");

    // === 1. Load Wasm Module ===
    let config = ConfigBuilder::new(CommonConfigOptions::default())
        .with_host_registration_config(HostRegistrationConfigOptions::default())
        .build()?;
    
    let vm = VmBuilder::new()
        .with_config(config)
        .build()?
        .register_module_from_file("processor", "../../../wasm-modules/camera-processor/target/wasm32-wasip1/release/camera-processor.wasm")?;

    // === 2. MQTT Setup ===
    let mut mqttoptions = MqttOptions::new("edge-2", "your-mqtt-broker-ip", 1883);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));
    let (mut client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    // === 3. Start RTSP Stream (background) ===
    std::thread::spawn(|| {
        Command::new("gst-launch-1.0")
            .args([
                "v4l2src", "device=/dev/video0", "!",
                "video/x-raw,width=1280,height=720,framerate=15/1", "!",
                "x264enc", "tune=zerolatency", "speed-preset=ultrafast", "!",
                "rtph264pay", "config-interval=10", "!",
                "udpsink", "host=0.0.0.0", "port=5000"
            ])
            .spawn()
            .expect("Failed to start RTSP stream");
    });

    println!("RTSP stream started on rtsp://<pi-ip>:8554/stream (configure rtsp-simple-server)");

    // === Main loop: Capture metadata and process with Wasm ===
    let mut frame_number: u64 = 0;
    loop {
        frame_number += 1;

        let input = serde_json::json!({
            "camera_id": "edge-2-c922",
            "frame_number": frame_number,
            "brightness": None::<f32>,
            "motion_score": None::<f32>
        });

        // Call Wasm
        let results = vm.run_func("processor", "main", &[WasmValue::from(input.to_string())])?;

        if let Some(output) = results.first() {
            let metadata = output.to_string();
            println!("Metadata: {}", metadata);

            // Publish to MQTT
            client.publish(
                "cameras/edge-2/metadata",
                QoS::AtLeastOnce,
                false,
                metadata.as_bytes()
            ).await?;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await; // ~5 fps metadata
    }
}
