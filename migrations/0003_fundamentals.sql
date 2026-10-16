-- Data fundamental terkini per saham (rupiah per saham), diperbarui setiap update harian.
CREATE TABLE fundamentals (
    ticker          TEXT PRIMARY KEY REFERENCES stocks(ticker) ON DELETE CASCADE,
    eps             DOUBLE PRECISION,
    book_value      DOUBLE PRECISION,
    dividend_yield  DOUBLE PRECISION,
    market_cap      DOUBLE PRECISION,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
