use std::io::{Read, Write};

fn main() {
    let mut input: String = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        std::process::exit(2);
    }
    let trimmed: &str = input.trim();
    if trimmed.is_empty() {
        std::process::exit(3);
    }
    let output: String = match camera_processor_cli::process(trimmed) {
        Ok(value) => value,
        Err(_) => std::process::exit(4),
    };
    let _ = std::io::stdout().write_all(output.as_bytes());
}

mod camera_processor_cli {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Deserialize)]
    struct ProcessorInput {
        device_id: String,
        stream_url: String,
        sequence: u64,
        unix_millis: u128,
    }

    #[derive(Debug, Clone, Serialize)]
    struct ProcessorOutput {
        device_id: String,
        stream_url: String,
        sequence: u64,
        unix_millis: u128,
        iso8601_utc: String,
    }

    pub fn process(input: &str) -> Result<String, ()> {
        let parsed: ProcessorInput = serde_json::from_str(input).map_err(|_| ())?;
        let output: ProcessorOutput = ProcessorOutput {
            device_id: parsed.device_id,
            stream_url: parsed.stream_url,
            sequence: parsed.sequence,
            unix_millis: parsed.unix_millis,
            iso8601_utc: format_iso8601_utc(parsed.unix_millis),
        };
        serde_json::to_string(&output).map_err(|_| ())
    }

    fn format_iso8601_utc(unix_millis: u128) -> String {
        let seconds: i64 = (unix_millis / 1000) as i64;
        let millis: i64 = (unix_millis % 1000) as i64;
        let days: i64 = seconds.div_euclid(86_400);
        let secs_of_day: i64 = seconds.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        let hour: i64 = secs_of_day / 3600;
        let minute: i64 = (secs_of_day % 3600) / 60;
        let second: i64 = secs_of_day % 60;
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            year, month, day, hour, minute, second, millis
        )
    }

    fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
        let z: i64 = days_since_epoch + 719_468;
        let era: i64 = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
        let doe: i64 = z - era * 146_097;
        let yoe: i64 = (doe - doe / 1460 + doe / 36_524 - doe / 146_096).div_euclid(365);
        let y: i64 = yoe + era * 400;
        let doy: i64 = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp: i64 = (5 * doy + 2).div_euclid(153);
        let d: i64 = doy - (153 * mp + 2).div_euclid(5) + 1;
        let m: i64 = mp + if mp < 10 { 3 } else { -9 };
        let year: i64 = y + if m <= 2 { 1 } else { 0 };
        (year, m, d)
    }
}
