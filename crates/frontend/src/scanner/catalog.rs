//! Daftar indikator yang bisa dipilih di penyusun aturan, beserta parameternya.

use shared::Indicator;

pub struct Param {
    pub name: &'static str,
    pub default: u16,
    /// Nilai disimpan sebagai `tampilan × scale` (mis. 2.0σ disimpan 20).
    pub scale: u16,
}

pub struct Kind {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub params: &'static [Param],
}

const fn p(name: &'static str, default: u16) -> Param {
    Param { name, default, scale: 1 }
}

const fn ps(name: &'static str, default: u16, scale: u16) -> Param {
    Param { name, default, scale }
}

const MACD: &[Param] = &[p("fast", 12), p("slow", 26), p("signal", 9)];
const BB: &[Param] = &[p("periode", 20), ps("σ", 20, 10)];
const ICHI: &[Param] = &[p("tenkan", 9), p("kijun", 26), p("senkou B", 52)];

pub const KINDS: &[Kind] = &[
    Kind { id: "close", label: "Harga penutupan", group: "Harga", params: &[] },
    Kind { id: "open", label: "Harga pembukaan", group: "Harga", params: &[] },
    Kind { id: "high", label: "Harga tertinggi", group: "Harga", params: &[] },
    Kind { id: "low", label: "Harga terendah", group: "Harga", params: &[] },
    Kind { id: "change_pct", label: "Perubahan %", group: "Harga", params: &[p("hari", 1)] },
    Kind { id: "highest_high", label: "High tertinggi", group: "Harga", params: &[p("hari", 20)] },
    Kind { id: "lowest_low", label: "Low terendah", group: "Harga", params: &[p("hari", 20)] },
    Kind { id: "volume", label: "Volume", group: "Volume", params: &[] },
    Kind { id: "volume_sma", label: "Rata-rata volume", group: "Volume", params: &[p("hari", 20)] },
    Kind { id: "sma", label: "SMA", group: "Tren", params: &[p("periode", 20)] },
    Kind { id: "ema", label: "EMA", group: "Tren", params: &[p("periode", 20)] },
    Kind { id: "supertrend", label: "Supertrend", group: "Tren", params: &[p("periode", 10), ps("× ATR", 30, 10)] },
    Kind { id: "psar", label: "Parabolic SAR", group: "Tren", params: &[ps("step", 20, 1000), ps("max", 200, 1000)] },
    Kind { id: "ichimoku_tenkan", label: "Ichimoku Tenkan", group: "Tren", params: ICHI },
    Kind { id: "ichimoku_kijun", label: "Ichimoku Kijun", group: "Tren", params: ICHI },
    Kind { id: "ichimoku_senkou_a", label: "Ichimoku Senkou A", group: "Tren", params: ICHI },
    Kind { id: "ichimoku_senkou_b", label: "Ichimoku Senkou B", group: "Tren", params: ICHI },
    Kind { id: "adx", label: "ADX", group: "Tren", params: &[p("periode", 14)] },
    Kind { id: "plus_di", label: "+DI", group: "Tren", params: &[p("periode", 14)] },
    Kind { id: "minus_di", label: "−DI", group: "Tren", params: &[p("periode", 14)] },
    Kind { id: "rsi", label: "RSI", group: "Momentum", params: &[p("periode", 14)] },
    Kind { id: "macd_line", label: "MACD", group: "Momentum", params: MACD },
    Kind { id: "macd_signal", label: "MACD signal", group: "Momentum", params: MACD },
    Kind { id: "macd_hist", label: "MACD histogram", group: "Momentum", params: MACD },
    Kind { id: "stoch_k", label: "Stochastic %K", group: "Momentum", params: &[p("periode", 14), p("smooth", 3)] },
    Kind { id: "stoch_d", label: "Stochastic %D", group: "Momentum", params: &[p("periode", 14), p("smooth", 3), p("%D", 3)] },
    Kind { id: "bb_upper", label: "Bollinger atas", group: "Volatilitas", params: BB },
    Kind { id: "bb_middle", label: "Bollinger tengah", group: "Volatilitas", params: BB },
    Kind { id: "bb_lower", label: "Bollinger bawah", group: "Volatilitas", params: BB },
    Kind { id: "bb_width", label: "Lebar Bollinger", group: "Volatilitas", params: BB },
    Kind {
        id: "bb_width_lowest",
        label: "Lebar Bollinger terendah",
        group: "Volatilitas",
        params: &[p("periode", 20), ps("σ", 20, 10), p("dalam hari", 120)],
    },
    Kind { id: "atr", label: "ATR", group: "Volatilitas", params: &[p("periode", 14)] },
    Kind { id: "rel_volume", label: "Volume relatif (× rata-rata)", group: "Aliran dana", params: &[p("hari", 20)] },
    Kind { id: "cmf", label: "Chaikin Money Flow", group: "Aliran dana", params: &[p("hari", 20)] },
    Kind { id: "mfi", label: "Money Flow Index", group: "Aliran dana", params: &[p("periode", 14)] },
    Kind { id: "obv", label: "On-Balance Volume", group: "Aliran dana", params: &[] },
    Kind { id: "acc_dist", label: "Accumulation/Distribution", group: "Aliran dana", params: &[] },
];

pub const GROUPS: &[&str] = &["Harga", "Volume", "Aliran dana", "Tren", "Momentum", "Volatilitas"];

pub fn kind(id: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.id == id)
}

pub fn defaults(id: &str) -> Vec<u16> {
    kind(id).map(|k| k.params.iter().map(|p| p.default).collect()).unwrap_or_default()
}

/// Membuat `Indicator` dari id katalog dan parameter (nilai tersimpan, sudah dikali scale).
pub fn build(id: &str, a: &[u16]) -> Option<Indicator> {
    use Indicator::*;
    let g = |i: usize| a.get(i).copied();
    Some(match id {
        "close" => Close,
        "open" => Open,
        "high" => High,
        "low" => Low,
        "volume" => Volume,
        "change_pct" => ChangePct { period: g(0)? },
        "highest_high" => HighestHigh { period: g(0)? },
        "lowest_low" => LowestLow { period: g(0)? },
        "volume_sma" => VolumeSma { period: g(0)? },
        "sma" => Sma { period: g(0)? },
        "ema" => Ema { period: g(0)? },
        "supertrend" => Supertrend { period: g(0)?, mult_x10: g(1)? },
        "psar" => ParabolicSar { step_x1000: g(0)?, max_x1000: g(1)? },
        "ichimoku_tenkan" => IchimokuTenkan { tenkan: g(0)?, kijun: g(1)?, senkou_b: g(2)? },
        "ichimoku_kijun" => IchimokuKijun { tenkan: g(0)?, kijun: g(1)?, senkou_b: g(2)? },
        "ichimoku_senkou_a" => IchimokuSenkouA { tenkan: g(0)?, kijun: g(1)?, senkou_b: g(2)? },
        "ichimoku_senkou_b" => IchimokuSenkouB { tenkan: g(0)?, kijun: g(1)?, senkou_b: g(2)? },
        "adx" => Adx { period: g(0)? },
        "plus_di" => PlusDi { period: g(0)? },
        "minus_di" => MinusDi { period: g(0)? },
        "rsi" => Rsi { period: g(0)? },
        "macd_line" => MacdLine { fast: g(0)?, slow: g(1)?, signal: g(2)? },
        "macd_signal" => MacdSignal { fast: g(0)?, slow: g(1)?, signal: g(2)? },
        "macd_hist" => MacdHist { fast: g(0)?, slow: g(1)?, signal: g(2)? },
        "stoch_k" => StochK { period: g(0)?, smooth: g(1)? },
        "stoch_d" => StochD { period: g(0)?, smooth: g(1)?, d: g(2)? },
        "bb_upper" => BbUpper { period: g(0)?, std_x10: g(1)? },
        "bb_middle" => BbMiddle { period: g(0)?, std_x10: g(1)? },
        "bb_lower" => BbLower { period: g(0)?, std_x10: g(1)? },
        "bb_width" => BbWidth { period: g(0)?, std_x10: g(1)? },
        "bb_width_lowest" => LowestOf { inner: Box::new(BbWidth { period: g(0)?, std_x10: g(1)? }), period: g(2)? },
        "atr" => Atr { period: g(0)? },
        "rel_volume" => RelVolume { period: g(0)? },
        "cmf" => Cmf { period: g(0)? },
        "mfi" => Mfi { period: g(0)? },
        "obv" => Obv,
        "acc_dist" => AccDist,
        _ => return None,
    })
}

/// Kebalikan `build`. `None` untuk kombinasi yang tidak ada di katalog.
pub fn decompose(ind: &Indicator) -> Option<(&'static str, Vec<u16>)> {
    use Indicator::*;
    Some(match ind {
        Close => ("close", vec![]),
        Open => ("open", vec![]),
        High => ("high", vec![]),
        Low => ("low", vec![]),
        Volume => ("volume", vec![]),
        ChangePct { period } => ("change_pct", vec![*period]),
        HighestHigh { period } => ("highest_high", vec![*period]),
        LowestLow { period } => ("lowest_low", vec![*period]),
        VolumeSma { period } => ("volume_sma", vec![*period]),
        Sma { period } => ("sma", vec![*period]),
        Ema { period } => ("ema", vec![*period]),
        Supertrend { period, mult_x10 } => ("supertrend", vec![*period, *mult_x10]),
        ParabolicSar { step_x1000, max_x1000 } => ("psar", vec![*step_x1000, *max_x1000]),
        IchimokuTenkan { tenkan, kijun, senkou_b } => ("ichimoku_tenkan", vec![*tenkan, *kijun, *senkou_b]),
        IchimokuKijun { tenkan, kijun, senkou_b } => ("ichimoku_kijun", vec![*tenkan, *kijun, *senkou_b]),
        IchimokuSenkouA { tenkan, kijun, senkou_b } => ("ichimoku_senkou_a", vec![*tenkan, *kijun, *senkou_b]),
        IchimokuSenkouB { tenkan, kijun, senkou_b } => ("ichimoku_senkou_b", vec![*tenkan, *kijun, *senkou_b]),
        Adx { period } => ("adx", vec![*period]),
        PlusDi { period } => ("plus_di", vec![*period]),
        MinusDi { period } => ("minus_di", vec![*period]),
        Rsi { period } => ("rsi", vec![*period]),
        MacdLine { fast, slow, signal } => ("macd_line", vec![*fast, *slow, *signal]),
        MacdSignal { fast, slow, signal } => ("macd_signal", vec![*fast, *slow, *signal]),
        MacdHist { fast, slow, signal } => ("macd_hist", vec![*fast, *slow, *signal]),
        StochK { period, smooth } => ("stoch_k", vec![*period, *smooth]),
        StochD { period, smooth, d } => ("stoch_d", vec![*period, *smooth, *d]),
        BbUpper { period, std_x10 } => ("bb_upper", vec![*period, *std_x10]),
        BbMiddle { period, std_x10 } => ("bb_middle", vec![*period, *std_x10]),
        BbLower { period, std_x10 } => ("bb_lower", vec![*period, *std_x10]),
        BbWidth { period, std_x10 } => ("bb_width", vec![*period, *std_x10]),
        LowestOf { inner, period } => match inner.as_ref() {
            BbWidth { period: p, std_x10 } => ("bb_width_lowest", vec![*p, *std_x10, *period]),
            _ => return None,
        },
        Atr { period } => ("atr", vec![*period]),
        RelVolume { period } => ("rel_volume", vec![*period]),
        Cmf { period } => ("cmf", vec![*period]),
        Mfi { period } => ("mfi", vec![*period]),
        Obv => ("obv", vec![]),
        AccDist => ("acc_dist", vec![]),
    })
}
