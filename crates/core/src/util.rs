/// Format epoch milliseconds as a UTC datetime string.
/// Uses Howard Hinnant's civil_from_days algorithm to avoid chrono dependency.
pub fn format_ts(epoch_ms: u64) -> String {
    let total_secs = (epoch_ms / 1000) as i64;
    let ms = epoch_ms % 1000;

    let secs_in_day = total_secs.rem_euclid(86400);
    let days = (total_secs - secs_in_day).div_euclid(86400);

    let h = secs_in_day / 3600;
    let m = (secs_in_day % 3600) / 60;
    let s = secs_in_day % 60;

    // civil_from_days: convert days since Unix epoch to (year, month, day)
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    format!("{year:04}-{month:02}-{d:02}T{h:02}:{m:02}:{s:02}.{ms:03}Z")
}

/// Current epoch time in milliseconds.
pub fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
