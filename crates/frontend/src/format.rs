//! Format angka gaya Indonesia: titik untuk ribuan, koma untuk desimal.

fn group(int: u64) -> String {
    let digits = int.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Angka dengan `decimals` digit desimal, mis. `1.234,50`.
pub fn number(v: f64, decimals: usize) -> String {
    if !v.is_finite() {
        return "–".into();
    }
    let sign = if v < 0.0 { "-" } else { "" };
    let s = format!("{:.*}", decimals, v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let int = group(int.parse().unwrap_or(0));
    if frac.is_empty() { format!("{sign}{int}") } else { format!("{sign}{int},{frac}") }
}

/// Harga saham: tanpa desimal untuk harga bulat atau ≥ 100, dua desimal untuk harga kecil
/// yang tidak bulat (mis. hasil penyesuaian dividen).
pub fn price(v: f64) -> String {
    number(v, if v.abs() >= 100.0 || v.fract() == 0.0 { 0 } else { 2 })
}

pub fn pct(v: Option<f64>) -> String {
    match v {
        Some(v) if v.is_finite() => format!("{}{}%", if v > 0.0 { "+" } else { "" }, number(v, 2)),
        _ => "–".into(),
    }
}

/// Satuan ringkas: rb, jt, M (miliar), T (triliun).
pub fn compact(v: f64) -> String {
    let a = v.abs();
    let (div, unit) = match a {
        a if a >= 1e12 => (1e12, " T"),
        a if a >= 1e9 => (1e9, " M"),
        a if a >= 1e6 => (1e6, " jt"),
        a if a >= 1e3 => (1e3, " rb"),
        _ => (1.0, ""),
    };
    if div == 1.0 { number(v, 0) } else { format!("{}{unit}", number(v / div, 1)) }
}

pub fn rupiah(v: f64) -> String {
    format!("Rp{}", compact(v))
}

/// Indikator umum: dua desimal, ringkas kalau besar.
pub fn value(v: Option<f64>) -> String {
    match v {
        Some(v) if v.abs() >= 1e6 => compact(v),
        Some(v) if v.abs() >= 100.0 => number(v, 0),
        Some(v) => number(v, 2),
        None => "–".into(),
    }
}

/// Nama emiten tanpa "PT" dan "Tbk" untuk tabel yang sempit.
pub fn short_name(name: &str) -> String {
    let mut s = name.trim();
    for prefix in ["PT. ", "PT ", "Pt. ", "Pt "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim_start();
            break;
        }
    }
    for suffix in [", Tbk.", ", Tbk", " Tbk.", " Tbk", " TBK", " (Persero)"] {
        if let Some(rest) = s.strip_suffix(suffix) {
            s = rest.trim_end();
        }
    }
    s.to_string()
}

/// Kelas warna Tailwind untuk perubahan harga.
pub fn tone(v: Option<f64>) -> &'static str {
    match v {
        Some(v) if v > 0.0 => "text-up",
        Some(v) if v < 0.0 => "text-down",
        _ => "text-muted",
    }
}

/// `2026-10-15` → `15 Okt 2026`.
pub fn date(iso: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"];
    let parts: Vec<&str> = iso.get(..10).unwrap_or(iso).split('-').collect();
    match parts.as_slice() {
        [y, m, d] => {
            let month = m.parse::<usize>().ok().and_then(|m| MONTHS.get(m.wrapping_sub(1))).copied().unwrap_or("?");
            format!("{} {month} {y}", d.trim_start_matches('0'))
        }
        _ => iso.into(),
    }
}

/// RFC 3339 → `15 Okt 2026 20.37` (jam lokal server, cukup untuk riwayat update).
pub fn datetime(rfc3339: &str) -> String {
    let time = rfc3339.get(11..16).unwrap_or("").replace(':', ".");
    format!("{} {time}", date(rfc3339))
}

/// Ukuran byte, mis. `1,5 GB`.
pub fn bytes(v: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut x = v as f64;
    let mut i = 0;
    while x >= 1024.0 && i < units.len() - 1 {
        x /= 1024.0;
        i += 1;
    }
    format!("{} {}", number(x, if i == 0 || x >= 100.0 { 0 } else { 1 }), units[i])
}

/// Lama waktu, mis. `3 hari 4 jam` atau `12 menit`.
pub fn duration(secs: u64) -> String {
    let (d, h, m) = (secs / 86_400, secs % 86_400 / 3_600, secs % 3_600 / 60);
    match (d, h) {
        (0, 0) => format!("{m} menit"),
        (0, _) => format!("{h} jam {m} menit"),
        _ => format!("{d} hari {h} jam"),
    }
}
