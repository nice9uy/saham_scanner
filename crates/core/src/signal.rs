//! Rating teknikal gabungan (Best Buy … Best Sell). Dua belas indikator memberi suara
//! +1 (bullish), −1 (bearish), atau 0; rata-ratanya menjadi skor −100..100. "Best Buy" dan
//! "Best Sell" juga mensyaratkan pemicu yang baru muncul, supaya menandai momen masuk atau
//! keluar, bukan sekadar kondisi yang sudah lama berlangsung.

use std::collections::HashMap;

use polars::prelude::*;
use shared::{Indicator, SignalDto, SignalLevel};

use crate::indicators::add_indicators;
use crate::{CoreError, frame::sorted};

/// Pemicu dianggap baru kalau terjadi dalam N hari bursa terakhir.
const TRIGGER_DAYS: usize = 3;
const BUY_SCORE: i32 = 35;
const HOLD_SCORE: i32 = 10;
const SELL_SCORE: i32 = -10;
const BEST_BUY_MIN_SCORE: i32 = 25;
/// Best Buy tidak diberikan kalau harga sudah > 8% di atas SMA 20 (terlambat masuk).
const MAX_EXTENSION: f64 = 1.08;
/// Best Sell karena pemicu turun hanya selagi harga masih ≤ 8% di bawah high 20 hari.
const NEAR_TOP: f64 = 0.92;
const MAX_REASONS: usize = 5;

use Indicator::*;

const SMA20: Indicator = Sma { period: 20 };
const SMA50: Indicator = Sma { period: 50 };
const SMA200: Indicator = Sma { period: 200 };
const MACD: Indicator = MacdLine { fast: 12, slow: 26, signal: 9 };
const MACD_SIGNAL: Indicator = MacdSignal { fast: 12, slow: 26, signal: 9 };
const MACD_HIST: Indicator = MacdHist { fast: 12, slow: 26, signal: 9 };
const SUPERTREND: Indicator = Supertrend { period: 10, mult_x10: 30 };
const RSI: Indicator = Rsi { period: 14 };
const STOCH_K: Indicator = StochK { period: 14, smooth: 3 };
const STOCH_D: Indicator = StochD { period: 14, smooth: 3, d: 3 };
const MFI: Indicator = Mfi { period: 14 };
const CMF: Indicator = Cmf { period: 20 };
const ADX: Indicator = Adx { period: 14 };
const PLUS_DI: Indicator = PlusDi { period: 14 };
const MINUS_DI: Indicator = MinusDi { period: 14 };
const HIGH20: Indicator = HighestHigh { period: 20 };
const CHANGE1: Indicator = ChangePct { period: 1 };
/// Best Buy tidak diberikan di hari harga turun lebih dari 5%.
const MAX_DAY_DROP: f64 = -5.0;

fn needed() -> Vec<Indicator> {
    vec![
        SMA20, SMA50, SMA200, MACD, MACD_SIGNAL, MACD_HIST, SUPERTREND, RSI, STOCH_K, STOCH_D, MFI, CMF, Obv, ADX,
        PLUS_DI, MINUS_DI, HIGH20, CHANGE1,
    ]
}

/// Nilai bar terakhir satu saham yang dipakai untuk menilai.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Snapshot {
    pub close: f64,
    pub sma20: Option<f64>,
    pub sma50: Option<f64>,
    pub sma200: Option<f64>,
    pub macd: Option<f64>,
    pub macd_signal: Option<f64>,
    pub macd_hist: Option<f64>,
    pub macd_hist_prev: Option<f64>,
    pub supertrend: Option<f64>,
    pub rsi: Option<f64>,
    pub stoch_k: Option<f64>,
    pub stoch_d: Option<f64>,
    pub mfi: Option<f64>,
    pub cmf: Option<f64>,
    pub obv: Option<f64>,
    pub obv_prev20: Option<f64>,
    pub adx: Option<f64>,
    pub plus_di: Option<f64>,
    pub minus_di: Option<f64>,
    pub high20: Option<f64>,
    pub change_1: Option<f64>,
    /// Pemicu yang terjadi dalam `TRIGGER_DAYS` hari terakhir.
    pub macd_up: bool,
    pub macd_down: bool,
    pub st_up: bool,
    pub st_down: bool,
    pub sma50_up: bool,
    pub sma50_down: bool,
    pub rsi_exit_overbought: bool,
}

struct Vote {
    value: i32,
    reason: String,
}

fn vote(value: i32, reason: impl Into<String>) -> Option<Vote> {
    Some(Vote { value, reason: reason.into() })
}

fn above(a: Option<f64>, b: Option<f64>, up: &str, down: &str) -> Option<Vote> {
    match (a, b) {
        (Some(a), Some(b)) if a > b => vote(1, up),
        (Some(a), Some(b)) if a < b => vote(-1, down),
        _ => None,
    }
}

fn num(v: f64, decimals: usize) -> String {
    format!("{v:.decimals$}").replace('.', ",")
}

fn votes(s: &Snapshot) -> Vec<Vote> {
    let c = Some(s.close);
    let mut v: Vec<Option<Vote>> = vec![
        above(c, s.sma20, "Harga di atas SMA 20", "Harga di bawah SMA 20"),
        above(c, s.sma50, "Harga di atas SMA 50", "Harga di bawah SMA 50"),
        above(c, s.sma200, "Harga di atas SMA 200 (tren besar naik)", "Harga di bawah SMA 200 (tren besar turun)"),
        above(s.sma50, s.sma200, "SMA 50 di atas SMA 200", "SMA 50 di bawah SMA 200"),
        above(c, s.supertrend, "Supertrend naik", "Supertrend turun"),
        above(s.macd, s.macd_signal, "MACD di atas signal", "MACD di bawah signal"),
        above(s.obv, s.obv_prev20, "OBV naik dalam 20 hari (akumulasi)", "OBV turun dalam 20 hari (distribusi)"),
    ];
    v.push(match (s.adx, s.plus_di, s.minus_di) {
        (Some(adx), Some(p), Some(m)) if adx >= 20.0 && p > m => vote(1, format!("ADX {}: tren naik kuat", num(adx, 0))),
        (Some(adx), Some(p), Some(m)) if adx >= 20.0 && p < m => vote(-1, format!("ADX {}: tren turun kuat", num(adx, 0))),
        _ => None,
    });
    v.push(match s.rsi {
        Some(r) if r < 30.0 => vote(1, format!("RSI {}: jenuh jual", num(r, 0))),
        Some(r) if r > 70.0 => vote(-1, format!("RSI {}: jenuh beli", num(r, 0))),
        _ => None,
    });
    v.push(match (s.stoch_k, s.stoch_d) {
        (Some(k), Some(d)) if k < 20.0 && k > d => vote(1, "Stochastic berbalik naik dari jenuh jual"),
        (Some(k), Some(d)) if k > 80.0 && k < d => vote(-1, "Stochastic berbalik turun dari jenuh beli"),
        _ => None,
    });
    v.push(match s.mfi {
        Some(m) if m < 20.0 => vote(1, format!("MFI {}: dana jenuh jual", num(m, 0))),
        Some(m) if m > 80.0 => vote(-1, format!("MFI {}: dana jenuh beli", num(m, 0))),
        _ => None,
    });
    v.push(match s.cmf {
        Some(c) if c > 0.05 => vote(1, format!("Dana masuk (CMF {})", num(c, 2))),
        Some(c) if c < -0.05 => vote(-1, format!("Dana keluar (CMF {})", num(c, 2))),
        _ => None,
    });
    v.into_iter().flatten().collect()
}

/// Jumlah indikator yang memberi suara, termasuk yang netral (pembagi skor).
const VOTERS: i32 = 12;

pub(crate) fn evaluate(s: &Snapshot) -> SignalDto {
    let votes = votes(s);
    let score = (votes.iter().map(|v| v.value).sum::<i32>() * 100) / VOTERS;

    let mut bull_triggers = Vec::new();
    if s.macd_up {
        bull_triggers.push("MACD baru memotong ke atas signal");
    }
    if s.st_up {
        bull_triggers.push("Harga baru menembus ke atas Supertrend");
    }
    if s.sma50_up {
        bull_triggers.push("Harga baru menembus ke atas SMA 50");
    }
    let mut bear_triggers = Vec::new();
    if s.macd_down {
        bear_triggers.push("MACD baru memotong ke bawah signal");
    }
    if s.st_down {
        bear_triggers.push("Harga baru jatuh di bawah Supertrend");
    }
    if s.sma50_down {
        bear_triggers.push("Harga baru jatuh di bawah SMA 50");
    }

    let not_extended = s.sma20.is_some_and(|m| s.close <= m * MAX_EXTENSION)
        && s.rsi.is_some_and(|r| r < 70.0)
        && s.change_1.is_none_or(|c| c > MAX_DAY_DROP);
    let near_top = s.high20.is_some_and(|h| s.close >= h * NEAR_TOP);
    let hist_falling = matches!((s.macd_hist, s.macd_hist_prev), (Some(h), Some(p)) if h < p);

    let (level, triggers): (SignalLevel, Vec<&str>) =
        if !bull_triggers.is_empty() && score >= BEST_BUY_MIN_SCORE && not_extended {
            (SignalLevel::BestBuy, bull_triggers)
        } else if !bear_triggers.is_empty() && score <= SELL_SCORE && near_top {
            (SignalLevel::BestSell, bear_triggers)
        } else if s.rsi_exit_overbought && hist_falling && score < BUY_SCORE {
            (SignalLevel::BestSell, vec!["RSI baru turun dari zona jenuh beli"])
        } else if score >= BUY_SCORE {
            (SignalLevel::Buy, vec![])
        } else if score >= HOLD_SCORE {
            (SignalLevel::Hold, vec![])
        } else if score > SELL_SCORE {
            (SignalLevel::Neutral, vec![])
        } else {
            (SignalLevel::Sell, vec![])
        };

    // Alasan: pemicu dulu, lalu suara yang searah dengan rating.
    let bullish = matches!(level, SignalLevel::BestBuy | SignalLevel::Buy | SignalLevel::Hold)
        || (level == SignalLevel::Neutral && score >= 0);
    let mut reasons: Vec<String> = triggers.into_iter().map(String::from).collect();
    reasons.extend(votes.iter().filter(|v| (v.value > 0) == bullish).map(|v| v.reason.clone()));
    reasons.truncate(MAX_REASONS);
    SignalDto { level, score, reasons }
}

/// `left` memotong `right` (ke atas kalau `up`) dalam `TRIGGER_DAYS` hari terakhir.
fn crossed(left: Expr, right: Expr, up: bool) -> Expr {
    let prev = |e: Expr| e.shift(lit(1));
    let event = if up {
        left.clone().gt(right.clone()).and(prev(left).lt_eq(prev(right)))
    } else {
        left.clone().lt(right.clone()).and(prev(left).gt_eq(prev(right)))
    };
    event
        .cast(DataType::Int32)
        .rolling_max(RollingOptionsFixedWindow { window_size: TRIGGER_DAYS, min_periods: 1, ..Default::default() })
        .gt(lit(0))
}

fn f64_col(df: &DataFrame, name: &str) -> Result<Vec<Option<f64>>, CoreError> {
    let c = df.column(name)?.cast(&DataType::Float64)?;
    Ok(c.f64()?.iter().map(|v| v.filter(|x| x.is_finite())).collect())
}

fn bool_col(df: &DataFrame, name: &str) -> Result<Vec<bool>, CoreError> {
    Ok(df.column(name)?.bool()?.iter().map(|v| v.unwrap_or(false)).collect())
}

/// Rating untuk bar terakhir setiap ticker.
pub fn signals(prices: &DataFrame) -> Result<HashMap<String, SignalDto>, CoreError> {
    Ok(snapshots(prices, true)?.into_iter().filter(|(_, s)| s.close.is_finite()).map(|(t, s)| (t, evaluate(&s))).collect())
}

/// Snapshot per baris, terurut per (ticker, date). `last_only`: hanya bar terakhir tiap ticker.
pub(crate) fn snapshots(prices: &DataFrame, last_only: bool) -> Result<Vec<(String, Snapshot)>, CoreError> {
    if prices.height() == 0 {
        return Ok(vec![]);
    }
    let df = add_indicators(sorted(prices)?, &needed())?;
    let k = |i: &Indicator| col(i.key());
    let tk = || [col("ticker")];
    let triggers = [
        ("__macd_up", crossed(k(&MACD), k(&MACD_SIGNAL), true)),
        ("__macd_down", crossed(k(&MACD), k(&MACD_SIGNAL), false)),
        ("__st_up", crossed(col("close"), k(&SUPERTREND), true)),
        ("__st_down", crossed(col("close"), k(&SUPERTREND), false)),
        ("__sma50_up", crossed(col("close"), k(&SMA50), true)),
        ("__sma50_down", crossed(col("close"), k(&SMA50), false)),
        ("__rsi_exit", crossed(k(&RSI), lit(70.0), false)),
    ];
    let mut exprs = vec![
        k(&MACD_HIST).shift(lit(1)).over(tk())?.alias("__hist_prev"),
        col("obv").shift(lit(20)).over(tk())?.alias("__obv_prev20"),
    ];
    for (name, e) in triggers {
        exprs.push(e.over(tk())?.fill_null(lit(false)).alias(name));
    }
    let full = df.lazy().with_columns(exprs);
    let last = if last_only {
        full.group_by_stable(tk()).agg([all().exclude_cols(["ticker"]).as_expr().last()]).collect()?
    } else {
        full.collect()?
    };

    let tickers = last.column("ticker")?.str()?;
    let g = |i: &Indicator| f64_col(&last, &i.key());
    let close = f64_col(&last, "close")?;
    let (sma20, sma50, sma200) = (g(&SMA20)?, g(&SMA50)?, g(&SMA200)?);
    let (macd, macd_signal, macd_hist) = (g(&MACD)?, g(&MACD_SIGNAL)?, g(&MACD_HIST)?);
    let (supertrend, rsi, stoch_k, stoch_d) = (g(&SUPERTREND)?, g(&RSI)?, g(&STOCH_K)?, g(&STOCH_D)?);
    let (mfi, cmf, obv) = (g(&MFI)?, g(&CMF)?, g(&Obv)?);
    let (adx, plus_di, minus_di, high20) = (g(&ADX)?, g(&PLUS_DI)?, g(&MINUS_DI)?, g(&HIGH20)?);
    let change_1 = g(&CHANGE1)?;
    let (hist_prev, obv_prev20) = (f64_col(&last, "__hist_prev")?, f64_col(&last, "__obv_prev20")?);
    let b = |name: &str| bool_col(&last, name);
    let (macd_up, macd_down, st_up, st_down) = (b("__macd_up")?, b("__macd_down")?, b("__st_up")?, b("__st_down")?);
    let (sma50_up, sma50_down, rsi_exit) = (b("__sma50_up")?, b("__sma50_down")?, b("__rsi_exit")?);

    let mut out = Vec::with_capacity(last.height());
    for i in 0..last.height() {
        let t = tickers.get(i).unwrap_or_default();
        let Some(c) = close[i] else {
            out.push((t.to_string(), Snapshot { close: f64::NAN, ..Default::default() }));
            continue;
        };
        let snap = Snapshot {
            close: c,
            sma20: sma20[i],
            sma50: sma50[i],
            sma200: sma200[i],
            macd: macd[i],
            macd_signal: macd_signal[i],
            macd_hist: macd_hist[i],
            macd_hist_prev: hist_prev[i],
            supertrend: supertrend[i],
            rsi: rsi[i],
            stoch_k: stoch_k[i],
            stoch_d: stoch_d[i],
            mfi: mfi[i],
            cmf: cmf[i],
            obv: obv[i],
            obv_prev20: obv_prev20[i],
            adx: adx[i],
            plus_di: plus_di[i],
            minus_di: minus_di[i],
            high20: high20[i],
            change_1: change_1[i],
            macd_up: macd_up[i],
            macd_down: macd_down[i],
            st_up: st_up[i],
            st_down: st_down[i],
            sma50_up: sma50_up[i],
            sma50_down: sma50_down[i],
            rsi_exit_overbought: rsi_exit[i],
        };
        out.push((t.to_string(), snap));
    }
    Ok(out)
}

/// Rating berdasarkan skor saja (tanpa status Best Buy/Best Sell).
pub(crate) fn level_from_score(score: i32) -> SignalLevel {
    if score >= BUY_SCORE {
        SignalLevel::Buy
    } else if score >= HOLD_SCORE {
        SignalLevel::Hold
    } else if score > SELL_SCORE {
        SignalLevel::Neutral
    } else {
        SignalLevel::Sell
    }
}

/// Peringatan risiko yang membatalkan Best Buy: membeli di kondisi ini terlalu berisiko.
pub(crate) const BLOCKS_BEST_BUY: [&str; 6] =
    ["unusual", "volatile_history", "liquidity_jump", "low_price", "distribution", "downtrend"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    /// Tren naik sehat, tanpa pemicu baru.
    fn uptrend() -> Snapshot {
        Snapshot {
            close: 1050.0,
            sma20: Some(1000.0),
            sma50: Some(950.0),
            sma200: Some(900.0),
            macd: Some(10.0),
            macd_signal: Some(8.0),
            macd_hist: Some(2.0),
            macd_hist_prev: Some(1.5),
            supertrend: Some(980.0),
            rsi: Some(60.0),
            stoch_k: Some(60.0),
            stoch_d: Some(55.0),
            mfi: Some(55.0),
            cmf: Some(0.1),
            obv: Some(2e9),
            obv_prev20: Some(1e9),
            adx: Some(28.0),
            plus_di: Some(30.0),
            minus_di: Some(15.0),
            high20: Some(1060.0),
            ..Default::default()
        }
    }

    fn mirror(s: &Snapshot) -> Snapshot {
        Snapshot {
            close: 950.0,
            sma20: Some(1000.0),
            sma50: Some(1050.0),
            sma200: Some(1100.0),
            macd: Some(-10.0),
            macd_signal: Some(-8.0),
            supertrend: Some(1020.0),
            rsi: Some(40.0),
            cmf: Some(-0.1),
            obv: Some(1e9),
            obv_prev20: Some(2e9),
            plus_di: Some(15.0),
            minus_di: Some(30.0),
            high20: Some(1000.0),
            ..*s
        }
    }

    #[test]
    fn healthy_uptrend_is_buy() {
        let s = evaluate(&uptrend());
        assert_eq!(s.level, SignalLevel::Buy);
        assert!(s.score >= 50, "score {}", s.score);
        assert!(s.reasons.iter().any(|r| r.contains("SMA 200")));
        assert!(s.reasons.len() <= MAX_REASONS);
    }

    #[test]
    fn fresh_trigger_in_uptrend_is_best_buy() {
        let mut snap = uptrend();
        snap.macd_up = true;
        let s = evaluate(&snap);
        assert_eq!(s.level, SignalLevel::BestBuy);
        assert_eq!(s.reasons[0], "MACD baru memotong ke atas signal");

        // Hari ini anjlok lebih dari 5%: bukan momen beli.
        snap.change_1 = Some(-8.0);
        assert_eq!(evaluate(&snap).level, SignalLevel::Buy);
        snap.change_1 = Some(-1.0);

        // Terlambat: harga sudah jauh di atas SMA 20.
        snap.close = 1200.0;
        snap.high20 = Some(1200.0);
        assert_eq!(evaluate(&snap).level, SignalLevel::Buy);
    }

    #[test]
    fn downtrend_is_sell_and_fresh_breakdown_near_top_is_best_sell() {
        let mut snap = mirror(&uptrend());
        let s = evaluate(&snap);
        assert_eq!(s.level, SignalLevel::Sell);
        assert!(s.score <= -50);
        assert!(s.reasons.iter().all(|r| !r.contains("di atas")));

        snap.st_down = true;
        assert_eq!(evaluate(&snap).level, SignalLevel::BestSell);
        // Sudah jauh dari puncak: bukan momen jual terbaik lagi, cukup Sell.
        snap.high20 = Some(1300.0);
        assert_eq!(evaluate(&snap).level, SignalLevel::Sell);
    }

    #[test]
    fn rsi_leaving_overbought_with_fading_momentum_is_best_sell() {
        let mut snap = uptrend();
        snap.rsi = Some(68.0);
        snap.rsi_exit_overbought = true;
        // Momentum melemah: MACD sudah di bawah signal dan histogram mengecil.
        snap.macd = Some(7.0);
        snap.macd_signal = Some(8.0);
        snap.macd_hist = Some(-1.0);
        snap.macd_hist_prev = Some(0.5);
        snap.cmf = Some(0.0);
        snap.obv = Some(1e9);
        snap.adx = Some(15.0);
        snap.stoch_k = Some(50.0);
        let s = evaluate(&snap);
        assert_eq!(s.level, SignalLevel::BestSell, "score {}", s.score);
        assert_eq!(s.reasons[0], "RSI baru turun dari zona jenuh beli");
    }

    #[test]
    fn mixed_signals_are_hold_or_neutral() {
        let mut snap = uptrend();
        // Tren jangka pendek melemah (di bawah SMA 20, MACD di bawah signal), tren besar masih naik.
        snap.close = 990.0;
        snap.macd = Some(5.0);
        snap.macd_signal = Some(6.0);
        snap.cmf = Some(0.0);
        snap.adx = Some(15.0);
        let s = evaluate(&snap);
        assert_eq!(s.level, SignalLevel::Hold, "score {}", s.score);

        let empty = evaluate(&Snapshot { close: 100.0, ..Default::default() });
        assert_eq!((empty.level, empty.score), (SignalLevel::Neutral, 0));
    }

    #[test]
    fn signals_from_frame() {
        let up = closes(&[("UPPP", (0..260).map(|i| 1000.0 + i as f64 * 3.0).collect())]);
        let r = signals(&up).unwrap();
        assert!(matches!(r["UPPP"].level, SignalLevel::Buy | SignalLevel::Hold), "{:?}", r["UPPP"]);
        let down = closes(&[("DOWN", (0..260).map(|i| 2000.0 - i as f64 * 3.0).collect())]);
        assert!(matches!(signals(&down).unwrap()["DOWN"].level, SignalLevel::Sell | SignalLevel::Neutral));
        assert!(signals(&closes(&[])).unwrap().is_empty());
    }
}
