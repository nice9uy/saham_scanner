//! Menjalankan `ScanRequest` atas DataFrame OHLCV multi-ticker.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use polars::prelude::*;
use shared::{
    ColumnDto, Comparator, Condition, Indicator, Logic, Operand, OperandSource, RiskDto, ScanRequest, ScanResultDto,
    ScanRowDto, SignalDto, Universe,
};

use crate::indicators::{add_indicators, vector::window};
use crate::{CoreError, validate};

/// Metadata emiten untuk nama dan filter universe.
#[derive(Debug, Clone, PartialEq)]
pub struct StockMeta {
    pub name: String,
    pub sector: Option<String>,
    pub board: Option<String>,
}

/// Rata-rata `close × volume` 20 hari.
pub const AVG_VALUE_COL: &str = "avg_value_20";
const MATCHED_COL: &str = "__matched";
const SORT_COL: &str = "__sort";

/// Hari sejak 1970-01-01 → `YYYY-MM-DD`.
pub fn format_day(days_since_epoch: i32) -> String {
    NaiveDate::from_num_days_from_ce_opt(days_since_epoch + 719_163)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// Peringatan risiko dan rating sinyal per saham. Hanya bergantung pada data harga, jadi
/// backend menghitungnya sekali setiap data diperbarui dan memakainya ulang untuk setiap scan.
#[derive(Debug, Clone, Default)]
pub struct Annotations {
    pub risks: HashMap<String, RiskDto>,
    pub signals: HashMap<String, SignalDto>,
    /// Kesehatan fundamental; diisi backend dari data fundamental (kosong = tidak ada data).
    pub quality: HashMap<String, shared::QualityDto>,
    /// Win rate sinyal beli dari seluruh riwayat; diisi backend (kosong = belum dihitung).
    pub win_rates: HashMap<String, shared::WinRateDto>,
}

impl Annotations {
    /// `market`: kondisi IHSG; kosong = tanpa pembatasan dari kondisi pasar.
    pub fn compute(prices: &DataFrame, market: &crate::MarketRegime) -> Result<Self, CoreError> {
        let risks = crate::risk::risk_reports(prices)?;
        let mut signals = crate::signal::signals(prices)?;
        let latest = prices.column("date")?.cast(&DataType::Int32)?.i32()?.max();
        let weak = latest.and_then(|d| market.strong_at(d)) == Some(false);
        guard_best_buy(&risks, &mut signals, weak);
        Ok(Self { risks, signals, quality: HashMap::new(), win_rates: HashMap::new() })
    }
}

/// Kode alasan pembatalan Best Buy saat IHSG di bawah SMA 50.
const MARKET_WEAK: &str = "market_weak";

/// Alasan pembatalan Best Buy: peringatan di `BLOCKS_BEST_BUY` (pergerakan tidak wajar, riwayat
/// volatil, likuiditas mendadak, harga < Rp100, distribusi, tren turun), atau pasar sedang lemah.
pub(crate) fn best_buy_block<'a>(codes: impl IntoIterator<Item = &'a str>, market_weak: bool) -> Option<&'a str> {
    codes.into_iter().find(|c| crate::signal::BLOCKS_BEST_BUY.contains(c)).or(market_weak.then_some(MARKET_WEAK))
}

/// Best Buy yang dibatalkan kembali mengikuti skor, dan alasannya dicatat paling atas.
pub(crate) fn guard_best_buy(
    risks: &HashMap<String, RiskDto>,
    signals: &mut HashMap<String, SignalDto>,
    market_weak: bool,
) {
    for (ticker, signal) in signals.iter_mut() {
        if signal.level != shared::SignalLevel::BestBuy {
            continue;
        }
        let flags = risks.get(ticker).map(|r| r.flags.as_slice()).unwrap_or_default();
        let Some(code) = best_buy_block(flags.iter().map(|f| f.code.as_str()), market_weak) else { continue };
        let title = match flags.iter().find(|f| f.code == code) {
            Some(f) => f.title.to_lowercase(),
            None => "IHSG di bawah SMA 50 (pasar melemah)".into(),
        };
        signal.level = crate::signal::level_from_score(signal.score);
        signal.reasons.insert(0, format!("Bukan Best Buy: {title}"));
    }
}

/// `allowed` berisi daftar ticker watchlist (sudah di-resolve backend dari `universe.watchlist_id`).
/// Peringatan risiko dan rating dihitung di tempat; pakai [`run_scan_annotated`] kalau sudah ada.
pub fn run_scan(
    prices: &DataFrame,
    meta: &HashMap<String, StockMeta>,
    req: &ScanRequest,
    allowed: Option<&HashSet<String>>,
) -> Result<ScanResultDto, CoreError> {
    run_scan_annotated(prices, meta, req, allowed, None)
}

pub fn run_scan_annotated(
    prices: &DataFrame,
    meta: &HashMap<String, StockMeta>,
    req: &ScanRequest,
    allowed: Option<&HashSet<String>>,
    annotations: Option<&Annotations>,
) -> Result<ScanResultDto, CoreError> {
    validate(req)?;

    let shown = requested_indicators(req);
    let change = Indicator::ChangePct { period: 1 };
    let mut needed = shown.clone();
    if !needed.contains(&change) {
        needed.push(change.clone());
    }

    let sorted = prices.clone().lazy().sort(["ticker", "date"], SortMultipleOptions::default()).collect()?;
    let as_of = latest_date(&sorted)?;
    let df = add_indicators(sorted, &needed)?;

    let conds = req.conditions.iter().map(condition_expr).collect::<Result<Vec<_>, _>>()?;
    let matched = match req.logic {
        _ if conds.is_empty() => lit(true),
        Logic::All => all_horizontal(conds)?,
        Logic::Any => any_horizontal(conds)?,
    };
    let sort = match &req.sort_by {
        Some(s) => per_ticker(operand_expr(&s.operand, 0))?,
        None => col(AVG_VALUE_COL),
    };

    let last = df
        .lazy()
        .with_column(per_ticker((col("close") * col("volume")).rolling_mean(window(20)))?.alias(AVG_VALUE_COL))
        .with_columns([matched.alias(MATCHED_COL), sort.alias(SORT_COL)])
        .group_by_stable([col("ticker")])
        .agg([all().exclude_cols(["ticker"]).as_expr().last()])
        .filter(col("date").eq(col("date").max()))
        .filter(col(MATCHED_COL))
        .collect()?;

    let columns: Vec<ColumnDto> = shown
        .iter()
        .filter(|i| !matches!(i, Indicator::Close | Indicator::Volume))
        .map(|i| ColumnDto { key: i.key(), label: i.label() })
        .collect();

    let tickers = last.column("ticker")?.str()?;
    let close = f64_col(&last, "close")?;
    let change_pct = f64_col(&last, &change.key())?;
    let volume = f64_col(&last, "volume")?;
    let avg_value = f64_col(&last, AVG_VALUE_COL)?;
    let sort_value = f64_col(&last, SORT_COL)?;
    let extra = columns
        .iter()
        .map(|c| Ok((c.key.clone(), f64_col(&last, &c.key)?)))
        .collect::<Result<Vec<_>, CoreError>>()?;

    // Peringatan risiko dan rating hanya dihitung kalau ada saham yang cocok.
    let (computed, empty);
    let ann = match annotations {
        Some(a) => a,
        None if last.height() > 0 => {
            computed = Annotations::compute(prices, &crate::MarketRegime::default())?;
            &computed
        }
        None => {
            empty = Annotations::default();
            &empty
        }
    };
    let (risks, signals) = (&ann.risks, &ann.signals);
    let mut rows: Vec<(Option<f64>, ScanRowDto)> = Vec::new();
    for i in 0..last.height() {
        let (Some(ticker), Some(close_i)) = (tickers.get(i), close[i]) else { continue };
        let m = meta.get(ticker);
        let row = ScanRowDto {
            ticker: ticker.to_string(),
            name: m.map_or_else(|| ticker.to_string(), |m| m.name.clone()),
            close: close_i,
            change_pct: change_pct[i],
            volume: volume[i].unwrap_or(0.0),
            avg_value_20: avg_value[i],
            values: extra.iter().map(|(k, v)| (k.clone(), v[i])).collect(),
            risk: risks.get(ticker).map(|r| r.flags.clone()).unwrap_or_default(),
            signal: signals.get(ticker).cloned(),
            quality: ann.quality.get(ticker).and_then(|q| q.level),
            win_rate: ann.win_rates.get(ticker).copied(),
        };
        if passes(&req.universe, &row, m, allowed) {
            rows.push((sort_value[i], row));
        }
    }

    let descending = req.sort_by.as_ref().is_none_or(|s| s.descending);
    rows.sort_by(|(a, ra), (b, rb)| compare_sort(*a, *b, descending).then_with(|| ra.ticker.cmp(&rb.ticker)));
    let total_matched = rows.len();
    let rows = rows.into_iter().take(req.limit as usize).map(|(_, r)| r).collect();

    Ok(ScanResultDto { as_of, total_matched, columns, rows })
}

/// Indikator unik dari kondisi dan urutan, sesuai urutan kemunculan.
fn requested_indicators(req: &ScanRequest) -> Vec<Indicator> {
    let operands = req
        .conditions
        .iter()
        .flat_map(|c| [&c.left, &c.right])
        .chain(req.sort_by.iter().map(|s| &s.operand));
    let mut out: Vec<Indicator> = Vec::new();
    for ind in operands.filter_map(Operand::as_indicator) {
        if !out.contains(ind) {
            out.push(ind.clone());
        }
    }
    out
}

fn per_ticker(e: Expr) -> Result<Expr, CoreError> {
    Ok(e.over([col("ticker")])?)
}

/// `extra_shift` dipakai untuk membaca nilai bar sebelumnya saat mendeteksi cross.
/// Konstanta tidak pernah di-shift (shift pada literal menghasilkan null).
fn operand_expr(op: &Operand, extra_shift: u16) -> Expr {
    let base = match &op.source {
        OperandSource::Constant(v) => return lit(*v * op.multiplier),
        OperandSource::Indicator(ind) => col(ind.key()),
    };
    let shift = op.offset + extra_shift;
    let base = if shift > 0 { base.shift(lit(i64::from(shift))) } else { base };
    if op.multiplier == 1.0 { base } else { base * lit(op.multiplier) }
}

fn condition_expr(c: &Condition) -> Result<Expr, CoreError> {
    let l = |s| operand_expr(&c.left, s);
    let r = |s| operand_expr(&c.right, s);
    let e = match c.cmp {
        Comparator::Gt => l(0).gt(r(0)),
        Comparator::Gte => l(0).gt_eq(r(0)),
        Comparator::Lt => l(0).lt(r(0)),
        Comparator::Lte => l(0).lt_eq(r(0)),
        Comparator::CrossAbove { within } => recent(l(0).gt(r(0)).and(l(1).lt_eq(r(1))), within),
        Comparator::CrossBelow { within } => recent(l(0).lt(r(0)).and(l(1).gt_eq(r(1))), within),
    };
    Ok(per_ticker(e)?.fill_null(lit(false)))
}

/// True kalau `event` terjadi di salah satu dari `within` bar terakhir.
fn recent(event: Expr, within: u8) -> Expr {
    if within <= 1 {
        return event;
    }
    event
        .cast(DataType::Int32)
        .rolling_max(RollingOptionsFixedWindow {
            window_size: usize::from(within),
            min_periods: 1,
            ..Default::default()
        })
        .gt(lit(0))
}

fn latest_date(df: &DataFrame) -> Result<String, CoreError> {
    let days = df.column("date")?.cast(&DataType::Int32)?;
    Ok(days.i32()?.max().map(format_day).unwrap_or_default())
}

/// Nilai NaN/inf dianggap kosong.
fn f64_col(df: &DataFrame, name: &str) -> Result<Vec<Option<f64>>, CoreError> {
    let c = df.column(name)?.cast(&DataType::Float64)?;
    Ok(c.f64()?.iter().map(|v| v.filter(|x| x.is_finite())).collect())
}

/// Nilai kosong selalu di akhir, apa pun arah urutannya.
fn compare_sort(a: Option<f64>, b: Option<f64>, descending: bool) -> Ordering {
    match (a, b) {
        (Some(x), Some(y)) => {
            let o = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
            if descending { o.reverse() } else { o }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn passes(u: &Universe, row: &ScanRowDto, meta: Option<&StockMeta>, allowed: Option<&HashSet<String>>) -> bool {
    if allowed.is_some_and(|a| !a.contains(&row.ticker)) {
        return false;
    }
    if !u.signals.is_empty() && !row.signal.as_ref().is_some_and(|s| u.signals.contains(&s.level)) {
        return false;
    }
    if !u.quality.is_empty() && !row.quality.is_some_and(|q| u.quality.contains(&q)) {
        return false;
    }
    if let Some(min) = u.min_win_rate {
        let min_n = u.min_win_signals.unwrap_or(shared::WinRateDto::MIN_SIGNALS);
        if !row.win_rate.is_some_and(|w| w.signals >= min_n && w.rate() >= min) {
            return false;
        }
    }
    if u.min_price.is_some_and(|p| row.close < p) || u.max_price.is_some_and(|p| row.close > p) {
        return false;
    }
    if let Some(min) = u.min_avg_value_20d
        && !row.avg_value_20.is_some_and(|v| v >= min)
    {
        return false;
    }
    let in_list = |list: &[String], value: Option<&String>| {
        list.is_empty() || value.is_some_and(|v| list.iter().any(|x| x.eq_ignore_ascii_case(v)))
    };
    in_list(&u.sectors, meta.and_then(|m| m.sector.as_ref()))
        && in_list(&u.boards, meta.and_then(|m| m.board.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    use shared::SortSpec;

    fn ind(i: Indicator) -> Operand {
        Operand::indicator(i)
    }

    fn close() -> Operand {
        ind(Indicator::Close)
    }

    fn konst(v: f64) -> Operand {
        Operand::constant(v)
    }

    fn req(conds: Vec<Condition>) -> ScanRequest {
        ScanRequest::new(conds)
    }

    fn scan(df: &DataFrame, r: &ScanRequest) -> ScanResultDto {
        run_scan(df, &HashMap::new(), r, None).unwrap()
    }

    fn tickers(r: &ScanResultDto) -> Vec<String> {
        r.rows.iter().map(|r| r.ticker.clone()).collect()
    }

    fn rising(n: usize, start: f64) -> Vec<f64> {
        (0..n).map(|i| start + i as f64).collect()
    }

    fn falling(n: usize, start: f64) -> Vec<f64> {
        (0..n).map(|i| start - i as f64).collect()
    }

    // Close memotong SMA(3) di bar terakhir.
    const CROSS_NOW: [f64; 9] = [10.0, 10.0, 10.0, 10.0, 10.0, 9.0, 9.0, 9.0, 12.0];
    // Close memotong SMA(3) di indeks 3 (5 bar sebelum bar terakhir).
    const CROSS_OLD: [f64; 9] = [9.0, 9.0, 9.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0];

    #[test]
    fn format_day_is_iso() {
        assert_eq!(format_day(0), "1970-01-01");
        assert_eq!(format_day(BASE_DAY), "2024-10-04");
    }

    #[test]
    fn filters_by_simple_comparison() {
        let df = closes(&[("AAA", rising(10, 100.0)), ("BBB", falling(10, 100.0))]);
        let r = scan(&df, &req(vec![Condition::new(close(), Comparator::Gt, konst(100.0))]));
        assert_eq!(tickers(&r), ["AAA"]);
        assert_eq!(r.total_matched, 1);
        assert_eq!(r.as_of, format_day(BASE_DAY + 9));
        assert_eq!(r.rows[0].close, 109.0);
        assert!((r.rows[0].change_pct.unwrap() - (109.0 / 108.0 - 1.0) * 100.0).abs() < 1e-9);
        assert_eq!(r.rows[0].name, "AAA");
    }

    #[test]
    fn cross_above_respects_within() {
        let df = closes(&[("NOW", CROSS_NOW.to_vec()), ("OLD", CROSS_OLD.to_vec())]);
        let sma3 = ind(Indicator::Sma { period: 3 });
        let r1 = scan(&df, &req(vec![Condition::new(close(), Comparator::CrossAbove { within: 1 }, sma3.clone())]));
        assert_eq!(tickers(&r1), ["NOW"]);
        let r6 = scan(&df, &req(vec![Condition::new(close(), Comparator::CrossAbove { within: 6 }, sma3)]));
        assert_eq!(tickers(&r6), ["NOW", "OLD"]);
    }

    #[test]
    fn cross_against_constant_and_cross_below() {
        let df = closes(&[("NOW", CROSS_NOW.to_vec()), ("OLD", CROSS_OLD.to_vec())]);
        let up = scan(&df, &req(vec![Condition::new(close(), Comparator::CrossAbove { within: 1 }, konst(11.0))]));
        assert_eq!(tickers(&up), ["NOW"]);
        let df = closes(&[("DROP", vec![10.0, 10.0, 9.0]), ("FLAT", vec![10.0, 10.0, 10.0])]);
        let down = scan(&df, &req(vec![Condition::new(close(), Comparator::CrossBelow { within: 1 }, konst(9.5))]));
        assert_eq!(tickers(&down), ["DROP"]);
    }

    #[test]
    fn all_versus_any() {
        let df = closes(&[("UP", rising(10, 100.0)), ("DOWN", falling(10, 100.0))]);
        let conds = vec![
            Condition::new(close(), Comparator::Gt, konst(105.0)),
            Condition::new(close(), Comparator::Lt, konst(95.0)),
        ];
        let mut r = req(conds);
        assert!(scan(&df, &r).rows.is_empty());
        r.logic = Logic::Any;
        assert_eq!(tickers(&scan(&df, &r)), ["DOWN", "UP"]);
    }

    #[test]
    fn multiplier_and_offset() {
        let mut spike = vec![100.0; 9];
        spike.push(500.0);
        let df = closes_volumes(&[("SPIKE", vec![10.0; 10], spike), ("FLAT", vec![10.0; 10], vec![100.0; 10])]);
        let c = Condition::new(
            ind(Indicator::Volume),
            Comparator::Gt,
            ind(Indicator::VolumeSma { period: 3 }).times(2.0),
        );
        assert_eq!(tickers(&scan(&df, &req(vec![c]))), ["SPIKE"]);

        let df = closes(&[("UP", rising(5, 10.0)), ("DOWN", falling(5, 10.0))]);
        let c = Condition::new(close(), Comparator::Gt, close().shifted(1));
        assert_eq!(tickers(&scan(&df, &req(vec![c]))), ["UP"]);
    }

    #[test]
    fn stale_tickers_are_excluded() {
        let df = closes(&[("AAA", rising(10, 100.0)), ("OLD", rising(8, 100.0))]);
        let r = scan(&df, &req(vec![Condition::new(close(), Comparator::Gt, konst(0.0))]));
        assert_eq!(tickers(&r), ["AAA"]);
    }

    fn universe_fixture() -> (DataFrame, HashMap<String, StockMeta>) {
        let df = closes(&[("AAA", rising(25, 100.0)), ("BBB", rising(25, 200.0)), ("CCC", rising(25, 50.0))]);
        let meta = |name: &str, sector: &str, board: &str| StockMeta {
            name: name.into(),
            sector: Some(sector.into()),
            board: Some(board.into()),
        };
        let meta = HashMap::from([
            ("AAA".to_string(), meta("Bank AAA", "Finance", "Utama")),
            ("BBB".to_string(), meta("Energi BBB", "Energy", "Utama")),
            ("CCC".to_string(), meta("CCC Tbk", "Energy", "Pengembangan")),
        ]);
        (df, meta)
    }

    #[test]
    fn universe_filters() {
        let (df, meta) = universe_fixture();
        let base = req(vec![Condition::new(close(), Comparator::Gt, konst(0.0))]);
        let run = |r: &ScanRequest, allowed: Option<&HashSet<String>>| {
            let mut t = tickers(&run_scan(&df, &meta, r, allowed).unwrap());
            t.sort();
            t
        };

        let mut r = base.clone();
        r.universe.min_price = Some(100.0);
        assert_eq!(run(&r, None), ["AAA", "BBB"]);

        let mut r = base.clone();
        r.universe.max_price = Some(150.0);
        assert_eq!(run(&r, None), ["AAA", "CCC"]);

        let mut r = base.clone();
        r.universe.sectors = vec!["finance".into()];
        assert_eq!(run(&r, None), ["AAA"]);

        let mut r = base.clone();
        r.universe.boards = vec!["Pengembangan".into()];
        assert_eq!(run(&r, None), ["CCC"]);

        let mut r = base.clone();
        r.universe.min_avg_value_20d = Some(1.5e8);
        assert_eq!(run(&r, None), ["BBB"]);

        let allowed = HashSet::from(["BBB".to_string()]);
        assert_eq!(run(&base, Some(&allowed)), ["BBB"]);

        let r = run_scan(&df, &meta, &base, None).unwrap();
        assert!(r.rows.iter().any(|row| row.name == "Bank AAA"));
    }

    #[test]
    fn sorting_and_limit() {
        let (df, meta) = universe_fixture();
        let base = req(vec![Condition::new(close(), Comparator::Gt, konst(0.0))]);
        let by_default = run_scan(&df, &meta, &base, None).unwrap();
        assert_eq!(tickers(&by_default), ["BBB", "AAA", "CCC"]);

        let mut r = base.clone();
        r.sort_by = Some(SortSpec { operand: close(), descending: false });
        r.limit = 2;
        let out = run_scan(&df, &meta, &r, None).unwrap();
        assert_eq!(tickers(&out), ["CCC", "AAA"]);
        assert_eq!(out.total_matched, 3);
    }

    #[test]
    fn returns_indicator_columns() {
        let df = closes(&[("AAA", wave(40, 100.0, 5.0, 0.3))]);
        let r = scan(
            &df,
            &req(vec![
                Condition::new(ind(Indicator::Rsi { period: 14 }), Comparator::Gt, konst(0.0)),
                Condition::new(close(), Comparator::Gt, konst(0.0)),
            ]),
        );
        assert_eq!(r.columns, vec![ColumnDto { key: "rsi_14".into(), label: "RSI(14)".into() }]);
        assert!(r.rows[0].values["rsi_14"].is_some());
    }

    #[test]
    fn filters_by_signal_without_conditions() {
        let df = closes(&[
            ("UPPP", (0..260).map(|i| 1000.0 + i as f64 * 3.0).collect()),
            ("DOWN", (0..260).map(|i| 2000.0 - i as f64 * 3.0).collect()),
        ]);
        let mut r = req(vec![]);
        r.universe.signals = vec![shared::SignalLevel::Buy, shared::SignalLevel::Hold];
        let out = scan(&df, &r);
        assert_eq!(tickers(&out), ["UPPP"]);
        assert!(out.rows[0].signal.is_some());

        r.universe.signals = vec![];
        assert!(run_scan(&df, &HashMap::new(), &r, None).is_err(), "tanpa kondisi dan tanpa filter sinyal");
    }

    #[test]
    fn best_buy_is_downgraded_by_serious_risk() {
        use shared::{RiskFlagDto, RiskLevel, SignalLevel};
        let flag = |code: &str| RiskFlagDto {
            code: code.into(),
            level: RiskLevel::Medium,
            title: "Tren turun".into(),
            detail: String::new(),
        };
        let best = |score| SignalDto {
            level: SignalLevel::BestBuy,
            score,
            reasons: vec!["MACD baru memotong ke atas signal".into()],
        };
        let risks = HashMap::from([
            ("DOWN".to_string(), RiskDto { flags: vec![flag("downtrend")], atr_14: None, stop_loss: None, trailing_stop: None }),
            ("THIN".to_string(), RiskDto { flags: vec![flag("illiquid")], atr_14: None, stop_loss: None, trailing_stop: None }),
        ]);
        let mut signals =
            HashMap::from([("DOWN".to_string(), best(40)), ("THIN".to_string(), best(30)), ("OKAY".to_string(), best(30))]);
        guard_best_buy(&risks, &mut signals, false);
        assert_eq!(signals["DOWN"].level, SignalLevel::Buy);
        assert_eq!(signals["DOWN"].reasons[0], "Bukan Best Buy: tren turun");
        assert_eq!(signals["THIN"].level, SignalLevel::BestBuy, "kurang likuid tidak membatalkan");
        assert_eq!(signals["OKAY"].level, SignalLevel::BestBuy);

        // Pasar melemah: semua Best Buy dibatalkan.
        let mut signals = HashMap::from([("OKAY".to_string(), best(30))]);
        guard_best_buy(&risks, &mut signals, true);
        assert_eq!(signals["OKAY"].level, SignalLevel::Hold);
        assert_eq!(signals["OKAY"].reasons[0], "Bukan Best Buy: IHSG di bawah SMA 50 (pasar melemah)");
    }

    #[test]
    fn invalid_request_is_rejected() {
        let df = closes(&[("AAA", rising(5, 1.0))]);
        let out = run_scan(&df, &HashMap::new(), &req(vec![]), None);
        assert!(matches!(out, Err(CoreError::Invalid(_))));
    }
}
