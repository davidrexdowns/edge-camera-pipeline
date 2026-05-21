use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::config::AppConfig;

#[derive(Debug, Clone, Serialize)]
pub struct MetadataMessage {
    pub device_id: String,
    pub device_type: String,
    pub stream_url: String,
    pub sequence: u64,
    pub unix_millis: u128,
    pub iso8601_utc: String,
    pub status: String,
}

/// Build timestamped metadata JSON for MQTT publish.
pub fn build_metadata_message(
    config: &AppConfig,
    stream_url: &str,
    sequence: u64,
    unix_millis: u128,
) -> MetadataMessage {
    let iso8601_utc: String = format_unix_millis_rfc3339(unix_millis);
    MetadataMessage {
        device_id: config.device.id.clone(),
        device_type: config.device.device_type.as_str().to_string(),
        stream_url: stream_url.to_string(),
        sequence,
        unix_millis,
        iso8601_utc,
        status: "live".to_string(),
    }
}

pub fn format_unix_millis_rfc3339(unix_millis: u128) -> String {
    let seconds: i64 = (unix_millis / 1000) as i64;
    let nanos: i32 = ((unix_millis % 1000) * 1_000_000) as i32;
    let datetime: OffsetDateTime = OffsetDateTime::from_unix_timestamp(seconds)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
        + time::Duration::nanoseconds(nanos as i64);
    datetime
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

pub fn now_unix_millis() -> u128 {
    let now: OffsetDateTime = OffsetDateTime::now_utc();
    (now.unix_timestamp() as u128) * 1000 + (now.nanosecond() / 1_000_000) as u128
}
