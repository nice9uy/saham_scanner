//! Statistik ringkas satu saham untuk halaman detail.

use polars::prelude::*;
use shared::StockStatsDto;

use crate::frame::{columns, sorted};
use crate::{CoreError, format_day};

const BARS_1M: usize = 21;
const BARS_3M: usize = 63;
const BARS_1Y: usize = 250;

/// `history` berisi bar satu ticker (harga sudah disesuaikan). `None` kalau kosong.
pub fn stock_stats(history: &DataFrame) -> Result<Option<StockStatsDto>, CoreError> {
    if history.height() == 0 {
        return Ok(None);
    }
    let df = sorted(history)?;
    let cols = columns(&df)?;
    let open = df.column("open")?.f64()?.get(cols.close.len() - 1);
    let n = cols.close.len();
    let last = n - 1;
    let close = cols.close[last];
    let change = |bars: usize| (n > bars).then(|| (close / cols.close[last - bars] - 1.0) * 100.0);
    let year = n.saturating_sub(BARS_1Y)..n;
    let avg_value_20 = (n >= 20).then(|| {
        (n - 20..n).map(|i| cols.close[i] * cols.volume[i]).sum::<f64>() / 20.0
    });

    Ok(Some(StockStatsDto {
        last_date: format_day(cols.dates[last]),
        close,
        change_1d: change(1),
        change_1m: change(BARS_1M),
        change_3m: change(BARS_3M),
        change_1y: change(BARS_1Y),
        high_52w: cols.high[year.clone()].iter().copied().reduce(f64::max),
        low_52w: cols.low[year].iter().copied().reduce(f64::min),
        avg_value_20,
        open: open.filter(|v| v.is_finite()),
        high: Some(cols.high[last]).filter(|v| v.is_finite()),
        low: Some(cols.low[last]).filter(|v| v.is_finite()),
        prev_close: (n > 1).then(|| cols.close[last - 1]).filter(|v| v.is_finite()),
        volume: Some(cols.volume[last]).filter(|v| v.is_finite()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn stats_basic() {
        let bs: Vec<Bar> = (0..300)
            .map(|i| {
                let c = 100.0 + i as f64;
                [c, c + 2.0, c - 2.0, c, 1000.0]
            })
            .collect();
        let s = stock_stats(&bars(&[("AAA", bs)])).unwrap().unwrap();
        assert_eq!(s.close, 399.0);
        assert_eq!((s.open, s.high, s.low, s.prev_close, s.volume), (Some(399.0), Some(401.0), Some(397.0), Some(398.0), Some(1000.0)));
        assert_eq!(s.last_date, format_day(BASE_DAY + 299));
        assert!((s.change_1d.unwrap() - (399.0 / 398.0 - 1.0) * 100.0).abs() < 1e-9);
        assert!((s.change_1y.unwrap() - (399.0 / 149.0 - 1.0) * 100.0).abs() < 1e-9);
        assert_eq!(s.high_52w, Some(401.0));
        assert_eq!(s.low_52w, Some(148.0));
        let avg = (380..400).map(|c| c as f64 * 1000.0).sum::<f64>() / 20.0;
        assert!((s.avg_value_20.unwrap() - avg).abs() < 1e-6);

        let short = stock_stats(&closes(&[("B", vec![1.0; 10])])).unwrap().unwrap();
        assert_eq!(short.change_1m, None);
        assert_eq!(short.avg_value_20, None);
        assert!(stock_stats(&closes(&[])).unwrap().is_none());
    }
}
