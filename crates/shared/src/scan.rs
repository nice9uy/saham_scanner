use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Indicator;

pub const DEFAULT_LIMIT: u32 = 100;

fn default_limit() -> u32 {
    DEFAULT_LIMIT
}

fn default_multiplier() -> f64 {
    1.0
}

fn default_within() -> u8 {
    1
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanRequest {
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub logic: Logic,
    #[serde(default)]
    pub universe: Universe,
    #[serde(default)]
    pub sort_by: Option<SortSpec>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

impl ScanRequest {
    pub fn new(conditions: Vec<Condition>) -> Self {
        Self {
            conditions,
            logic: Logic::All,
            universe: Universe::default(),
            sort_by: None,
            limit: DEFAULT_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Logic {
    #[default]
    All,
    Any,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Universe {
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    /// Rata-rata `close × volume` 20 hari, dalam Rupiah.
    pub min_avg_value_20d: Option<f64>,
    /// Kosong = semua sektor.
    pub sectors: Vec<String>,
    /// Kosong = semua papan.
    pub boards: Vec<String>,
    pub watchlist_id: Option<i64>,
    /// Hanya saham dengan rating ini. Kosong = semua rating.
    pub signals: Vec<SignalLevel>,
    /// Hanya saham dengan kesehatan fundamental ini. Kosong = semua.
    pub quality: Vec<QualityLevel>,
    /// Win rate sinyal beli minimal (persen). Hanya saham dengan minimal `min_win_signals` sinyal.
    pub min_win_rate: Option<f64>,
    /// Default [`WinRateDto::MIN_SIGNALS`].
    pub min_win_signals: Option<u32>,
}

/// Riwayat sinyal beli satu saham: setiap kali rating baru menjadi Buy/Best Buy, apakah posisi
/// untung setelah 20 hari bursa (masuk di open besok, setelah biaya). Uji 2022–2024 → 2025–2026
/// menunjukkan angka ini **tidak** memprediksi hasil berikutnya.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WinRateDto {
    pub signals: u32,
    pub wins: u32,
    /// Rata-rata hasil per sinyal, persen (sebelum biaya).
    pub avg_return: f64,
}

impl WinRateDto {
    /// Jumlah sinyal minimal supaya win rate ditampilkan sebagai angka yang layak dibaca.
    pub const MIN_SIGNALS: u32 = 5;

    pub fn rate(&self) -> f64 {
        if self.signals == 0 { 0.0 } else { f64::from(self.wins) / f64::from(self.signals) * 100.0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SortSpec {
    pub operand: Operand,
    #[serde(default = "default_true")]
    pub descending: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub left: Operand,
    pub cmp: Comparator,
    pub right: Operand,
}

impl Condition {
    pub fn new(left: Operand, cmp: Comparator, right: Operand) -> Self {
        Self { left, cmp, right }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operand {
    pub source: OperandSource,
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,
    /// Ambil nilai N bar ke belakang (0 = bar terakhir). Diabaikan untuk konstanta.
    #[serde(default)]
    pub offset: u16,
}

impl Operand {
    pub fn indicator(ind: Indicator) -> Self {
        Self { source: OperandSource::Indicator(ind), multiplier: 1.0, offset: 0 }
    }

    pub fn constant(value: f64) -> Self {
        Self { source: OperandSource::Constant(value), multiplier: 1.0, offset: 0 }
    }

    pub fn times(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    pub fn shifted(mut self, bars: u16) -> Self {
        self.offset = bars;
        self
    }

    pub fn as_indicator(&self) -> Option<&Indicator> {
        match &self.source {
            OperandSource::Indicator(ind) => Some(ind),
            OperandSource::Constant(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperandSource {
    Indicator(Indicator),
    Constant(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Comparator {
    Gt,
    Gte,
    Lt,
    Lte,
    /// `left` memotong ke atas `right` dalam `within` bar terakhir.
    CrossAbove {
        #[serde(default = "default_within")]
        within: u8,
    },
    CrossBelow {
        #[serde(default = "default_within")]
        within: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnDto {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanRowDto {
    pub ticker: String,
    pub name: String,
    pub close: f64,
    pub change_pct: Option<f64>,
    pub volume: f64,
    pub avg_value_20: Option<f64>,
    /// Nilai setiap indikator di `ScanResultDto::columns`, dengan `ColumnDto::key` sebagai kunci.
    pub values: BTreeMap<String, Option<f64>>,
    /// Peringatan risiko, paling berat lebih dulu.
    #[serde(default)]
    pub risk: Vec<RiskFlagDto>,
    #[serde(default)]
    pub signal: Option<SignalDto>,
    #[serde(default)]
    pub quality: Option<QualityLevel>,
    #[serde(default)]
    pub win_rate: Option<WinRateDto>,
}

/// Kesehatan fundamental dari laba, ROE, valuasi, dan dividen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityLevel {
    Healthy,
    Fair,
    Weak,
    /// Rugi atau ekuitas negatif.
    Poor,
}

impl QualityLevel {
    pub const ALL: [QualityLevel; 4] = [QualityLevel::Healthy, QualityLevel::Fair, QualityLevel::Weak, QualityLevel::Poor];

    pub fn label(self) -> &'static str {
        match self {
            QualityLevel::Healthy => "Sehat",
            QualityLevel::Fair => "Cukup",
            QualityLevel::Weak => "Lemah",
            QualityLevel::Poor => "Buruk",
        }
    }
}

/// Ringkasan fundamental satu saham. Rasio dihitung dari harga terakhir.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityDto {
    /// `None` = data laba tidak tersedia.
    pub level: Option<QualityLevel>,
    pub score: i32,
    pub pe: Option<f64>,
    /// Median PER saham berlaba di sektor yang sama (atau seluruh pasar kalau sektornya kecil).
    pub sector_pe: Option<f64>,
    pub pbv: Option<f64>,
    /// Persen.
    pub roe: Option<f64>,
    pub eps: Option<f64>,
    /// Persen per tahun; `None` kalau tidak tersedia atau tidak andal.
    pub dividend_yield: Option<f64>,
    pub market_cap: Option<f64>,
    pub reasons: Vec<String>,
}

/// Rating teknikal gabungan, dari paling bullish ke paling bearish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalLevel {
    /// Momen beli awal: pemicu naik baru muncul, tren mendukung, belum terlalu tinggi.
    BestBuy,
    Buy,
    /// Cenderung positif; tahan kalau sudah punya.
    Hold,
    Neutral,
    Sell,
    /// Momen jual: pemicu turun baru muncul selagi harga dekat puncak.
    BestSell,
}

impl SignalLevel {
    pub const ALL: [SignalLevel; 6] = [
        SignalLevel::BestBuy,
        SignalLevel::Buy,
        SignalLevel::Hold,
        SignalLevel::Neutral,
        SignalLevel::Sell,
        SignalLevel::BestSell,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SignalLevel::BestBuy => "Best Buy",
            SignalLevel::Buy => "Buy",
            SignalLevel::Hold => "Hold",
            SignalLevel::Neutral => "Netral",
            SignalLevel::Sell => "Sell",
            SignalLevel::BestSell => "Best Sell",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalDto {
    pub level: SignalLevel,
    /// −100 (semua indikator bearish) sampai +100 (semua bullish).
    pub score: i32,
    /// Alasan utama, paling penting lebih dulu.
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Risiko tinggi, mis. pergerakan tidak wajar.
    High,
    Medium,
    Info,
}

/// Satu peringatan risiko hasil heuristik harga dan volume (bukan bukti manipulasi).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskFlagDto {
    /// `unusual` | `volatile_history` | `liquidity_jump` | `illiquid` | `low_price` | `distribution` | `downtrend` | `overbought`
    pub code: String,
    pub level: RiskLevel,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskDto {
    pub flags: Vec<RiskFlagDto>,
    pub atr_14: Option<f64>,
    /// Saran stop loss: harga penutupan − 2 × ATR(14).
    pub stop_loss: Option<f64>,
    /// Trailing stop (chandelier): high tertinggi 22 hari − 3 × ATR(22). Batas keluar untuk posisi
    /// yang sudah berjalan; naik mengikuti harga.
    #[serde(default)]
    pub trailing_stop: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanResultDto {
    /// Tanggal bar terakhir, format `YYYY-MM-DD`.
    pub as_of: String,
    pub total_matched: usize,
    pub columns: Vec<ColumnDto>,
    pub rows: Vec<ScanRowDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PresetDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub request: ScanRequest,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Indicator;

    #[test]
    fn minimal_json_uses_defaults() {
        let req: ScanRequest = serde_json::from_str(
            r#"{"conditions":[{"left":{"source":{"indicator":{"kind":"rsi","period":14}}},
                               "cmp":{"op":"cross_above"},
                               "right":{"source":{"constant":30.0}}}]}"#,
        )
        .unwrap();
        assert_eq!(req.logic, Logic::All);
        assert_eq!(req.limit, DEFAULT_LIMIT);
        assert_eq!(req.universe, Universe::default());
        let c = &req.conditions[0];
        assert_eq!(c.cmp, Comparator::CrossAbove { within: 1 });
        assert_eq!(c.left.multiplier, 1.0);
        assert_eq!(c.left.offset, 0);
        assert_eq!(c.left.as_indicator(), Some(&Indicator::Rsi { period: 14 }));
        assert_eq!(c.right.as_indicator(), None);
    }

    #[test]
    fn builders_round_trip() {
        let req = ScanRequest::new(vec![Condition::new(
            Operand::indicator(Indicator::Volume),
            Comparator::Gt,
            Operand::indicator(Indicator::VolumeSma { period: 20 }).times(2.0).shifted(1),
        )]);
        let json = serde_json::to_string(&req).unwrap();
        let back: ScanRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
        assert_eq!(back.conditions[0].right.multiplier, 2.0);
        assert_eq!(back.conditions[0].right.offset, 1);
    }
}
