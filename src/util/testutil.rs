//! Parallel-safe unique suffixes for unit test temp paths.
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_PATH_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn temp_path_suffix() -> String {
    let seq = TEMP_PATH_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}_{}_{}", std::process::id(), nanos, seq)
}
