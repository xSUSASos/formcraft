//! AI request/reply records (JSON lines, one file per day) for debugging and future tuning.
//! Directory: `FORMCRAFT_LOG_DIR/ai`, or `%LOCALAPPDATA%/FormCraft/logs/ai`, or
//! `~/.local/state/formcraft/logs/ai`. Logging never fails the request.

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

pub fn dir() -> Option<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(d) = var("FORMCRAFT_LOG_DIR") {
        return Some(d.join("ai"));
    }
    if let Some(d) = var("LOCALAPPDATA") {
        return Some(d.join("FormCraft").join("logs").join("ai"));
    }
    let state = var("XDG_STATE_HOME").or_else(|| var("HOME").map(|h| h.join(".local").join("state")))?;
    Some(state.join("formcraft").join("logs").join("ai"))
}

/// Civil date (UTC) from days since 1970-01-01 (H. Hinnant's algorithm).
pub fn date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn write(record: &Value) {
    let Some(dir) = dir() else { return };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (y, m, d) = date((now / 86_400) as i64);
    let mut rec = record.clone();
    rec["time"] = Value::from(now);
    let path = dir.join(format!("ai-{y:04}-{m:02}-{d:02}.jsonl"));
    let r = std::fs::create_dir_all(&dir).and_then(|()| {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        writeln!(f, "{rec}")
    });
    if let Err(e) = r {
        log::warn!("AI log {}: {e}", path.display());
    }
}
