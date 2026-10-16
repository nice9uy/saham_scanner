//! Indikator vektor sebagai ekspresi Polars. `raw()` menghasilkan ekspresi untuk satu
//! ticker; `expr()` membungkusnya dengan `.over("ticker")` supaya window tidak bocor
//! antar ticker. DataFrame harus terurut per (ticker, date).

use polars::prelude::*;
use shared::Indicator;

use crate::CoreError;

pub fn window(n: u16) -> RollingOptionsFixedWindow {
    RollingOptionsFixedWindow {
        window_size: usize::from(n),
        min_periods: usize::from(n),
        ..Default::default()
    }
}

fn ema_opts(n: u16) -> EWMOptions {
    EWMOptions {
        alpha: 2.0 / (f64::from(n) + 1.0),
        adjust: false,
        bias: false,
        min_periods: usize::from(n),
        ignore_nulls: true,
    }
}

/// Smoothing Wilder (RMA): alpha = 1/n.
fn wilder_opts(n: u16) -> EWMOptions {
    EWMOptions { alpha: 1.0 / f64::from(n), ..ema_opts(n) }
}

fn close() -> Expr {
    col("close")
}

fn std_dev(n: u16) -> Expr {
    close().rolling_std(RollingOptionsFixedWindow {
        fn_params: Some(RollingFnParams::Var(RollingVarParams { ddof: 0 })),
        ..window(n)
    })
}

fn k(std_x10: u16) -> f64 {
    f64::from(std_x10) / 10.0
}

fn macd_line(fast: u16, slow: u16) -> Expr {
    close().ewm_mean(ema_opts(fast)) - close().ewm_mean(ema_opts(slow))
}

fn rsi(n: u16) -> Expr {
    let diff = close() - close().shift(lit(1));
    // cast(bool) * diff mempertahankan null di bar pertama.
    let gain = diff.clone().gt(lit(0.0)).cast(DataType::Float64) * diff.clone();
    let loss = diff.clone().lt(lit(0.0)).cast(DataType::Float64) * (lit(0.0) - diff);
    let (g, l) = (gain.ewm_mean(wilder_opts(n)), loss.ewm_mean(wilder_opts(n)));
    when(l.clone().eq(lit(0.0)))
        .then(lit(100.0))
        .otherwise(lit(100.0) - lit(100.0) / (lit(1.0) + g / l))
}

fn true_range() -> Result<Expr, CoreError> {
    let prev = close().shift(lit(1));
    Ok(polars::lazy::dsl::max_horizontal([
        col("high") - col("low"),
        (col("high") - prev.clone()).abs(),
        (col("low") - prev).abs(),
    ])?)
}

fn stoch_k(n: u16, smooth: u16) -> Expr {
    let ll = col("low").rolling_min(window(n));
    let range = col("high").rolling_max(window(n)) - ll.clone();
    when(range.clone().eq(lit(0.0)))
        .then(lit(50.0))
        .otherwise(lit(100.0) * (close() - ll) / range)
        .rolling_mean(window(smooth))
}

/// Ekspresi satu ticker, tanpa `.over()`.
pub fn raw(ind: &Indicator) -> Result<Expr, CoreError> {
    use Indicator::*;
    Ok(match ind {
        Open => col("open"),
        High => col("high"),
        Low => col("low"),
        Close => close(),
        Volume => col("volume"),
        ChangePct { period } => (close() / close().shift(lit(i64::from(*period))) - lit(1.0)) * lit(100.0),
        Sma { period } => close().rolling_mean(window(*period)),
        Ema { period } => close().ewm_mean(ema_opts(*period)),
        VolumeSma { period } => col("volume").rolling_mean(window(*period)),
        Rsi { period } => rsi(*period),
        MacdLine { fast, slow, .. } => macd_line(*fast, *slow),
        MacdSignal { fast, slow, signal } => macd_line(*fast, *slow).ewm_mean(ema_opts(*signal)),
        MacdHist { fast, slow, signal } => {
            let line = macd_line(*fast, *slow);
            line.clone() - line.ewm_mean(ema_opts(*signal))
        }
        BbMiddle { period, .. } => close().rolling_mean(window(*period)),
        BbUpper { period, std_x10 } => close().rolling_mean(window(*period)) + lit(k(*std_x10)) * std_dev(*period),
        BbLower { period, std_x10 } => close().rolling_mean(window(*period)) - lit(k(*std_x10)) * std_dev(*period),
        BbWidth { period, std_x10 } => {
            lit(2.0 * k(*std_x10)) * std_dev(*period) / close().rolling_mean(window(*period))
        }
        Atr { period } => true_range()?.ewm_mean(wilder_opts(*period)),
        StochK { period, smooth } => stoch_k(*period, *smooth),
        StochD { period, smooth, d } => stoch_k(*period, *smooth).rolling_mean(window(*d)),
        HighestHigh { period } => col("high").rolling_max(window(*period)),
        LowestLow { period } => col("low").rolling_min(window(*period)),
        LowestOf { inner, period } => {
            if inner.is_recursive() {
                return Err(CoreError::Invalid(format!(
                    "{} tidak bisa dipakai di dalam Lowest Of",
                    inner.label()
                )));
            }
            raw(inner)?.rolling_min(window(*period))
        }
        Obv => {
            let diff = close() - close().shift(lit(1));
            when(diff.clone().gt(lit(0.0)))
                .then(col("volume"))
                .when(diff.lt(lit(0.0)))
                .then(lit(0.0) - col("volume"))
                .otherwise(lit(0.0))
                .cum_sum(false)
        }
        AccDist => money_flow_volume().cum_sum(false),
        Cmf { period } => {
            let volume = col("volume").rolling_sum(window(*period));
            when(volume.clone().eq(lit(0.0)))
                .then(lit(0.0))
                .otherwise(money_flow_volume().rolling_sum(window(*period)) / volume)
        }
        Mfi { period } => mfi(*period),
        RelVolume { period } => col("volume") / col("volume").shift(lit(1)).rolling_mean(window(*period)),
        other => {
            return Err(CoreError::Invalid(format!("{} bukan indikator vektor", other.label())));
        }
    })
}

/// Volume berbobot posisi penutupan dalam rentang hari itu (dasar A/D dan CMF).
fn money_flow_volume() -> Expr {
    let range = col("high") - col("low");
    let multiplier = when(range.clone().eq(lit(0.0)))
        .then(lit(0.0))
        .otherwise(((close() - col("low")) - (col("high") - close())) / range);
    multiplier * col("volume")
}

fn mfi(n: u16) -> Expr {
    let typical = (col("high") + col("low") + close()) / lit(3.0);
    let raw_flow = typical.clone() * col("volume");
    let prev = typical.clone().shift(lit(1));
    // cast(bool) * flow mempertahankan null di bar pertama.
    let positive = typical.clone().gt(prev.clone()).cast(DataType::Float64) * raw_flow.clone();
    let negative = typical.lt(prev).cast(DataType::Float64) * raw_flow;
    let (pos, neg) = (positive.rolling_sum(window(n)), negative.rolling_sum(window(n)));
    when(neg.clone().eq(lit(0.0)))
        .then(lit(100.0))
        .otherwise(lit(100.0) - lit(100.0) / (lit(1.0) + pos / neg))
}

/// Ekspresi per ticker, siap dipakai di `with_columns`.
pub fn expr(ind: &Indicator) -> Result<Expr, CoreError> {
    Ok(raw(ind)?.over([col("ticker")])?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    fn eval(df: &DataFrame, ind: &Indicator) -> Vec<Option<f64>> {
        let out = df.clone().lazy().with_column(expr(ind).unwrap().alias("out")).collect().unwrap();
        values(&out, "out")
    }

    fn sma_ref(x: &[f64], n: usize) -> Vec<Option<f64>> {
        (0..x.len())
            .map(|i| (i + 1 >= n).then(|| x[i + 1 - n..=i].iter().sum::<f64>() / n as f64))
            .collect()
    }

    /// EWM `adjust=false`, dimulai dari nilai non-null pertama.
    fn ewm_ref(x: &[Option<f64>], alpha: f64, min_periods: usize) -> Vec<Option<f64>> {
        let (mut state, mut count) = (None::<f64>, 0);
        x.iter()
            .map(|v| {
                let v = (*v)?;
                count += 1;
                let s = state.map_or(v, |p| (1.0 - alpha) * p + alpha * v);
                state = Some(s);
                (count >= min_periods).then_some(s)
            })
            .collect()
    }

    fn std_ref(x: &[f64], n: usize) -> Vec<Option<f64>> {
        (0..x.len())
            .map(|i| {
                (i + 1 >= n).then(|| {
                    let w = &x[i + 1 - n..=i];
                    let m = w.iter().sum::<f64>() / n as f64;
                    (w.iter().map(|v| (v - m).powi(2)).sum::<f64>() / n as f64).sqrt()
                })
            })
            .collect()
    }

    fn some(x: &[f64]) -> Vec<Option<f64>> {
        x.iter().copied().map(Some).collect()
    }

    fn sample() -> Vec<f64> {
        wave(60, 100.0, 8.0, 0.1)
    }

    #[test]
    fn sma_and_ema() {
        let x = sample();
        let df = closes(&[("A", x.clone())]);
        assert_close(&eval(&df, &Indicator::Sma { period: 5 }), &sma_ref(&x, 5));
        assert_close(&eval(&df, &Indicator::Ema { period: 5 }), &ewm_ref(&some(&x), 2.0 / 6.0, 5));
    }

    #[test]
    fn rsi_wilder() {
        let x = sample();
        let df = closes(&[("A", x.clone())]);
        let diff: Vec<Option<f64>> = (0..x.len()).map(|i| (i > 0).then(|| x[i] - x[i - 1])).collect();
        let gain: Vec<Option<f64>> = diff.iter().map(|d| d.map(|d| d.max(0.0))).collect();
        let loss: Vec<Option<f64>> = diff.iter().map(|d| d.map(|d| (-d).max(0.0))).collect();
        let (g, l) = (ewm_ref(&gain, 1.0 / 14.0, 14), ewm_ref(&loss, 1.0 / 14.0, 14));
        let expected: Vec<Option<f64>> = g
            .iter()
            .zip(&l)
            .map(|(g, l)| match (g, l) {
                (Some(_), Some(l)) if *l == 0.0 => Some(100.0),
                (Some(g), Some(l)) => Some(100.0 - 100.0 / (1.0 + g / l)),
                _ => None,
            })
            .collect();
        let rsi = eval(&df, &Indicator::Rsi { period: 14 });
        assert_close(&rsi, &expected);
        assert_eq!(rsi[13], None);
    }

    #[test]
    fn macd_family() {
        let x = sample();
        let df = closes(&[("A", x.clone())]);
        let (f, s) = (ewm_ref(&some(&x), 2.0 / 4.0, 3), ewm_ref(&some(&x), 2.0 / 7.0, 6));
        let line: Vec<Option<f64>> = f.iter().zip(&s).map(|(a, b)| Some((*a)? - (*b)?)).collect();
        let signal = ewm_ref(&line, 2.0 / 5.0, 4);
        let hist: Vec<Option<f64>> = line.iter().zip(&signal).map(|(a, b)| Some((*a)? - (*b)?)).collect();
        assert_close(&eval(&df, &Indicator::MacdLine { fast: 3, slow: 6, signal: 4 }), &line);
        assert_close(&eval(&df, &Indicator::MacdSignal { fast: 3, slow: 6, signal: 4 }), &signal);
        assert_close(&eval(&df, &Indicator::MacdHist { fast: 3, slow: 6, signal: 4 }), &hist);
    }

    #[test]
    fn bollinger_population_std() {
        let x = sample();
        let df = closes(&[("A", x.clone())]);
        let (mid, sd) = (sma_ref(&x, 10), std_ref(&x, 10));
        let upper: Vec<Option<f64>> = mid.iter().zip(&sd).map(|(m, s)| Some((*m)? + 2.0 * (*s)?)).collect();
        let width: Vec<Option<f64>> = mid.iter().zip(&sd).map(|(m, s)| Some(4.0 * (*s)? / (*m)?)).collect();
        assert_close(&eval(&df, &Indicator::BbMiddle { period: 10, std_x10: 20 }), &mid);
        assert_close(&eval(&df, &Indicator::BbUpper { period: 10, std_x10: 20 }), &upper);
        assert_close(&eval(&df, &Indicator::BbWidth { period: 10, std_x10: 20 }), &width);
    }

    #[test]
    fn atr_wilder() {
        let x = sample();
        let bs: Vec<Bar> = x.iter().map(|&c| [c, c + 1.5, c - 1.0, c, 1e6]).collect();
        let df = bars(&[("A", bs.clone())]);
        let tr: Vec<Option<f64>> = (0..bs.len())
            .map(|i| {
                let (h, l) = (bs[i][1], bs[i][2]);
                if i == 0 {
                    return Some(h - l);
                }
                let pc = bs[i - 1][3];
                Some((h - l).max((h - pc).abs()).max((l - pc).abs()))
            })
            .collect();
        assert_close(&eval(&df, &Indicator::Atr { period: 14 }), &ewm_ref(&tr, 1.0 / 14.0, 14));
    }

    #[test]
    fn stochastic_and_channels() {
        let x = sample();
        let bs: Vec<Bar> = x.iter().map(|&c| [c, c + 2.0, c - 1.0, c, 1e6]).collect();
        let df = bars(&[("A", bs.clone())]);
        let n = 5;
        let raw_k: Vec<Option<f64>> = (0..bs.len())
            .map(|i| {
                (i + 1 >= n).then(|| {
                    let w = &bs[i + 1 - n..=i];
                    let hh = w.iter().map(|b| b[1]).fold(f64::MIN, f64::max);
                    let ll = w.iter().map(|b| b[2]).fold(f64::MAX, f64::min);
                    100.0 * (bs[i][3] - ll) / (hh - ll)
                })
            })
            .collect();
        let k: Vec<Option<f64>> = (0..raw_k.len())
            .map(|i| {
                if i < 2 {
                    return None;
                }
                raw_k[i - 2..=i].iter().copied().sum::<Option<f64>>().map(|s| s / 3.0)
            })
            .collect();
        assert_close(&eval(&df, &Indicator::StochK { period: 5, smooth: 3 }), &k);
        let hh = eval(&df, &Indicator::HighestHigh { period: 5 });
        assert_eq!(hh[4], Some(bs[..5].iter().map(|b| b[1]).fold(f64::MIN, f64::max)));
        let ll = eval(&df, &Indicator::LowestLow { period: 5 });
        assert_eq!(ll[4], Some(bs[..5].iter().map(|b| b[2]).fold(f64::MAX, f64::min)));
    }

    #[test]
    fn change_pct_and_lowest_of() {
        let x = sample();
        let df = closes(&[("A", x.clone())]);
        let ch = eval(&df, &Indicator::ChangePct { period: 2 });
        assert_eq!(ch[1], None);
        assert!((ch[5].unwrap() - (x[5] / x[3] - 1.0) * 100.0).abs() < 1e-9);

        let inner = Indicator::Sma { period: 3 };
        let lowest = eval(&df, &Indicator::LowestOf { inner: Box::new(inner), period: 4 });
        let sma = sma_ref(&x, 3);
        let expected: Vec<Option<f64>> = (0..x.len())
            .map(|i| (i >= 5).then(|| sma[i - 3..=i].iter().map(|v| v.unwrap()).fold(f64::MAX, f64::min)))
            .collect();
        assert_close(&lowest, &expected);
    }

    #[test]
    fn windows_do_not_leak_across_tickers() {
        let df = closes(&[("A", vec![1.0, 2.0, 3.0, 4.0]), ("B", vec![10.0, 20.0, 30.0])]);
        let out = df
            .lazy()
            .with_column(expr(&Indicator::Sma { period: 2 }).unwrap().alias("sma"))
            .collect()
            .unwrap();
        assert_eq!(values_for(&out, "B", "sma"), vec![None, Some(15.0), Some(25.0)]);
        assert_eq!(values_for(&out, "A", "sma"), vec![None, Some(1.5), Some(2.5), Some(3.5)]);
    }

    fn flow_bars() -> Vec<Bar> {
        let x = sample();
        x.iter()
            .enumerate()
            .map(|(i, &c)| [c, c + 1.0 + (i % 3) as f64, c - 1.5, c, 1_000.0 + (i * 37 % 500) as f64])
            .collect()
    }

    fn mfv(b: &Bar) -> f64 {
        let range = b[1] - b[2];
        if range == 0.0 { 0.0 } else { ((b[3] - b[2]) - (b[1] - b[3])) / range * b[4] }
    }

    #[test]
    fn obv_and_acc_dist() {
        let bs = flow_bars();
        let df = bars(&[("A", bs.clone())]);
        let mut obv = 0.0;
        let expected_obv: Vec<Option<f64>> = (0..bs.len())
            .map(|i| {
                if i > 0 {
                    let d = bs[i][3] - bs[i - 1][3];
                    obv += if d > 0.0 { bs[i][4] } else if d < 0.0 { -bs[i][4] } else { 0.0 };
                }
                Some(obv)
            })
            .collect();
        assert_close(&eval(&df, &Indicator::Obv), &expected_obv);

        let mut ad = 0.0;
        let expected_ad: Vec<Option<f64>> = bs.iter().map(|b| {
            ad += mfv(b);
            Some(ad)
        }).collect();
        assert_close(&eval(&df, &Indicator::AccDist), &expected_ad);
    }

    #[test]
    fn cmf_mfi_and_relative_volume() {
        let bs = flow_bars();
        let df = bars(&[("A", bs.clone())]);
        let n = 10;
        let cmf: Vec<Option<f64>> = (0..bs.len())
            .map(|i| {
                (i + 1 >= n).then(|| {
                    let w = &bs[i + 1 - n..=i];
                    w.iter().map(mfv).sum::<f64>() / w.iter().map(|b| b[4]).sum::<f64>()
                })
            })
            .collect();
        assert_close(&eval(&df, &Indicator::Cmf { period: 10 }), &cmf);

        let tp: Vec<f64> = bs.iter().map(|b| (b[1] + b[2] + b[3]) / 3.0).collect();
        let mfi: Vec<Option<f64>> = (0..bs.len())
            .map(|i| {
                (i >= n).then(|| {
                    let (mut pos, mut neg) = (0.0, 0.0);
                    for j in i + 1 - n..=i {
                        let flow = tp[j] * bs[j][4];
                        if tp[j] > tp[j - 1] { pos += flow } else if tp[j] < tp[j - 1] { neg += flow }
                    }
                    if neg == 0.0 { 100.0 } else { 100.0 - 100.0 / (1.0 + pos / neg) }
                })
            })
            .collect();
        let got = eval(&df, &Indicator::Mfi { period: 10 });
        assert_close(&got, &mfi);
        assert!(got.iter().flatten().all(|v| (0.0..=100.0).contains(v)));

        let rvol = eval(&df, &Indicator::RelVolume { period: 5 });
        assert_eq!(rvol[4], None);
        let avg = bs[1..6].iter().map(|b| b[4]).sum::<f64>() / 5.0;
        assert!((rvol[6].unwrap() - bs[6][4] / avg).abs() < 1e-9);
    }

    #[test]
    fn recursive_indicator_is_rejected() {
        assert!(matches!(raw(&Indicator::Adx { period: 14 }), Err(CoreError::Invalid(_))));
    }
}
