pub mod recursive;
pub mod vector;

use polars::prelude::*;
use shared::Indicator;

use crate::CoreError;

/// Menambahkan kolom `ind.key()` untuk setiap indikator. Kolom yang sudah ada dilewati.
/// `df` harus terurut per (ticker, date).
pub fn add_indicators(df: DataFrame, inds: &[Indicator]) -> Result<DataFrame, CoreError> {
    let (recursive, vectorized): (Vec<&Indicator>, Vec<&Indicator>) =
        inds.iter().partition(|i| i.is_recursive());

    let mut exprs = Vec::new();
    for ind in vectorized {
        let key = ind.key();
        if df.get_column_index(&key).is_none() {
            exprs.push(vector::expr(ind)?.alias(key));
        }
    }
    let mut df = if exprs.is_empty() { df } else { df.lazy().with_columns(exprs).collect()? };
    recursive::add_columns(&mut df, &recursive)?;
    Ok(df)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn adds_vector_and_recursive_columns_once() {
        let df = closes(&[("A", wave(40, 100.0, 5.0, 0.2))]);
        let inds = vec![
            Indicator::Close,
            Indicator::Sma { period: 3 },
            Indicator::Supertrend { period: 10, mult_x10: 30 },
        ];
        let out = add_indicators(df, &inds).unwrap();
        let names: Vec<String> = out.get_column_names().iter().map(|n| n.to_string()).collect();
        assert_eq!(names.iter().filter(|n| *n == "close").count(), 1);
        assert!(names.contains(&"sma_3".to_string()));
        assert!(names.contains(&"supertrend_10_30".to_string()));
    }
}
