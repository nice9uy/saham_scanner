CREATE TABLE stocks (
    ticker      TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    sector      TEXT,
    board       TEXT,
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE daily_prices (
    ticker     TEXT NOT NULL REFERENCES stocks(ticker) ON DELETE CASCADE,
    date       DATE NOT NULL,
    open       DOUBLE PRECISION NOT NULL,
    high       DOUBLE PRECISION NOT NULL,
    low        DOUBLE PRECISION NOT NULL,
    close      DOUBLE PRECISION NOT NULL,
    adj_close  DOUBLE PRECISION NOT NULL,
    volume     BIGINT NOT NULL,
    PRIMARY KEY (ticker, date)
);
CREATE INDEX daily_prices_date_idx ON daily_prices (date);

CREATE TABLE watchlists (
    id          BIGSERIAL PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE watchlist_items (
    watchlist_id BIGINT NOT NULL REFERENCES watchlists(id) ON DELETE CASCADE,
    ticker       TEXT NOT NULL REFERENCES stocks(ticker) ON DELETE CASCADE,
    added_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (watchlist_id, ticker)
);

CREATE TABLE saved_scans (
    id          BIGSERIAL PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    rules       JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE ingest_runs (
    id              BIGSERIAL PRIMARY KEY,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at     TIMESTAMPTZ,
    status          TEXT NOT NULL,
    trigger         TEXT NOT NULL,
    ok_count        INTEGER NOT NULL DEFAULT 0,
    failed_tickers  TEXT[] NOT NULL DEFAULT '{}',
    error           TEXT
);
