use serde::{Deserialize, Serialize};

/// Indikator teknikal yang bisa dipakai di aturan scan dan chart.
/// Parameter pecahan disimpan sebagai integer berskala (`std_x10 = 20` → 2.0)
/// supaya tipe ini bisa `Eq + Hash` dan key kolomnya stabil.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Indicator {
    Open,
    High,
    Low,
    Close,
    Volume,
    ChangePct { period: u16 },
    Sma { period: u16 },
    Ema { period: u16 },
    VolumeSma { period: u16 },
    Rsi { period: u16 },
    MacdLine { fast: u16, slow: u16, signal: u16 },
    MacdSignal { fast: u16, slow: u16, signal: u16 },
    MacdHist { fast: u16, slow: u16, signal: u16 },
    BbUpper { period: u16, std_x10: u16 },
    BbMiddle { period: u16, std_x10: u16 },
    BbLower { period: u16, std_x10: u16 },
    BbWidth { period: u16, std_x10: u16 },
    Atr { period: u16 },
    StochK { period: u16, smooth: u16 },
    StochD { period: u16, smooth: u16, d: u16 },
    HighestHigh { period: u16 },
    LowestLow { period: u16 },
    /// Nilai minimum `inner` dalam `period` bar terakhir.
    LowestOf { inner: Box<Indicator>, period: u16 },
    Supertrend { period: u16, mult_x10: u16 },
    Adx { period: u16 },
    PlusDi { period: u16 },
    MinusDi { period: u16 },
    IchimokuTenkan { tenkan: u16, kijun: u16, senkou_b: u16 },
    IchimokuKijun { tenkan: u16, kijun: u16, senkou_b: u16 },
    IchimokuSenkouA { tenkan: u16, kijun: u16, senkou_b: u16 },
    IchimokuSenkouB { tenkan: u16, kijun: u16, senkou_b: u16 },
    ParabolicSar { step_x1000: u16, max_x1000: u16 },
    /// On-Balance Volume: volume kumulatif, ditambah saat harga naik dan dikurangi saat turun.
    Obv,
    /// Accumulation/Distribution line (Chaikin).
    AccDist,
    /// Chaikin Money Flow, −1..1: aliran dana masuk (+) atau keluar (−) dalam `period` hari.
    Cmf { period: u16 },
    /// Money Flow Index 0..100: RSI berbobot volume.
    Mfi { period: u16 },
    /// Volume hari ini dibagi rata-rata volume `period` hari sebelumnya.
    RelVolume { period: u16 },
}

fn x10(v: u16) -> String {
    format!("{:.1}", f64::from(v) / 10.0)
}

fn x1000(v: u16) -> String {
    format!("{:.3}", f64::from(v) / 1000.0)
}

impl Indicator {
    /// Nama kolom yang stabil, dipakai backend dan frontend.
    pub fn key(&self) -> String {
        use Indicator::*;
        match self {
            Open => "open".into(),
            High => "high".into(),
            Low => "low".into(),
            Close => "close".into(),
            Volume => "volume".into(),
            ChangePct { period } => format!("change_pct_{period}"),
            Sma { period } => format!("sma_{period}"),
            Ema { period } => format!("ema_{period}"),
            VolumeSma { period } => format!("volume_sma_{period}"),
            Rsi { period } => format!("rsi_{period}"),
            MacdLine { fast, slow, signal } => format!("macd_line_{fast}_{slow}_{signal}"),
            MacdSignal { fast, slow, signal } => format!("macd_signal_{fast}_{slow}_{signal}"),
            MacdHist { fast, slow, signal } => format!("macd_hist_{fast}_{slow}_{signal}"),
            BbUpper { period, std_x10 } => format!("bb_upper_{period}_{std_x10}"),
            BbMiddle { period, std_x10 } => format!("bb_middle_{period}_{std_x10}"),
            BbLower { period, std_x10 } => format!("bb_lower_{period}_{std_x10}"),
            BbWidth { period, std_x10 } => format!("bb_width_{period}_{std_x10}"),
            Atr { period } => format!("atr_{period}"),
            StochK { period, smooth } => format!("stoch_k_{period}_{smooth}"),
            StochD { period, smooth, d } => format!("stoch_d_{period}_{smooth}_{d}"),
            HighestHigh { period } => format!("highest_high_{period}"),
            LowestLow { period } => format!("lowest_low_{period}"),
            LowestOf { inner, period } => format!("lowest_{period}_of_{}", inner.key()),
            Supertrend { period, mult_x10 } => format!("supertrend_{period}_{mult_x10}"),
            Adx { period } => format!("adx_{period}"),
            PlusDi { period } => format!("plus_di_{period}"),
            MinusDi { period } => format!("minus_di_{period}"),
            IchimokuTenkan { tenkan, kijun, senkou_b } => {
                format!("ichimoku_tenkan_{tenkan}_{kijun}_{senkou_b}")
            }
            IchimokuKijun { tenkan, kijun, senkou_b } => {
                format!("ichimoku_kijun_{tenkan}_{kijun}_{senkou_b}")
            }
            IchimokuSenkouA { tenkan, kijun, senkou_b } => {
                format!("ichimoku_senkou_a_{tenkan}_{kijun}_{senkou_b}")
            }
            IchimokuSenkouB { tenkan, kijun, senkou_b } => {
                format!("ichimoku_senkou_b_{tenkan}_{kijun}_{senkou_b}")
            }
            ParabolicSar { step_x1000, max_x1000 } => format!("psar_{step_x1000}_{max_x1000}"),
            Obv => "obv".into(),
            AccDist => "acc_dist".into(),
            Cmf { period } => format!("cmf_{period}"),
            Mfi { period } => format!("mfi_{period}"),
            RelVolume { period } => format!("rel_volume_{period}"),
        }
    }

    /// Nama yang mudah dibaca untuk header tabel dan UI.
    pub fn label(&self) -> String {
        use Indicator::*;
        match self {
            Open => "Open".into(),
            High => "High".into(),
            Low => "Low".into(),
            Close => "Close".into(),
            Volume => "Volume".into(),
            ChangePct { period } => format!("Change%({period})"),
            Sma { period } => format!("SMA({period})"),
            Ema { period } => format!("EMA({period})"),
            VolumeSma { period } => format!("Vol SMA({period})"),
            Rsi { period } => format!("RSI({period})"),
            MacdLine { fast, slow, signal } => format!("MACD({fast}, {slow}, {signal})"),
            MacdSignal { fast, slow, signal } => format!("MACD Signal({fast}, {slow}, {signal})"),
            MacdHist { fast, slow, signal } => format!("MACD Hist({fast}, {slow}, {signal})"),
            BbUpper { period, std_x10 } => format!("BB Upper({period}, {})", x10(*std_x10)),
            BbMiddle { period, std_x10 } => format!("BB Middle({period}, {})", x10(*std_x10)),
            BbLower { period, std_x10 } => format!("BB Lower({period}, {})", x10(*std_x10)),
            BbWidth { period, std_x10 } => format!("BB Width({period}, {})", x10(*std_x10)),
            Atr { period } => format!("ATR({period})"),
            StochK { period, smooth } => format!("Stoch %K({period}, {smooth})"),
            StochD { period, smooth, d } => format!("Stoch %D({period}, {smooth}, {d})"),
            HighestHigh { period } => format!("Highest High({period})"),
            LowestLow { period } => format!("Lowest Low({period})"),
            LowestOf { inner, period } => format!("Lowest {period} of {}", inner.label()),
            Supertrend { period, mult_x10 } => format!("Supertrend({period}, {})", x10(*mult_x10)),
            Adx { period } => format!("ADX({period})"),
            PlusDi { period } => format!("+DI({period})"),
            MinusDi { period } => format!("-DI({period})"),
            IchimokuTenkan { tenkan, kijun, senkou_b } => format!("Tenkan({tenkan}, {kijun}, {senkou_b})"),
            IchimokuKijun { tenkan, kijun, senkou_b } => format!("Kijun({tenkan}, {kijun}, {senkou_b})"),
            IchimokuSenkouA { tenkan, kijun, senkou_b } => format!("Senkou A({tenkan}, {kijun}, {senkou_b})"),
            IchimokuSenkouB { tenkan, kijun, senkou_b } => format!("Senkou B({tenkan}, {kijun}, {senkou_b})"),
            ParabolicSar { step_x1000, max_x1000 } => {
                format!("PSAR({}, {})", x1000(*step_x1000), x1000(*max_x1000))
            }
            Obv => "OBV".into(),
            AccDist => "A/D".into(),
            Cmf { period } => format!("CMF({period})"),
            Mfi { period } => format!("MFI({period})"),
            RelVolume { period } => format!("Rel. Volume({period})"),
        }
    }

    /// Jumlah bar minimum agar indikator punya nilai.
    pub fn lookback(&self) -> usize {
        use Indicator::*;
        let n = |v: &u16| usize::from(*v);
        match self {
            Open | High | Low | Close | Volume | Obv | AccDist => 1,
            ParabolicSar { .. } => 2,
            ChangePct { period }
            | Rsi { period }
            | PlusDi { period }
            | MinusDi { period }
            | Mfi { period }
            | RelVolume { period } => n(period) + 1,
            Sma { period }
            | Ema { period }
            | VolumeSma { period }
            | Atr { period }
            | HighestHigh { period }
            | LowestLow { period }
            | Cmf { period }
            | Supertrend { period, .. }
            | BbUpper { period, .. }
            | BbMiddle { period, .. }
            | BbLower { period, .. }
            | BbWidth { period, .. } => n(period),
            MacdLine { slow, .. } => n(slow),
            MacdSignal { slow, signal, .. } | MacdHist { slow, signal, .. } => n(slow) + n(signal) - 1,
            StochK { period, smooth } => n(period) + n(smooth) - 1,
            StochD { period, smooth, d } => n(period) + n(smooth) + n(d) - 2,
            LowestOf { inner, period } => inner.lookback() + n(period) - 1,
            Adx { period } => 2 * n(period),
            IchimokuTenkan { tenkan, .. } => n(tenkan),
            IchimokuKijun { kijun, .. } => n(kijun),
            IchimokuSenkouA { kijun, .. } => 2 * n(kijun),
            IchimokuSenkouB { kijun, senkou_b, .. } => n(senkou_b) + n(kijun),
        }
    }

    /// Semua parameter periode, untuk validasi rentang.
    pub fn periods(&self) -> Vec<u16> {
        use Indicator::*;
        match self {
            Open | High | Low | Close | Volume | ParabolicSar { .. } | Obv | AccDist => vec![],
            ChangePct { period }
            | Sma { period }
            | Ema { period }
            | VolumeSma { period }
            | Rsi { period }
            | Atr { period }
            | HighestHigh { period }
            | LowestLow { period }
            | Adx { period }
            | PlusDi { period }
            | MinusDi { period }
            | Cmf { period }
            | Mfi { period }
            | RelVolume { period }
            | Supertrend { period, .. }
            | BbUpper { period, .. }
            | BbMiddle { period, .. }
            | BbLower { period, .. }
            | BbWidth { period, .. } => vec![*period],
            MacdLine { fast, slow, signal }
            | MacdSignal { fast, slow, signal }
            | MacdHist { fast, slow, signal } => vec![*fast, *slow, *signal],
            StochK { period, smooth } => vec![*period, *smooth],
            StochD { period, smooth, d } => vec![*period, *smooth, *d],
            LowestOf { inner, period } => {
                let mut v = inner.periods();
                v.push(*period);
                v
            }
            IchimokuTenkan { tenkan, kijun, senkou_b }
            | IchimokuKijun { tenkan, kijun, senkou_b }
            | IchimokuSenkouA { tenkan, kijun, senkou_b }
            | IchimokuSenkouB { tenkan, kijun, senkou_b } => vec![*tenkan, *kijun, *senkou_b],
        }
    }

    /// Indikator rekursif dihitung per ticker di luar ekspresi Polars.
    pub fn is_recursive(&self) -> bool {
        use Indicator::*;
        matches!(
            self,
            Supertrend { .. }
                | Adx { .. }
                | PlusDi { .. }
                | MinusDi { .. }
                | IchimokuTenkan { .. }
                | IchimokuKijun { .. }
                | IchimokuSenkouA { .. }
                | IchimokuSenkouB { .. }
                | ParabolicSar { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Indicator::*;
    use super::*;

    #[test]
    fn keys_are_stable() {
        assert_eq!(Sma { period: 50 }.key(), "sma_50");
        assert_eq!(MacdSignal { fast: 12, slow: 26, signal: 9 }.key(), "macd_signal_12_26_9");
        assert_eq!(BbUpper { period: 20, std_x10: 20 }.key(), "bb_upper_20_20");
        let lowest = LowestOf { inner: Box::new(BbWidth { period: 20, std_x10: 20 }), period: 120 };
        assert_eq!(lowest.key(), "lowest_120_of_bb_width_20_20");
        assert_eq!(Close.key(), "close");
        assert_eq!(Obv.key(), "obv");
        assert_eq!(Cmf { period: 20 }.key(), "cmf_20");
        assert_eq!(RelVolume { period: 20 }.key(), "rel_volume_20");
    }

    #[test]
    fn labels_are_readable() {
        assert_eq!(Sma { period: 50 }.label(), "SMA(50)");
        assert_eq!(BbUpper { period: 20, std_x10: 20 }.label(), "BB Upper(20, 2.0)");
        assert_eq!(Supertrend { period: 10, mult_x10: 30 }.label(), "Supertrend(10, 3.0)");
        assert_eq!(ParabolicSar { step_x1000: 20, max_x1000: 200 }.label(), "PSAR(0.020, 0.200)");
        assert_eq!(Mfi { period: 14 }.label(), "MFI(14)");
    }

    #[test]
    fn lookback_and_periods() {
        assert_eq!(MacdSignal { fast: 12, slow: 26, signal: 9 }.lookback(), 34);
        assert_eq!(Rsi { period: 14 }.lookback(), 15);
        let lowest = LowestOf { inner: Box::new(Sma { period: 20 }), period: 120 };
        assert_eq!(lowest.lookback(), 139);
        assert_eq!(lowest.periods(), vec![20, 120]);
        assert!(Close.periods().is_empty());
        assert!(Obv.periods().is_empty());
        assert_eq!(RelVolume { period: 20 }.lookback(), 21);
    }

    #[test]
    fn recursive_flag() {
        assert!(Supertrend { period: 10, mult_x10: 30 }.is_recursive());
        assert!(Adx { period: 14 }.is_recursive());
        assert!(!Sma { period: 10 }.is_recursive());
        assert!(!Obv.is_recursive());
    }

    #[test]
    fn json_shape() {
        let json = serde_json::to_string(&Sma { period: 50 }).unwrap();
        assert_eq!(json, r#"{"kind":"sma","period":50}"#);
        let back: Indicator = serde_json::from_str(r#"{"kind":"close"}"#).unwrap();
        assert_eq!(back, Close);
        let nested: Indicator = serde_json::from_str(
            r#"{"kind":"lowest_of","inner":{"kind":"bb_width","period":20,"std_x10":20},"period":120}"#,
        )
        .unwrap();
        assert_eq!(nested.key(), "lowest_120_of_bb_width_20_20");
        let flow: Indicator = serde_json::from_str(r#"{"kind":"rel_volume","period":20}"#).unwrap();
        assert_eq!(flow, RelVolume { period: 20 });
    }
}
