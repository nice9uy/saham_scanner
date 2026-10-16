//! Update intraday: selama jam bursa, candle hari ini untuk semua saham diambil lewat
//! `/v7/finance/quote`, 100 saham per request, jadi satu putaran hanya butuh ±9 request. Supaya
//! tidak kena rate limit Yahoo, request dikirim satu per satu dengan jeda tetap, dan setiap balasan
//! 429 menghentikan putaran lalu menjeda update dengan waktu tunggu yang makin panjang.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveTime, Utc, Weekday};
use shared::LiveStatusDto;

use super::FetchError;
use crate::AppState;
use crate::db::{index_prices, prices};

/// Jam perdagangan BEI (WIB), termasuk pra-penutupan.
const OPEN: (u32, u32) = (9, 0);
const CLOSE: (u32, u32) = (16, 15);
/// Jeda pertama setelah 429; berlipat dua setiap kali terulang, maksimal `MAX_COOLDOWN_MIN`.
const COOLDOWN_MIN: i64 = 15;
const MAX_COOLDOWN_MIN: i64 = 120;
/// Jumlah simbol per request `/v7/finance/quote`.
pub const BATCH: usize = 100;
/// Yahoo menunda harga BEI ±15 menit, jadi candle hari ini baru muncul beberapa saat setelah
/// bursa buka. Bursa baru dianggap libur kalau sampai jam ini datanya belum juga ada.
const HOLIDAY_AFTER: (u32, u32) = (10, 30);
const IDLE: Duration = Duration::from_secs(60);

fn wib() -> FixedOffset {
    FixedOffset::east_opt(7 * 3600).expect("offset WIB valid")
}

fn hhmm(t: DateTime<Utc>) -> String {
    t.with_timezone(&wib()).format("%H:%M").to_string()
}

pub fn market_open(now: DateTime<Utc>) -> bool {
    let local = now.with_timezone(&wib());
    let at = |(h, m): (u32, u32)| NaiveTime::from_hms_opt(h, m, 0).expect("jam valid");
    !matches!(local.weekday(), Weekday::Sat | Weekday::Sun) && (at(OPEN)..at(CLOSE)).contains(&local.time())
}

/// Tanpa candle hari ini pada `now`: `true` = bursa libur, `false` = data belum masuk, coba lagi.
pub fn no_data_means_holiday(now: DateTime<Utc>) -> bool {
    let (h, m) = HOLIDAY_AFTER;
    now.with_timezone(&wib()).time() >= NaiveTime::from_hms_opt(h, m, 0).expect("jam valid")
}

/// Lama jeda setelah 429 ke-`strikes` berturut-turut.
pub fn cooldown(strikes: u32) -> chrono::Duration {
    let min = COOLDOWN_MIN.saturating_mul(1 << strikes.saturating_sub(1).min(6)).min(MAX_COOLDOWN_MIN);
    chrono::Duration::minutes(min)
}

#[derive(Default)]
struct Inner {
    status: Option<LiveStatusDto>,
}

#[derive(Clone, Default)]
pub struct LiveHandle {
    inner: Arc<Mutex<Inner>>,
}

impl LiveHandle {
    pub fn status(&self) -> LiveStatusDto {
        self.inner.lock().expect("live lock").status.clone().unwrap_or_else(|| LiveStatusDto {
            state: "closed".into(),
            done: 0,
            total: 0,
            last_update: None,
            next_at: None,
            note: None,
        })
    }

    fn update(&self, f: impl FnOnce(&mut LiveStatusDto)) {
        let mut status = self.status();
        f(&mut status);
        self.inner.lock().expect("live lock").status = Some(status);
    }
}

enum RoundEnd {
    Finished,
    /// Belum ada candle hari ini.
    NoData,
    RateLimited,
    Interrupted,
}

struct Round {
    end: RoundEnd,
    updated: usize,
}

async fn run_round(state: &AppState, tickers: &[String], today: NaiveDate, gap: Duration) -> Round {
    let live = &state.live;
    let mut updated = 0;
    // IHSG ikut di request pertama: kondisi pasar sekaligus penanda data hari ini sudah masuk.
    let symbols: Vec<String> = std::iter::once(index_prices::IHSG.to_string()).chain(tickers.iter().cloned()).collect();
    for (i, chunk) in symbols.chunks(BATCH).enumerate() {
        if i > 0 {
            tokio::time::sleep(gap).await;
        }
        // Update harian sedang berjalan atau bursa sudah tutup: hentikan, jangan menambah beban.
        if state.ingest.progress().is_some() || !market_open(Utc::now()) {
            return Round { end: RoundEnd::Interrupted, updated };
        }
        match state.source.fetch_today(chunk, today).await {
            Ok(bars) => {
                if i == 0 && !bars.iter().any(|(s, _)| s == index_prices::IHSG) {
                    return Round { end: RoundEnd::NoData, updated };
                }
                for (symbol, bar) in bars {
                    let saved = if symbol == index_prices::IHSG {
                        index_prices::upsert(&state.pool, &symbol, std::slice::from_ref(&bar)).await.map(|_| false)
                    } else {
                        prices::upsert_bars(&state.pool, &symbol, std::slice::from_ref(&bar)).await.map(|_| true)
                    };
                    match saved {
                        Ok(true) => updated += 1,
                        Ok(false) => {}
                        Err(e) => tracing::warn!(%symbol, "gagal menyimpan harga intraday: {e}"),
                    }
                }
            }
            Err(FetchError::RateLimited) => return Round { end: RoundEnd::RateLimited, updated },
            Err(e) => tracing::warn!(batch = i + 1, "harga intraday gagal diambil: {e}"),
        }
        live.update(|s| s.done = ((i + 1) * BATCH).min(symbols.len()).saturating_sub(1));
    }
    Round { end: RoundEnd::Finished, updated }
}

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let gap = Duration::from_secs(state.config.live_gap_secs);
        let interval = chrono::Duration::minutes(state.config.live_interval_min as i64);
        let live = state.live.clone();
        let mut strikes = 0u32;
        let mut holiday: Option<NaiveDate> = None;
        let mut resume_at: Option<DateTime<Utc>> = None;
        tracing::info!(jeda_detik = gap.as_secs(), interval_menit = interval.num_minutes(), "update intraday aktif");

        loop {
            let now = Utc::now();
            let today = now.with_timezone(&wib()).date_naive();
            if !market_open(now) || holiday == Some(today) {
                strikes = 0;
                live.update(|s| {
                    s.state = "closed".into();
                    s.next_at = None;
                    s.note = (holiday == Some(today)).then(|| "Bursa libur hari ini".into());
                });
                tokio::time::sleep(IDLE).await;
                continue;
            }
            if let Some(at) = resume_at.filter(|at| *at > now) {
                tokio::time::sleep((at - now).to_std().unwrap_or(IDLE).min(IDLE)).await;
                continue;
            }
            if state.ingest.progress().is_some() {
                tokio::time::sleep(IDLE).await;
                continue;
            }

            // Semua saham aktif yang punya data harga.
            let mut tickers: Vec<String> =
                state.cache.snapshot().map(|d| d.last.keys().cloned().collect()).unwrap_or_default();
            tickers.sort();
            if tickers.is_empty() {
                tokio::time::sleep(IDLE).await;
                continue;
            }
            live.update(|s| {
                s.state = "running".into();
                (s.done, s.total) = (0, tickers.len());
                s.next_at = None;
                s.note = None;
            });

            let round = run_round(&state, &tickers, today, gap).await;
            if round.updated > 0 {
                if let Err(e) = state.cache.reload(&state.pool, state.config.cache_bars).await {
                    tracing::error!("gagal memuat ulang cache: {e:#}");
                }
                live.update(|s| s.last_update = Some(hhmm(Utc::now())));
            }
            match round.end {
                RoundEnd::NoData if no_data_means_holiday(Utc::now()) => {
                    tracing::info!("tidak ada candle hari ini, bursa dianggap libur");
                    holiday = Some(today);
                }
                RoundEnd::NoData => {
                    let at = Utc::now() + interval;
                    tracing::info!(coba_lagi = %hhmm(at), "candle hari ini belum ada di Yahoo");
                    resume_at = Some(at);
                    live.update(|s| {
                        s.state = "waiting".into();
                        s.next_at = Some(hhmm(at));
                        s.note = Some("Menunggu data hari ini dari Yahoo (tertunda ±15 menit setelah bursa buka)".into());
                    });
                }
                RoundEnd::RateLimited => {
                    strikes += 1;
                    let at = Utc::now() + cooldown(strikes);
                    tracing::warn!(jeda_sampai = %hhmm(at), "kena rate limit Yahoo, update intraday dijeda");
                    resume_at = Some(at);
                    live.update(|s| {
                        s.state = "cooldown".into();
                        s.next_at = Some(hhmm(at));
                        s.note = Some("Dibatasi Yahoo (429), update intraday dijeda".into());
                    });
                }
                RoundEnd::Finished | RoundEnd::Interrupted => {
                    if matches!(round.end, RoundEnd::Finished) {
                        strikes = 0;
                    }
                    let at = now + interval;
                    resume_at = Some(at);
                    tracing::info!(diperbarui = round.updated, saham = tickers.len(), "putaran intraday selesai");
                    live.update(|s| {
                        s.state = "waiting".into();
                        s.next_at = Some(hhmm(at.max(Utc::now())));
                    });
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wib_time(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&format!("{s}+07:00")).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn market_hours() {
        // 2026-10-16 hari Jumat, 2026-10-17 hari Sabtu.
        assert!(!market_open(wib_time("2026-10-16T08:59:59")));
        assert!(market_open(wib_time("2026-10-16T09:00:00")));
        assert!(market_open(wib_time("2026-10-16T16:14:59")));
        assert!(!market_open(wib_time("2026-10-16T16:15:00")));
        assert!(!market_open(wib_time("2026-10-17T10:00:00")));
    }

    #[test]
    fn missing_candle_is_holiday_only_late_in_the_morning() {
        assert!(!no_data_means_holiday(wib_time("2026-10-16T09:16:00")), "Yahoo tertunda ±15 menit");
        assert!(!no_data_means_holiday(wib_time("2026-10-16T10:29:59")));
        assert!(no_data_means_holiday(wib_time("2026-10-16T10:30:00")));
    }

    #[test]
    fn cooldown_doubles_up_to_cap() {
        let mins: Vec<i64> = (1..=5).map(|s| cooldown(s).num_minutes()).collect();
        assert_eq!(mins, [15, 30, 60, 120, 120]);
        assert_eq!(cooldown(40).num_minutes(), 120);
    }
}
