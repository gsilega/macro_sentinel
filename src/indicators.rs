// Data types for all macroeconomic indicators tracked by the application.
// Pure data modeling — no network calls, no business logic.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Unique identifier for each of the 13 tracked indicators.
/// Using an enum rather than string constants makes incorrect usages a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IndicatorId {
    Cpi,
    CoreCpi,
    Pce,
    Ppi,
    Nfp,
    UnemploymentU6,
    Jolts,
    AverageHourlyEarnings,
    Gdp,
    IsmPmi,
    RetailSales,
    TwoYearTreasuryYield,
    FedFundsFutures,
}

impl IndicatorId {
    /// FRED series ID used in API requests. See https://fred.stlouisfed.org/
    pub fn fred_series_id(&self) -> &'static str {
        match self {
            // Headline CPI — Consumer Price Index, All Urban Consumers, seasonally adjusted
            IndicatorId::Cpi => "CPIAUCSL",
            // Core CPI — excludes food and energy for a less volatile inflation signal
            IndicatorId::CoreCpi => "CPILFESL",
            // PCE — the Fed's actual 2% inflation target (not CPI)
            IndicatorId::Pce => "PCEPI",
            // PPI — producer-level price pressure; leads CPI by ~1–3 months
            IndicatorId::Ppi => "PPIFID",
            // NFP — total monthly jobs added, the headline labor market print
            IndicatorId::Nfp => "PAYEMS",
            // U6 — broad unemployment including discouraged and involuntary part-time workers
            IndicatorId::UnemploymentU6 => "U6RATE",
            // JOLTS — total job openings; high readings signal tight labor and wage pressure
            IndicatorId::Jolts => "JTSJOL",
            // Average Hourly Earnings — wage growth feeds directly into services inflation
            IndicatorId::AverageHourlyEarnings => "CES0500000003",
            // Real GDP — quarterly output; context for how much rate pressure the economy can absorb
            IndicatorId::Gdp => "GDPC1",
            // ISM PMI — >50 expansion, <50 contraction; leading signal on business activity
            IndicatorId::IsmPmi => "ISMMAN",
            // Retail Sales ex-autos/gas — direct measure of consumer demand (~70% of GDP)
            IndicatorId::RetailSales => "RSXFS",
            // 2Y Treasury Yield — most Fed-sensitive bond; tracks near-term rate expectations
            IndicatorId::TwoYearTreasuryYield => "DGS2",
            // Effective Fed Funds Rate — the actual policy rate the FOMC sets
            IndicatorId::FedFundsFutures => "EFFR",
        }
    }

    /// Display name shown in the UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            IndicatorId::Cpi => "CPI (Headline)",
            IndicatorId::CoreCpi => "Core CPI",
            IndicatorId::Pce => "PCE Inflation",
            IndicatorId::Ppi => "PPI (Producer Prices)",
            IndicatorId::Nfp => "Non-Farm Payrolls",
            IndicatorId::UnemploymentU6 => "Unemployment (U6)",
            IndicatorId::Jolts => "JOLTS Job Openings",
            IndicatorId::AverageHourlyEarnings => "Avg Hourly Earnings",
            IndicatorId::Gdp => "GDP (Real)",
            IndicatorId::IsmPmi => "ISM PMI",
            IndicatorId::RetailSales => "Retail Sales",
            IndicatorId::TwoYearTreasuryYield => "2Y Treasury Yield",
            IndicatorId::FedFundsFutures => "Fed Funds Rate",
        }
    }

    /// Unit label for display.
    pub fn unit(&self) -> &'static str {
        match self {
            IndicatorId::Cpi | IndicatorId::CoreCpi | IndicatorId::Pce => "% YoY",
            IndicatorId::Ppi => "% YoY",
            IndicatorId::Nfp => "K jobs",
            IndicatorId::UnemploymentU6 => "%",
            IndicatorId::Jolts => "M openings",
            IndicatorId::AverageHourlyEarnings => "$/hr",
            IndicatorId::Gdp => "% SAAR",
            IndicatorId::IsmPmi => "index",
            IndicatorId::RetailSales => "$M",
            IndicatorId::TwoYearTreasuryYield => "%",
            IndicatorId::FedFundsFutures => "%",
        }
    }

    /// All indicator IDs. Used by the polling loop to iterate over every series.
    pub fn all() -> &'static [IndicatorId] {
        use IndicatorId::*;
        &[
            Cpi,
            CoreCpi,
            Pce,
            Ppi,
            Nfp,
            UnemploymentU6,
            Jolts,
            AverageHourlyEarnings,
            Gdp,
            IsmPmi,
            RetailSales,
            TwoYearTreasuryYield,
            FedFundsFutures,
        ]
    }
}

/// A single observed data point for one indicator at one point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorReading {
    pub id: IndicatorId,
    pub date: NaiveDate,
    pub value: f64,
    /// Pre-formatted display string (e.g., "3.24%", "256K"). Computed once at parse time.
    pub display_value: String,
    pub sentiment: Sentiment,
}

/// Fed policy implication of a reading.
///
/// Hawkish: inflationary / rate-hike or hold pressure.
/// Dovish: disinflationary / rate-cut pressure.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Sentiment {
    Hawkish,
    Neutral,
    Dovish,
}

impl Sentiment {
    /// CSS class name for color-coding in the frontend.
    pub fn css_class(&self) -> &'static str {
        match self {
            Sentiment::Hawkish => "hawkish",
            Sentiment::Neutral => "neutral",
            Sentiment::Dovish => "dovish",
        }
    }

    /// Short label with indicator emoji.
    pub fn label(&self) -> &'static str {
        match self {
            Sentiment::Hawkish => "🔴 Hawkish",
            Sentiment::Neutral => "🟡 Neutral",
            Sentiment::Dovish => "🟢 Dovish",
        }
    }
}

/// Classify a reading's sentiment using simplified heuristic thresholds.
/// These drive UI color-coding only — nuanced interpretation is handled by the AI module.
pub fn classify_sentiment(id: IndicatorId, value: f64) -> Sentiment {
    match id {
        IndicatorId::Cpi | IndicatorId::CoreCpi | IndicatorId::Pce => {
            if value > 2.5 {
                Sentiment::Hawkish
            } else if value < 2.0 {
                Sentiment::Dovish
            } else {
                Sentiment::Neutral
            }
        }
        IndicatorId::Ppi => {
            if value > 2.5 {
                Sentiment::Hawkish
            } else if value < 1.5 {
                Sentiment::Dovish
            } else {
                Sentiment::Neutral
            }
        }
        IndicatorId::Nfp => {
            if value > 200.0 {
                Sentiment::Hawkish
            } else if value < 100.0 {
                Sentiment::Dovish
            } else {
                Sentiment::Neutral
            }
        }
        IndicatorId::UnemploymentU6 => {
            if value > 8.0 {
                Sentiment::Dovish
            } else if value < 7.0 {
                Sentiment::Hawkish
            } else {
                Sentiment::Neutral
            }
        }
        // JTSJOL is in thousands — 9,000 = 9M openings, 7,000 = 7M openings
        IndicatorId::Jolts => {
            if value > 9_000.0 {
                Sentiment::Hawkish
            } else if value < 7_000.0 {
                Sentiment::Dovish
            } else {
                Sentiment::Neutral
            }
        }
        IndicatorId::AverageHourlyEarnings => {
            if value > 35.0 {
                Sentiment::Hawkish
            } else {
                Sentiment::Neutral
            }
        }
        IndicatorId::Gdp => {
            if value > 3.0 {
                Sentiment::Hawkish
            } else if value < 1.0 {
                Sentiment::Dovish
            } else {
                Sentiment::Neutral
            }
        }
        IndicatorId::IsmPmi => {
            if value > 55.0 {
                Sentiment::Hawkish
            } else if value < 47.0 {
                Sentiment::Dovish
            } else {
                Sentiment::Neutral
            }
        }
        // Retail Sales is an absolute dollar level — MoM change is needed for meaningful
        // sentiment classification. Neutral until delta tracking is implemented.
        IndicatorId::RetailSales => Sentiment::Neutral,
        IndicatorId::TwoYearTreasuryYield => {
            if value > 4.5 {
                Sentiment::Hawkish
            } else if value < 3.5 {
                Sentiment::Dovish
            } else {
                Sentiment::Neutral
            }
        }
        IndicatorId::FedFundsFutures => {
            if value > 4.5 {
                Sentiment::Hawkish
            } else if value < 3.0 {
                Sentiment::Dovish
            } else {
                Sentiment::Neutral
            }
        }
    }
}

// --- FRED API response types ---
// Partial mirrors of the FRED JSON response. Serde ignores fields not defined here.

#[derive(Debug, Deserialize)]
pub struct FredResponse {
    pub observations: Vec<FredObservation>,
}

#[derive(Debug, Deserialize)]
pub struct FredObservation {
    /// ISO date string: "YYYY-MM-DD"
    pub date: String,
    /// Numeric value as a string. FRED returns "." for unreported periods.
    pub value: String,
}
