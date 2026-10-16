//! Klien REST bertipe. Semua fungsi mengembalikan pesan error yang siap ditampilkan.

use gloo_net::http::{Request, RequestBuilder, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use shared::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ApiFailure {
    pub message: String,
    /// Status HTTP; 0 kalau server tidak bisa dihubungi.
    pub status: u16,
}

pub type Res<T> = Result<T, ApiFailure>;

fn offline(_: gloo_net::Error) -> ApiFailure {
    ApiFailure { message: "Tidak dapat terhubung ke server. Pastikan backend berjalan (./prod.sh).".into(), status: 0 }
}

async fn failure(resp: Response) -> ApiFailure {
    let status = resp.status();
    let message = match resp.json::<ApiError>().await {
        Ok(e) => e.message,
        Err(_) => format!("Server membalas HTTP {status}"),
    };
    ApiFailure { message, status }
}

async fn json<T: DeserializeOwned>(req: RequestBuilder) -> Res<T> {
    let resp = req.send().await.map_err(offline)?;
    if !resp.ok() {
        return Err(failure(resp).await);
    }
    resp.json::<T>().await.map_err(|e| ApiFailure { message: format!("Respons server tidak valid: {e}"), status: 200 })
}

async fn json_body<B: Serialize, T: DeserializeOwned>(req: RequestBuilder, body: &B) -> Res<T> {
    let resp = req.json(body).map_err(offline)?.send().await.map_err(offline)?;
    if !resp.ok() {
        return Err(failure(resp).await);
    }
    resp.json::<T>().await.map_err(|e| ApiFailure { message: format!("Respons server tidak valid: {e}"), status: 200 })
}

async fn unit(req: RequestBuilder) -> Res<()> {
    let resp = req.send().await.map_err(offline)?;
    if resp.ok() { Ok(()) } else { Err(failure(resp).await) }
}

fn enc(s: &str) -> String {
    String::from(js_sys::encode_uri_component(s))
}

pub async fn presets() -> Res<Vec<PresetDto>> {
    json(Request::get("/api/presets")).await
}

pub async fn scan(req: &ScanRequest) -> Res<ScanResultDto> {
    json_body(Request::post("/api/scan"), req).await
}

/// Bisa memakan beberapa detik saat pertama kali dihitung setelah update data.
pub async fn backtest(liquid: bool) -> Res<BacktestDto> {
    json(Request::get(&format!("/api/backtest?liquid={liquid}"))).await
}

pub async fn server_status() -> Res<ServerStatusDto> {
    json(Request::get("/api/server")).await
}

pub async fn brief() -> Res<BriefDto> {
    json(Request::get("/api/brief")).await
}

pub async fn market_summary() -> Res<MarketSummaryDto> {
    json(Request::get("/api/market/summary")).await
}

pub async fn stocks(q: &str, limit: u32) -> Res<Vec<StockDto>> {
    json(Request::get(&format!("/api/stocks?q={}&limit={limit}", enc(q)))).await
}

pub async fn stock(ticker: &str) -> Res<StockDetailDto> {
    json(Request::get(&format!("/api/stocks/{}", enc(ticker)))).await
}

pub async fn candles(ticker: &str, range: &str, indicators: &str) -> Res<CandlesDto> {
    json(Request::get(&format!("/api/stocks/{}/candles?range={range}&indicators={}", enc(ticker), enc(indicators)))).await
}

pub async fn import_csv(file: &web_sys::File) -> Res<ImportResultDto> {
    let form = web_sys::FormData::new().map_err(|_| ApiFailure { message: "Browser tidak mendukung upload".into(), status: 0 })?;
    form.append_with_blob("file", file).map_err(|_| ApiFailure { message: "Gagal membaca file".into(), status: 0 })?;
    let req = Request::post("/api/stocks/import").body(form).map_err(offline)?;
    let resp = req.send().await.map_err(offline)?;
    if !resp.ok() {
        return Err(failure(resp).await);
    }
    resp.json().await.map_err(|e| ApiFailure { message: e.to_string(), status: 200 })
}

pub async fn watchlists() -> Res<Vec<WatchlistDto>> {
    json(Request::get("/api/watchlists")).await
}

pub async fn create_watchlist(name: &str) -> Res<WatchlistDto> {
    json_body(Request::post("/api/watchlists"), &CreateWatchlistDto { name: name.into() }).await
}

pub async fn delete_watchlist(id: i64) -> Res<()> {
    unit(Request::delete(&format!("/api/watchlists/{id}"))).await
}

pub async fn add_to_watchlist(id: i64, ticker: &str) -> Res<()> {
    unit(Request::post(&format!("/api/watchlists/{id}/items/{}", enc(ticker)))).await
}

pub async fn remove_from_watchlist(id: i64, ticker: &str) -> Res<()> {
    unit(Request::delete(&format!("/api/watchlists/{id}/items/{}", enc(ticker)))).await
}

pub async fn saved_scans() -> Res<Vec<SavedScanDto>> {
    json(Request::get("/api/scans")).await
}

pub async fn save_scan(body: &SaveScanDto) -> Res<SavedScanDto> {
    json_body(Request::post("/api/scans"), body).await
}

pub async fn update_scan(id: i64, body: &SaveScanDto) -> Res<SavedScanDto> {
    json_body(Request::put(&format!("/api/scans/{id}")), body).await
}

pub async fn delete_scan(id: i64) -> Res<()> {
    unit(Request::delete(&format!("/api/scans/{id}"))).await
}

pub async fn ingest_status() -> Res<IngestStatusDto> {
    json(Request::get("/api/ingest/status")).await
}

pub async fn ingest_run() -> Res<IngestStatusDto> {
    json(Request::post("/api/ingest/run")).await
}
