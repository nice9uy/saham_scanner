//! Ringkasan pasar harian dalam bentuk tulisan. Semua isinya disusun dari data aplikasi (harga,
//! rating sinyal, peringatan risiko, kondisi IHSG), tanpa sumber berita luar, jadi selalu
//! konsisten dengan angka di halaman lain.

use std::collections::HashMap;

use polars::prelude::*;
use shared::{BriefDto, BriefItemDto, BriefSectionDto, IndexQuoteDto, RiskLevel, SignalLevel};

use crate::frame::{columns, sorted};
use crate::presets::LIQUID_MIN_VALUE;
use crate::{Annotations, CoreError, StockMeta, format_day};

const MAX_ITEMS: usize = 10;
/// Saat pasar lemah, kandidat pantauan hanya saham yang hari ini turun kurang dari ini.
const RESILIENT_PCT: f64 = -3.0;
/// Turun sedalam ini dalam sehari dianggap mendekati batas auto reject bawah.
const NEAR_ARB_PCT: f64 = -14.0;
const BIG_UP_PCT: f64 = 20.0;
const MIN_SECTOR_STOCKS: usize = 3;

pub struct BriefInput<'a> {
    pub prices: &'a DataFrame,
    pub meta: &'a HashMap<String, StockMeta>,
    pub annotations: &'a Annotations,
    /// Penutupan IHSG terakhir dan penutupan hari bursa sebelumnya.
    pub ihsg: Option<(f64, f64)>,
    pub market_strong: Option<bool>,
    pub watchlist: &'a [String],
}

#[derive(Debug, Clone)]
struct Row {
    ticker: String,
    name: String,
    sector: Option<String>,
    close: f64,
    change: f64,
    liquid: bool,
}

/// Angka gaya Indonesia: titik pemisah ribuan, koma desimal.
fn id_num(v: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    let sign = if v < 0.0 && s.chars().any(|c| c.is_ascii_digit() && c != '0') { "-" } else { "" };
    if frac.is_empty() { format!("{sign}{grouped}") } else { format!("{sign}{grouped},{frac}") }
}

fn pct(v: f64) -> String {
    format!("{}{}%", if v > 0.0 { "+" } else { "" }, id_num(v, 2))
}

fn section(title: &str, tone: &str, paragraphs: Vec<String>, items: Vec<BriefItemDto>) -> BriefSectionDto {
    BriefSectionDto { title: title.into(), tone: tone.into(), paragraphs, items }
}

/// Saham dengan bar terakhir di tanggal terbaru, beserta perubahan harian dan status likuid.
fn rows(prices: &DataFrame, meta: &HashMap<String, StockMeta>) -> Result<(Option<i32>, Vec<Row>), CoreError> {
    let cols = columns(&sorted(prices)?)?;
    let latest = cols.dates.iter().copied().max();
    let mut out = Vec::new();
    for g in &cols.groups {
        let i = g.end - 1;
        if Some(cols.dates[i]) != latest || g.len() < 2 {
            continue;
        }
        let (close, prev) = (cols.close[i], cols.close[i - 1]);
        if !close.is_finite() || !prev.is_finite() || prev == 0.0 {
            continue;
        }
        let liquid = g.len() >= 20
            && (i + 1 - 20..=i).map(|j| cols.close[j] * cols.volume[j]).sum::<f64>() / 20.0 >= LIQUID_MIN_VALUE;
        let ticker = cols.tickers[i].clone();
        let m = meta.get(&ticker);
        out.push(Row {
            name: m.map_or_else(|| ticker.clone(), |m| m.name.clone()),
            sector: m.and_then(|m| m.sector.clone()),
            ticker,
            close,
            change: (close / prev - 1.0) * 100.0,
            liquid,
        });
    }
    Ok((latest, out))
}

pub fn brief(input: &BriefInput) -> Result<BriefDto, CoreError> {
    let (latest, rows) = rows(input.prices, input.meta)?;
    let quote = input.ihsg.filter(|(_, prev)| *prev != 0.0).map(|(close, prev)| IndexQuoteDto {
        close,
        change: close - prev,
        change_pct: (close / prev - 1.0) * 100.0,
    });
    let ihsg = quote.as_ref().map(|q| (q.close, q.change_pct));
    let signals = &input.annotations.signals;
    let risks = &input.annotations.risks;
    let signal_of = |t: &str| signals.get(t);
    let item = |r: &Row, note: String| BriefItemDto {
        ticker: r.ticker.clone(),
        name: r.name.clone(),
        close: r.close,
        change_pct: Some(r.change),
        signal: signal_of(&r.ticker).map(|s| s.level),
        note,
    };
    let first_reason = |t: &str| signal_of(t).and_then(|s| s.reasons.first().cloned()).unwrap_or_default();
    let liquid: Vec<&Row> = rows.iter().filter(|r| r.liquid).collect();
    let by_level = |level: SignalLevel| -> Vec<&Row> {
        let mut v: Vec<&Row> =
            liquid.iter().copied().filter(|r| signal_of(&r.ticker).is_some_and(|s| s.level == level)).collect();
        v.sort_by_key(|r| -signal_of(&r.ticker).map_or(0, |s| s.score.abs()));
        v
    };
    let best_buy = by_level(SignalLevel::BestBuy);
    let best_sell = by_level(SignalLevel::BestSell);
    let mut sections = Vec::new();

    // 1. Kondisi pasar.
    let (adv, dec) = (rows.iter().filter(|r| r.change > 0.0).count(), rows.iter().filter(|r| r.change < 0.0).count());
    let mut market = Vec::new();
    if let Some((close, change)) = ihsg {
        let dir = if change > 0.0 { "naik" } else if change < 0.0 { "turun" } else { "tidak berubah" };
        let size = if change.abs() >= 3.0 { " Ini pergerakan yang sangat besar untuk indeks." } else { "" };
        market.push(format!("IHSG berada di {}, {dir} {}% dibanding hari bursa sebelumnya.{size}", id_num(close, 2), id_num(change.abs(), 2)));
    }
    if !rows.is_empty() {
        let mood = if dec > adv * 2 {
            "Tekanan jual mendominasi."
        } else if adv > dec * 2 {
            "Pembeli mendominasi."
        } else {
            "Pasar bergerak campuran."
        };
        market.push(format!(
            "Dari {} saham yang bertransaksi, {adv} naik, {dec} turun, dan {} stagnan. {mood}",
            rows.len(),
            rows.len() - adv - dec
        ));
        let near_arb = rows.iter().filter(|r| r.change <= NEAR_ARB_PCT).count();
        let big_up = rows.iter().filter(|r| r.change >= BIG_UP_PCT).count();
        if near_arb > 0 {
            market.push(format!("{near_arb} saham turun 14% atau lebih, mendekati batas auto reject bawah."));
        }
        if big_up > 0 {
            market.push(format!("{big_up} saham naik 20% atau lebih dalam sehari."));
        }
    }
    match input.market_strong {
        Some(true) => market.push(
            "IHSG di atas SMA 50: tren pasar masih naik. Dalam kondisi ini sinyal beli secara historis lebih bisa diandalkan."
                .into(),
        ),
        Some(false) => market.push(
            "IHSG di bawah SMA 50: pasar sedang melemah. Secara historis, membeli saat kondisi ini hasilnya jauh lebih buruk, jadi Best Buy ditahan sampai IHSG pulih."
                .into(),
        ),
        None => {}
    }
    let market_tone = match (ihsg, input.market_strong) {
        (Some((_, c)), _) if c <= -1.0 => "down",
        (Some((_, c)), _) if c >= 1.0 => "up",
        (_, Some(false)) => "down",
        (_, Some(true)) => "up",
        _ => "neutral",
    };
    sections.push(section("Kondisi pasar", market_tone, market, vec![]));

    // 2. Sektor (saham likuid).
    let mut sectors: HashMap<&str, Vec<f64>> = HashMap::new();
    for r in &liquid {
        if let Some(s) = r.sector.as_deref() {
            sectors.entry(s).or_default().push(r.change);
        }
    }
    let mut avg: Vec<(&str, f64)> = sectors
        .into_iter()
        .filter(|(_, v)| v.len() >= MIN_SECTOR_STOCKS)
        .map(|(s, v)| (s, v.iter().sum::<f64>() / v.len() as f64))
        .collect();
    avg.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    if avg.len() >= 2 {
        let list = |v: &[(&str, f64)]| v.iter().map(|(s, c)| format!("{s} ({})", pct(*c))).collect::<Vec<_>>().join(", ");
        let n = (avg.len() / 2).min(3);
        let worst: Vec<(&str, f64)> = avg.iter().rev().take(n).copied().collect();
        let (best_label, worst_label, intro) = if avg[0].1 < 0.0 {
            ("Paling tahan", "Paling tertekan", "Semua sektor turun (rata-rata saham likuid per sektor).")
        } else if avg[avg.len() - 1].1 > 0.0 {
            ("Paling kuat", "Paling lambat", "Semua sektor naik (rata-rata saham likuid per sektor).")
        } else {
            ("Terkuat", "Terlemah", "Rata-rata perubahan saham likuid per sektor.")
        };
        sections.push(section(
            "Sektor",
            "neutral",
            vec![
                format!("{intro} {best_label}: {}.", list(&avg[..n])),
                format!("{worst_label}: {}.", list(&worst)),
            ],
            vec![],
        ));
    }

    // 3. Peluang beli.
    let mut buy_par = Vec::new();
    let buy_items: Vec<BriefItemDto> = if !best_buy.is_empty() {
        buy_par.push(format!(
            "{} saham likuid mendapat Best Buy: pemicu naik baru muncul, tren mendukung, dan IHSG kuat.",
            best_buy.len()
        ));
        buy_par.push(
            "Dalam backtest 2022–2026, Best Buy seperti ini rata-rata naik ±1,5% dalam 20 hari bursa, tapi lebih dari separuhnya tetap berakhir rugi. Keluar dengan trailing stop (high − 3 × ATR) memperbaiki hasil dan memotong rugi besar."
                .into(),
        );
        best_buy.iter().take(MAX_ITEMS).map(|r| item(r, first_reason(&r.ticker))).collect()
    } else if input.market_strong == Some(false) {
        // Pasar lemah: rating Buy masih memakai tren lama, jadi yang layak dipantau hanya saham
        // yang tetap tahan hari ini (relatif kuat terhadap pasar).
        let mut tough: Vec<&Row> = liquid
            .iter()
            .copied()
            .filter(|r| r.change > RESILIENT_PCT)
            .filter(|r| signal_of(&r.ticker).is_some_and(|s| matches!(s.level, SignalLevel::Buy | SignalLevel::Hold)))
            .collect();
        tough.sort_by(|a, b| b.change.total_cmp(&a.change));
        buy_par.push(
            "Tidak ada Best Buy karena IHSG di bawah SMA 50. Lebih aman menunggu pasar pulih sebelum membeli.".into(),
        );
        buy_par.push(if tough.is_empty() {
            "Tidak ada saham likuid bertren positif yang bertahan hari ini (semuanya turun 3% atau lebih).".into()
        } else {
            "Untuk dipantau saat pasar pulih: saham likuid bertren positif (Buy/Hold) yang paling tahan hari ini, turun kurang dari 3% atau naik.".into()
        });
        tough.iter().take(MAX_ITEMS).map(|r| item(r, first_reason(&r.ticker))).collect()
    } else {
        let mut buys = by_level(SignalLevel::Buy);
        buys.sort_by_key(|r| -signal_of(&r.ticker).map_or(0, |s| s.score));
        buy_par.push(
            "Belum ada Best Buy hari ini. Saham dengan rating Buy di bawah trennya positif, tapi belum ada pemicu naik yang baru.".into(),
        );
        buys.iter().take(MAX_ITEMS).map(|r| item(r, first_reason(&r.ticker))).collect()
    };
    sections.push(section("Peluang beli", "up", buy_par, buy_items));

    // 4. Sebaiknya dihindari atau dijual.
    let mut sells = by_level(SignalLevel::Sell);
    sells.sort_by_key(|r| signal_of(&r.ticker).map_or(0, |s| s.score));
    let avoid: Vec<&Row> = best_sell.iter().chain(sells.iter()).copied().take(MAX_ITEMS).collect();
    let mut avoid_par = vec![format!(
        "{} saham likuid mendapat Best Sell dan {} mendapat Sell.",
        best_sell.len(),
        by_level(SignalLevel::Sell).len()
    )];
    avoid_par.push(
        "Dalam backtest, saham yang baru masuk rating Sell konsisten lebih lemah daripada rata-rata di setiap tahun. Kalau memegang saham di bawah ini, pertimbangkan mengurangi posisi atau memasang stop."
            .into(),
    );
    sections.push(section(
        "Sebaiknya dihindari atau dijual",
        "down",
        avoid_par,
        avoid.iter().map(|r| item(r, first_reason(&r.ticker))).collect(),
    ));

    // 5. Waspada: peringatan risiko tinggi pada saham likuid.
    let mut warn: Vec<(&Row, String)> = liquid
        .iter()
        .filter_map(|r| {
            let f = risks.get(&r.ticker)?.flags.iter().find(|f| f.level == RiskLevel::High)?;
            Some((*r, format!("{}: {}", f.title, f.detail)))
        })
        .collect();
    warn.sort_by(|a, b| b.0.change.abs().total_cmp(&a.0.change.abs()));
    if !warn.is_empty() {
        sections.push(section(
            "Waspada",
            "warn",
            vec![format!(
                "{} saham likuid bergerak tidak wajar: naik/turun 25% atau lebih dalam 5 hari, atau volume melonjak ekstrem. Harga saham seperti ini bergejolak besar; hindari mengejar kenaikan dan jangan buru-buru menangkap penurunan.",
                warn.len()
            )],
            warn.into_iter().take(MAX_ITEMS).map(|(r, note)| item(r, note)).collect(),
        ));
    }

    // 6. Watchlist.
    if !input.watchlist.is_empty() {
        let by_ticker: HashMap<&str, &Row> = rows.iter().map(|r| (r.ticker.as_str(), r)).collect();
        let flagged: Vec<BriefItemDto> = input
            .watchlist
            .iter()
            .filter_map(|t| {
                let r = by_ticker.get(t.as_str())?;
                let below = risks.get(t).and_then(|x| x.trailing_stop).filter(|s| r.close < *s);
                let level = signal_of(t).map(|s| s.level);
                let note = match (below, level) {
                    (Some(s), _) => format!("Harga di bawah trailing stop {}. Batas keluar kalau sedang memegang.", id_num(s, 0)),
                    (None, Some(SignalLevel::BestSell | SignalLevel::Sell)) => first_reason(t),
                    (None, Some(SignalLevel::BestBuy)) => first_reason(t),
                    _ => return None,
                };
                Some(item(r, note))
            })
            .collect();
        let par = if flagged.is_empty() {
            "Tidak ada saham watchlist yang menembus trailing stop atau mendapat rating Best Buy, Sell, atau Best Sell."
                .to_string()
        } else {
            format!("{} saham watchlist perlu diperhatikan hari ini.", flagged.len())
        };
        sections.push(section("Watchlist kamu", "neutral", vec![par], flagged));
    }

    sections.push(section(
        "Catatan",
        "neutral",
        vec![
            "Ringkasan ini disusun otomatis dari harga dan volume. Tidak ada berita, laporan keuangan, pengumuman bursa (UMA, suspensi, notasi khusus), atau aliran dana asing yang diperiksa. Bukan rekomendasi jual/beli; selalu pakai stop loss dan atur besar posisi sesuai modal."
                .into(),
        ],
        vec![],
    ));

    let ihsg_txt = match ihsg {
        Some((_, c)) if c > 0.0 => format!("IHSG naik {}%", id_num(c, 2)),
        Some((_, c)) if c < 0.0 => format!("IHSG turun {}%", id_num(c.abs(), 2)),
        Some(_) => "IHSG stagnan".to_string(),
        None => "Ringkasan pasar".to_string(),
    };
    let regime = match input.market_strong {
        Some(true) => ", tren pasar naik",
        Some(false) => ", pasar melemah",
        None => "",
    };
    Ok(BriefDto {
        as_of: latest.map(format_day).unwrap_or_default(),
        ihsg: quote,
        headline: format!("{ihsg_txt}{regime}: {} Best Buy, {} Best Sell", best_buy.len(), best_sell.len()),
        market_strong: input.market_strong,
        sections,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;
    use shared::SignalDto;

    fn meta(entries: &[(&str, &str)]) -> HashMap<String, StockMeta> {
        entries
            .iter()
            .map(|(t, s)| (t.to_string(), StockMeta { name: format!("{t} Tbk"), sector: Some(s.to_string()), board: None }))
            .collect()
    }

    #[test]
    fn number_format() {
        assert_eq!(id_num(5393.924, 2), "5.393,92");
        assert_eq!(id_num(-8.0, 2), "-8,00");
        assert_eq!(id_num(-0.001, 2), "0,00");
        assert_eq!(id_num(1234567.0, 0), "1.234.567");
        assert_eq!(pct(1.5), "+1,50%");
    }

    #[test]
    fn brief_reflects_market_and_signals() {
        // Tiga saham likuid (harga 2000 × volume 1 juta = Rp2 M/hari), dua sektor.
        let flat = |end: f64| {
            let mut v = vec![2000.0; 30];
            *v.last_mut().unwrap() = end;
            v
        };
        let prices = closes(&[("AAAA", flat(2100.0)), ("BBBB", flat(1700.0)), ("CCCC", flat(1990.0)), ("DDDD", flat(1980.0))]);
        let meta = meta(&[("AAAA", "Energi"), ("BBBB", "Keuangan"), ("CCCC", "Keuangan"), ("DDDD", "Keuangan")]);
        let sig = |level, score| SignalDto { level, score, reasons: vec!["alasan".into()] };
        let annotations = Annotations {
            risks: HashMap::new(),
            signals: HashMap::from([
                ("AAAA".to_string(), sig(SignalLevel::Buy, 50)),
                ("BBBB".to_string(), sig(SignalLevel::BestSell, -40)),
                ("CCCC".to_string(), sig(SignalLevel::Sell, -20)),
            ]),
            quality: HashMap::new(),
            win_rates: HashMap::new(),
        };
        let watchlist = vec!["BBBB".to_string()];
        let dto = brief(&BriefInput {
            prices: &prices,
            meta: &meta,
            annotations: &annotations,
            ihsg: Some((5393.92, 5863.78)),
            market_strong: Some(false),
            watchlist: &watchlist,
        })
        .unwrap();

        assert_eq!(dto.headline, "IHSG turun 8,01%, pasar melemah: 0 Best Buy, 1 Best Sell");
        let q = dto.ihsg.clone().unwrap();
        assert!((q.change - (5393.92 - 5863.78)).abs() < 1e-9);
        assert!((q.change_pct - (5393.92 / 5863.78 - 1.0) * 100.0).abs() < 1e-9);
        let get = |t: &str| dto.sections.iter().find(|s| s.title == t).unwrap();
        let market = get("Kondisi pasar");
        assert_eq!(market.tone, "down");
        assert!(market.paragraphs[0].contains("5.393,92"));
        assert!(market.paragraphs[1].contains("1 naik, 3 turun"), "{:?}", market.paragraphs);
        assert!(market.paragraphs.iter().any(|p| p.contains("1 saham turun 14%")));

        let buy = get("Peluang beli");
        assert!(buy.paragraphs[0].contains("IHSG di bawah SMA 50"));
        // Hanya saham Buy/Hold yang tahan hari ini (AAAA naik 5%).
        assert_eq!(buy.items.iter().map(|i| i.ticker.as_str()).collect::<Vec<_>>(), ["AAAA"]);

        let avoid = get("Sebaiknya dihindari atau dijual");
        assert_eq!(avoid.items.iter().map(|i| i.ticker.as_str()).collect::<Vec<_>>(), ["BBBB", "CCCC"], "Best Sell dulu");

        // Sektor dengan < 3 saham tidak dihitung, jadi hanya ada satu sektor: bagian Sektor dilewati.
        assert!(dto.sections.iter().all(|s| s.title != "Sektor"));
        assert_eq!(get("Watchlist kamu").items[0].ticker, "BBBB");

        // Saham Buy yang hari ini anjlok tidak ditawarkan saat pasar lemah.
        let crashed = closes(&[("AAAA", flat(1800.0))]);
        let dto = brief(&BriefInput {
            prices: &crashed,
            meta: &meta,
            annotations: &annotations,
            ihsg: None,
            market_strong: Some(false),
            watchlist: &[],
        })
        .unwrap();
        let buy = dto.sections.iter().find(|s| s.title == "Peluang beli").unwrap();
        assert!(buy.items.is_empty());
        assert!(buy.paragraphs[1].contains("Tidak ada saham likuid"));
        assert_eq!(dto.sections.last().unwrap().title, "Catatan");
    }
}
