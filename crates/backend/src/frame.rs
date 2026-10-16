//! Baris harga dari database → DataFrame input `scanner-core`.

use chrono::NaiveDate;
use polars::prelude::*;

use crate::db::prices::PriceRow;

fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).expect("tanggal epoch valid")
}

pub fn days_since_epoch(d: NaiveDate) -> i32 {
    (d - epoch()).num_days() as i32
}

/// Faktor penyesuaian split/dividen: `adj_close / close` (1 kalau tidak bisa dihitung).
pub fn adjust_factor(row: &PriceRow) -> f64 {
    if row.close != 0.0 && row.adj_close.is_finite() && row.adj_close > 0.0 {
        row.adj_close / row.close
    } else {
        1.0
    }
}

/// Kolom: `ticker, date, open, high, low, close, volume` dengan OHLC disesuaikan.
pub fn prices_frame(rows: &[PriceRow]) -> PolarsResult<DataFrame> {
    let n = rows.len();
    let (mut ticker, mut date) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let (mut open, mut high, mut low, mut close, mut volume) =
        (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    for r in rows {
        let f = adjust_factor(r);
        ticker.push(r.ticker.as_str());
        date.push(days_since_epoch(r.date));
        open.push(r.open * f);
        high.push(r.high * f);
        low.push(r.low * f);
        close.push(r.close * f);
        volume.push(r.volume as f64);
    }
    let mut df = df!(
        "ticker" => ticker,
        "open" => open,
        "high" => high,
        "low" => low,
        "close" => close,
        "volume" => volume,
    )?;
    df.with_column(Column::new("date".into(), date).cast(&DataType::Date)?)?;
    Ok(df)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(close: f64, adj: f64) -> PriceRow {
        PriceRow {
            ticker: "AAA".into(),
            date: NaiveDate::from_ymd_opt(2024, 10, 4).unwrap(),
            open: close,
            high: close + 10.0,
            low: close - 10.0,
            close,
            adj_close: adj,
            volume: 1000,
        }
    }

    #[test]
    fn frame_applies_adjustment() {
        let df = prices_frame(&[row(100.0, 50.0), row(0.0, 0.0)]).unwrap();
        let col = |n: &str| df.column(n).unwrap().f64().unwrap().iter().collect::<Vec<_>>();
        assert_eq!(col("close"), [Some(50.0), Some(0.0)]);
        assert_eq!(col("high"), [Some(55.0), Some(10.0)]);
        assert_eq!(col("volume"), [Some(1000.0), Some(1000.0)]);
        let days = df.column("date").unwrap().cast(&DataType::Int32).unwrap();
        assert_eq!(days.i32().unwrap().get(0), Some(20_000));
    }
}
