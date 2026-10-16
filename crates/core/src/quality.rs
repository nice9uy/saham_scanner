//! Kesehatan fundamental dari data laba dan nilai buku: laba positif, ekuitas positif, ROE,
//! PER dibanding median sektor, dan dividen. Rasio dihitung ulang dari harga terakhir.
//!
//! Skor ini tidak bisa diuji dengan backtest: yang tersedia hanya data fundamental terkini, dan
//! memakainya untuk menilai masa lalu akan membuat hasilnya tampak lebih bagus dari kenyataan.

use std::collections::HashMap;

use shared::{QualityDto, QualityLevel};

use crate::StockMeta;

/// Data fundamental terkini per saham (rupiah per saham).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Fundamentals {
    /// Laba per saham 12 bulan terakhir.
    pub eps: Option<f64>,
    /// Nilai buku per saham.
    pub book_value: Option<f64>,
    /// Persen per tahun; `None` kalau tidak tersedia atau tidak andal.
    pub dividend_yield: Option<f64>,
    pub market_cap: Option<f64>,
}

const HIGH_ROE: f64 = 15.0;
const OK_ROE: f64 = 8.0;
const GOOD_DIVIDEND: f64 = 2.0;
/// PER lebih dari kelipatan ini dari median sektor dianggap mahal.
const EXPENSIVE_PE_X: f64 = 2.0;
/// Median sektor baru dipakai kalau sektornya punya minimal sekian saham berlaba.
const MIN_SECTOR_PEERS: usize = 5;

fn num(v: f64, decimals: usize) -> String {
    format!("{v:.decimals$}").replace('.', ",")
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

/// `closes`: harga terakhir per saham.
pub fn quality(
    funds: &HashMap<String, Fundamentals>,
    meta: &HashMap<String, StockMeta>,
    closes: &HashMap<String, f64>,
) -> HashMap<String, QualityDto> {
    let pe_of = |t: &str| -> Option<f64> {
        let eps = funds.get(t)?.eps.filter(|e| *e > 0.0)?;
        closes.get(t).map(|c| c / eps).filter(|p| p.is_finite() && *p > 0.0)
    };
    let sector_of = |t: &str| meta.get(t).and_then(|m| m.sector.as_deref());

    // Median PER per sektor (saham berlaba), dengan median pasar sebagai cadangan.
    let mut by_sector: HashMap<&str, Vec<f64>> = HashMap::new();
    let mut all = Vec::new();
    for t in funds.keys() {
        if let Some(pe) = pe_of(t) {
            all.push(pe);
            if let Some(s) = sector_of(t) {
                by_sector.entry(s).or_default().push(pe);
            }
        }
    }
    let market_pe = median(all);
    let sector_pe: HashMap<&str, f64> = by_sector
        .into_iter()
        .filter(|(_, v)| v.len() >= MIN_SECTOR_PEERS)
        .filter_map(|(s, v)| median(v).map(|m| (s, m)))
        .collect();

    funds
        .iter()
        .filter_map(|(t, f)| {
            let close = *closes.get(t)?;
            let peer_pe = sector_of(t).and_then(|s| sector_pe.get(s).copied()).or(market_pe);
            Some((t.clone(), evaluate(f, close, pe_of(t), peer_pe)))
        })
        .collect()
}

fn evaluate(f: &Fundamentals, close: f64, pe: Option<f64>, peer_pe: Option<f64>) -> QualityDto {
    let bv = f.book_value;
    let roe = match (f.eps, bv) {
        (Some(e), Some(b)) if b > 0.0 => Some(e / b * 100.0),
        _ => None,
    };
    let mut dto = QualityDto {
        level: None,
        score: 0,
        pe,
        sector_pe: peer_pe,
        pbv: bv.filter(|b| *b > 0.0).map(|b| close / b),
        roe,
        eps: f.eps,
        dividend_yield: f.dividend_yield,
        market_cap: f.market_cap,
        reasons: vec![],
    };

    let Some(eps) = f.eps else {
        dto.reasons.push("Data laba tidak tersedia".into());
        return dto;
    };
    if eps <= 0.0 {
        dto.level = Some(QualityLevel::Poor);
        dto.reasons.push("Rugi dalam 12 bulan terakhir".into());
    }
    if bv.is_some_and(|b| b <= 0.0) {
        dto.level = Some(QualityLevel::Poor);
        dto.reasons.push("Ekuitas negatif".into());
    }
    if dto.level == Some(QualityLevel::Poor) {
        return dto;
    }

    let mut score = 0;
    match roe {
        Some(r) if r >= HIGH_ROE => {
            score += 2;
            dto.reasons.push(format!("ROE {}%: tinggi", num(r, 1)));
        }
        Some(r) if r >= OK_ROE => {
            score += 1;
            dto.reasons.push(format!("ROE {}%: cukup", num(r, 1)));
        }
        Some(r) => dto.reasons.push(format!("ROE {}%: rendah", num(r, 1))),
        None => dto.reasons.push("ROE tidak bisa dihitung".into()),
    }
    if let (Some(pe), Some(peer)) = (pe, peer_pe) {
        if pe <= peer {
            score += 1;
            dto.reasons.push(format!("PER {} di bawah median sektor {}", num(pe, 1), num(peer, 1)));
        } else if pe > peer * EXPENSIVE_PE_X {
            score -= 1;
            dto.reasons.push(format!("PER {} lebih dari 2× median sektor {}: mahal", num(pe, 1), num(peer, 1)));
        } else {
            dto.reasons.push(format!("PER {} di atas median sektor {}", num(pe, 1), num(peer, 1)));
        }
    }
    if let Some(d) = f.dividend_yield.filter(|d| *d >= GOOD_DIVIDEND) {
        score += 1;
        dto.reasons.push(format!("Dividen {}% per tahun", num(d, 1)));
    }
    dto.score = score;
    dto.level = Some(match score {
        s if s >= 3 => QualityLevel::Healthy,
        s if s >= 1 => QualityLevel::Fair,
        _ => QualityLevel::Weak,
    });
    dto
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(eps: Option<f64>, bv: Option<f64>, div: Option<f64>) -> Fundamentals {
        Fundamentals { eps, book_value: bv, dividend_yield: div, market_cap: Some(1e12) }
    }

    #[test]
    fn levels_follow_rules() {
        // BBCA-ish: ROE 21%, PER 11 vs median 15, dividen 4,6% → 2 + 1 + 1 = Sehat.
        let q = evaluate(&f(Some(472.0), Some(2201.5), Some(4.6)), 5200.0, Some(5200.0 / 472.0), Some(15.0));
        assert_eq!((q.level, q.score), (Some(QualityLevel::Healthy), 4));
        assert!((q.roe.unwrap() - 21.44).abs() < 0.01);
        assert!((q.pbv.unwrap() - 5200.0 / 2201.5).abs() < 1e-9);

        // ROE 6,9%, PER mahal → 0 − 1 = Lemah.
        let q = evaluate(&f(Some(18.0), Some(260.0), None), 950.0, Some(950.0 / 18.0), Some(15.0));
        assert_eq!((q.level, q.score), (Some(QualityLevel::Weak), -1));
        assert!(q.reasons.iter().any(|r| r.contains("mahal")));

        let loss = evaluate(&f(Some(-0.02), Some(30.0), None), 25.0, None, Some(15.0));
        assert_eq!(loss.level, Some(QualityLevel::Poor));
        let neg_equity = evaluate(&f(Some(5.0), Some(-10.0), None), 100.0, Some(20.0), Some(15.0));
        assert_eq!(neg_equity.reasons, ["Ekuitas negatif"]);

        let missing = evaluate(&f(None, Some(42.0), None), 66.0, None, Some(15.0));
        assert_eq!(missing.level, None);
    }

    #[test]
    fn sector_median_with_market_fallback() {
        let mut funds = HashMap::new();
        let mut meta = HashMap::new();
        let mut closes = HashMap::new();
        // Lima bank dengan PER 5..9 (median 7), satu saham energi (sektor kecil → median pasar).
        for (i, t) in ["B1", "B2", "B3", "B4", "B5"].iter().enumerate() {
            funds.insert(t.to_string(), f(Some(100.0), Some(500.0), None));
            closes.insert(t.to_string(), 100.0 * (5 + i) as f64);
            meta.insert(t.to_string(), StockMeta { name: t.to_string(), sector: Some("Keuangan".into()), board: None });
        }
        funds.insert("E1".into(), f(Some(100.0), Some(500.0), None));
        closes.insert("E1".into(), 3000.0);
        meta.insert("E1".into(), StockMeta { name: "E1".into(), sector: Some("Energi".into()), board: None });
        funds.insert("NOPRICE".into(), f(Some(1.0), Some(1.0), None));

        let q = quality(&funds, &meta, &closes);
        assert_eq!(q["B1"].sector_pe, Some(7.0));
        assert_eq!(q["E1"].sector_pe, Some(7.5), "median pasar dari 6 saham berlaba");
        assert!(!q.contains_key("NOPRICE"), "tanpa harga tidak dinilai");
    }
}
