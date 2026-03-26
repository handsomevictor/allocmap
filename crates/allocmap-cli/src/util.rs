use std::time::Duration;
use anyhow::{anyhow, Result};

/// Parse a human-friendly duration string into a `std::time::Duration`.
///
/// Supported formats: `30s` (seconds), `5m` (minutes), `1h` (hours),
/// or a plain integer (interpreted as seconds).
///
/// # Errors
/// Returns an error if the string cannot be parsed.
pub fn parse_duration(s: &str) -> Result<Duration> {
    if let Some(v) = s.strip_suffix('s') {
        let n: u64 = v.parse().map_err(|_| {
            anyhow!("Invalid duration '{}': expected format like 30s, 5m, 1h", s)
        })?;
        return Ok(Duration::from_secs(n));
    }
    if let Some(v) = s.strip_suffix('m') {
        let n: u64 = v.parse().map_err(|_| {
            anyhow!("Invalid duration '{}': expected format like 30s, 5m, 1h", s)
        })?;
        return Ok(Duration::from_secs(n * 60));
    }
    if let Some(v) = s.strip_suffix('h') {
        let n: u64 = v.parse().map_err(|_| {
            anyhow!("Invalid duration '{}': expected format like 30s, 5m, 1h", s)
        })?;
        return Ok(Duration::from_secs(n * 3600));
    }
    // Try plain integer as seconds
    if let Ok(n) = s.parse::<u64>() {
        return Ok(Duration::from_secs(n));
    }
    Err(anyhow!(
        "Invalid duration '{}': expected format like 30s, 5m, 1h",
        s
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn test_parse_duration_plain_integer() {
        assert_eq!(parse_duration("60").unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn test_parse_duration_invalid_suffix() {
        assert!(parse_duration("30x").is_err());
    }

    #[test]
    fn test_parse_duration_invalid_string() {
        assert!(parse_duration("invalid").is_err());
    }

    #[test]
    fn test_parse_duration_empty_with_suffix() {
        // "s" alone means parse "" as u64 — should fail
        assert!(parse_duration("s").is_err());
    }

    #[test]
    fn test_parse_duration_empty_string() {
        // A truly empty string has no suffix and cannot be parsed as u64
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn test_parse_duration_zero_seconds() {
        // Zero is a valid duration value
        assert_eq!(parse_duration("0s").unwrap(), Duration::from_secs(0));
    }

    #[test]
    fn test_parse_duration_large_value() {
        // Large durations should round-trip correctly
        assert_eq!(parse_duration("3600s").unwrap(), Duration::from_secs(3600));
    }
}
