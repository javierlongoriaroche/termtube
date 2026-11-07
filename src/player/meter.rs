use serde_json::Value;

/// Lowest level shown by the meter, in dBFS.
const FLOOR_DB: f64 = -60.0;

/// Stereo levels normalised to 0.0..=1.0.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Levels {
    pub left_rms: f64,
    pub right_rms: f64,
    pub left_peak: f64,
    pub right_peak: f64,
}

/// mpv audio filter chain entry that publishes per-channel stats.
pub const METER_FILTER: &str = "@meter:lavfi=[astats=metadata=1:reset=1]";

/// Reads the `af-metadata/meter` property map produced by `METER_FILTER`.
pub fn parse_levels(data: &Value) -> Option<Levels> {
    let get = |key: &str| data.get(key).and_then(Value::as_str).map(db_to_unit);
    let left_rms = get("lavfi.astats.1.RMS_level")?;
    let right_rms = get("lavfi.astats.2.RMS_level").unwrap_or(left_rms);
    let left_peak = get("lavfi.astats.1.Peak_level").unwrap_or(left_rms);
    let right_peak = get("lavfi.astats.2.Peak_level").unwrap_or(left_peak);
    Some(Levels {
        left_rms,
        right_rms,
        left_peak,
        right_peak,
    })
}

/// Maps a dBFS string ("-18.5", "-inf") to 0.0..=1.0 over the meter range.
fn db_to_unit(db: &str) -> f64 {
    match db.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => ((v - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_levels_maps_db_range() {
        let data = json!({
            "lavfi.astats.1.RMS_level": "-30.0",
            "lavfi.astats.2.RMS_level": "-60",
            "lavfi.astats.1.Peak_level": "0",
            "lavfi.astats.2.Peak_level": "-inf",
        });
        let l = parse_levels(&data).unwrap();
        assert!((l.left_rms - 0.5).abs() < 1e-9);
        assert_eq!(l.right_rms, 0.0);
        assert_eq!(l.left_peak, 1.0);
        assert_eq!(l.right_peak, 0.0);
    }

    #[test]
    fn test_parse_levels_mono_and_missing() {
        let mono = parse_levels(&json!({"lavfi.astats.1.RMS_level": "-15"})).unwrap();
        assert_eq!(mono.left_rms, mono.right_rms);
        assert_eq!(parse_levels(&json!({})), None);
    }

    #[test]
    fn test_db_to_unit_clamps() {
        assert_eq!(db_to_unit("6"), 1.0);
        assert_eq!(db_to_unit("-120"), 0.0);
        assert_eq!(db_to_unit("garbage"), 0.0);
    }
}
