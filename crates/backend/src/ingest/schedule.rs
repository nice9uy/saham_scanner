//! Jadwal update otomatis: setiap hari bursa (Senin–Jumat) pada jam WIB tertentu.

use chrono::{DateTime, Datelike, FixedOffset, Utc, Weekday};

use crate::AppState;

fn wib() -> FixedOffset {
    FixedOffset::east_opt(7 * 3600).expect("offset WIB valid")
}

/// Waktu jalan berikutnya yang selalu setelah `now`.
pub fn next_run(now: DateTime<Utc>, at: (u32, u32)) -> DateTime<Utc> {
    let mut date = now.with_timezone(&wib()).date_naive();
    loop {
        let weekday = date.weekday();
        if !matches!(weekday, Weekday::Sat | Weekday::Sun) {
            let candidate = date
                .and_hms_opt(at.0, at.1, 0)
                .expect("jam sudah divalidasi")
                .and_local_timezone(wib())
                .single()
                .expect("offset tetap selalu tunggal")
                .with_timezone(&Utc);
            if candidate > now {
                return candidate;
            }
        }
        date = date.succ_opt().expect("tanggal berikutnya ada");
    }
}

pub fn spawn_scheduler(state: AppState) {
    tokio::spawn(async move {
        loop {
            let now = Utc::now();
            let next = next_run(now, state.config.ingest_at);
            tracing::info!(%next, "update data berikutnya");
            tokio::time::sleep((next - now).to_std().unwrap_or_default()).await;
            if let Err(e) = super::trigger(&state, "schedule") {
                tracing::warn!("update terjadwal dilewati: {e:?}");
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
    fn next_run_cases() {
        let at = (17, 0);
        // 2026-10-12 adalah hari Senin.
        assert_eq!(next_run(wib_time("2026-10-12T10:00:00"), at), wib_time("2026-10-12T17:00:00"));
        assert_eq!(next_run(wib_time("2026-10-12T17:00:00"), at), wib_time("2026-10-13T17:00:00"));
        assert_eq!(next_run(wib_time("2026-10-16T18:00:00"), at), wib_time("2026-10-19T17:00:00"));
        assert_eq!(next_run(wib_time("2026-10-17T09:00:00"), at), wib_time("2026-10-19T17:00:00"));
    }
}
