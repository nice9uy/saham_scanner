//! Cache DataFrame harga terakhir semua saham untuk scan dan ringkasan pasar.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use polars::prelude::DataFrame;
use scanner_core::StockMeta;
use sqlx::PgPool;

use crate::db::{fundamentals, index_prices, prices, stocks, win_rates};
use crate::frame::{adjust_factor, prices_frame};

/// Bar terakhir satu saham, untuk tabel watchlist.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LastBar {
    pub close: f64,
    pub change_pct: Option<f64>,
    pub volume: f64,
}

pub struct CacheData {
    pub prices: DataFrame,
    pub meta: HashMap<String, StockMeta>,
    pub last: HashMap<String, LastBar>,
    /// Kondisi IHSG (di atas SMA 50) untuk aturan Best Buy.
    pub market: scanner_core::MarketRegime,
    /// Peringatan risiko dan rating per saham, dihitung sekali setiap cache dimuat.
    pub annotations: scanner_core::Annotations,
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Default)]
pub struct PriceCache {
    inner: Arc<RwLock<Option<Arc<CacheData>>>>,
}

impl PriceCache {
    /// Memuat ulang `bars` bar terakhir per saham. Cache dikosongkan kalau belum ada harga.
    pub async fn reload(&self, pool: &PgPool, bars: usize) -> Result<()> {
        let rows = prices::latest_bars(pool, bars).await?;
        let meta = stocks::meta_map(pool).await?;
        let market = index_prices::market_regime(pool).await?;
        let funds = fundamentals::load(pool).await?;
        let wins = win_rates::load(pool).await?;
        let data = tokio::task::spawn_blocking(move || -> Result<Option<CacheData>> {
            if rows.is_empty() {
                return Ok(None);
            }
            let mut last = HashMap::new();
            for pair in rows.windows(2) {
                if pair[0].ticker == pair[1].ticker {
                    let (prev, cur) = (&pair[0], &pair[1]);
                    let prev_adj = prev.close * adjust_factor(prev);
                    let cur_adj = cur.close * adjust_factor(cur);
                    let change = (prev_adj != 0.0).then(|| (cur_adj / prev_adj - 1.0) * 100.0);
                    last.insert(cur.ticker.clone(), LastBar { close: cur.close, change_pct: change, volume: cur.volume as f64 });
                }
            }
            // Saham dengan satu bar saja.
            for r in &rows {
                last.entry(r.ticker.clone()).or_insert(LastBar { close: r.close, change_pct: None, volume: r.volume as f64 });
            }
            let prices = prices_frame(&rows)?;
            let mut annotations = scanner_core::Annotations::compute(&prices, &market)?;
            let closes = last.iter().map(|(t, l)| (t.clone(), l.close)).collect();
            annotations.quality = scanner_core::quality::quality(&funds, &meta, &closes);
            annotations.win_rates = wins;
            Ok(Some(CacheData { prices, meta, last, market, annotations, loaded_at: chrono::Utc::now() }))
        })
        .await??;
        let count = data.as_ref().map_or(0, |d| d.last.len());
        *self.inner.write().expect("cache lock") = data.map(Arc::new);
        tracing::info!(saham = count, "cache harga dimuat");
        Ok(())
    }

    pub fn snapshot(&self) -> Option<Arc<CacheData>> {
        self.inner.read().expect("cache lock").clone()
    }
}
