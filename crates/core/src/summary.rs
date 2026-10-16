//! Ringkasan pasar untuk dashboard.

use std::cmp::Ordering;
use std::collections::HashMap;

use polars::prelude::*;
use shared::{MarketSummaryDto, MoverDto};

use crate::frame::{columns, sorted};
use crate::presets::LIQUID_MIN_VALUE;
use crate::{CoreError, StockMeta, format_day};

struct Last {
    mover: MoverDto,
    avg_value_20: Option<f64>,
}

/// Hanya ticker yang bar terakhirnya sama dengan tanggal terbaru yang dihitung.
pub fn market_summary(
    prices: &DataFrame,
    meta: &HashMap<String, StockMeta>,
    top: usize,
) -> Result<MarketSummaryDto, CoreError> {
    let cols = columns(&sorted(prices)?)?;
    let latest = cols.dates.iter().copied().max();

    let mut lasts: Vec<Last> = Vec::new();
    for g in &cols.groups {
        let i = g.end - 1;
        if Some(cols.dates[i]) != latest || g.len() < 2 {
            continue;
        }
        let (close, prev) = (cols.close[i], cols.close[i - 1]);
        if !close.is_finite() || !prev.is_finite() || prev == 0.0 {
            continue;
        }
        let ticker = cols.tickers[i].clone();
        let avg_value_20 = (g.len() >= 20)
            .then(|| (i + 1 - 20..=i).map(|j| cols.close[j] * cols.volume[j]).sum::<f64>() / 20.0);
        lasts.push(Last {
            mover: MoverDto {
                name: meta.get(&ticker).map_or_else(|| ticker.clone(), |m| m.name.clone()),
                ticker,
                close,
                change_pct: (close / prev - 1.0) * 100.0,
                value: close * cols.volume[i],
            },
            avg_value_20,
        });
    }

    let advancers = lasts.iter().filter(|l| l.mover.change_pct > 0.0).count();
    let decliners = lasts.iter().filter(|l| l.mover.change_pct < 0.0).count();
    let unchanged = lasts.len() - advancers - decliners;

    let by = |key: fn(&MoverDto) -> f64, desc: bool, liquid_only: bool| -> Vec<MoverDto> {
        let mut v: Vec<&Last> = lasts
            .iter()
            .filter(|l| !liquid_only || l.avg_value_20.is_some_and(|a| a >= LIQUID_MIN_VALUE))
            .collect();
        v.sort_by(|a, b| {
            let o = key(&a.mover).partial_cmp(&key(&b.mover)).unwrap_or(Ordering::Equal);
            (if desc { o.reverse() } else { o }).then_with(|| a.mover.ticker.cmp(&b.mover.ticker))
        });
        v.into_iter().take(top).map(|l| l.mover.clone()).collect()
    };
    let mut top_gainers = by(|m| m.change_pct, true, true);
    top_gainers.retain(|m| m.change_pct > 0.0);
    let mut top_losers = by(|m| m.change_pct, false, true);
    top_losers.retain(|m| m.change_pct < 0.0);

    Ok(MarketSummaryDto {
        as_of: latest.map(format_day).unwrap_or_default(),
        advancers,
        decliners,
        unchanged,
        top_gainers,
        top_losers,
        top_value: by(|m| m.value, true, false),
        market_strong: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    fn series(last: f64) -> Vec<f64> {
        let mut v = vec![5000.0; 25];
        v[24] = last;
        v
    }

    #[test]
    fn summary_counts_and_tops() {
        let mut thin = vec![100.0; 25];
        thin[24] = 150.0;
        let df = closes_volumes(&[
            ("UPPP", series(5250.0), vec![1e6; 25]),
            ("DOWN", series(4500.0), vec![1e6; 25]),
            ("FLAT", series(5000.0), vec![3e6; 25]),
            ("THIN", thin, vec![10.0; 25]),
            ("OLDD", vec![5000.0; 20], vec![1e6; 20]),
        ]);
        let meta = HashMap::from([(
            "UPPP".to_string(),
            StockMeta { name: "Naik Tbk".into(), sector: None, board: None },
        )]);
        let s = market_summary(&df, &meta, 10).unwrap();
        assert_eq!(s.as_of, format_day(BASE_DAY + 24));
        assert_eq!((s.advancers, s.decliners, s.unchanged), (2, 1, 1));

        let gainers: Vec<&str> = s.top_gainers.iter().map(|m| m.ticker.as_str()).collect();
        assert_eq!(gainers, ["UPPP"]);
        assert_eq!(s.top_gainers[0].name, "Naik Tbk");
        assert!((s.top_gainers[0].change_pct - 5.0).abs() < 1e-9);
        let losers: Vec<&str> = s.top_losers.iter().map(|m| m.ticker.as_str()).collect();
        assert_eq!(losers, ["DOWN"]);
        let value: Vec<&str> = s.top_value.iter().map(|m| m.ticker.as_str()).collect();
        assert_eq!(value, ["FLAT", "UPPP", "DOWN", "THIN"]);
    }
}
