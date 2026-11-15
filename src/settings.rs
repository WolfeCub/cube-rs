//! User settings, saved as JSON.

use crate::gesture::SLAM_VERTICAL;
use nanoserde::{DeJson, SerJson};
use std::fs;
use std::path::Path;

/// Slam sensitivity notches, from least to most sensitive: a name and the slam threshold (peak
/// vertical m/s²). Medium is the tuned threshold; each notch is 1.25 times the next.
pub const SENSITIVITIES: [(&str, f32); 5] = [
    ("Low", SLAM_VERTICAL * 1.5625),
    ("Low–medium", SLAM_VERTICAL * 1.25),
    ("Medium", SLAM_VERTICAL),
    ("Medium–high", SLAM_VERTICAL / 1.25),
    ("High", SLAM_VERTICAL / 1.5625),
];
/// Hold times to choose from, in ms.
pub const HOLD_MS: [u64; 4] = [100, 300, 500, 1000];

/// Missing fields take the default given here.
#[derive(Debug, Clone, PartialEq, Eq, DeJson, SerJson)]
pub struct Settings {
    /// A slam stops the timer.
    #[nserde(default = "true")]
    pub slam: bool,
    /// Index into `SENSITIVITIES`.
    #[nserde(default = "2")]
    pub sensitivity: usize,
    /// How long to hold before a release starts the timer.
    #[nserde(default = "300")]
    pub hold_ms: u64,
    /// A tap starts 15 s of WCA inspection before the hold.
    #[nserde(default = "false")]
    pub inspection: bool,
    /// Save a trace of each solve. Needs the microphone permission.
    #[nserde(default = "false")]
    pub tracing: bool,
    /// Ask how each solve stopped. Needs tracing.
    #[nserde(default = "false")]
    pub questions: bool,
    /// The open session's name.
    #[nserde(default = "Default")]
    pub session: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self::deserialize_json("{}").unwrap()
    }
}

impl Settings {
    /// Reads `path`, or the defaults if it's missing or invalid.
    pub fn load(path: &Path) -> Self {
        let text = fs::read_to_string(path).unwrap_or_default();
        Self::deserialize_json(&text).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Err(e) = fs::write(path, self.serialize_json()) {
            log::error!("saving {}: {e}", path.display());
        }
    }

    pub fn slam_vertical(&self) -> f32 {
        SENSITIVITIES[self.sensitivity].1
    }

    pub fn hold_ns(&self) -> u64 {
        self.hold_ms * 1_000_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Settings {
            slam: false,
            sensitivity: 4,
            hold_ms: 1000,
            inspection: true,
            tracing: true,
            questions: true,
            session: "OH".into(),
        };
        assert_eq!(Settings::deserialize_json(&s.serialize_json()).unwrap(), s);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s = Settings::deserialize_json(r#"{"hold_ms": 500}"#).unwrap();
        let hold_ms = 500;
        assert_eq!(
            s,
            Settings {
                hold_ms,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn medium_is_the_tuned_threshold() {
        assert_eq!(Settings::default().slam_vertical(), SLAM_VERTICAL);
    }
}
