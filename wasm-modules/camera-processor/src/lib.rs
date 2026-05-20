use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessorInput {
    pub device_id: String,
    pub stream_url: String,
    pub sequence: u64,
    pub unix_millis: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessorOutput {
    pub device_id: String,
    pub stream_url: String,
    pub sequence: u64,
    pub unix_millis: u128,
    pub iso8601_utc: String,
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

// Howard Hinnant's civil_from_days algorithm (public domain-like usage).
// Converts days since 1970-01-01 (Unix epoch) to (year, month, day).
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

#[no_mangle]
pub extern "C" fn process_json_ptr(
    input_ptr: *const u8,
    input_len: usize,
    output_ptr_ptr: *mut *mut u8,
    output_len_ptr: *mut usize,
) -> i32 {
    if input_ptr.is_null() || output_ptr_ptr.is_null() || output_len_ptr.is_null() {
        return -1;
    }
    let input: &[u8] = unsafe { std::slice::from_raw_parts(input_ptr, input_len) };
    let input_str: &str = match std::str::from_utf8(input) {
        Ok(value) => value,
        Err(_) => return -2,
    };
    let parsed: ProcessorInput = match serde_json::from_str(input_str) {
        Ok(value) => value,
        Err(_) => return -3,
    };
    let output: ProcessorOutput = ProcessorOutput {
        device_id: parsed.device_id,
        stream_url: parsed.stream_url,
        sequence: parsed.sequence,
        unix_millis: parsed.unix_millis,
        iso8601_utc: format_iso8601_utc(parsed.unix_millis),
    };
    let output_json: Vec<u8> = match serde_json::to_vec(&output) {
        Ok(value) => value,
        Err(_) => return -4,
    };
    let mut buffer: Vec<u8> = output_json;
    let out_ptr: *mut u8 = buffer.as_mut_ptr();
    let out_len: usize = buffer.len();
    std::mem::forget(buffer);
    unsafe {
        *output_ptr_ptr = out_ptr;
        *output_len_ptr = out_len;
    }
    0
}

#[no_mangle]
pub extern "C" fn alloc(len: i32) -> *mut u8 {
    if len <= 0 {
        return std::ptr::null_mut();
    }
    let capacity: usize = len as usize;
    let mut buffer: Vec<u8> = Vec::with_capacity(capacity);
    let ptr: *mut u8 = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

#[no_mangle]
pub extern "C" fn free_ptr(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    unsafe {
        let _ = Vec::from_raw_parts(ptr, len, len);
    }
}

