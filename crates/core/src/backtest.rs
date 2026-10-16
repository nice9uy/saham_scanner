//! Uji historis rating sinyal: untuk setiap hari rating sebuah saham baru berubah menjadi level
//! tertentu, hitung perubahan harga kalau dibeli di open hari berikutnya lalu dipegang N hari
//! bursa. Pembandingnya semua hari bursa pada saham yang sama, supaya kelihatan apakah sinyal
//! benar-benar lebih baik daripada membeli di sembarang hari.

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, NaiveDate};
use polars::prelude::*;
use shared::{
    BacktestDto, BacktestExitDto, BacktestRowDto, BacktestStatsDto, BacktestYearDto, Indicator, SignalLevel, WinRateDto,
};

use crate::frame::{columns, sorted};
use crate::presets::LIQUID_MIN_VALUE;
use crate::engine::best_buy_block;
use crate::indicators::add_indicators;
use crate::risk::{TRAIL_ATR_MULT, TRAIL_DAYS};
use crate::{CoreError, MarketRegime, format_day, risk, signal};

/// Posisi ditutup pada penutupan hari bursa ke-N setelah hari sinyal.
pub const HORIZONS: [u32; 3] = [5, 10, 20];
/// Horizon untuk rincian per tahun.
pub const YEARLY_HORIZON: u32 = 10;
/// Perkiraan biaya beli (0,15%) + jual (0,25%, termasuk pajak penjualan).
pub const COST_PCT: f64 = 0.4;
/// Bar awal tiap saham yang dilewati supaya semua indikator (sampai SMA 200) sudah terisi.
const WARMUP: usize = 200;
const LIQUID_DAYS: usize = 20;
/// Cara keluar "pegang tetap" memakai horizon ini; trailing stop dibatasi `TRAIL_MAX_DAYS`.
const EXIT_HOLD: usize = 20;
const TRAIL_MAX_DAYS: usize = 60;
const EXIT_NAMES: [&str; 2] = ["Jual setelah 20 hari", "Trailing stop (high − 3 × ATR 22)"];

/// `None` = pembanding (semua hari).
type Key = Option<SignalLevel>;

fn is_sell(key: Key) -> bool {
    matches!(key, Some(SignalLevel::Sell | SignalLevel::BestSell))
}

fn year_of(days_since_epoch: i32) -> i32 {
    NaiveDate::from_num_days_from_ce_opt(days_since_epoch + 719_163).map_or(0, |d| d.year())
}

/// Rating setiap baris (urut per ticker, date), termasuk pembatalan Best Buy oleh peringatan
/// risiko dan oleh kondisi pasar pada tanggal itu.
pub fn levels(df: &DataFrame, market: &MarketRegime) -> Result<Vec<Option<SignalLevel>>, CoreError> {
    let snaps = signal::snapshots(df, false)?;
    let flags = risk::flag_codes(df)?;
    let dates: Vec<Option<i32>> = df.column("date")?.cast(&DataType::Int32)?.i32()?.iter().collect();
    debug_assert_eq!(snaps.len(), flags.len());
    Ok(snaps
        .iter()
        .zip(flags)
        .zip(dates)
        .map(|(((_, s), codes), date)| {
            if !s.close.is_finite() {
                return None;
            }
            let sig = signal::evaluate(s);
            let weak = date.and_then(|d| market.strong_at(d)) == Some(false);
            let blocked = best_buy_block(codes.iter().map(String::as_str), weak).is_some();
            Some(if sig.level == SignalLevel::BestBuy && blocked { signal::level_from_score(sig.score) } else { sig.level })
        })
        .collect())
}

pub fn stats(returns: &[f64], sell: bool) -> BacktestStatsDto {
    let n = returns.len();
    if n == 0 {
        return BacktestStatsDto::default();
    }
    let mean = |v: &[f64]| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);
    let mut sorted = returns.to_vec();
    sorted.sort_by(f64::total_cmp);
    let median = if n % 2 == 1 { sorted[n / 2] } else { (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0 };
    let hits = returns.iter().filter(|&&r| if sell { r < 0.0 } else { r > COST_PCT }).count();
    let up: Vec<f64> = returns.iter().copied().filter(|r| *r > 0.0).collect();
    let down: Vec<f64> = returns.iter().copied().filter(|r| *r < 0.0).collect();
    BacktestStatsDto {
        count: n,
        hit_rate: Some(hits as f64 / n as f64 * 100.0),
        avg: mean(returns),
        median: Some(median),
        avg_up: mean(&up),
        avg_down: mean(&down),
        worst: sorted.first().copied(),
        best: sorted.last().copied(),
        p5: Some(sorted[n * 5 / 100]),
    }
}

/// Win rate sinyal beli per saham dari seluruh riwayat di `prices` (setiap ticker harus lengkap):
/// setiap kali rating baru menjadi Buy/Best Buy, hasil `EXIT_HOLD` hari dari open besok.
pub fn win_rates(prices: &DataFrame, market: &MarketRegime) -> Result<HashMap<String, WinRateDto>, CoreError> {
    let mut out = HashMap::new();
    if prices.height() == 0 {
        return Ok(out);
    }
    let df = sorted(prices)?;
    let levels = levels(&df, market)?;
    let cols = columns(&df)?;
    let open: Vec<f64> = df.column("open")?.f64()?.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
    let buy = |l: Option<SignalLevel>| matches!(l, Some(SignalLevel::Buy | SignalLevel::BestBuy));
    for g in &cols.groups {
        let (mut signals, mut wins, mut sum) = (0u32, 0u32, 0.0);
        for t in (g.start + WARMUP).max(g.start + 1)..g.end.saturating_sub(EXIT_HOLD) {
            if !buy(levels[t]) || buy(levels[t - 1]) {
                continue;
            }
            let (entry, exit) = (open[t + 1], cols.close[t + EXIT_HOLD]);
            if !(entry > 0.0 && exit.is_finite()) {
                continue;
            }
            let r = (exit / entry - 1.0) * 100.0;
            signals += 1;
            wins += u32::from(r > COST_PCT);
            sum += r;
        }
        if signals > 0 {
            out.insert(cols.tickers[g.start].clone(), WinRateDto { signals, wins, avg_return: sum / f64::from(signals) });
        }
    }
    Ok(out)
}

/// Masuk di open hari `t + 1`, lalu keluar di penutupan pertama yang berada di bawah high tertinggi
/// sejak masuk − 3 × ATR(22), paling lama `TRAIL_MAX_DAYS` hari. Mengembalikan (hasil %, lama pegang).
fn trail_exit(high: &[f64], close: &[f64], atr: &[f64], t: usize, end: usize, entry: f64) -> (f64, usize) {
    let last = (t + TRAIL_MAX_DAYS).min(end - 1);
    let mut peak = entry;
    for j in t + 1..=last {
        peak = peak.max(high[j]);
        if close[j] < peak - TRAIL_ATR_MULT * atr[j] {
            return ((close[j] / entry - 1.0) * 100.0, j - t);
        }
    }
    ((close[last] / entry - 1.0) * 100.0, last - t)
}

/// Mengumpulkan hasil per kelompok saham supaya seluruh riwayat tidak perlu dimuat sekaligus.
#[derive(Default)]
pub struct Backtest {
    liquid_only: bool,
    market: MarketRegime,
    returns: BTreeMap<(Key, usize), Vec<f64>>,
    yearly: BTreeMap<(i32, Key), Vec<f64>>,
    /// Per cara keluar: (hasil, lama pegang).
    exits: [Vec<(f64, usize)>; 2],
    tickers: usize,
    first: Option<i32>,
    last: Option<i32>,
}

impl Backtest {
    /// `liquid_only`: hanya hari ketika rata-rata nilai transaksi 20 hari ≥ batas likuid.
    /// `market`: kondisi IHSG untuk aturan Best Buy (kosong = tanpa aturan itu).
    pub fn new(liquid_only: bool, market: MarketRegime) -> Self {
        Self { liquid_only, market, ..Default::default() }
    }

    /// Menambahkan satu kelompok saham. Seluruh riwayat satu ticker harus ada dalam satu panggilan.
    pub fn add(&mut self, prices: &DataFrame) -> Result<(), CoreError> {
        if prices.height() == 0 {
            return Ok(());
        }
        let df = sorted(prices)?;
        let levels = levels(&df, &self.market)?;
        let cols = columns(&df)?;
        let open: Vec<f64> = df.column("open")?.f64()?.iter().map(|v| v.unwrap_or(f64::NAN)).collect();
        let atr_ind = Indicator::Atr { period: TRAIL_DAYS };
        let atr: Vec<f64> = add_indicators(df.clone(), std::slice::from_ref(&atr_ind))?
            .column(&atr_ind.key())?
            .f64()?
            .iter()
            .map(|v| v.unwrap_or(f64::NAN))
            .collect();
        let yearly_k = HORIZONS.iter().position(|h| *h == YEARLY_HORIZON);

        for g in &cols.groups {
            if g.len() <= WARMUP + 1 {
                continue;
            }
            let mut used = false;
            for t in g.start + WARMUP..g.end - 1 {
                if self.liquid_only {
                    let avg = (t + 1 - LIQUID_DAYS..=t).map(|j| cols.close[j] * cols.volume[j]).sum::<f64>()
                        / LIQUID_DAYS as f64;
                    if avg.is_nan() || avg < LIQUID_MIN_VALUE {
                        continue;
                    }
                }
                let entry = open[t + 1];
                if !(entry.is_finite() && entry > 0.0) {
                    continue;
                }
                let level = levels[t];
                // Sinyal dihitung saat rating baru berubah menjadi level itu.
                let fresh = level.is_some() && level != levels[t - 1];
                if fresh && level == Some(SignalLevel::BestBuy) && t + EXIT_HOLD < g.end {
                    self.exits[0].push(((cols.close[t + EXIT_HOLD] / entry - 1.0) * 100.0, EXIT_HOLD));
                    self.exits[1].push(trail_exit(&cols.high, &cols.close, &atr, t, g.end, entry));
                }
                let year = year_of(cols.dates[t]);
                for (k, h) in HORIZONS.iter().enumerate() {
                    let exit = t + *h as usize;
                    if exit >= g.end {
                        break;
                    }
                    let close = cols.close[exit];
                    if !close.is_finite() {
                        continue;
                    }
                    let r = (close / entry - 1.0) * 100.0;
                    used = true;
                    self.first = Some(self.first.map_or(cols.dates[t], |d| d.min(cols.dates[t])));
                    self.last = Some(self.last.map_or(cols.dates[t], |d| d.max(cols.dates[t])));
                    self.returns.entry((None, k)).or_default().push(r);
                    if fresh {
                        self.returns.entry((level, k)).or_default().push(r);
                    }
                    if Some(k) == yearly_k {
                        self.yearly.entry((year, None)).or_default().push(r);
                        if fresh && matches!(level, Some(SignalLevel::BestBuy | SignalLevel::BestSell)) {
                            self.yearly.entry((year, level)).or_default().push(r);
                        }
                    }
                }
            }
            if used {
                self.tickers += 1;
            }
        }
        Ok(())
    }

    pub fn finish(self) -> BacktestDto {
        let exits = EXIT_NAMES
            .iter()
            .zip(&self.exits)
            .map(|(name, v)| {
                let rets: Vec<f64> = v.iter().map(|x| x.0).filter(|r| r.is_finite()).collect();
                BacktestExitDto {
                    name: name.to_string(),
                    stats: stats(&rets, false),
                    avg_days: (!v.is_empty()).then(|| v.iter().map(|x| x.1 as f64).sum::<f64>() / v.len() as f64),
                }
            })
            .collect();
        let keys: Vec<Key> = SignalLevel::ALL.iter().copied().map(Some).chain([None]).collect();
        let rows = keys
            .into_iter()
            .map(|key| BacktestRowDto {
                level: key,
                stats: (0..HORIZONS.len())
                    .map(|k| stats(self.returns.get(&(key, k)).map_or(&[][..], |v| v), is_sell(key)))
                    .collect(),
            })
            .collect();
        let yearly = self
            .yearly
            .iter()
            .map(|((year, key), v)| BacktestYearDto { year: *year, level: *key, stats: stats(v, is_sell(*key)) })
            .collect();
        BacktestDto {
            from: self.first.map(format_day).unwrap_or_default(),
            to: self.last.map(format_day).unwrap_or_default(),
            tickers: self.tickers,
            liquid_only: self.liquid_only,
            cost_pct: COST_PCT,
            horizons: HORIZONS.to_vec(),
            rows,
            yearly_horizon: YEARLY_HORIZON,
            yearly,
            exits,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    fn row(dto: &BacktestDto, level: Key) -> &BacktestRowDto {
        dto.rows.iter().find(|r| r.level == level).unwrap()
    }

    #[test]
    fn stats_basics() {
        let s = stats(&[2.0, -1.0, 0.3, 5.0], false);
        assert_eq!(s.count, 4);
        // Untung setelah biaya 0,4%: hanya 2,0 dan 5,0.
        assert_eq!(s.hit_rate, Some(50.0));
        assert_eq!(s.avg, Some(1.575));
        assert_eq!(s.median, Some(1.15));
        assert_eq!((s.worst, s.best), (Some(-1.0), Some(5.0)));
        assert_eq!(s.avg_down, Some(-1.0));
        // Sinyal jual benar kalau harga turun.
        assert_eq!(stats(&[2.0, -1.0, 0.3, 5.0], true).hit_rate, Some(25.0));
        assert_eq!(stats(&[], false), BacktestStatsDto::default());
    }

    #[test]
    fn baseline_and_signals_from_prices() {
        // Naik 0,5% sehari: semua pembanding untung, dan rating berubah setidaknya sekali.
        let up: Vec<f64> = (0..400).map(|i| 2000.0 * 1.005f64.powi(i)).collect();
        let mut bt = Backtest::new(false, MarketRegime::default());
        bt.add(&closes(&[("UPPP", up.clone()), ("SHRT", vec![1000.0; 50])])).unwrap();
        let dto = bt.finish();
        assert_eq!(dto.tickers, 1, "saham dengan riwayat pendek dilewati");
        assert_eq!(dto.horizons, HORIZONS);
        let base = &row(&dto, None).stats;
        // 400 bar − 200 warmup − 1 (butuh open besok) − horizon.
        assert_eq!(base[0].count, 400 - WARMUP - 1 - 5 + 1);
        assert_eq!(base[0].hit_rate, Some(100.0));
        // Masuk di open hari t+1 (= close t+1 di data sintetis), keluar di close hari t+5.
        assert!((base[0].avg.unwrap() - (1.005f64.powi(4) - 1.0) * 100.0).abs() < 1e-9);
        assert!(!dto.yearly.is_empty());
        assert!(dto.from < dto.to);
    }

    #[test]
    fn rating_changes_become_signals() {
        // Naik lalu berbalik turun: rating berubah, jadi ada sinyal baru yang tercatat.
        let mut prices: Vec<f64> = (0..300).map(|i| 2000.0 * 1.004f64.powi(i)).collect();
        let top = *prices.last().unwrap();
        prices.extend((1..150).map(|i| top * 0.995f64.powi(i)));
        let mut bt = Backtest::new(false, MarketRegime::default());
        bt.add(&closes(&[("TURN", prices)])).unwrap();
        let dto = bt.finish();
        let fresh: usize = SignalLevel::ALL.iter().map(|l| row(&dto, Some(*l)).stats[0].count).sum();
        assert!(fresh >= 1);
        assert!(fresh < row(&dto, None).stats[0].count, "sinyal jauh lebih jarang daripada pembanding");
    }

    #[test]
    fn trailing_exit_cuts_losses_and_follows_peak() {
        // Naik ke 120 lalu jatuh: keluar saat harga turun lebih dari 3 × ATR dari puncak.
        let high = [100.0, 110.0, 120.0, 118.0, 100.0, 90.0];
        let close = high;
        let atr = [5.0; 6];
        let (r, days) = trail_exit(&high, &close, &atr, 0, 6, 100.0);
        assert_eq!((r, days), (0.0, 4), "120 − 15 = 105; penutupan 100 di hari ke-4");
        // Tidak pernah tersentuh: keluar di bar terakhir yang tersedia.
        let (r, days) = trail_exit(&high[..4], &close[..4], &atr, 0, 4, 100.0);
        assert!((r - 18.0).abs() < 1e-9 && days == 3, "{r} {days}");
    }

    #[test]
    fn win_rates_count_fresh_buy_signals() {
        // Naik lalu turun lalu naik lagi: rating beli muncul lebih dari sekali.
        let mut p: Vec<f64> = (0..260).map(|i| 1000.0 * 1.004f64.powi(i)).collect();
        let top = *p.last().unwrap();
        p.extend((1..60).map(|i| top * 0.99f64.powi(i)));
        let bottom = *p.last().unwrap();
        // Rating Buy baru muncul ±50 bar setelah dasar; sisakan > 20 bar untuk mengukur hasilnya.
        p.extend((1..120).map(|i| bottom * 1.01f64.powi(i)));
        let w = win_rates(&closes(&[("WAVE", p), ("SHRT", vec![100.0; 50])]), &MarketRegime::default()).unwrap();
        assert!(!w.contains_key("SHRT"), "riwayat pendek tanpa sinyal");
        let x = w["WAVE"];
        assert_eq!((x.signals, x.wins), (1, 1), "satu sinyal Buy baru saat naik lagi, dan untung: {x:?}");
        assert!((x.rate() - f64::from(x.wins) / f64::from(x.signals) * 100.0).abs() < 1e-9);
        assert!(win_rates(&closes(&[]), &MarketRegime::default()).unwrap().is_empty());
    }

    #[test]
    fn liquid_only_skips_thin_trading() {
        let flat = wave(300, 50.0, 2.0, 0.0);
        let mut bt = Backtest::new(true, MarketRegime::default());
        bt.add(&closes(&[("TIPS", flat)])).unwrap();
        let dto = bt.finish();
        assert_eq!(dto.tickers, 0);
        assert_eq!(row(&dto, None).stats[0].count, 0);
    }
}
