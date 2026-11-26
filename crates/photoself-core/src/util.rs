use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, NaiveDateTime, Utc};

pub const EPOCH_SORT_KEY: &str = "1970-01-01T00:00:00";

pub fn encode_cursor(taken_at: Option<&str>, id: &str) -> String {
    let key = taken_at.unwrap_or(EPOCH_SORT_KEY);
    URL_SAFE_NO_PAD.encode(format!("{key}\n{id}"))
}

pub fn decode_cursor(raw: &str) -> Option<(String, String)> {
    let decoded = URL_SAFE_NO_PAD.decode(raw).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (key, id) = text.split_once('\n')?;
    if id.is_empty() {
        return None;
    }
    Some((
        if key.is_empty() {
            EPOCH_SORT_KEY.to_string()
        } else {
            key.to_string()
        },
        id.to_string(),
    ))
}

/// EXIF dates are `YYYY:MM:DD HH:MM:SS` without a timezone. Store them as
/// a naive local timestamp string so they sort correctly and are displayed
/// as captured.
pub fn normalize_exif_datetime(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let core = raw.get(0..19)?;
    NaiveDateTime::parse_from_str(core, "%Y:%m:%d %H:%M:%S")
        .ok()
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S").to_string())
}

/// Video creation_time is normally RFC3339 UTC; store it in UTC.
pub fn rfc3339_to_utc(raw: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(raw.trim())
        .ok()
        .map(|dt| dt.naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string())
}

pub fn mtime_to_utc(seconds: i64) -> String {
    DateTime::<Utc>::from_timestamp(seconds, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_else(|| EPOCH_SORT_KEY.to_string())
}

pub fn is_supported_path(path: &std::path::Path) -> Option<crate::model::MediaKind> {
    let ext = path.extension()?.to_str()?;
    crate::model::MediaKind::from_extension(ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_roundtrip() {
        let cursor = encode_cursor(Some("2026-01-02T03:04:05"), "abc");
        let decoded = decode_cursor(&cursor).unwrap();
        assert_eq!(decoded.0, "2026-01-02T03:04:05");
        assert_eq!(decoded.1, "abc");
    }

    #[test]
    fn exif_datetime() {
        assert_eq!(
            normalize_exif_datetime("2023:11:10 08:12:33").unwrap(),
            "2023-11-10T08:12:33"
        );
        assert!(normalize_exif_datetime("not a date").is_none());
    }

    #[test]
    fn rfc3339() {
        assert_eq!(
            rfc3339_to_utc("2023-11-10T08:12:33.000000Z").unwrap(),
            "2023-11-10T08:12:33"
        );
    }
}
