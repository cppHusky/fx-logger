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
