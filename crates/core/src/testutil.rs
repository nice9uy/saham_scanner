//! Pembuat DataFrame sintetis untuk test.

use polars::prelude::*;

/// Hari pertama data sintetis (2024-10-04), dalam hari sejak 1970-01-01.
pub const BASE_DAY: i32 = 20_000;

/// Satu bar: `[open, high, low, close, volume]`.
pub type Bar = [f64; 5];

/// Bar ke-i setiap ticker diberi tanggal `BASE_DAY + i`, jadi seri yang lebih pendek
/// berakhir di tanggal yang lebih awal.
pub fn bars(series: &[(&str, Vec<Bar>)]) -> DataFrame {
    let (mut ticker, mut date) = (Vec::new(), Vec::new());
    let (mut o, mut h, mut l, mut c, mut v) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (t, bs) in series {
        for (i, b) in bs.iter().enumerate() {
            ticker.push(t.to_string());
            date.push(BASE_DAY + i as i32);
            o.push(b[0]);
            h.push(b[1]);
            l.push(b[2]);
            c.push(b[3]);
            v.push(b[4]);
        }
    }
    let mut df = df!(
        "ticker" => ticker,
        "open" => o,
        "high" => h,
        "low" => l,
        "close" => c,
        "volume" => v,
    )
    .unwrap();
    df.with_column(Column::new("date".into(), date).cast(&DataType::Date).unwrap()).unwrap();
    df
}

/// OHLC = close, volume = 1.000.000.
pub fn closes(series: &[(&str, Vec<f64>)]) -> DataFrame {
    let s: Vec<(&str, Vec<f64>, Vec<f64>)> =
        series.iter().map(|(t, cs)| (*t, cs.clone(), vec![1_000_000.0; cs.len()])).collect();
    closes_volumes(&s)
}

pub fn closes_volumes(series: &[(&str, Vec<f64>, Vec<f64>)]) -> DataFrame {
    let s: Vec<(&str, Vec<Bar>)> = series
        .iter()
        .map(|(t, cs, vs)| (*t, cs.iter().zip(vs).map(|(&c, &v)| [c, c, c, c, v]).collect()))
        .collect();
    bars(&s)
}

/// Seri bergelombang yang deterministik.
pub fn wave(n: usize, base: f64, amp: f64, drift: f64) -> Vec<f64> {
    (0..n).map(|i| base + (i as f64 * 0.37).sin() * amp + i as f64 * drift).collect()
}

pub fn values(df: &DataFrame, name: &str) -> Vec<Option<f64>> {
    df.column(name).unwrap().f64().unwrap().iter().collect()
}

pub fn values_for(df: &DataFrame, ticker: &str, name: &str) -> Vec<Option<f64>> {
    let tickers = df.column("ticker").unwrap().str().unwrap();
    tickers
        .iter()
        .zip(values(df, name))
        .filter(|(t, _)| *t == Some(ticker))
        .map(|(_, v)| v)
        .collect()
}

pub fn assert_close(actual: &[Option<f64>], expected: &[Option<f64>]) {
    assert_eq!(actual.len(), expected.len(), "panjang berbeda");
    for (i, (a, e)) in actual.iter().zip(expected).enumerate() {
        match (a, e) {
            (None, None) => {}
            (Some(a), Some(e)) => assert!((a - e).abs() < 1e-6, "index {i}: {a} != {e}"),
            _ => panic!("index {i}: {a:?} != {e:?}"),
        }
    }
}
