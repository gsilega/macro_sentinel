// FRED API client and background polling loop.
//
// FRED (Federal Reserve Economic Data) is the St. Louis Fed's free public data API.
// Each economic series is identified by a short string ID (e.g., "CPIAUCSL").
// API key registration: https://fred.stlouisfed.org/docs/api/api_key.html
//
// --- Indicator Reference ---
//
// CPI (CPIAUCSL)
//   Average price change of ~80,000 consumer goods/services. Monthly, BLS.
//   Lagging. Fed watches for trend vs. 2% target. Above 3% = rate cuts off the table.
//
// Core CPI (CPILFESL)
//   CPI excluding food and energy. Strips out supply-shock volatility for a cleaner signal.
//   Lagging. Above 3% = persistent underlying inflation, unlikely to cut.
//
// PCE (PCEPI)
//   The Fed's actual 2% inflation target (not CPI). Broader than CPI; adjusts for substitution.
//   Lagging. THE primary rate-decision input.
//
// PPI (PPIFID)
//   Prices at the producer/wholesale level, before reaching consumers.
//   Leading (~1–3 months ahead of CPI). Rising PPI predicts future consumer price increases.
//
// NFP (PAYEMS)
//   Total monthly jobs added to the non-farm economy.
//   Lagging. >200K = tight labor = wage pressure. <100K = cooling = rate cuts more likely.
//
// U6 Unemployment (U6RATE)
//   Broad unemployment: headline U3 + discouraged workers + involuntary part-timers.
//   Lagging. Better captures actual labor market slack than the headline U3 rate.
//
// JOLTS (JTSJOL)
//   Total unfilled job openings at US employers. Monthly survey, BLS.
//   Leading. High openings = employers competing for workers = wage growth = services inflation.
//
// Average Hourly Earnings (CES0500000003)
//   Mean hourly wage for all private nonfarm workers.
//   Coincident. Wage growth >4% YoY is incompatible with 2% services inflation.
//
// GDP (GDPC1)
//   Real (inflation-adjusted) output. Quarterly, reported as SAAR.
//   Lagging. Strong GDP means the economy can absorb high rates. Weak GDP = cut pressure.
//
// ISM PMI (ISMMAN)
//   Purchasing managers survey. >50 = expansion, <50 = contraction.
//   Leading. Falls below 50 → recession risk rising → Fed shifts dovish.
//
// Retail Sales (RSXFS)
//   Monthly retail and food service sales, ex-autos and gas.
//   Coincident. Consumer spending is ~70% of GDP. Strength = demand-driven inflation risk.
//
// 2Y Treasury Yield (DGS2)
//   Yield on 2-year US Treasuries; directly prices near-term Fed rate expectations.
//   Leading. Falling 2Y = market pricing in cuts before the Fed acts.
//
// Fed Funds Rate (EFFR)
//   The actual overnight policy rate set by the FOMC.
//   Coincident. Benchmark context for interpreting all other readings.

use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::Config;
use crate::error::AppError;
use crate::indicators::{FredResponse, IndicatorId, IndicatorReading, classify_sentiment};
use crate::state::AppState;

const FRED_BASE_URL: &str = "https://api.stlouisfed.org/fred/series/observations";
// Fetch the 10 most recent observations per series to safely find the latest non-missing value.
const FRED_OBSERVATION_LIMIT: u32 = 10;

/// Background task that polls all indicators on a fixed interval.
/// Runs immediately on startup, then sleeps between cycles.
pub async fn run_polling_loop(config: Config, state: Arc<RwLock<AppState>>) {
    // Single client reused across all requests to leverage connection pooling.
    let http_client = reqwest::Client::new();

    info!(
        "FRED polling loop started. Interval: {}s",
        config.poll_interval_seconds
    );

    // Poll immediately so the UI has data before the first interval elapses.
    poll_all_indicators(&http_client, &config, &state).await;

    loop {
        tokio::time::sleep(Duration::from_secs(config.poll_interval_seconds)).await;
        info!("Polling FRED API for updates...");
        poll_all_indicators(&http_client, &config, &state).await;
    }
}

/// Fetch and store the latest value for every tracked indicator.
/// Individual fetch failures are logged and skipped — they don't abort the cycle.
async fn poll_all_indicators(
    client: &reqwest::Client,
    config: &Config,
    state: &Arc<RwLock<AppState>>,
) {
    for indicator_id in IndicatorId::all() {
        match fetch_indicator(client, config, *indicator_id).await {
            Ok(reading) => {
                // Write lock held only for the store operation, not across the HTTP fetch.
                let mut guard = state.write().await;
                guard.upsert_reading(reading);
                guard.last_updated = Some(chrono::Utc::now());
            }
            Err(e) => {
                warn!(indicator = ?indicator_id, error = %e, "Failed to fetch indicator");
            }
        }
    }
    info!("FRED poll complete.");
}

/// Returns the FRED `units` transformation parameter for series that need it.
///
/// FRED returns raw index levels by default. Several series need a units transform
/// to produce the values the rest of the app expects:
///   pc1 — percent change from a year ago (YoY %)
///   chg — change from prior period (month-over-month absolute change)
///   pca — compounded annual rate of change (annualized quarterly growth)
fn fred_units(id: IndicatorId) -> Option<&'static str> {
    match id {
        // Index levels → need YoY percent change
        IndicatorId::Cpi | IndicatorId::CoreCpi | IndicatorId::Pce | IndicatorId::Ppi => {
            Some("pc1")
        }
        // Total employed (level) → need monthly change in thousands
        IndicatorId::Nfp => Some("chg"),
        // Quarterly level → need annualized growth rate
        IndicatorId::Gdp => Some("pca"),
        // All others are already in usable units (%, $/hr, index points, thousands)
        _ => None,
    }
}

/// Fetch the most recent valid observation for a single FRED series.
async fn fetch_indicator(
    client: &reqwest::Client,
    config: &Config,
    id: IndicatorId,
) -> Result<IndicatorReading, AppError> {
    let series_id = id.fred_series_id();

    // sort_order=desc puts newest observations first so we find the latest valid value quickly.
    let units_param = fred_units(id)
        .map(|u| format!("&units={}", u))
        .unwrap_or_default();

    let url = format!(
        "{}?series_id={}&api_key={}&file_type=json&sort_order=desc&limit={}{}",
        FRED_BASE_URL, series_id, config.fred_api_key, FRED_OBSERVATION_LIMIT, units_param
    );

    // Fetch as raw text first so we can log the body if deserialization fails.
    let body = client.get(&url).send().await?.text().await?;

    let response: FredResponse = serde_json::from_str(&body).map_err(|e| {
        tracing::error!(
            series = series_id,
            body = &body[..body.len().min(300)],
            "FRED response parse failed: {}",
            e
        );
        AppError::ParseError(format!("Failed to parse FRED response for {}: {}", series_id, e))
    })?;

    // FRED returns "." for unreported periods — skip them and take the first real value.
    let valid_obs = response
        .observations
        .iter()
        .find(|obs| obs.value != "." && !obs.value.is_empty())
        .ok_or_else(|| {
            AppError::ParseError(format!("No valid observations for series {}", series_id))
        })?;

    let date = NaiveDate::parse_from_str(&valid_obs.date, "%Y-%m-%d")
        .map_err(|e| AppError::ParseError(format!("Invalid date '{}': {}", valid_obs.date, e)))?;

    let value: f64 = valid_obs.value.parse().map_err(|e| {
        AppError::ParseError(format!(
            "Invalid value '{}' for {}: {}",
            valid_obs.value, series_id, e
        ))
    })?;

    Ok(IndicatorReading {
        id,
        date,
        sentiment: classify_sentiment(id, value),
        display_value: format_display_value(id, value),
        value,
    })
}

/// Format a raw numeric value into a human-readable display string for the UI.
fn format_display_value(id: IndicatorId, value: f64) -> String {
    match id {
        IndicatorId::Cpi
        | IndicatorId::CoreCpi
        | IndicatorId::Pce
        | IndicatorId::Ppi
        | IndicatorId::TwoYearTreasuryYield
        | IndicatorId::FedFundsFutures => format!("{:.2}%", value),
        // units=chg gives the monthly change already in thousands — display directly as K
        IndicatorId::Nfp => format!("{:.0}K", value),
        IndicatorId::UnemploymentU6 => format!("{:.1}%", value),
        IndicatorId::Jolts => format!("{:.2}M", value / 1_000.0),
        IndicatorId::AverageHourlyEarnings => format!("${:.2}/hr", value),
        IndicatorId::Gdp => format!("{:.1}% SAAR", value),
        IndicatorId::IsmPmi => format!("{:.1}", value),
        IndicatorId::RetailSales => format!("${:.0}M", value),
    }
}
