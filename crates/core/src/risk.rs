//! Peringatan risiko dari pola harga dan volume. Semua aturan di sini heuristik: tanda yang
//! patut diwaspadai, bukan bukti adanya manipulasi atau "bandar".

use std::collections::HashMap;

use polars::prelude::*;
use shared::{Indicator, RiskDto, RiskFlagDto, RiskLevel};

use crate::indicators::add_indicators;
use crate::presets::LIQUID_MIN_VALUE;
use crate::{CoreError, frame::sorted};

/// Kenaikan atau penurunan 5 hari yang dianggap tidak wajar (mirip pemicu UMA bursa).
const UNUSUAL_5D_PCT: f64 = 25.0;
const SPIKE_VOLUME_X: f64 = 5.0;
const SPIKE_MOVE_PCT: f64 = 7.0;
const LOW_PRICE: f64 = 100.0;
const DISTRIBUTION_CMF: f64 = -0.1;
const OVERBOUGHT_RSI: f64 = 80.0;
const STOP_ATR_MULT: f64 = 2.0;
/// Chandelier exit: high tertinggi `TRAIL_DAYS` hari − `TRAIL_ATR_MULT` × ATR(`TRAIL_DAYS`).
pub const TRAIL_DAYS: u16 = 22;
pub const TRAIL_ATR_MULT: f64 = 3.0;
const LAG: i64 = 10;
/// Riwayat dilihat 60 hari bursa terakhir (±3 bulan).
const HISTORY_DAYS: usize = 60;
/// Gerakan sehari setajam ini mendekati batas ARA/ARB bursa.
const EXTREME_DAY_PCT: f64 = 20.0;

fn needed() -> Vec<Indicator> {
    use Indicator::*;
    vec![
        ChangePct { period: 1 },
        ChangePct { period: 5 },
        Sma { period: 50 },
        Sma { period: 200 },
        Rsi { period: 14 },
        Cmf { period: 20 },
        Obv,
        RelVolume { period: 20 },
        Atr { period: 14 },
        HighestHigh { period: TRAIL_DAYS },
        Atr { period: TRAIL_DAYS },
    ]
}

/// Nilai-nilai bar terakhir yang dipakai aturan risiko.
#[derive(Debug, Default, Clone, Copy)]
struct Snapshot {
    close: f64,
    change_1: Option<f64>,
    change_5: Option<f64>,
    sma_50: Option<f64>,
    sma_200: Option<f64>,
    rsi: Option<f64>,
    cmf: Option<f64>,
    obv: Option<f64>,
    obv_lag: Option<f64>,
    close_lag: Option<f64>,
    rel_volume: Option<f64>,
    atr: Option<f64>,
    avg_value: Option<f64>,
    /// Kenaikan dan penurunan sehari terbesar dalam `HISTORY_DAYS` hari.
    max_up_1d: Option<f64>,
    max_down_1d: Option<f64>,
    /// Median nilai transaksi harian dalam `HISTORY_DAYS` hari.
    median_value: Option<f64>,
    high_trail: Option<f64>,
    atr_trail: Option<f64>,
}

fn flag(code: &str, level: RiskLevel, title: &str, detail: String) -> RiskFlagDto {
    RiskFlagDto { code: code.into(), level, title: title.into(), detail }
}

fn rupiah(v: f64) -> String {
    if v >= 1e9 { format!("Rp{:.1} M", v / 1e9) } else { format!("Rp{:.0} jt", v / 1e6) }
}

fn evaluate(s: &Snapshot) -> RiskDto {
    let mut flags = Vec::new();

    if let Some(c5) = s.change_5.filter(|c| c.abs() >= UNUSUAL_5D_PCT) {
        flags.push(flag(
            "unusual",
            RiskLevel::High,
            "Pergerakan tidak wajar",
            if c5 > 0.0 {
                format!("Harga +{c5:.1}% dalam 5 hari. Lonjakan setajam ini sering terjadi pada saham \"gorengan\" dan rawan berbalik turun tajam.")
            } else {
                format!("Harga {c5:.1}% dalam 5 hari. Penurunan setajam ini biasanya diikuti gejolak besar; tunggu harga stabil sebelum masuk.")
            },
        ));
    } else if let (Some(rv), Some(c1)) = (s.rel_volume, s.change_1)
        && rv >= SPIKE_VOLUME_X
        && c1.abs() >= SPIKE_MOVE_PCT
    {
        flags.push(flag(
            "unusual",
            RiskLevel::High,
            "Lonjakan volume ekstrem",
            format!("Volume {rv:.1}× rata-rata 20 hari dengan harga {c1:+.1}% dalam sehari. Waspadai aksi goreng atau berita yang belum terkonfirmasi."),
        ));
    }

    match s.avg_value {
        Some(v) if v < LIQUID_MIN_VALUE => flags.push(flag(
            "illiquid",
            RiskLevel::Medium,
            "Kurang likuid",
            format!("Nilai transaksi rata-rata {} per hari. Bisa sulit menjual di harga yang diinginkan.", rupiah(v)),
        )),
        None => flags.push(flag(
            "illiquid",
            RiskLevel::Medium,
            "Riwayat transaksi pendek",
            "Belum ada 20 hari data transaksi, jadi likuiditasnya belum bisa dinilai.".into(),
        )),
        _ => {}
    }

    let extreme = [s.max_up_1d, s.max_down_1d].into_iter().flatten().filter(|c| c.abs() >= EXTREME_DAY_PCT);
    if let Some(c) = extreme.max_by(|a, b| a.abs().total_cmp(&b.abs())) {
        flags.push(flag(
            "volatile_history",
            RiskLevel::Medium,
            "Riwayat volatilitas ekstrem",
            format!("Harga pernah {c:+.0}% dalam sehari pada 3 bulan terakhir, mendekati batas ARA/ARB. Saham seperti ini mudah digerakkan."),
        ));
    }

    if let (Some(avg), Some(median)) = (s.avg_value, s.median_value)
        && avg >= LIQUID_MIN_VALUE
        && median < LIQUID_MIN_VALUE
    {
        flags.push(flag(
            "liquidity_jump",
            RiskLevel::Medium,
            "Likuiditas tidak konsisten",
            format!(
                "Biasanya sepi (median {} per hari dalam 3 bulan), tapi belakangan tiba-tiba ramai ({} per hari). Keramaian mendadak bisa cepat hilang.",
                rupiah(median),
                rupiah(avg)
            ),
        ));
    }

    if s.close < LOW_PRICE {
        flags.push(flag(
            "low_price",
            RiskLevel::Medium,
            "Harga di bawah Rp100",
            "Saham berharga rendah mudah digerakkan; satu fraksi harga bisa bernilai 1% atau lebih.".into(),
        ));
    }

    if let (Some(cmf), Some(obv), Some(obv_lag), Some(close_lag)) = (s.cmf, s.obv, s.obv_lag, s.close_lag)
        && cmf < DISTRIBUTION_CMF
        && obv < obv_lag
        && s.close >= close_lag * 0.98
    {
        flags.push(flag(
            "distribution",
            RiskLevel::Medium,
            "Tanda distribusi",
            format!("Dana keluar (CMF {cmf:.2}) dan OBV turun 10 hari terakhir, padahal harga masih bertahan. Pola ini sering muncul saat pemegang besar melepas barang."),
        ));
    }

    if let (Some(s50), Some(s200)) = (s.sma_50, s.sma_200)
        && s.close < s200
        && s50 < s200
    {
        flags.push(flag(
            "downtrend",
            RiskLevel::Medium,
            "Tren turun",
            "Harga di bawah SMA 200 dan SMA 50 di bawah SMA 200. Membeli melawan tren besar lebih berisiko.".into(),
        ));
    }

    let trailing_stop = match (s.high_trail, s.atr_trail) {
        (Some(h), Some(a)) => Some(h - TRAIL_ATR_MULT * a),
        _ => None,
    };
    if let Some(stop) = trailing_stop.filter(|stop| s.close < *stop) {
        flags.push(flag(
            "below_trailing",
            RiskLevel::Info,
            "Di bawah trailing stop",
            format!(
                "Penutupan di bawah trailing stop {stop:.0} (high 22 hari − 3 × ATR). Kalau sedang memegang saham ini, ini batas keluar yang disarankan."
            ),
        ));
    }

    if let Some(rsi) = s.rsi.filter(|r| *r > OVERBOUGHT_RSI) {
        flags.push(flag(
            "overbought",
            RiskLevel::Info,
            "Jenuh beli",
            format!("RSI {rsi:.0}. Harga sudah naik kencang dan rawan koreksi jangka pendek."),
        ));
    }

    flags.sort_by_key(|f| f.level);
    RiskDto { flags, atr_14: s.atr, stop_loss: s.atr.map(|a| s.close - STOP_ATR_MULT * a), trailing_stop }
}

fn f64_col(df: &DataFrame, name: &str) -> Result<Vec<Option<f64>>, CoreError> {
    let c = df.column(name)?.cast(&DataType::Float64)?;
    Ok(c.f64()?.iter().map(|v| v.filter(|x| x.is_finite())).collect())
}

/// Peringatan risiko per ticker, dihitung dari bar terakhir masing-masing ticker.
pub fn risk_reports(prices: &DataFrame) -> Result<HashMap<String, RiskDto>, CoreError> {
    Ok(snapshots(prices, true)?.into_iter().filter(|(_, s)| s.close.is_finite()).map(|(t, s)| (t, evaluate(&s))).collect())
}

/// Kode peringatan untuk setiap baris, terurut per (ticker, date).
pub(crate) fn flag_codes(prices: &DataFrame) -> Result<Vec<Vec<String>>, CoreError> {
    Ok(snapshots(prices, false)?
        .into_iter()
        .map(|(_, s)| if s.close.is_finite() { evaluate(&s).flags.into_iter().map(|f| f.code).collect() } else { vec![] })
        .collect())
}

/// Snapshot per baris, terurut per (ticker, date). `last_only`: hanya bar terakhir tiap ticker.
fn snapshots(prices: &DataFrame, last_only: bool) -> Result<Vec<(String, Snapshot)>, CoreError> {
    if prices.height() == 0 {
        return Ok(vec![]);
    }
    let df = add_indicators(sorted(prices)?, &needed())?;
    let tk = || [col("ticker")];
    let history = || RollingOptionsFixedWindow { window_size: HISTORY_DAYS, min_periods: 20, ..Default::default() };
    let change_1 = col(Indicator::ChangePct { period: 1 }.key());
    let full = df
        .lazy()
        .with_columns([
            col("obv").shift(lit(LAG)).over(tk())?.alias("__obv_lag"),
            col("close").shift(lit(LAG)).over(tk())?.alias("__close_lag"),
            (col("close") * col("volume"))
                .rolling_mean(crate::indicators::vector::window(20))
                .over(tk())?
                .alias("__avg_value"),
            (col("close") * col("volume")).rolling_median(history()).over(tk())?.alias("__median_value"),
            change_1.clone().rolling_max(history()).over(tk())?.alias("__max_up_1d"),
            change_1.rolling_min(history()).over(tk())?.alias("__max_down_1d"),
        ]);
    let last = if last_only {
        full.group_by_stable(tk()).agg([all().exclude_cols(["ticker"]).as_expr().last()]).collect()?
    } else {
        full.collect()?
    };

    let tickers = last.column("ticker")?.str()?;
    let get = |ind: Indicator| f64_col(&last, &ind.key());
    let close = f64_col(&last, "close")?;
    let (change_1, change_5) = (get(Indicator::ChangePct { period: 1 })?, get(Indicator::ChangePct { period: 5 })?);
    let (sma_50, sma_200) = (get(Indicator::Sma { period: 50 })?, get(Indicator::Sma { period: 200 })?);
    let rsi = get(Indicator::Rsi { period: 14 })?;
    let cmf = get(Indicator::Cmf { period: 20 })?;
    let obv = get(Indicator::Obv)?;
    let rel_volume = get(Indicator::RelVolume { period: 20 })?;
    let atr = get(Indicator::Atr { period: 14 })?;
    let (high_trail, atr_trail) =
        (get(Indicator::HighestHigh { period: TRAIL_DAYS })?, get(Indicator::Atr { period: TRAIL_DAYS })?);
    let (obv_lag, close_lag, avg_value) =
        (f64_col(&last, "__obv_lag")?, f64_col(&last, "__close_lag")?, f64_col(&last, "__avg_value")?);
    let (max_up_1d, max_down_1d, median_value) =
        (f64_col(&last, "__max_up_1d")?, f64_col(&last, "__max_down_1d")?, f64_col(&last, "__median_value")?);

    let mut out = Vec::with_capacity(last.height());
    for i in 0..last.height() {
        let t = tickers.get(i).unwrap_or_default();
        let Some(c) = close[i] else {
            out.push((t.to_string(), Snapshot { close: f64::NAN, ..Default::default() }));
            continue;
        };
        let snap = Snapshot {
            close: c,
            change_1: change_1[i],
            change_5: change_5[i],
            sma_50: sma_50[i],
            sma_200: sma_200[i],
            rsi: rsi[i],
            cmf: cmf[i],
            obv: obv[i],
            obv_lag: obv_lag[i],
            close_lag: close_lag[i],
            rel_volume: rel_volume[i],
            atr: atr[i],
            avg_value: avg_value[i],
            max_up_1d: max_up_1d[i],
            max_down_1d: max_down_1d[i],
            median_value: median_value[i],
            high_trail: high_trail[i],
            atr_trail: atr_trail[i],
        };
        out.push((t.to_string(), snap));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    fn codes(r: &RiskDto) -> Vec<&str> {
        r.flags.iter().map(|f| f.code.as_str()).collect()
    }

    fn liquid_uptrend() -> Snapshot {
        Snapshot {
            close: 5000.0,
            change_1: Some(0.5),
            change_5: Some(2.0),
            sma_50: Some(4800.0),
            sma_200: Some(4500.0),
            rsi: Some(55.0),
            cmf: Some(0.05),
            obv: Some(1e9),
            obv_lag: Some(9e8),
            close_lag: Some(4900.0),
            rel_volume: Some(1.0),
            atr: Some(100.0),
            avg_value: Some(5e10),
            max_up_1d: Some(6.0),
            max_down_1d: Some(-5.0),
            median_value: Some(4e10),
            high_trail: Some(5100.0),
            atr_trail: Some(100.0),
        }
    }

    #[test]
    fn healthy_stock_has_no_flags_and_atr_stop() {
        let r = evaluate(&liquid_uptrend());
        assert!(r.flags.is_empty(), "{:?}", codes(&r));
        assert_eq!(r.stop_loss, Some(4800.0));
        assert_eq!(r.trailing_stop, Some(4800.0));
    }

    #[test]
    fn each_rule_fires() {
        let mut s = liquid_uptrend();
        s.change_5 = Some(32.0);
        assert_eq!(codes(&evaluate(&s)), ["unusual"]);

        let mut s = liquid_uptrend();
        s.rel_volume = Some(6.0);
        s.change_1 = Some(-9.0);
        assert_eq!(evaluate(&s).flags[0].title, "Lonjakan volume ekstrem");

        let mut s = liquid_uptrend();
        s.avg_value = Some(2e8);
        s.close = 80.0;
        s.sma_50 = Some(70.0);
        s.sma_200 = Some(60.0);
        s.close_lag = Some(78.0);
        s.high_trail = Some(82.0);
        s.atr_trail = Some(2.0);
        assert_eq!(codes(&evaluate(&s)), ["illiquid", "low_price"]);

        let mut s = liquid_uptrend();
        s.cmf = Some(-0.2);
        s.obv = Some(8e8);
        assert_eq!(codes(&evaluate(&s)), ["distribution"]);

        let mut s = liquid_uptrend();
        s.close = 4000.0;
        s.sma_50 = Some(4300.0);
        s.close_lag = Some(4100.0);
        // Tren turun hampir selalu juga berada di bawah trailing stop.
        assert_eq!(codes(&evaluate(&s)), ["downtrend", "below_trailing"]);

        let mut s = liquid_uptrend();
        s.high_trail = Some(5400.0);
        let r = evaluate(&s);
        assert_eq!(codes(&r), ["below_trailing"]);
        assert!(r.flags[0].detail.contains("5100"), "{}", r.flags[0].detail);

        let mut s = liquid_uptrend();
        s.max_down_1d = Some(-24.0);
        let r = evaluate(&s);
        assert_eq!(codes(&r), ["volatile_history"]);
        assert!(r.flags[0].detail.contains("-24%"), "{}", r.flags[0].detail);

        let mut s = liquid_uptrend();
        s.avg_value = Some(3e9);
        s.median_value = Some(4e8);
        assert_eq!(codes(&evaluate(&s)), ["liquidity_jump"]);

        let mut s = liquid_uptrend();
        s.rsi = Some(85.0);
        s.change_5 = Some(30.0);
        let r = evaluate(&s);
        assert_eq!(codes(&r), ["unusual", "overbought"], "urut dari level tertinggi");
    }

    #[test]
    fn reports_per_ticker_from_frame() {
        let mut pump = vec![100.0; 30];
        for (i, v) in pump.iter_mut().enumerate().skip(25) {
            *v = 100.0 * 1.08f64.powi(i as i32 - 24);
        }
        let df = closes(&[("PUMP", pump), ("CALM", wave(30, 5000.0, 20.0, 1.0))]);
        let reports = risk_reports(&df).unwrap();
        assert!(codes(&reports["PUMP"]).contains(&"unusual"));
        assert!(!codes(&reports["CALM"]).contains(&"unusual"));
        assert!(reports["CALM"].stop_loss.is_some());

        // Satu lonjakan +35% (ARA) 30 hari lalu masih tercatat setelah harganya tenang lagi.
        let mut ara = vec![100.0; 40];
        for v in ara.iter_mut().skip(10) {
            *v = 135.0;
        }
        let r = risk_reports(&closes(&[("ARAA", ara)])).unwrap();
        assert!(codes(&r["ARAA"]).contains(&"volatile_history"), "{:?}", codes(&r["ARAA"]));
        assert!(!codes(&r["ARAA"]).contains(&"unusual"));
        assert!(risk_reports(&closes(&[])).unwrap().is_empty());
    }
}
