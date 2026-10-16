//! Daftar seluruh emiten IDX dari scanner publik TradingView (tanpa API key).
//! Dipakai karena idx.co.id memblokir akses otomatis dan screener Yahoo butuh cookie/crumb.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use super::FetchError;
use crate::db::stocks::StockRecord;

const URL: &str = "https://scanner.tradingview.com/indonesia/scan";

#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    data: Vec<Row>,
}

#[derive(Deserialize)]
struct Row {
    /// `[kode, nama, sektor]`, urutan sesuai `columns` di request.
    d: Vec<serde_json::Value>,
}

/// Sektor TradingView (klasifikasi FactSet) → nama sektor IDX-IC yang dipakai aplikasi.
pub fn map_sector(tv: &str) -> Option<&'static str> {
    Some(match tv {
        "Finance" => "Keuangan",
        "Energy Minerals" => "Energi",
        "Non-Energy Minerals" | "Process Industries" => "Barang Baku",
        "Consumer Non-Durables" => "Barang Konsumen Primer",
        "Consumer Durables" | "Consumer Services" | "Retail Trade" => "Barang Konsumen Non-Primer",
        "Health Technology" | "Health Services" => "Kesehatan",
        "Technology Services" | "Electronic Technology" => "Teknologi",
        "Communications" | "Utilities" => "Infrastruktur",
        "Transportation" => "Transportasi & Logistik",
        "Producer Manufacturing" | "Industrial Services" | "Commercial Services" | "Distribution Services" => {
            "Perindustrian"
        }
        _ => return None,
    })
}

/// Merapikan nama dari TradingView, mis. "Pt Adaro Andalan Indonesia Tbk Class A".
fn clean_name(name: &str) -> String {
    let name = name.trim().trim_end_matches(" Class A").trim_end_matches(" Class B");
    match name.strip_prefix("Pt ") {
        Some(rest) => format!("PT {rest}"),
        None => name.to_string(),
    }
}

pub fn parse(json: &str) -> Result<Vec<StockRecord>, FetchError> {
    let resp: Response = serde_json::from_str(json).map_err(|e| FetchError::Invalid(e.to_string()))?;
    Ok(resp
        .data
        .iter()
        .filter_map(|row| {
            let text = |i: usize| row.d.get(i).and_then(|v| v.as_str()).unwrap_or("");
            let sector = map_sector(text(2));
            StockRecord::new(text(0), &clean_name(text(1)), sector, None)
        })
        .collect())
}

pub async fn fetch(client: &reqwest::Client) -> Result<Vec<StockRecord>, FetchError> {
    let body = json!({
        "filter": [
            {"left": "type", "operation": "equal", "right": "stock"},
            {"left": "exchange", "operation": "equal", "right": "IDX"}
        ],
        "columns": ["name", "description", "sector"],
        "sort": {"sortBy": "name", "sortOrder": "asc"},
        "range": [0, 5000]
    });
    let resp = client
        .post(URL)
        .timeout(Duration::from_secs(30))
        .json(&body)
        .send()
        .await
        .map_err(|e| FetchError::Transient(e.to_string()))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(FetchError::Transient(format!("HTTP {status}")));
    }
    parse(&resp.text().await.map_err(|e| FetchError::Transient(e.to_string()))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_maps_sectors() {
        let json = r#"{"totalCount":3,"data":[
            {"s":"IDX:AADI","d":["AADI","Pt Adaro Andalan Indonesia Tbk","Energy Minerals"]},
            {"s":"IDX:ABDA","d":["ABDA","PT Asuransi Bina Dana Arta Tbk Class A","Finance"]},
            {"s":"IDX:XXXX","d":["XXXX","Contoh Tbk","Sektor Baru"]},
            {"s":"IDX:bad","d":[null,"tanpa kode","Finance"]}
        ]}"#;
        let rows = parse(json).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].ticker, "AADI");
        assert_eq!(rows[0].name, "PT Adaro Andalan Indonesia Tbk");
        assert_eq!(rows[0].sector.as_deref(), Some("Energi"));
        assert_eq!(rows[1].name, "PT Asuransi Bina Dana Arta Tbk");
        assert_eq!(rows[2].sector, None);
        assert!(matches!(parse("bukan json"), Err(FetchError::Invalid(_))));
    }
}
