//! Kondisi server dan aplikasi (halaman Server).

use axum::Json;
use axum::extract::State;
use shared::{CacheInfoDto, ServerStatusDto};

use crate::AppState;
use crate::db::{ingest_runs, stats};
use crate::error::ApiResult;
use crate::system;

pub async fn get(State(state): State<AppState>) -> ApiResult<ServerStatusDto> {
    let database = stats::database(&state.pool).await?;
    let last_ingest = ingest_runs::recent(&state.pool, 1).await?.into_iter().next();
    let cache = state.cache.snapshot();
    Ok(Json(ServerStatusDto {
        hostname: system::hostname(),
        os: system::os_name(),
        kernel: system::kernel(),
        uptime_secs: system::uptime_secs(),
        app_uptime_secs: state.system.app_uptime().as_secs(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        cpu: state.system.cpu(),
        memory: system::memory(),
        disks: system::disk("/").into_iter().collect(),
        database,
        cache: CacheInfoDto {
            stocks: cache.as_ref().map_or(0, |c| c.last.len()),
            bars: cache.as_ref().map_or(0, |c| c.prices.height()),
            loaded_at: cache.as_ref().map(|c| c.loaded_at.to_rfc3339()),
        },
        last_ingest,
        live: state.config.live_enabled.then(|| state.live.status()),
    }))
}
