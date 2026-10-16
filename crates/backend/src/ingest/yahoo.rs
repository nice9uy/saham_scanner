//! Klien Yahoo Finance untuk saham IDX (`KODE.JK`): `/v8/finance/chart` untuk riwayat per saham,
//! `/v7/finance/quote` untuk candle hari ini banyak saham sekaligus (butuh cookie + crumb).

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Days, NaiveDate};
use reqwest::StatusCode;
use serde::Deserialize;

use super::{FetchError, Fetched, PriceSource};
use crate::db::prices::PriceBar;
use scanner_core::quality::Fundamentals;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";

#[derive(Deserialize)]
struct Response {
    chart: Chart,
}

#[derive(Deserialize)]
struct Chart {
    result: Option<Vec<ChartResult>>,
    error: Option<ChartError>,
}

#[derive(Deserialize)]
struct ChartError {
    code: String,
    description: Option<String>,
}

#[derive(Deserialize)]
struct ChartResult {
    meta: Meta,
    #[serde(default)]
    timestamp: Vec<i64>,
    indicators: Indicators,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    long_name: Option<String>,
    short_name: Option<String>,
    #[serde(default)]
    gmtoffset: i64,
}

#[derive(Deserialize)]
struct Indicators {
    #[serde(default)]
    quote: Vec<Quote>,
    #[serde(default)]
    adjclose: Vec<AdjClose>,
}

#[derive(Deserialize, Default)]
struct Quote {
    #[serde(default)]
    open: Vec<Option<f64>>,
    #[serde(default)]
    high: Vec<Option<f64>>,
    #[serde(default)]
    low: Vec<Option<f64>>,
    #[serde(default)]
    close: Vec<Option<f64>>,
    #[serde(default)]
    volume: Vec<Option<f64>>,
}

#[derive(Deserialize)]
struct AdjClose {
    #[serde(default)]
    adjclose: Vec<Option<f64>>,
}

/// Bar dengan OHLC kosong dibuang; tanggal dihitung di zona waktu bursa (`gmtoffset`).
pub fn parse_chart(json: &str) -> Result<Fetched, FetchError> {
    let resp: Response = serde_json::from_str(json).map_err(|e| FetchError::Invalid(e.to_string()))?;
    if let Some(err) = resp.chart.error {
        return Err(if err.code == "Not Found" {
            FetchError::NotFound
        } else {
            FetchError::Invalid(err.description.unwrap_or(err.code))
        });
    }
    let Some(result) = resp.chart.result.and_then(|r| r.into_iter().next()) else {
        return Err(FetchError::Invalid("respons tanpa data".into()));
    };
    let quote = result.indicators.quote.into_iter().next().unwrap_or_default();
    let adj = result.indicators.adjclose.into_iter().next().map(|a| a.adjclose).unwrap_or_default();
    let at = |v: &Vec<Option<f64>>, i: usize| v.get(i).copied().flatten();

    let mut bars = BTreeMap::new();
    for (i, ts) in result.timestamp.iter().enumerate() {
        let (Some(open), Some(high), Some(low), Some(close)) =
            (at(&quote.open, i), at(&quote.high, i), at(&quote.low, i), at(&quote.close, i))
        else {
            continue;
        };
        let Some(date) = DateTime::from_timestamp(ts + result.meta.gmtoffset, 0).map(|d| d.date_naive()) else {
            continue;
        };
        bars.insert(date, PriceBar {
            date,
            open,
            high,
            low,
            close,
            adj_close: at(&adj, i).unwrap_or(close),
            volume: at(&quote.volume, i).unwrap_or(0.0).round() as i64,
        });
    }
    let name = result.meta.long_name.or(result.meta.short_name).filter(|n| !n.trim().is_empty());
    Ok(Fetched { name, bars: bars.into_values().collect() })
}

#[derive(Deserialize)]
struct QuoteResponse {
    #[serde(rename = "quoteResponse")]
    quote_response: QuoteResult,
}

#[derive(Deserialize)]
struct QuoteResult {
    #[serde(default)]
    result: Vec<QuoteItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuoteItem {
    symbol: String,
    regular_market_open: Option<f64>,
    regular_market_day_high: Option<f64>,
    regular_market_day_low: Option<f64>,
    regular_market_price: Option<f64>,
    regular_market_volume: Option<f64>,
    regular_market_time: Option<i64>,
    #[serde(default)]
    gmt_off_set_milliseconds: i64,
}

/// Hanya simbol yang transaksi terakhirnya bertanggal `today` (zona waktu bursa). Kode saham
/// dikembalikan tanpa `.JK`.
pub fn parse_quotes(json: &str, today: NaiveDate) -> Result<Vec<(String, PriceBar)>, FetchError> {
    let resp: QuoteResponse = serde_json::from_str(json).map_err(|e| FetchError::Invalid(e.to_string()))?;
    Ok(resp
        .quote_response
        .result
        .into_iter()
        .filter_map(|q| {
            let date = DateTime::from_timestamp(q.regular_market_time? + q.gmt_off_set_milliseconds / 1000, 0)?.date_naive();
            if date != today {
                return None;
            }
            let close = q.regular_market_price?;
            let bar = PriceBar {
                date,
                open: q.regular_market_open?,
                high: q.regular_market_day_high?,
                low: q.regular_market_day_low?,
                close,
                adj_close: close,
                volume: q.regular_market_volume.unwrap_or(0.0).round() as i64,
            };
            let symbol = q.symbol.strip_suffix(".JK").unwrap_or(&q.symbol).to_string();
            Some((symbol, bar))
        })
        .collect())
}

const QUOTE_BATCH: usize = 100;
const FUNDAMENTALS_GAP: Duration = Duration::from_secs(3);
/// Simbol Yahoo untuk kurs 1 USD dalam rupiah.
const USD_IDR: &str = "IDR=X";

#[derive(Deserialize)]
struct FundResponse {
    #[serde(rename = "quoteResponse")]
    quote_response: FundResult,
}

#[derive(Deserialize)]
struct FundResult {
    #[serde(default)]
    result: Vec<FundItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FundItem {
    symbol: String,
    regular_market_price: Option<f64>,
    eps_trailing_twelve_months: Option<f64>,
    book_value: Option<f64>,
    trailing_annual_dividend_yield: Option<f64>,
    market_cap: Option<f64>,
    financial_currency: Option<String>,
}

/// `(ticker, data)` per saham, dan kurs USD/IDR yang dipakai.
pub type ParsedFundamentals = (Vec<(String, Fundamentals)>, Option<f64>);

/// Data fundamental dalam rupiah per saham, plus kurs USD/IDR kalau `IDR=X` ada di respons.
/// EPS dari Yahoo sudah dalam mata uang harga (rupiah), tetapi nilai buku mengikuti mata uang
/// laporan keuangan: laporan berdolar dikonversi dengan kurs, mata uang lain dikosongkan.
/// Dividen hanya dipakai untuk laporan rupiah, karena untuk laporan berdolar Yahoo mengisinya 0.
pub fn parse_fundamentals(json: &str, usd_idr: Option<f64>) -> Result<ParsedFundamentals, FetchError> {
    let resp: FundResponse = serde_json::from_str(json).map_err(|e| FetchError::Invalid(e.to_string()))?;
    let items = resp.quote_response.result;
    let rate = items
        .iter()
        .find(|q| q.symbol == USD_IDR)
        .and_then(|q| q.regular_market_price)
        .filter(|r| *r > 0.0)
        .or(usd_idr);
    let rows = items
        .into_iter()
        .filter_map(|q| {
            let ticker = q.symbol.strip_suffix(".JK")?.to_string();
            let idr = q.financial_currency.as_deref().is_none_or(|c| c == "IDR");
            let multiplier = match q.financial_currency.as_deref() {
                None | Some("IDR") => Some(1.0),
                Some("USD") => rate,
                _ => None,
            };
            let f = Fundamentals {
                eps: q.eps_trailing_twelve_months,
                book_value: q.book_value.zip(multiplier).map(|(b, m)| b * m),
                dividend_yield: q.trailing_annual_dividend_yield.filter(|_| idr).map(|d| d * 100.0),
                market_cap: q.market_cap,
            };
            Some((ticker, f))
        })
        .collect();
    Ok((rows, rate))
}

/// Cookie dan crumb Yahoo untuk endpoint `/v7/finance/quote`.
#[derive(Clone)]
struct Session {
    cookie: String,
    crumb: String,
}

pub struct YahooSource {
    client: reqwest::Client,
    session: tokio::sync::Mutex<Option<Session>>,
}

impl YahooSource {
    pub fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder().user_agent(USER_AGENT).timeout(Duration::from_secs(20)).build()?;
        Ok(Self { client, session: tokio::sync::Mutex::new(None) })
    }

    async fn session(&self, refresh: bool) -> Result<Session, FetchError> {
        let mut guard = self.session.lock().await;
        if let Some(s) = guard.as_ref().filter(|_| !refresh) {
            return Ok(s.clone());
        }
        let transient = |e: reqwest::Error| FetchError::Transient(e.to_string());
        // Halaman ini membalas 404, tapi tetap memberi cookie sesi.
        let resp = self.client.get("https://fc.yahoo.com/consent").send().await.map_err(transient)?;
        let cookie = resp
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .filter_map(|v| v.split(';').next().map(str::trim).filter(|p| p.contains('=')))
            .collect::<Vec<_>>()
            .join("; ");
        if cookie.is_empty() {
            return Err(FetchError::Invalid("Yahoo tidak memberi cookie".into()));
        }
        let resp = self
            .client
            .get("https://query1.finance.yahoo.com/v1/test/getcrumb")
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .map_err(transient)?;
        if resp.status() == StatusCode::TOO_MANY_REQUESTS {
            return Err(FetchError::RateLimited);
        }
        let crumb = resp.text().await.map_err(transient)?.trim().to_string();
        if crumb.is_empty() || crumb.len() > 64 || crumb.contains(char::is_whitespace) || crumb.contains('<') {
            return Err(FetchError::Invalid("crumb Yahoo tidak valid".into()));
        }
        let s = Session { cookie, crumb };
        *guard = Some(s.clone());
        Ok(s)
    }
}

fn unix(d: NaiveDate) -> i64 {
    d.and_hms_opt(0, 0, 0).expect("tengah malam valid").and_utc().timestamp()
}

#[async_trait]
impl PriceSource for YahooSource {
    async fn listing(&self) -> Result<Vec<crate::db::stocks::StockRecord>, FetchError> {
        super::listing::fetch(&self.client).await
    }

    async fn fetch(&self, ticker: &str, from: NaiveDate, to: NaiveDate) -> Result<Fetched, FetchError> {
        self.chart(&format!("{ticker}.JK"), from, to).await
    }

    async fn fetch_index(&self, symbol: &str, from: NaiveDate, to: NaiveDate) -> Result<Fetched, FetchError> {
        self.chart(&symbol.replace('^', "%5E"), from, to).await
    }

    async fn fetch_today(&self, symbols: &[String], today: NaiveDate) -> Result<Vec<(String, PriceBar)>, FetchError> {
        if symbols.is_empty() {
            return Ok(vec![]);
        }
        let list = symbols
            .iter()
            .map(|s| if s.starts_with('^') { s.replace('^', "%5E") } else { format!("{s}.JK") })
            .collect::<Vec<_>>()
            .join(",");
        parse_quotes(&self.quote_body(&list).await?, today)
    }

    async fn fetch_fundamentals(&self, tickers: &[String]) -> Result<Vec<(String, Fundamentals)>, FetchError> {
        let mut out = Vec::new();
        let mut usd_idr = None;
        for (i, chunk) in tickers.chunks(QUOTE_BATCH).enumerate() {
            if i > 0 {
                tokio::time::sleep(FUNDAMENTALS_GAP).await;
            }
            // Kurs USD/IDR ikut di request pertama, untuk emiten yang laporannya dalam dolar.
            let mut symbols: Vec<String> = chunk.iter().map(|t| format!("{t}.JK")).collect();
            if i == 0 {
                symbols.insert(0, USD_IDR.to_string());
            }
            let list = symbols.join(",");
            let (rows, rate) = parse_fundamentals(&self.quote_body(&list).await?, usd_idr)?;
            usd_idr = usd_idr.or(rate);
            out.extend(rows);
        }
        Ok(out)
    }
}

impl YahooSource {
    /// Isi mentah `/v7/finance/quote` untuk daftar simbol Yahoo yang sudah dipisah koma.
    async fn quote_body(&self, list: &str) -> Result<String, FetchError> {
        // Sesi bisa kedaluwarsa: kalau ditolak, minta cookie + crumb baru sekali lalu ulangi.
        for refresh in [false, true] {
            let session = self.session(refresh).await?;
            let url = format!("https://query1.finance.yahoo.com/v7/finance/quote?symbols={list}&crumb={}", session.crumb);
            let resp = self
                .client
                .get(&url)
                .header(reqwest::header::COOKIE, &session.cookie)
                .send()
                .await
                .map_err(|e| FetchError::Transient(e.to_string()))?;
            let status = resp.status();
            let body = resp.text().await.map_err(|e| FetchError::Transient(e.to_string()))?;
            match status {
                StatusCode::TOO_MANY_REQUESTS => return Err(FetchError::RateLimited),
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN if !refresh => continue,
                s if s.is_server_error() => return Err(FetchError::Transient(format!("HTTP {s}"))),
                s if !s.is_success() => return Err(FetchError::Invalid(format!("HTTP {s}"))),
                _ => return Ok(body),
            }
        }
        Err(FetchError::Invalid("sesi Yahoo ditolak".into()))
    }

    /// `symbol` sudah dalam bentuk URL Yahoo (mis. `BBCA.JK`, `%5EJKSE`).
    async fn chart(&self, symbol: &str, from: NaiveDate, to: NaiveDate) -> Result<Fetched, FetchError> {
        let period1 = unix(from.checked_sub_days(Days::new(1)).unwrap_or(from));
        let period2 = unix(to.checked_add_days(Days::new(1)).unwrap_or(to));
        let url = format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}?interval=1d&period1={period1}&period2={period2}&events=split,div&includeAdjustedClose=true"
        );
        let resp = self.client.get(&url).send().await.map_err(|e| FetchError::Transient(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| FetchError::Transient(e.to_string()))?;
        match status {
            StatusCode::NOT_FOUND => Err(FetchError::NotFound),
            StatusCode::TOO_MANY_REQUESTS => Err(FetchError::RateLimited),
            s if s.is_server_error() => Err(FetchError::Transient(format!("HTTP {s}"))),
            _ => parse_chart(&body),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fixture() {
        let json = include_str!("../../tests/fixtures/bbca_chart.json");
        let f = parse_chart(json).unwrap();
        assert_eq!(f.name.as_deref(), Some("PT Bank Central Asia Tbk"));
        assert_eq!(f.bars.len(), 5);
        let first = &f.bars[0];
        // 1791511200 + 25200 detik → tanggal lokal WIB
        let expected = DateTime::from_timestamp(1_791_511_200 + 25_200, 0).unwrap().date_naive();
        assert_eq!(first.date, expected);
        assert_eq!((first.open, first.high, first.low, first.close), (6025.0, 6075.0, 5975.0, 6050.0));
        assert_eq!(first.volume, 87_098_600);
        assert!(f.bars.windows(2).all(|w| w[0].date < w[1].date));
    }

    #[test]
    fn parse_quotes_keeps_only_today() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        // 1792115892 = 2026-10-16 01:58 UTC (08:58 WIB); 1792055689 = kemarin.
        let json = r#"{"quoteResponse":{"result":[
            {"symbol":"BBRI.JK","regularMarketOpen":2640,"regularMarketDayHigh":2650,"regularMarketDayLow":2600,
             "regularMarketPrice":2620,"regularMarketVolume":60323500,"regularMarketTime":1792115892,"gmtOffSetMilliseconds":25200000},
            {"symbol":"BBCA.JK","regularMarketOpen":6075,"regularMarketDayHigh":6125,"regularMarketDayLow":6025,
             "regularMarketPrice":6075,"regularMarketVolume":134763200,"regularMarketTime":1792055689,"gmtOffSetMilliseconds":25200000},
            {"symbol":"^JKSE","regularMarketOpen":5393.9,"regularMarketDayHigh":5393.9,"regularMarketDayLow":5393.9,
             "regularMarketPrice":5393.9,"regularMarketVolume":0,"regularMarketTime":1792116000,"gmtOffSetMilliseconds":25200000},
            {"symbol":"NOPE.JK","regularMarketTime":1792115892,"gmtOffSetMilliseconds":25200000}
        ],"error":null}}"#;
        let q = parse_quotes(json, today).unwrap();
        let syms: Vec<&str> = q.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(syms, ["BBRI", "^JKSE"], "BBCA belum bertransaksi hari ini; NOPE tanpa harga");
        let b = &q[0].1;
        assert_eq!((b.date, b.open, b.high, b.low, b.close, b.volume), (today, 2640.0, 2650.0, 2600.0, 2620.0, 60_323_500));
        assert!(matches!(parse_quotes("bukan json", today), Err(FetchError::Invalid(_))));
    }

    #[test]
    fn parse_fundamentals_converts_usd_books() {
        let json = r#"{"quoteResponse":{"result":[
            {"symbol":"IDR=X","regularMarketPrice":16400.0},
            {"symbol":"BBCA.JK","epsTrailingTwelveMonths":472.0,"bookValue":2201.51,"trailingAnnualDividendYield":0.046,
             "marketCap":638800000000000,"financialCurrency":"IDR"},
            {"symbol":"ADRO.JK","epsTrailingTwelveMonths":339.58,"bookValue":0.16,"trailingAnnualDividendYield":0.0,
             "marketCap":64200000000000,"financialCurrency":"USD"},
            {"symbol":"XXXX.JK","epsTrailingTwelveMonths":10.0,"bookValue":2.0,"financialCurrency":"EUR"},
            {"symbol":"KKES.JK","bookValue":42.47,"financialCurrency":"IDR"}
        ],"error":null}}"#;
        let (rows, rate) = parse_fundamentals(json, None).unwrap();
        assert_eq!(rate, Some(16400.0));
        let get = |t: &str| rows.iter().find(|(x, _)| x == t).unwrap().1;
        assert_eq!(get("BBCA").book_value, Some(2201.51));
        assert!((get("BBCA").dividend_yield.unwrap() - 4.6).abs() < 1e-9);
        assert!((get("ADRO").book_value.unwrap() - 0.16 * 16400.0).abs() < 1e-6, "nilai buku USD dikonversi");
        assert_eq!(get("ADRO").dividend_yield, None, "dividen laporan berdolar tidak andal");
        assert_eq!(get("XXXX").book_value, None, "mata uang lain tidak dikonversi");
        assert_eq!(get("KKES").eps, None);
        assert!(rows.iter().all(|(t, _)| t != "IDR=X"));
        // Tanpa IDR=X di respons, kurs dari request sebelumnya dipakai.
        let (rows, rate) = parse_fundamentals(r#"{"quoteResponse":{"result":[
            {"symbol":"ITMG.JK","bookValue":1.72,"financialCurrency":"USD"}]}}"#, Some(16000.0)).unwrap();
        assert_eq!((rate, rows[0].1.book_value), (Some(16000.0), Some(1.72 * 16000.0)));
    }

    #[test]
    fn parse_nulls_and_not_found() {
        let json = r#"{"chart":{"result":[{"meta":{"gmtoffset":25200},"timestamp":[1700000000,1700086400],
            "indicators":{"quote":[{"open":[1.0,null],"high":[2.0,2.0],"low":[0.5,0.5],"close":[1.5,1.5],"volume":[null,10]}]}}],"error":null}}"#;
        let f = parse_chart(json).unwrap();
        assert_eq!(f.bars.len(), 1);
        assert_eq!(f.bars[0].adj_close, 1.5);
        assert_eq!(f.bars[0].volume, 0);
        assert_eq!(f.name, None);

        let not_found = r#"{"chart":{"result":null,"error":{"code":"Not Found","description":"No data found, symbol may be delisted"}}}"#;
        assert_eq!(parse_chart(not_found).unwrap_err(), FetchError::NotFound);
        assert!(matches!(parse_chart("bukan json"), Err(FetchError::Invalid(_))));
    }
}
