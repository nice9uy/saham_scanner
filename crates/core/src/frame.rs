//! Pembacaan kolom DataFrame OHLCV ke vektor Rust, dikelompokkan per ticker.

use std::ops::Range;

use polars::prelude::*;

use crate::CoreError;

/// Kolom OHLCV dalam bentuk vektor. Nilai null menjadi NaN.
pub(crate) struct Columns {
    pub tickers: Vec<String>,
    pub dates: Vec<i32>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub volume: Vec<f64>,
    /// Rentang baris untuk tiap ticker (DataFrame harus terurut per ticker, date).
    pub groups: Vec<Range<usize>>,
}

pub(crate) fn sorted(df: &DataFrame) -> Result<DataFrame, CoreError> {
    Ok(df.clone().lazy().sort(["ticker", "date"], SortMultipleOptions::default()).collect()?)
}

pub(crate) fn columns(df: &DataFrame) -> Result<Columns, CoreError> {
    let f = |name: &str| -> Result<Vec<f64>, CoreError> {
        Ok(df.column(name)?.f64()?.iter().map(|v| v.unwrap_or(f64::NAN)).collect())
    };
    let tickers: Vec<String> =
        df.column("ticker")?.str()?.iter().map(|t| t.unwrap_or_default().to_string()).collect();
    let date_col = df.column("date")?.cast(&DataType::Int32)?;
    let dates: Vec<i32> = date_col.i32()?.iter().map(|d| d.unwrap_or(i32::MIN)).collect();

    let mut groups = Vec::new();
    let mut start = 0;
    for i in 1..=tickers.len() {
        if i == tickers.len() || tickers[i] != tickers[start] {
            groups.push(start..i);
            start = i;
        }
    }
    Ok(Columns { tickers, dates, high: f("high")?, low: f("low")?, close: f("close")?, volume: f("volume")?, groups })
}
