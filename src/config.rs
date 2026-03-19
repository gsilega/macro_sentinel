// Application configuration loaded from environment variables.
//
// All API keys are read exclusively via std::env::var() at startup.
// They are never hardcoded, never read from files, and never logged.
// Set them in your shell before running: export FRED_API_KEY=your_key

use crate::error::AppError;
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub fred_api_key: String,
    pub anthropic_api_key: String,
    // How often (seconds) to poll FRED. Defaults to 300s — most series update daily or monthly.
    pub poll_interval_seconds: u64,
}

impl Config {
    /// Load all configuration from environment variables. Fails fast if required keys are absent.
    pub fn from_env() -> Result<Self, AppError> {
        let fred_api_key =
            env::var("FRED_API_KEY").map_err(|_| AppError::MissingEnvVar("FRED_API_KEY"))?;

        let anthropic_api_key = env::var("ANTHROPIC_API_KEY")
            .map_err(|_| AppError::MissingEnvVar("ANTHROPIC_API_KEY"))?;

        // POLL_INTERVAL_SECONDS is optional — falls back to 300 if absent or unparseable.
        let poll_interval_seconds = env::var("POLL_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "300".to_string())
            .parse::<u64>()
            .unwrap_or(300);

        Ok(Config {
            fred_api_key,
            anthropic_api_key,
            poll_interval_seconds,
        })
    }
}
