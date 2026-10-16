//! Engine analisa saham: indikator teknikal dan scan atas DataFrame OHLCV multi-ticker.
//! Tidak melakukan IO; input dan output berupa `DataFrame` dan tipe dari `shared`.

pub mod error;
pub mod indicators;
pub mod validate;

#[cfg(test)]
mod testutil;

pub use error::CoreError;
pub use validate::validate;

pub mod engine;
pub use engine::{AVG_VALUE_COL, Annotations, StockMeta, format_day, run_scan, run_scan_annotated};

pub mod backtest;
pub mod brief;
pub mod market;
pub use market::MarketRegime;
pub mod presets;
pub mod quality;
pub mod risk;
pub mod signal;
pub use backtest::Backtest;
pub use signal::signals;
pub use risk::risk_reports;
pub use presets::{LIQUID_MIN_VALUE, presets};

mod frame;
pub mod stats;
pub mod summary;
pub use stats::stock_stats;
pub use summary::market_summary;
