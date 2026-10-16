//! Parameter `indicators` endpoint candles, mis. `sma:50,bb:20:20,macd:12:26:9`.

use shared::Indicator;

const MAX_TOKENS: usize = 20;

/// Satu token bisa menghasilkan beberapa garis (bb → upper/middle/lower, dst).
pub fn parse_indicators(spec: &str) -> Result<Vec<Indicator>, String> {
    use Indicator::*;
    let tokens: Vec<&str> = spec.split(',').map(str::trim).filter(|t| !t.is_empty()).collect();
    if tokens.len() > MAX_TOKENS {
        return Err(format!("Maksimal {MAX_TOKENS} indikator per chart"));
    }
    let mut out: Vec<Indicator> = Vec::new();
    for token in tokens {
        let mut parts = token.split(':');
        let name = parts.next().unwrap_or_default().to_lowercase();
        let args: Vec<u16> = parts
            .map(|p| p.parse::<u16>())
            .collect::<Result<_, _>>()
            .map_err(|_| format!("Parameter indikator '{token}' harus angka"))?;
        let bad = || format!("Format indikator '{token}' tidak valid");
        let lines: Vec<Indicator> = match (name.as_str(), args.as_slice()) {
            ("sma", [p]) => vec![Sma { period: *p }],
            ("ema", [p]) => vec![Ema { period: *p }],
            ("rsi", [p]) => vec![Rsi { period: *p }],
            ("atr", [p]) => vec![Atr { period: *p }],
            ("volume_sma", [p]) => vec![VolumeSma { period: *p }],
            ("obv", []) => vec![Obv],
            ("ad", []) => vec![AccDist],
            ("cmf", [p]) => vec![Cmf { period: *p }],
            ("mfi", [p]) => vec![Mfi { period: *p }],
            ("rvol", [p]) => vec![RelVolume { period: *p }],
            ("bb", [p, s]) => vec![
                BbUpper { period: *p, std_x10: *s },
                BbMiddle { period: *p, std_x10: *s },
                BbLower { period: *p, std_x10: *s },
            ],
            ("macd", [f, s, g]) => vec![
                MacdLine { fast: *f, slow: *s, signal: *g },
                MacdSignal { fast: *f, slow: *s, signal: *g },
                MacdHist { fast: *f, slow: *s, signal: *g },
            ],
            ("stoch", [p, s, d]) => vec![StochK { period: *p, smooth: *s }, StochD { period: *p, smooth: *s, d: *d }],
            ("supertrend", [p, m]) => vec![Supertrend { period: *p, mult_x10: *m }],
            ("adx", [p]) => vec![Adx { period: *p }, PlusDi { period: *p }, MinusDi { period: *p }],
            ("psar", [s, m]) => vec![ParabolicSar { step_x1000: *s, max_x1000: *m }],
            ("ichimoku", [t, k, b]) => vec![
                IchimokuTenkan { tenkan: *t, kijun: *k, senkou_b: *b },
                IchimokuKijun { tenkan: *t, kijun: *k, senkou_b: *b },
                IchimokuSenkouA { tenkan: *t, kijun: *k, senkou_b: *b },
                IchimokuSenkouB { tenkan: *t, kijun: *k, senkou_b: *b },
            ],
            _ => return Err(bad()),
        };
        for ind in lines {
            scanner_core::validate::indicator(&ind)?;
            if !out.contains(&ind) {
                out.push(ind);
            }
        }
    }
    Ok(out)
}

/// Jumlah bar untuk parameter `range`.
pub fn range_bars(range: Option<&str>) -> Result<usize, String> {
    match range.unwrap_or("1y") {
        "3m" => Ok(63),
        "6m" => Ok(126),
        "1y" => Ok(250),
        "3y" => Ok(750),
        "5y" => Ok(1250),
        other => Err(format!("Range '{other}' tidak dikenal (pilihan: 3m, 6m, 1y, 3y, 5y)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_groups() {
        let inds = parse_indicators("sma:50, bb:20:20,macd:12:26:9,sma:50").unwrap();
        assert_eq!(inds.len(), 7);
        assert_eq!(inds[0], Indicator::Sma { period: 50 });
        assert!(parse_indicators("").unwrap().is_empty());
        assert_eq!(parse_indicators("obv,ad,cmf:20,mfi:14,rvol:20").unwrap().len(), 5);
        assert!(parse_indicators("foo:1").is_err());
        assert!(parse_indicators("sma:x").is_err());
        assert!(parse_indicators("sma:0").is_err());
        assert!(parse_indicators("bb:20").is_err());
        assert_eq!(range_bars(None), Ok(250));
        assert!(range_bars(Some("2y")).is_err());
    }
}
