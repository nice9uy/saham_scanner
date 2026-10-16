use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

pub const DEFAULT_DATABASE_URL: &str = "postgres://scanner:scanner@localhost:5440/scanner";

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    /// Jam:menit WIB untuk update otomatis (Senin–Jumat).
    pub ingest_at: (u32, u32),
    pub ingest_concurrency: usize,
    pub backfill_years: u32,
    /// Jumlah bar terakhir per saham yang dimuat ke cache scan.
    pub cache_bars: usize,
    /// Update intraday saham likuid dan watchlist selama jam bursa.
    pub live_enabled: bool,
    /// Jeda antar request intraday (detik, 100 saham per request). Satu request dalam satu waktu.
    pub live_gap_secs: u64,
    /// Jarak minimum antar awal putaran intraday (menit).
    pub live_interval_min: u64,
    /// Direktori hasil `trunk build`; `None` kalau belum ada `index.html`.
    pub static_dir: Option<PathBuf>,
    pub seed_csv: PathBuf,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        // Variabel kosong (mis. `${INGEST_AT}` yang tidak di-set di compose) dianggap tidak ada.
        let var = |key: &str, default: &str| {
            env::var(key).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| default.to_string())
        };
        let num = |key: &str, default: &str| -> Result<u64> {
            var(key, default).parse().with_context(|| format!("{key} harus berupa angka"))
        };
        Ok(Self {
            database_url: var("DATABASE_URL", DEFAULT_DATABASE_URL),
            bind_addr: var("BIND_ADDR", "0.0.0.0:3000"),
            ingest_at: parse_hhmm(&var("INGEST_AT", "17:00"))?,
            ingest_concurrency: num("INGEST_CONCURRENCY", "4")?.max(1) as usize,
            backfill_years: num("BACKFILL_YEARS", "5")?.max(1) as u32,
            cache_bars: num("CACHE_BARS", "300")?.max(260) as usize,
            live_enabled: !matches!(var("LIVE_UPDATE", "1").trim(), "0" | "false" | "off"),
            live_gap_secs: num("LIVE_GAP_SECS", "3")?.max(1),
            live_interval_min: num("LIVE_INTERVAL_MIN", "2")?.max(1),
            static_dir: Some(PathBuf::from(var("STATIC_DIR", "crates/frontend/dist")))
                .filter(|dir| dir.join("index.html").exists()),
            seed_csv: PathBuf::from(var("SEED_CSV", "data/idx_stocks.csv")),
        })
    }

    /// Konfigurasi untuk test: tanpa file statis dan tanpa seed.
    pub fn for_tests(database_url: &str) -> Self {
        Self {
            database_url: database_url.to_string(),
            bind_addr: "127.0.0.1:0".into(),
            ingest_at: (17, 0),
            ingest_concurrency: 2,
            backfill_years: 1,
            cache_bars: 300,
            live_enabled: false,
            live_gap_secs: 3,
            live_interval_min: 5,
            static_dir: None,
            seed_csv: PathBuf::new(),
        }
    }
}

pub fn parse_hhmm(s: &str) -> Result<(u32, u32)> {
    let Some((h, m)) = s.trim().split_once(':') else { bail!("INGEST_AT harus berformat HH:MM, dapat {s:?}") };
    let (h, m): (u32, u32) = (
        h.parse().with_context(|| format!("jam tidak valid: {s:?}"))?,
        m.parse().with_context(|| format!("menit tidak valid: {s:?}"))?,
    );
    if h > 23 || m > 59 {
        bail!("INGEST_AT di luar rentang: {s:?}");
    }
    Ok((h, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hhmm() {
        assert_eq!(parse_hhmm("17:00").unwrap(), (17, 0));
        assert_eq!(parse_hhmm(" 08:30 ").unwrap(), (8, 30));
        assert!(parse_hhmm("25:00").is_err());
        assert!(parse_hhmm("17:60").is_err());
        assert!(parse_hhmm("abc").is_err());
    }
}
