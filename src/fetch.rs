use std::collections::HashMap;

use anyhow::{bail, Context};
use serde::Deserialize;

use crate::cache::{self, CacheEntry};

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(default)]
    timestamp: Option<u64>,
    rates: HashMap<String, f64>,
    #[serde(default)]
    error: Option<bool>,
    #[serde(default)]
    description: Option<String>,
}

pub fn fetch_rates(app_id: &str) -> anyhow::Result<CacheEntry> {
    let url = format!(
        "https://openexchangerates.org/api/latest.json?app_id={}",
        app_id
    );

    let response: ApiResponse = ureq::get(&url)
        .call()
        .context("API request failed")?
        .into_json()
        .context("failed to parse API response")?;

    if response.error.unwrap_or(false) {
        bail!(
            "API error: {}",
            response.description.as_deref().unwrap_or("unknown error")
        );
    }

    let timestamp = response.timestamp.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    });

    let usd_rate = response
        .rates
        .get("USD")
        .copied()
        .filter(|rate| rate.is_finite() && *rate > 0.0)
        .context("API response does not contain a valid USD rate")?;
    let mut rates = response
        .rates
        .into_iter()
        .map(|(code, rate)| (code, rate / usd_rate))
        .collect::<HashMap<_, _>>();
    rates.insert("USD".to_string(), 1.0);

    Ok(CacheEntry {
        timestamp,
        base: "USD".to_string(),
        rates,
    })
}

pub fn fetch_and_cache(app_id: &str) -> anyhow::Result<()> {
    let entry = fetch_rates(app_id)?;
    let ts = entry.timestamp;
    cache::write(&entry)?;
    eprintln!("Stored rates at timestamp {}", ts);
    Ok(())
}
