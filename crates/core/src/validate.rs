use shared::{Comparator, Condition, Indicator, Operand, OperandSource, ScanRequest, Universe};

use crate::CoreError;

pub const MAX_PERIOD: u16 = 500;
pub const MAX_CONDITIONS: usize = 10;
pub const MAX_LIMIT: u32 = 500;
pub const MAX_WITHIN: u8 = 20;
pub const MAX_OFFSET: u16 = 20;

fn invalid(msg: impl Into<String>) -> CoreError {
    CoreError::Invalid(msg.into())
}

pub fn validate(req: &ScanRequest) -> Result<(), CoreError> {
    let u = &req.universe;
    if req.conditions.is_empty() && u.signals.is_empty() && u.quality.is_empty() && u.min_win_rate.is_none() {
        return Err(invalid("Minimal harus ada 1 kondisi, filter sinyal, fundamental, atau win rate"));
    }
    if req.conditions.len() > MAX_CONDITIONS {
        return Err(invalid(format!("Maksimal {MAX_CONDITIONS} kondisi")));
    }
    if req.limit == 0 || req.limit > MAX_LIMIT {
        return Err(invalid(format!("Limit harus 1–{MAX_LIMIT}")));
    }
    for (i, c) in req.conditions.iter().enumerate() {
        condition(c).map_err(|m| invalid(format!("Kondisi #{}: {m}", i + 1)))?;
    }
    if let Some(sort) = &req.sort_by {
        if sort.operand.as_indicator().is_none() {
            return Err(invalid("Urutan harus memakai indikator"));
        }
        operand(&sort.operand).map_err(|m| invalid(format!("Urutan: {m}")))?;
    }
    universe(&req.universe).map_err(invalid)
}

fn condition(c: &Condition) -> Result<(), String> {
    operand(&c.left)?;
    operand(&c.right)?;
    if c.left.as_indicator().is_none() && c.right.as_indicator().is_none() {
        return Err("minimal salah satu sisi harus indikator".into());
    }
    match c.cmp {
        Comparator::CrossAbove { within } | Comparator::CrossBelow { within }
            if within == 0 || within > MAX_WITHIN =>
        {
            Err(format!("'dalam N bar' harus 1–{MAX_WITHIN}"))
        }
        _ => Ok(()),
    }
}

fn operand(o: &Operand) -> Result<(), String> {
    if !o.multiplier.is_finite() {
        return Err("pengali harus berupa angka".into());
    }
    if o.offset > MAX_OFFSET {
        return Err(format!("offset harus 0–{MAX_OFFSET} bar"));
    }
    match &o.source {
        OperandSource::Constant(v) if !v.is_finite() => Err("konstanta harus berupa angka".into()),
        OperandSource::Constant(_) => Ok(()),
        OperandSource::Indicator(ind) => indicator(ind),
    }
}

pub fn indicator(ind: &Indicator) -> Result<(), String> {
    use Indicator::*;
    let label = ind.label();
    if ind.periods().iter().any(|p| *p == 0 || *p > MAX_PERIOD) {
        return Err(format!("periode {label} harus 1–{MAX_PERIOD}"));
    }
    match ind {
        MacdLine { fast, slow, .. } | MacdSignal { fast, slow, .. } | MacdHist { fast, slow, .. }
            if fast >= slow =>
        {
            Err(format!("{label}: periode fast harus lebih kecil dari slow"))
        }
        BbUpper { std_x10, .. } | BbMiddle { std_x10, .. } | BbLower { std_x10, .. } | BbWidth { std_x10, .. }
            if *std_x10 == 0 || *std_x10 > 50 =>
        {
            Err(format!("{label}: deviasi standar harus 0.1–5.0"))
        }
        Supertrend { mult_x10, .. } if *mult_x10 == 0 || *mult_x10 > 100 => {
            Err(format!("{label}: pengali ATR harus 0.1–10.0"))
        }
        Adx { period } | PlusDi { period } | MinusDi { period } if *period < 2 => {
            Err(format!("{label}: periode minimal 2"))
        }
        IchimokuTenkan { tenkan, kijun, senkou_b }
        | IchimokuKijun { tenkan, kijun, senkou_b }
        | IchimokuSenkouA { tenkan, kijun, senkou_b }
        | IchimokuSenkouB { tenkan, kijun, senkou_b }
            if !(tenkan < kijun && kijun < senkou_b) =>
        {
            Err(format!("{label}: harus tenkan < kijun < senkou B"))
        }
        ParabolicSar { step_x1000, max_x1000 }
            if *step_x1000 == 0 || step_x1000 >= max_x1000 || *max_x1000 > 1000 =>
        {
            Err(format!("{label}: step harus > 0 dan lebih kecil dari max (maks 1.0)"))
        }
        LowestOf { inner, .. } if inner.is_recursive() || matches!(**inner, LowestOf { .. }) => {
            Err(format!("{label}: hanya bisa memakai indikator non-rekursif"))
        }
        LowestOf { inner, .. } => indicator(inner),
        _ => Ok(()),
    }
}

fn universe(u: &Universe) -> Result<(), String> {
    for (name, v) in [
        ("harga minimum", u.min_price),
        ("harga maksimum", u.max_price),
        ("nilai transaksi minimum", u.min_avg_value_20d),
    ] {
        if v.is_some_and(|v| !v.is_finite() || v < 0.0) {
            return Err(format!("{name} harus angka ≥ 0"));
        }
    }
    if let (Some(min), Some(max)) = (u.min_price, u.max_price)
        && min > max
    {
        return Err("harga minimum lebih besar dari harga maksimum".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{Indicator::*, SortSpec};

    fn ind(i: Indicator) -> Operand {
        Operand::indicator(i)
    }

    fn one(left: Operand, cmp: Comparator, right: Operand) -> ScanRequest {
        ScanRequest::new(vec![Condition::new(left, cmp, right)])
    }

    fn err(req: &ScanRequest) -> String {
        match validate(req) {
            Err(CoreError::Invalid(m)) => m,
            other => panic!("harusnya Invalid, dapat {other:?}"),
        }
    }

    #[test]
    fn accepts_valid_request() {
        let req = one(ind(Rsi { period: 14 }), Comparator::CrossAbove { within: 3 }, Operand::constant(30.0));
        assert!(validate(&req).is_ok());
    }

    #[test]
    fn rejects_bad_condition_counts_and_limit() {
        assert!(err(&ScanRequest::new(vec![])).contains("Minimal"));
        let c = Condition::new(ind(Close), Comparator::Gt, Operand::constant(1.0));
        assert!(err(&ScanRequest::new(vec![c.clone(); 11])).contains("Maksimal 10"));
        let mut req = ScanRequest::new(vec![c]);
        req.limit = 0;
        assert!(err(&req).contains("Limit"));
    }

    #[test]
    fn rejects_bad_periods_with_condition_number() {
        let m = err(&one(ind(Sma { period: 0 }), Comparator::Gt, ind(Close)));
        assert!(m.starts_with("Kondisi #1") && m.contains("SMA(0)"), "{m}");
        assert!(validate(&one(ind(Sma { period: 501 }), Comparator::Gt, ind(Close))).is_err());
        let macd = MacdLine { fast: 26, slow: 12, signal: 9 };
        assert!(validate(&one(ind(macd), Comparator::Gt, Operand::constant(0.0))).is_err());
        assert!(validate(&one(ind(Adx { period: 1 }), Comparator::Gt, Operand::constant(25.0))).is_err());
    }

    #[test]
    fn rejects_bad_scaled_params() {
        let ich = IchimokuKijun { tenkan: 26, kijun: 9, senkou_b: 52 };
        assert!(validate(&one(ind(Close), Comparator::Gt, ind(ich))).is_err());
        let psar = ParabolicSar { step_x1000: 200, max_x1000: 20 };
        assert!(validate(&one(ind(Close), Comparator::Gt, ind(psar))).is_err());
        let bb = BbUpper { period: 20, std_x10: 0 };
        assert!(validate(&one(ind(Close), Comparator::Gt, ind(bb))).is_err());
        let lowest = LowestOf { inner: Box::new(Adx { period: 14 }), period: 10 };
        assert!(validate(&one(ind(Close), Comparator::Gt, ind(lowest))).is_err());
    }

    #[test]
    fn rejects_bad_operands() {
        assert!(err(&one(Operand::constant(1.0), Comparator::Gt, Operand::constant(2.0))).contains("indikator"));
        assert!(validate(&one(ind(Close), Comparator::Gt, Operand::constant(f64::NAN))).is_err());
        assert!(validate(&one(ind(Close).shifted(21), Comparator::Gt, Operand::constant(1.0))).is_err());
        assert!(validate(&one(ind(Close), Comparator::CrossAbove { within: 0 }, ind(Sma { period: 5 }))).is_err());
        assert!(validate(&one(ind(Close), Comparator::CrossAbove { within: 21 }, ind(Sma { period: 5 }))).is_err());
    }

    #[test]
    fn rejects_bad_universe_and_sort() {
        let mut req = one(ind(Close), Comparator::Gt, Operand::constant(1.0));
        req.universe.min_price = Some(500.0);
        req.universe.max_price = Some(100.0);
        assert!(validate(&req).is_err());

        let mut req = one(ind(Close), Comparator::Gt, Operand::constant(1.0));
        req.sort_by = Some(SortSpec { operand: Operand::constant(1.0), descending: true });
        assert!(err(&req).starts_with("Urutan"));
    }
}
