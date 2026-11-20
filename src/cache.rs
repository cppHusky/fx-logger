use std::collections::HashMap;

use anyhow::{bail, Context};
use postgres::{Client, NoTls};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub timestamp: u64,
    pub base: String,
    pub rates: HashMap<String, f64>,
}

fn connect() -> anyhow::Result<Client> {
    let database_url = std::env::var("FX_DATABASE_URL")
        .context("FX_DATABASE_URL not set in environment or .env file")?;
    let mut client =
        Client::connect(&database_url, NoTls).context("failed to connect to PostgreSQL")?;

    client
        .batch_execute(
            "
            CREATE TABLE IF NOT EXISTS rate_snapshot (
                fetched_at TIMESTAMPTZ PRIMARY KEY
            );

            CREATE TABLE IF NOT EXISTS currency_rate (
                fetched_at TIMESTAMPTZ NOT NULL
                    REFERENCES rate_snapshot(fetched_at) ON DELETE CASCADE,
                currency_code TEXT NOT NULL,
                units_per_usd DOUBLE PRECISION NOT NULL,
                PRIMARY KEY (fetched_at, currency_code)
            );
            ",
        )
        .context("failed to initialize PostgreSQL schema")?;

    Ok(client)
}

pub fn get_history() -> anyhow::Result<Vec<CacheEntry>> {
    let mut client = connect()?;
    let rows = client
        .query(
            "
            SELECT
                EXTRACT(EPOCH FROM snapshot.fetched_at)::BIGINT,
                rate.currency_code,
                rate.units_per_usd
            FROM rate_snapshot AS snapshot
            LEFT JOIN currency_rate AS rate USING (fetched_at)
            ORDER BY snapshot.fetched_at ASC, rate.currency_code ASC
            ",
            &[],
        )
        .context("failed to read rates from PostgreSQL")?;

    let mut history: Vec<CacheEntry> = Vec::new();
    for row in rows {
        let timestamp: i64 = row.get(0);
        let timestamp = u64::try_from(timestamp).context("snapshot has an invalid timestamp")?;

        if history.last().map(|entry| entry.timestamp) != Some(timestamp) {
            history.push(CacheEntry {
                timestamp,
                base: "USD".to_string(),
                rates: HashMap::new(),
            });
        }

        let code: Option<String> = row.get(1);
        let value: Option<f64> = row.get(2);
        if let (Some(code), Some(value)) = (code, value) {
            history
                .last_mut()
                .expect("snapshot was just added")
                .rates
                .insert(code, value);
        }
    }

    Ok(history)
}

pub fn write(entry: &CacheEntry) -> anyhow::Result<()> {
    let mut client = connect()?;
    let timestamp = i64::try_from(entry.timestamp).context("timestamp is out of range")?;
    // PostgreSQL's to_timestamp() takes a double-precision epoch value.
    let timestamp_seconds = timestamp as f64;
    let mut transaction = client
        .transaction()
        .context("failed to start PostgreSQL transaction")?;

    transaction
        .execute(
            "INSERT INTO rate_snapshot (fetched_at) VALUES (to_timestamp($1)) \
             ON CONFLICT (fetched_at) DO NOTHING",
            &[&timestamp_seconds],
        )
        .context("failed to store rate snapshot")?;

    // A repeated API timestamp should replace its previous rates as one atomic snapshot.
    transaction
        .execute(
            "DELETE FROM currency_rate WHERE fetched_at = to_timestamp($1)",
            &[&timestamp_seconds],
        )
        .context("failed to replace rates for existing snapshot")?;

    for (code, rate) in &entry.rates {
        if !rate.is_finite() || *rate <= 0.0 {
            bail!("invalid rate for {code}: {rate}");
        }
        transaction
            .execute(
                "INSERT INTO currency_rate (fetched_at, currency_code, units_per_usd) \
                 VALUES (to_timestamp($1), $2, $3)",
                &[&timestamp_seconds, code, rate],
            )
            .with_context(|| format!("failed to store rate for {code}"))?;
    }

    transaction
        .commit()
        .context("failed to commit rate snapshot")?;
    Ok(())
}
