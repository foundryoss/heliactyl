use once_cell::sync::Lazy;
use serde_json::json;
use std::{
    fs::File,
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::time::{self, Duration};

struct UptimeTrackerInner {
    start_time: Instant,
    file_path: String,
    previous_uptime: u64,
}

static UPTIME_TRACKER: Lazy<Arc<Mutex<Option<UptimeTrackerInner>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));

pub struct UptimeTracker;

impl UptimeTracker {
    /// Initialize and start the uptime tracker
    pub fn start(file_path: &str) {
        let previous_uptime = Self::read_uptime_from_file(file_path);

        let mut tracker = UPTIME_TRACKER.lock().unwrap();
        *tracker = Some(UptimeTrackerInner {
            start_time: Instant::now(),
            file_path: file_path.to_string(),
            previous_uptime,
        });

        // Start background task to save uptime periodically
        Self::spawn_save_task();
    }

    /// Get the current total uptime in seconds
    pub fn current() -> u64 {
        let tracker = UPTIME_TRACKER.lock().unwrap();
        if let Some(inner) = tracker.as_ref() {
            let elapsed = inner.start_time.elapsed().as_secs();
            inner.previous_uptime + elapsed
        } else {
            0
        }
    }

    /// Save the current uptime to file immediately
    pub fn save() {
        let tracker = UPTIME_TRACKER.lock().unwrap();
        if let Some(inner) = tracker.as_ref() {
            let total_uptime = inner.previous_uptime + inner.start_time.elapsed().as_secs();
            Self::write_uptime_to_file(&inner.file_path, total_uptime);
        }
    }

    // Private helper methods

    fn read_uptime_from_file(file_path: &str) -> u64 {
        if let Ok(mut file) = File::open(file_path) {
            let mut contents = String::new();
            if file.read_to_string(&mut contents).is_ok() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&contents) {
                    if let Some(uptime) = parsed.get("uptime_seconds").and_then(|v| v.as_u64()) {
                        return uptime;
                    }
                }
            }
        }
        0
    }

    fn write_uptime_to_file(file_path: &str, uptime: u64) {
        let data = json!({
            "uptime_seconds": uptime
        });

        if let Ok(mut file) = File::create(file_path) {
            let _ = file.write_all(data.to_string().as_bytes());
        }
    }

    fn spawn_save_task() {
        tokio::spawn(async {
            let mut interval = time::interval(Duration::from_secs(20));
            loop {
                interval.tick().await;
                Self::save();
            }
        });
    }
}

// Optional: Implement Drop to save uptime when the program exits
impl Drop for UptimeTrackerInner {
    fn drop(&mut self) {
        let total_uptime = self.previous_uptime + self.start_time.elapsed().as_secs();
        UptimeTracker::write_uptime_to_file(&self.file_path, total_uptime);
    }
}