//! Query PostgreSQL per tabel. Memakai query runtime (bukan macro `query!`) supaya build
//! tidak membutuhkan database.

pub mod fundamentals;
pub mod index_prices;
pub mod ingest_runs;
pub mod prices;
pub mod scans;
pub mod stats;
pub mod stocks;
pub mod watchlists;
pub mod win_rates;
