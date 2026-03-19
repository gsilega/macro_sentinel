// In-memory application state shared across all async tasks.
//
// Wrapped as Arc<RwLock<AppState>> throughout the app:
//   Arc     — shared ownership across tasks without copying data
//   RwLock  — concurrent reads allowed; writes are exclusive
//
// RwLock is preferred over Mutex here because HTTP reads (many, frequent)
// vastly outnumber FRED poll writes (one per interval).

use crate::indicators::{IndicatorId, IndicatorReading};
use std::collections::HashMap;

#[derive(Debug)]
pub struct AppState {
    /// Rolling window of recent readings per indicator.
    /// Bounded by MAX_READINGS_PER_INDICATOR to keep memory fixed.
    pub readings: HashMap<IndicatorId, Vec<IndicatorReading>>,

    /// Most recent AI-generated interpretation. None until first /api/interpret call.
    pub ai_interpretation: Option<String>,

    /// Timestamp of the last successful FRED poll cycle.
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
}

/// Number of historical readings retained per indicator.
/// Enough for AI trend context; bounded to keep memory predictable.
pub const MAX_READINGS_PER_INDICATOR: usize = 5;

impl AppState {
    pub fn new() -> Self {
        AppState {
            readings: HashMap::new(),
            ai_interpretation: None,
            last_updated: None,
        }
    }

    /// Insert a new reading, evicting the oldest if the window is full.
    pub fn upsert_reading(&mut self, reading: IndicatorReading) {
        let readings = self.readings.entry(reading.id).or_insert_with(Vec::new);
        readings.push(reading);
        while readings.len() > MAX_READINGS_PER_INDICATOR {
            readings.remove(0);
        }
    }

    /// Most recent reading for a given indicator. Returns None if not yet polled.
    pub fn latest_reading(&self, id: IndicatorId) -> Option<&IndicatorReading> {
        self.readings.get(&id).and_then(|v| v.last())
    }

    /// Latest reading for every indicator that has data. Used by REST and AI endpoints.
    pub fn all_latest_readings(&self) -> Vec<&IndicatorReading> {
        self.readings.values().filter_map(|v| v.last()).collect()
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
