//! Indikator rekursif (nilai hari ini bergantung pada status kemarin) yang dihitung
//! per grup ticker. ADX, Ichimoku, dan PSAR memakai yata. Supertrend ditulis sendiri
//! karena yata 0.7 tidak menyediakannya.

use std::ops::Range;

use polars::prelude::*;
use shared::Indicator;
use yata::core::{Candle, Source};
use yata::helpers::MA;
use yata::indicators::{AverageDirectionalIndex, IchimokuCloud, ParabolicSAR};
use yata::prelude::*;

use crate::CoreError;

/// Menambahkan satu kolom per indikator. `df` harus terurut per (ticker, date).
pub fn add_columns(df: &mut DataFrame, inds: &[&Indicator]) -> Result<(), CoreError> {
    if inds.is_empty() {
        return Ok(());
    }
    let (candles, groups) = candles_and_groups(df)?;
    for ind in inds {
        let key = ind.key();
        if df.get_column_index(&key).is_some() {
            continue;
        }
        let mut out: Vec<Option<f64>> = vec![None; candles.len()];
        for g in &groups {
            let vals = compute(ind, &candles[g.clone()])?;
            out[g.clone()].copy_from_slice(&vals);
        }
        df.with_column(Column::new(key.into(), out))?;
    }
    Ok(())
}

fn candles_and_groups(df: &DataFrame) -> Result<(Vec<Candle>, Vec<Range<usize>>), CoreError> {
    let f = |name: &str| -> Result<Vec<f64>, CoreError> {
        Ok(df.column(name)?.f64()?.iter().map(|v| v.unwrap_or(f64::NAN)).collect())
    };
    let (o, h, l, c, v) = (f("open")?, f("high")?, f("low")?, f("close")?, f("volume")?);
    let candles = (0..df.height())
        .map(|i| Candle { open: o[i], high: h[i], low: l[i], close: c[i], volume: v[i] })
        .collect();

    let tickers: Vec<Option<&str>> = df.column("ticker")?.str()?.iter().collect();
    let mut groups = Vec::new();
    let mut start = 0;
    for i in 1..=tickers.len() {
        if i == tickers.len() || tickers[i] != tickers[start] {
            groups.push(start..i);
            start = i;
        }
    }
    Ok((candles, groups))
}

fn compute(ind: &Indicator, c: &[Candle]) -> Result<Vec<Option<f64>>, CoreError> {
    use Indicator::*;
    if c.is_empty() {
        return Ok(vec![]);
    }
    let raw = match ind {
        Supertrend { period, mult_x10 } => {
            return Ok(supertrend(c, usize::from(*period), f64::from(*mult_x10) / 10.0));
        }
        Adx { period } => run(adx(*period), c, 0, 100.0)?,
        PlusDi { period } => run(adx(*period), c, 1, 100.0)?,
        MinusDi { period } => run(adx(*period), c, 2, 100.0)?,
        IchimokuTenkan { tenkan, kijun, senkou_b } => run(ichimoku(*tenkan, *kijun, *senkou_b), c, 0, 1.0)?,
        IchimokuKijun { tenkan, kijun, senkou_b } => run(ichimoku(*tenkan, *kijun, *senkou_b), c, 1, 1.0)?,
        IchimokuSenkouA { tenkan, kijun, senkou_b } => run(ichimoku(*tenkan, *kijun, *senkou_b), c, 2, 1.0)?,
        IchimokuSenkouB { tenkan, kijun, senkou_b } => run(ichimoku(*tenkan, *kijun, *senkou_b), c, 3, 1.0)?,
        ParabolicSar { step_x1000, max_x1000 } => run(
            ParabolicSAR {
                af_step: f64::from(*step_x1000) / 1000.0,
                af_max: f64::from(*max_x1000) / 1000.0,
            },
            c,
            0,
            1.0,
        )?,
        other => {
            return Err(CoreError::Invalid(format!("{} bukan indikator rekursif", other.label())));
        }
    };
    // yata langsung mengeluarkan angka sejak bar pertama; bar yang belum cukup data dibuang.
    let warmup = ind.lookback().saturating_sub(1);
    Ok(raw
        .into_iter()
        .enumerate()
        .map(|(i, v)| (i >= warmup && v.is_finite()).then_some(v))
        .collect())
}

fn run<C: IndicatorConfig>(cfg: C, c: &[Candle], idx: usize, scale: f64) -> Result<Vec<f64>, CoreError> {
    let mut inst = cfg
        .init(&c[0])
        .map_err(|e| CoreError::Invalid(format!("konfigurasi indikator tidak valid: {e:?}")))?;
    Ok(c.iter().map(|k| inst.next(k).value(idx) * scale).collect())
}

fn adx(period: u16) -> AverageDirectionalIndex {
    AverageDirectionalIndex { method1: MA::RMA(period), method2: MA::RMA(period), period1: 1, zone: 0.2 }
}

fn ichimoku(tenkan: u16, kijun: u16, senkou_b: u16) -> IchimokuCloud {
    IchimokuCloud { l1: tenkan, l2: kijun, l3: senkou_b, m: kijun, source: Source::Close }
}

/// Supertrend dengan ATR Wilder (seed = rata-rata TR `period` bar pertama).
pub(crate) fn supertrend(c: &[Candle], period: usize, mult: f64) -> Vec<Option<f64>> {
    let n = c.len();
    let mut out = vec![None; n];
    if period == 0 || n < period {
        return out;
    }
    let tr = |i: usize| {
        if i == 0 {
            return c[0].high - c[0].low;
        }
        let pc = c[i - 1].close;
        (c[i].high - c[i].low).max((c[i].high - pc).abs()).max((c[i].low - pc).abs())
    };
    let mut atr = (0..period).map(tr).sum::<f64>() / period as f64;
    let (mut upper, mut lower, mut up) = (0.0, 0.0, true);
    for i in (period - 1)..n {
        if i >= period {
            atr = (atr * (period as f64 - 1.0) + tr(i)) / period as f64;
        }
        let mid = (c[i].high + c[i].low) / 2.0;
        let (basic_upper, basic_lower) = (mid + mult * atr, mid - mult * atr);
        if i == period - 1 {
            upper = basic_upper;
            lower = basic_lower;
            up = c[i].close >= mid;
        } else {
            let pc = c[i - 1].close;
            upper = if basic_upper < upper || pc > upper { basic_upper } else { upper };
            lower = if basic_lower > lower || pc < lower { basic_lower } else { lower };
            up = if up { c[i].close >= lower } else { c[i].close > upper };
        }
        out[i] = Some(if up { lower } else { upper });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    fn trend_up_then_crash() -> Vec<Bar> {
        let mut bs: Vec<Bar> = (0..30)
            .map(|i| {
                let c = 100.0 + i as f64;
                [c, c + 1.0, c - 1.0, c, 1e6]
            })
            .collect();
        for i in 0..5 {
            let c = 125.0 - 10.0 * i as f64;
            bs.push([c, c + 1.0, c - 1.0, c, 1e6]);
        }
        bs
    }

    fn compute_one(bs: Vec<Bar>, ind: Indicator) -> Vec<Option<f64>> {
        let mut df = bars(&[("A", bs)]);
        add_columns(&mut df, &[&ind]).unwrap();
        values(&df, &ind.key())
    }

    #[test]
    fn supertrend_supports_uptrend_then_flips() {
        let bs = trend_up_then_crash();
        let st = compute_one(bs.clone(), Indicator::Supertrend { period: 10, mult_x10: 30 });
        assert!(st[..9].iter().all(Option::is_none));
        assert!(st[29].unwrap() < bs[29][3], "dalam uptrend supertrend di bawah harga");
        assert!(st[34].unwrap() > bs[34][3], "setelah crash supertrend di atas harga");
    }

    #[test]
    fn adx_detects_strong_uptrend() {
        let bs: Vec<Bar> = (0..80)
            .map(|i| {
                let c = 100.0 + 2.0 * i as f64;
                [c, c + 1.0, c - 1.0, c, 1e6]
            })
            .collect();
        let adx = compute_one(bs.clone(), Indicator::Adx { period: 14 });
        let plus = compute_one(bs.clone(), Indicator::PlusDi { period: 14 });
        let minus = compute_one(bs, Indicator::MinusDi { period: 14 });
        assert!(adx[..27].iter().all(Option::is_none));
        let last = adx[79].unwrap();
        assert!((0.0..=100.0).contains(&last) && last > 25.0, "adx = {last}");
        assert!(plus[79].unwrap() > minus[79].unwrap());
    }

    #[test]
    fn ichimoku_tenkan_is_midpoint_of_range() {
        let x = wave(80, 100.0, 6.0, 0.2);
        let bs: Vec<Bar> = x.iter().map(|&c| [c, c + 1.0, c - 1.0, c, 1e6]).collect();
        let ind = Indicator::IchimokuTenkan { tenkan: 9, kijun: 26, senkou_b: 52 };
        let tenkan = compute_one(bs.clone(), ind);
        let w = &bs[71..80];
        let hh = w.iter().map(|b| b[1]).fold(f64::MIN, f64::max);
        let ll = w.iter().map(|b| b[2]).fold(f64::MAX, f64::min);
        assert!((tenkan[79].unwrap() - (hh + ll) / 2.0).abs() < 1e-9);
        let senkou_b = compute_one(bs, Indicator::IchimokuSenkouB { tenkan: 9, kijun: 26, senkou_b: 52 });
        assert!(senkou_b[76].is_none() && senkou_b[77].is_some());
    }

    #[test]
    fn psar_stays_below_price_in_uptrend() {
        let bs: Vec<Bar> = (0..40)
            .map(|i| {
                let c = 100.0 + i as f64;
                [c, c + 1.0, c - 1.0, c, 1e6]
            })
            .collect();
        let sar = compute_one(bs.clone(), Indicator::ParabolicSar { step_x1000: 20, max_x1000: 200 });
        assert_eq!(sar[0], None);
        assert!(sar[39].unwrap() < bs[39][2]);
    }

    #[test]
    fn groups_are_computed_independently() {
        let a: Vec<Bar> = wave(50, 100.0, 5.0, 0.3).iter().map(|&c| [c, c + 1.0, c - 1.0, c, 1e6]).collect();
        let b: Vec<Bar> = wave(40, 50.0, 3.0, -0.1).iter().map(|&c| [c, c + 1.0, c - 1.0, c, 1e6]).collect();
        let ind = Indicator::Supertrend { period: 7, mult_x10: 20 };
        let mut both = bars(&[("A", a), ("B", b.clone())]);
        add_columns(&mut both, &[&ind]).unwrap();
        let alone = compute_one(b, ind.clone());
        assert_close(&values_for(&both, "B", &ind.key()), &alone);
    }
}
