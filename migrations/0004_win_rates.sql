-- Win rate sinyal beli per saham dari seluruh riwayat, dihitung ulang setiap update harian.
CREATE TABLE win_rates (
    ticker      TEXT PRIMARY KEY REFERENCES stocks(ticker) ON DELETE CASCADE,
    signals     INTEGER NOT NULL,
    wins        INTEGER NOT NULL,
    avg_return  DOUBLE PRECISION NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
