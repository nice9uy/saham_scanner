//! DTO untuk REST API. Semua tanggal berformat `YYYY-MM-DD`.

use serde::{Deserialize, Serialize};

use crate::ScanRequest;

/// Body error untuk semua respons non-2xx.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    /// `bad_request` | `not_found` | `conflict` | `unavailable` | `internal`
    pub error: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockDto {
    pub ticker: String,
    pub name: String,
    pub sector: Option<String>,
    pub board: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockStatsDto {
    pub last_date: String,
    pub close: f64,
    pub change_1d: Option<f64>,
    pub change_1m: Option<f64>,
    pub change_3m: Option<f64>,
    pub change_1y: Option<f64>,
    pub high_52w: Option<f64>,
    pub low_52w: Option<f64>,
    pub avg_value_20: Option<f64>,
    /// Bar terakhir: open, high, low, volume, dan penutupan hari bursa sebelumnya.
    #[serde(default)]
    pub open: Option<f64>,
    #[serde(default)]
    pub high: Option<f64>,
    #[serde(default)]
    pub low: Option<f64>,
    #[serde(default)]
    pub prev_close: Option<f64>,
    #[serde(default)]
    pub volume: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StockDetailDto {
    pub stock: StockDto,
    pub stats: Option<StockStatsDto>,
    #[serde(default)]
    pub risk: Option<crate::RiskDto>,
    #[serde(default)]
    pub signal: Option<crate::SignalDto>,
    #[serde(default)]
    pub quality: Option<crate::QualityDto>,
    #[serde(default)]
    pub win_rate: Option<crate::WinRateDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandleDto {
    pub date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Satu garis indikator, sejajar dengan `CandlesDto::candles`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndicatorSeriesDto {
    pub key: String,
    pub label: String,
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandlesDto {
    pub candles: Vec<CandleDto>,
    pub series: Vec<IndicatorSeriesDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportResultDto {
    pub inserted: u64,
    pub updated: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveScanDto {
    pub name: String,
    pub rules: ScanRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedScanDto {
    pub id: i64,
    pub name: String,
    pub rules: ScanRequest,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateWatchlistDto {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchlistItemDto {
    pub ticker: String,
    pub name: String,
    pub close: Option<f64>,
    pub change_pct: Option<f64>,
    pub volume: Option<f64>,
    #[serde(default)]
    pub signal: Option<crate::SignalLevel>,
    /// Trailing stop (high 22 hari − 3 × ATR 22).
    #[serde(default)]
    pub trailing_stop: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchlistDto {
    pub id: i64,
    pub name: String,
    pub items: Vec<WatchlistItemDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoverDto {
    pub ticker: String,
    pub name: String,
    pub close: f64,
    pub change_pct: f64,
    /// Nilai transaksi hari terakhir (`close × volume`).
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketSummaryDto {
    pub as_of: String,
    pub advancers: usize,
    pub decliners: usize,
    pub unchanged: usize,
    pub top_gainers: Vec<MoverDto>,
    pub top_losers: Vec<MoverDto>,
    pub top_value: Vec<MoverDto>,
    /// IHSG di atas SMA 50 pada tanggal `as_of`; `None` kalau data IHSG belum ada.
    #[serde(default)]
    pub market_strong: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestRunDto {
    pub id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// `running` | `success` | `partial` | `failed`
    pub status: String,
    /// `schedule` | `manual` | `backfill`
    pub trigger: String,
    pub ok_count: i32,
    pub failed_tickers: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestProgressDto {
    pub done: usize,
    pub total: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestStatusDto {
    pub current: Option<IngestProgressDto>,
    pub history: Vec<IngestRunDto>,
    #[serde(default)]
    pub live: Option<LiveStatusDto>,
}

/// Status update intraday (harga tertunda ±15 menit dari Yahoo) selama jam bursa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveStatusDto {
    /// `closed` | `running` | `waiting` | `cooldown`
    pub state: String,
    pub done: usize,
    pub total: usize,
    /// Jam WIB (`HH:MM`) putaran terakhir selesai.
    pub last_update: Option<String>,
    /// Jam WIB (`HH:MM`) putaran berikutnya atau akhir jeda rate limit.
    pub next_at: Option<String>,
    pub note: Option<String>,
}

/// Statistik hasil setelah sinyal untuk satu horizon. Semua angka dalam persen.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BacktestStatsDto {
    pub count: usize,
    /// Sinyal beli (dan pembanding): bagian yang untung setelah biaya. Sinyal jual: bagian yang
    /// harganya turun.
    pub hit_rate: Option<f64>,
    /// Rata-rata perubahan harga (sebelum biaya).
    pub avg: Option<f64>,
    pub median: Option<f64>,
    /// Rata-rata saat naik dan saat turun.
    pub avg_up: Option<f64>,
    pub avg_down: Option<f64>,
    pub worst: Option<f64>,
    pub best: Option<f64>,
    /// Batas 5% hasil terburuk.
    #[serde(default)]
    pub p5: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestRowDto {
    /// `None` = pembanding: semua hari bursa pada saham yang sama.
    pub level: Option<crate::SignalLevel>,
    /// Satu entri per `BacktestDto::horizons`.
    pub stats: Vec<BacktestStatsDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestYearDto {
    pub year: i32,
    pub level: Option<crate::SignalLevel>,
    /// Untuk horizon `BacktestDto::yearly_horizon`.
    pub stats: BacktestStatsDto,
}

/// Hasil uji historis rating sinyal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestDto {
    pub from: String,
    pub to: String,
    pub tickers: usize,
    pub liquid_only: bool,
    /// Biaya beli + jual (persen) yang dikurangkan untuk menilai untung sinyal beli.
    pub cost_pct: f64,
    /// Lama memegang posisi, dalam hari bursa.
    pub horizons: Vec<u32>,
    pub rows: Vec<BacktestRowDto>,
    pub yearly_horizon: u32,
    pub yearly: Vec<BacktestYearDto>,
    /// Cara keluar setelah Best Buy, dibandingkan pada sinyal yang sama.
    #[serde(default)]
    pub exits: Vec<BacktestExitDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BacktestExitDto {
    pub name: String,
    pub stats: BacktestStatsDto,
    /// Rata-rata lama memegang posisi (hari bursa).
    pub avg_days: Option<f64>,
}

/// Ringkasan pasar harian dalam bentuk tulisan, disusun dari data aplikasi.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefDto {
    pub as_of: String,
    /// IHSG terakhir; `None` kalau data IHSG belum ada.
    #[serde(default)]
    pub ihsg: Option<IndexQuoteDto>,
    pub headline: String,
    pub market_strong: Option<bool>,
    pub sections: Vec<BriefSectionDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexQuoteDto {
    pub close: f64,
    /// Perubahan dalam poin dibanding penutupan hari bursa sebelumnya.
    pub change: f64,
    pub change_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefSectionDto {
    pub title: String,
    /// `up` | `down` | `warn` | `neutral`
    pub tone: String,
    pub paragraphs: Vec<String>,
    pub items: Vec<BriefItemDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BriefItemDto {
    pub ticker: String,
    pub name: String,
    pub close: f64,
    pub change_pct: Option<f64>,
    pub signal: Option<crate::SignalLevel>,
    pub note: String,
}

/// Kondisi server dan aplikasi (halaman Server). Ukuran dalam byte.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerStatusDto {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub uptime_secs: u64,
    pub app_uptime_secs: u64,
    pub app_version: String,
    pub cpu: CpuDto,
    pub memory: MemoryDto,
    pub disks: Vec<DiskDto>,
    pub database: DatabaseDto,
    pub cache: CacheInfoDto,
    pub last_ingest: Option<IngestRunDto>,
    pub live: Option<LiveStatusDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CpuDto {
    pub model: String,
    pub cores: usize,
    /// Persen seluruh core; `None` sebelum sampel pertama selesai.
    pub usage_pct: Option<f64>,
    pub per_core: Vec<f64>,
    /// Pemakaian proses aplikasi, persen dari seluruh core.
    pub app_pct: Option<f64>,
    /// Beban rata-rata 1, 5, dan 15 menit.
    pub load: [f64; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryDto {
    pub total: u64,
    pub available: u64,
    pub app_rss: u64,
    /// Batas memori container, kalau ada.
    pub limit: Option<u64>,
    pub swap_total: u64,
    pub swap_free: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskDto {
    pub mount: String,
    pub total: u64,
    pub free: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabaseDto {
    pub version: String,
    pub size: u64,
    pub connections: i64,
    pub tables: Vec<TableDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableDto {
    pub name: String,
    /// Perkiraan jumlah baris dari statistik PostgreSQL.
    pub rows: i64,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheInfoDto {
    pub stocks: usize,
    pub bars: usize,
    pub loaded_at: Option<String>,
}
