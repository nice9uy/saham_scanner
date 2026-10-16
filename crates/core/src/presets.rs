//! Strategi siap pakai. Setiap preset hanyalah `ScanRequest` yang sudah diisi, jadi bisa
//! diedit dan disimpan ulang dari UI.

use shared::{Comparator, Condition, Indicator, Operand, PresetDto, QualityLevel, ScanRequest, SignalLevel, Universe};

/// Nilai transaksi rata-rata 20 hari minimum (Rp1 miliar) untuk menyaring saham tidak likuid.
pub const LIQUID_MIN_VALUE: f64 = 1_000_000_000.0;

fn ind(i: Indicator) -> Operand {
    Operand::indicator(i)
}

fn close() -> Operand {
    ind(Indicator::Close)
}

/// Preset yang hanya memfilter berdasarkan rating sinyal, tanpa kondisi tambahan.
fn signal_preset(id: &str, name: &str, description: &str, level: SignalLevel) -> PresetDto {
    let mut p = preset(id, name, description, vec![]);
    p.request.universe.signals = vec![level];
    p
}

fn preset(id: &str, name: &str, description: &str, conditions: Vec<Condition>) -> PresetDto {
    let mut request = ScanRequest::new(conditions);
    request.universe = Universe { min_avg_value_20d: Some(LIQUID_MIN_VALUE), ..Universe::default() };
    PresetDto { id: id.into(), name: name.into(), description: description.into(), request }
}

pub fn presets() -> Vec<PresetDto> {
    use Indicator::*;
    let bb_width = BbWidth { period: 20, std_x10: 20 };
    vec![
        signal_preset(
            "best-buy",
            "Best Buy",
            "Momen beli awal: sinyal naik baru muncul dalam 3 hari (MACD, Supertrend, atau SMA 50), mayoritas indikator positif, harga belum lebih dari 8% di atas SMA 20, tidak anjlok > 5% hari ini, dan tanpa tanda tren turun, distribusi, atau pergerakan tidak wajar.",
            SignalLevel::BestBuy,
        ),
        signal_preset(
            "best-sell",
            "Best Sell",
            "Momen jual: sinyal turun baru muncul selagi harga masih dekat puncak 20 hari, atau RSI baru turun dari zona jenuh beli dengan momentum melemah.",
            SignalLevel::BestSell,
        ),
        {
            let mut p = preset(
                "quality-uptrend",
                "Fundamental Sehat + Tren Naik",
                "Fundamental sehat (berlaba, ROE tinggi, PER wajar dibanding sektor, membagi dividen) dan harga dalam tren naik: di atas SMA 200, dengan SMA 50 di atas SMA 200.",
                vec![
                    Condition::new(close(), Comparator::Gt, ind(Sma { period: 200 })),
                    Condition::new(ind(Sma { period: 50 }), Comparator::Gt, ind(Sma { period: 200 })),
                ],
            );
            p.request.universe.quality = vec![QualityLevel::Healthy];
            p
        },
        preset(
            "golden-cross",
            "Golden Cross",
            "SMA 50 memotong ke atas SMA 200 dalam 3 hari terakhir.",
            vec![Condition::new(ind(Sma { period: 50 }), Comparator::CrossAbove { within: 3 }, ind(Sma { period: 200 }))],
        ),
        preset(
            "rsi-oversold-uptrend",
            "RSI Oversold dalam Uptrend",
            "RSI 14 di bawah 30 sementara harga masih di atas SMA 200.",
            vec![
                Condition::new(ind(Rsi { period: 14 }), Comparator::Lt, Operand::constant(30.0)),
                Condition::new(close(), Comparator::Gt, ind(Sma { period: 200 })),
            ],
        ),
        preset(
            "macd-bullish",
            "MACD Bullish Cross",
            "Garis MACD (12, 26, 9) memotong ke atas garis signal dalam 2 hari terakhir.",
            vec![Condition::new(
                ind(MacdLine { fast: 12, slow: 26, signal: 9 }),
                Comparator::CrossAbove { within: 2 },
                ind(MacdSignal { fast: 12, slow: 26, signal: 9 }),
            )],
        ),
        preset(
            "breakout-52w",
            "Breakout 52 Minggu",
            "Harga penutupan menembus high tertinggi 250 hari sebelumnya.",
            vec![Condition::new(close(), Comparator::Gt, ind(HighestHigh { period: 250 }).shifted(1))],
        ),
        preset(
            "volume-spike",
            "Volume Spike",
            "Volume lebih dari 2× rata-rata 20 hari dan harga naik.",
            vec![
                Condition::new(ind(Volume), Comparator::Gt, ind(VolumeSma { period: 20 }).times(2.0)),
                Condition::new(ind(ChangePct { period: 1 }), Comparator::Gt, Operand::constant(0.0)),
            ],
        ),
        preset(
            "bb-squeeze",
            "Bollinger Squeeze",
            "Lebar Bollinger Band (20, 2) berada di level tersempit dalam 120 hari.",
            vec![Condition::new(
                ind(bb_width.clone()),
                Comparator::Lte,
                ind(LowestOf { inner: Box::new(bb_width), period: 120 }),
            )],
        ),
        preset(
            "supertrend-flip",
            "Supertrend Flip Bullish",
            "Harga memotong ke atas Supertrend (10, 3) dalam 2 hari terakhir.",
            vec![Condition::new(
                close(),
                Comparator::CrossAbove { within: 2 },
                ind(Supertrend { period: 10, mult_x10: 30 }),
            )],
        ),
        preset(
            "akumulasi-senyap",
            "Akumulasi Senyap",
            "Volume lebih dari 2× rata-rata tetapi harga hampir tidak bergerak (±2%), dengan dana masuk (CMF > 0,1). Pola barang dikumpulkan tanpa harga dibiarkan naik.",
            vec![
                Condition::new(ind(RelVolume { period: 20 }), Comparator::Gt, Operand::constant(2.0)),
                Condition::new(ind(ChangePct { period: 1 }), Comparator::Lt, Operand::constant(2.0)),
                Condition::new(ind(ChangePct { period: 1 }), Comparator::Gt, Operand::constant(-2.0)),
                Condition::new(ind(Cmf { period: 20 }), Comparator::Gt, Operand::constant(0.1)),
            ],
        ),
        preset(
            "divergensi-obv",
            "Divergensi OBV",
            "Harga lebih rendah daripada 20 hari lalu, tetapi OBV justru lebih tinggi dan dana masuk. Tanda akumulasi saat harga masih lesu.",
            vec![
                Condition::new(close(), Comparator::Lt, close().shifted(20)),
                Condition::new(ind(Obv), Comparator::Gt, ind(Obv).shifted(20)),
                Condition::new(ind(Cmf { period: 20 }), Comparator::Gt, Operand::constant(0.0)),
            ],
        ),
        preset(
            "breakout-bervolume",
            "Breakout Bervolume",
            "Harga menembus high tertinggi 20 hari sebelumnya dengan volume lebih dari 2× rata-rata. Breakout tanpa volume sering gagal.",
            vec![
                Condition::new(close(), Comparator::Gt, ind(HighestHigh { period: 20 }).shifted(1)),
                Condition::new(ind(RelVolume { period: 20 }), Comparator::Gt, Operand::constant(2.0)),
                Condition::new(close(), Comparator::Gt, ind(Sma { period: 50 })),
            ],
        ),
        preset(
            "distribusi",
            "Distribusi (waspada)",
            "Dana keluar (CMF < −0,1) dan OBV turun 10 hari terakhir padahal harga bertahan. Peringatan untuk mengurangi atau keluar posisi, bukan sinyal beli.",
            vec![
                Condition::new(ind(Cmf { period: 20 }), Comparator::Lt, Operand::constant(-0.1)),
                Condition::new(ind(Obv), Comparator::Lt, ind(Obv).shifted(10)),
                Condition::new(close(), Comparator::Gte, close().shifted(10).times(0.98)),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::{run_scan, testutil::*, validate};

    #[test]
    fn all_presets_are_valid_and_unique() {
        let ps = presets();
        assert_eq!(ps.len(), 14);
        let ids: HashSet<&str> = ps.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids.len(), 14);
        for p in &ps {
            validate(&p.request).unwrap_or_else(|e| panic!("{}: {e}", p.id));
            assert_eq!(p.request.universe.min_avg_value_20d, Some(LIQUID_MIN_VALUE));
        }
    }

    #[test]
    fn all_presets_run_on_realistic_data() {
        let df = closes(&[
            ("AAA", wave(300, 5000.0, 400.0, 2.0)),
            ("BBB", wave(300, 3000.0, 200.0, -1.0)),
            ("CCC", wave(300, 8000.0, 900.0, 0.5)),
        ]);
        for p in presets() {
            run_scan(&df, &HashMap::new(), &p.request, None).unwrap_or_else(|e| panic!("{}: {e}", p.id));
        }
    }

    #[test]
    fn silent_accumulation_preset() {
        // Penutupan selalu di high (dana masuk), hari terakhir volume 3× tapi harga hanya +1%.
        let mut accum: Vec<Bar> = (0..30).map(|_| [5000.0, 5000.0, 4950.0, 5000.0, 1e6]).collect();
        accum[29] = [5000.0, 5050.0, 4990.0, 5050.0, 3e6];
        // Volume sama besar, tapi harga melonjak 8% (bukan akumulasi senyap).
        let mut jump: Vec<Bar> = (0..30).map(|_| [5000.0, 5000.0, 4950.0, 5000.0, 1e6]).collect();
        jump[29] = [5000.0, 5400.0, 4990.0, 5400.0, 3e6];
        let df = bars(&[("ACCU", accum), ("JUMP", jump)]);
        let preset = presets().into_iter().find(|p| p.id == "akumulasi-senyap").unwrap();
        let r = run_scan(&df, &HashMap::new(), &preset.request, None).unwrap();
        let tickers: Vec<&str> = r.rows.iter().map(|r| r.ticker.as_str()).collect();
        assert_eq!(tickers, ["ACCU"]);
    }

    #[test]
    fn volume_spike_preset_finds_spike() {
        let mut close = vec![5000.0; 30];
        close[29] = 5100.0;
        let mut volume = vec![1_000_000.0; 30];
        volume[29] = 5_000_000.0;
        let df = closes_volumes(&[("SPIKE", close, volume), ("QUIET", vec![5000.0; 30], vec![1_000_000.0; 30])]);
        let preset = presets().into_iter().find(|p| p.id == "volume-spike").unwrap();
        let r = run_scan(&df, &HashMap::new(), &preset.request, None).unwrap();
        let tickers: Vec<&str> = r.rows.iter().map(|r| r.ticker.as_str()).collect();
        assert_eq!(tickers, ["SPIKE"]);
    }
}
