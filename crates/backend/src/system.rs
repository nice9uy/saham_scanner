//! Kondisi mesin untuk halaman Server, dibaca langsung dari `/proc` dan `/sys` (Linux). Di dalam
//! container, angka CPU, memori, dan disk adalah milik mesin (VM) tempat container berjalan,
//! kecuali batas memori container dan pemakaian proses aplikasi.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use shared::{CpuDto, DiskDto, MemoryDto};

const SAMPLE_EVERY: Duration = Duration::from_secs(3);

/// Waktu CPU kumulatif satu baris `cpu`/`cpuN` di `/proc/stat`: (sibuk, total).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

/// Baris `cpu` (gabungan) lalu `cpu0..cpuN`.
pub fn parse_proc_stat(text: &str) -> Vec<CpuTimes> {
    text.lines()
        .filter(|l| l.starts_with("cpu"))
        .map(|l| {
            let v: Vec<u64> = l.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
            let total: u64 = v.iter().take(8).sum();
            // idle + iowait dianggap tidak sibuk.
            let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
            CpuTimes { busy: total.saturating_sub(idle), total }
        })
        .collect()
}

pub fn usage_pct(before: CpuTimes, after: CpuTimes) -> Option<f64> {
    let total = after.total.checked_sub(before.total)?;
    let busy = after.busy.checked_sub(before.busy)?;
    (total > 0).then(|| busy as f64 / total as f64 * 100.0)
}

/// Waktu CPU proses (utime + stime, satuan tick) dari `/proc/self/stat`.
pub fn parse_self_ticks(text: &str) -> Option<u64> {
    // Nama proses di dalam kurung bisa berisi spasi; field sesudah `)` dimulai dari field ke-3.
    let rest = text.rsplit_once(')')?.1;
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some(f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?)
}

/// Nilai `/proc/meminfo` dalam byte.
pub fn parse_meminfo(text: &str) -> HashMap<String, u64> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once(':')?;
            let kb: u64 = v.split_whitespace().next()?.parse().ok()?;
            Some((k.trim().to_string(), kb * 1024))
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
struct Sample {
    usage: Option<f64>,
    per_core: Vec<f64>,
    app: Option<f64>,
}

/// Mengukur pemakaian CPU di latar belakang (perlu dua bacaan berjarak).
#[derive(Clone)]
pub struct SystemMonitor {
    started: Instant,
    sample: Arc<RwLock<Sample>>,
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self { started: Instant::now(), sample: Arc::default() }
    }
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn cores() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

impl SystemMonitor {
    pub fn spawn(&self) {
        let sample = self.sample.clone();
        tokio::spawn(async move {
            // SAFETY: sysconf hanya membaca konfigurasi sistem.
            let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as f64;
            let mut prev = (parse_proc_stat(&read("/proc/stat")), parse_self_ticks(&read("/proc/self/stat")), Instant::now());
            loop {
                tokio::time::sleep(SAMPLE_EVERY).await;
                let now = (parse_proc_stat(&read("/proc/stat")), parse_self_ticks(&read("/proc/self/stat")), Instant::now());
                let pct = |i: usize| match (prev.0.get(i), now.0.get(i)) {
                    (Some(a), Some(b)) => usage_pct(*a, *b),
                    _ => None,
                };
                let elapsed = now.2.duration_since(prev.2).as_secs_f64();
                let app = match (prev.1, now.1) {
                    (Some(a), Some(b)) if elapsed > 0.0 => {
                        Some(b.saturating_sub(a) as f64 / hz / elapsed / cores() as f64 * 100.0)
                    }
                    _ => None,
                };
                *sample.write().expect("sample lock") = Sample {
                    usage: pct(0),
                    per_core: (1..now.0.len()).filter_map(pct).collect(),
                    app,
                };
                prev = now;
            }
        });
    }

    pub fn app_uptime(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn cpu(&self) -> CpuDto {
        let s = self.sample.read().expect("sample lock").clone();
        let load: Vec<f64> = read("/proc/loadavg").split_whitespace().take(3).filter_map(|x| x.parse().ok()).collect();
        CpuDto {
            model: read("/proc/cpuinfo")
                .lines()
                .find_map(|l| l.strip_prefix("model name").and_then(|r| r.split_once(':')).map(|(_, m)| m.trim().to_string()))
                .unwrap_or_default(),
            cores: cores(),
            usage_pct: s.usage,
            per_core: s.per_core,
            app_pct: s.app,
            load: [0, 1, 2].map(|i| load.get(i).copied().unwrap_or(0.0)),
        }
    }
}

pub fn memory() -> MemoryDto {
    let m = parse_meminfo(&read("/proc/meminfo"));
    let get = |k: &str| m.get(k).copied().unwrap_or(0);
    let rss = read("/proc/self/status")
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .map_or(0, |kb| kb * 1024);
    MemoryDto {
        total: get("MemTotal"),
        available: get("MemAvailable"),
        app_rss: rss,
        // cgroup v2 menulis "max" kalau container tidak dibatasi.
        limit: read("/sys/fs/cgroup/memory.max").trim().parse().ok(),
        swap_total: get("SwapTotal"),
        swap_free: get("SwapFree"),
    }
}

pub fn disk(mount: &str) -> Option<DiskDto> {
    let path = std::ffi::CString::new(mount).ok()?;
    // SAFETY: `statvfs` hanya mengisi struct yang kita sediakan; path berakhir NUL.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let block = s.f_frsize as u64;
    Some(DiskDto { mount: mount.into(), total: s.f_blocks as u64 * block, free: s.f_bavail as u64 * block })
}

pub fn uptime_secs() -> u64 {
    read("/proc/uptime").split_whitespace().next().and_then(|v| v.parse::<f64>().ok()).map_or(0, |v| v as u64)
}

pub fn hostname() -> String {
    read("/proc/sys/kernel/hostname").trim().to_string()
}

pub fn kernel() -> String {
    read("/proc/sys/kernel/osrelease").trim().to_string()
}

pub fn os_name() -> String {
    read("/etc/os-release")
        .lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_else(|| std::env::consts::OS.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_usage_from_proc_stat() {
        let a = parse_proc_stat("cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 50 0 50 400 0 0 0 0\ncpu1 50 0 50 400 0 0 0 0\nintr 1 2\n");
        let b = parse_proc_stat("cpu  200 0 200 1400 200 0 0 0 0 0\ncpu0 150 0 150 400 100 0 0 0\ncpu1 50 0 50 1000 100 0 0 0\n");
        assert_eq!(a.len(), 3);
        // Gabungan: sibuk +200 dari total +1000 (iowait dihitung menganggur).
        assert_eq!(usage_pct(a[0], b[0]), Some(20.0));
        assert!((usage_pct(a[1], b[1]).unwrap() - 66.666).abs() < 0.01);
        assert_eq!(usage_pct(a[2], b[2]), Some(0.0));
        assert_eq!(usage_pct(b[0], a[0]), None, "penghitung tidak boleh mundur");
    }

    #[test]
    fn process_ticks_and_meminfo() {
        let stat = "156 (scanner (be)) S 1 156 156 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 30 0";
        assert_eq!(parse_self_ticks(stat), Some(300));
        let m = parse_meminfo("MemTotal:       16121856 kB\nMemAvailable:   11020132 kB\nHugePages_Total: 0\n");
        assert_eq!(m["MemTotal"], 16_121_856 * 1024);
        assert_eq!(m["MemAvailable"], 11_020_132 * 1024);
    }

    #[test]
    fn root_disk_is_readable() {
        let d = disk("/").unwrap();
        assert!(d.total > 0 && d.free <= d.total);
        assert!(disk("/tidak/ada").is_none());
    }
}
