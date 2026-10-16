-- Penutupan indeks (mis. IHSG `^JKSE`) untuk menilai kondisi pasar.
CREATE TABLE index_prices (
    symbol  TEXT NOT NULL,
    date    DATE NOT NULL,
    close   DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (symbol, date)
);
