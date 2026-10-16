//! Kondisi pasar dari IHSG. Backtest menunjukkan Best Buy jauh lebih baik saat IHSG di atas
//! SMA 50, jadi Best Buy hanya diberikan saat pasar sedang kuat.

use std::collections::BTreeMap;

pub const REGIME_SMA: usize = 50;

/// IHSG di atas SMA 50 per tanggal (hari sejak 1970-01-01).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarketRegime {
    strong: BTreeMap<i32, bool>,
}

impl MarketRegime {
    /// `closes`: penutupan IHSG per tanggal, boleh tidak terurut.
    pub fn from_closes(closes: &[(i32, f64)]) -> Self {
        let mut c: Vec<(i32, f64)> = closes.iter().copied().filter(|(_, v)| v.is_finite()).collect();
        c.sort_by_key(|(d, _)| *d);
        let mut strong = BTreeMap::new();
        let mut sum = 0.0;
        for i in 0..c.len() {
            sum += c[i].1;
            if i >= REGIME_SMA {
                sum -= c[i - REGIME_SMA].1;
            }
            if i + 1 >= REGIME_SMA {
                strong.insert(c[i].0, c[i].1 > sum / REGIME_SMA as f64);
            }
        }
        Self { strong }
    }

    /// Kondisi pada `day`, atau hari bursa terakhir sebelumnya. `None` kalau belum ada data.
    pub fn strong_at(&self, day: i32) -> Option<bool> {
        self.strong.range(..=day).next_back().map(|(_, s)| *s)
    }

    pub fn is_empty(&self) -> bool {
        self.strong.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn above_sma_with_carry_forward() {
        let mut closes: Vec<(i32, f64)> = (0..60).map(|i| (i * 2, 100.0 + i as f64)).collect();
        closes.push((120, 50.0));
        let r = MarketRegime::from_closes(&closes);
        assert_eq!(r.strong_at(0), None, "belum 50 hari");
        assert_eq!(r.strong_at(98), Some(true));
        // Hari libur (tanggal ganjil) memakai hari bursa sebelumnya.
        assert_eq!(r.strong_at(99), Some(true));
        assert_eq!(r.strong_at(120), Some(false));
        assert_eq!(r.strong_at(500), Some(false));
        assert!(MarketRegime::default().strong_at(10).is_none());
    }
}
