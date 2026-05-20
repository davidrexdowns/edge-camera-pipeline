use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead};

#[derive(Debug, Deserialize)]
struct IncomingFrame {
    camera_id: String,
    frame_number: u64,
    brightness: Option<f32>,
    motion_score: Option<f32>,
}

#[derive(Debug, Serialize)]
struct TimestampedMetadata {
    camera_id: String,
    timestamp: DateTime<Utc>,
    processed_at: DateTime<Utc>,
    frame_number: u64,
    brightness: Option<f32>,
    motion_score: Option<f32>,
    source: String,
    version: String,
}

fn main() {
    println!("Wasm Camera Processor v0.1.0 started");

    let mut frame_counter: u64 = 0;
    let stdin = io::stdin();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l.trim().to_string(),
            Err(_) => continue,
        };

        if line.is_empty() {
            continue;
        }

        if let Ok(frame) = serde_json::from_str::<IncomingFrame>(&line) {
            frame_counter += 1;

            let metadata = TimestampedMetadata {
                camera_id: frame.camera_id,
                timestamp: Utc::now(),
                processed_at: Utc::now(),
                frame_number: frame.frame_number,
                brightness: frame.brightness,
                motion_score: frame.motion_score,
                source: "edge-pi".to_string(),
                version: "0.1.0".to_string(),
            };

            if let Ok(json) = serde_json::to_string(&metadata) {
                println!("{}", json);
            }
        }
    }
}
