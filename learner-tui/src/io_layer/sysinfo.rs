use std::fs;
use std::path::Path;

/// Live system resource information collected from /proc (Linux).
#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub cpu_count: usize,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub load_avg_1: f64,
    pub load_avg_5: f64,
    pub load_avg_15: f64,
    pub cpu_usage_pct: f64,
    pub ram_usage_pct: f64,
}

impl Default for SystemInfo {
    fn default() -> Self {
        Self {
            cpu_count: 1,
            total_ram_mb: 0,
            available_ram_mb: 0,
            load_avg_1: 0.0,
            load_avg_5: 0.0,
            load_avg_15: 0.0,
            cpu_usage_pct: 0.0,
            ram_usage_pct: 0.0,
        }
    }
}

impl SystemInfo {
    /// Collect system info from /proc filesystem (Linux).
    /// Falls back to safe defaults on non-Linux or read errors.
    pub fn collect() -> Self {
        let cpu_count = Self::read_cpu_count();
        let (total_ram_mb, available_ram_mb) = Self::read_memory();
        let (load_avg_1, load_avg_5, load_avg_15) = Self::read_loadavg();

        let cpu_usage_pct = if cpu_count > 0 {
            (load_avg_1 / cpu_count as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        let ram_usage_pct = if total_ram_mb > 0 {
            ((total_ram_mb - available_ram_mb) as f64 / total_ram_mb as f64 * 100.0).min(100.0)
        } else {
            0.0
        };

        Self {
            cpu_count,
            total_ram_mb,
            available_ram_mb,
            load_avg_1,
            load_avg_5,
            load_avg_15,
            cpu_usage_pct,
            ram_usage_pct,
        }
    }

    /// Determine system tier based on load-to-CPU ratio.
    ///   IDLE:   < 30% load per core
    ///   NORMAL: < 60%
    ///   BUSY:   < 80%
    ///   HEAVY:  >= 80%
    pub fn tier(&self) -> &'static str {
        let ratio = if self.cpu_count > 0 {
            self.load_avg_1 / self.cpu_count as f64
        } else {
            1.0
        };
        if ratio < 0.30 {
            "IDLE"
        } else if ratio < 0.60 {
            "NORMAL"
        } else if ratio < 0.80 {
            "BUSY"
        } else {
            "HEAVY"
        }
    }

    /// Tier as a load ratio (0.0 - 1.0+).
    pub fn load_ratio(&self) -> f64 {
        if self.cpu_count > 0 {
            self.load_avg_1 / self.cpu_count as f64
        } else {
            1.0
        }
    }

    /// Used RAM in MB.
    pub fn used_ram_mb(&self) -> u64 {
        self.total_ram_mb.saturating_sub(self.available_ram_mb)
    }

    /// Used RAM in GB (f64).
    pub fn used_ram_gb(&self) -> f64 {
        self.used_ram_mb() as f64 / 1024.0
    }

    /// Total RAM in GB (f64).
    pub fn total_ram_gb(&self) -> f64 {
        self.total_ram_mb as f64 / 1024.0
    }

    /// Save system info as key=value pairs to a plain text file.
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let content = format!(
            "# sysinfo — collected {}\n\
             cpu_count={}\n\
             total_ram_mb={}\n\
             available_ram_mb={}\n\
             load_avg_1={:.2}\n\
             load_avg_5={:.2}\n\
             load_avg_15={:.2}\n\
             cpu_usage_pct={:.1}\n\
             ram_usage_pct={:.1}\n\
             tier={}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            self.cpu_count,
            self.total_ram_mb,
            self.available_ram_mb,
            self.load_avg_1,
            self.load_avg_5,
            self.load_avg_15,
            self.cpu_usage_pct,
            self.ram_usage_pct,
            self.tier(),
        );
        fs::write(path, content)
    }

    // ── Private helpers ──────────────────────────────────────────────

    fn read_cpu_count() -> usize {
        // Try std::thread::available_parallelism first (works cross-platform)
        if let Ok(n) = std::thread::available_parallelism() {
            return n.get();
        }
        // Fallback: parse /proc/cpuinfo
        if let Ok(content) = fs::read_to_string("/proc/cpuinfo") {
            let count = content.lines().filter(|l| l.starts_with("processor")).count();
            if count > 0 {
                return count;
            }
        }
        1
    }

    fn read_memory() -> (u64, u64) {
        let content = match fs::read_to_string("/proc/meminfo") {
            Ok(c) => c,
            Err(_) => return (0, 0),
        };

        let mut total_kb: u64 = 0;
        let mut available_kb: u64 = 0;

        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total_kb = Self::parse_meminfo_value(line);
            } else if line.starts_with("MemAvailable:") {
                available_kb = Self::parse_meminfo_value(line);
            }
        }

        (total_kb / 1024, available_kb / 1024)
    }

    fn parse_meminfo_value(line: &str) -> u64 {
        // Format: "MemTotal:       32456789 kB"
        line.split_whitespace()
            .nth(1)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    }

    fn read_loadavg() -> (f64, f64, f64) {
        let content = match fs::read_to_string("/proc/loadavg") {
            Ok(c) => c,
            Err(_) => return (0.0, 0.0, 0.0),
        };
        // Format: "0.52 0.48 0.45 1/1234 5678"
        let parts: Vec<&str> = content.split_whitespace().collect();
        let load1 = parts.first().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        let load5 = parts.get(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        let load15 = parts.get(2).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        (load1, load5, load15)
    }
}

/// Compute slider thresholds from system info for a given base max value.
/// Returns (yellow_threshold, red_threshold) where:
///   - green zone:  0 .. yellow_threshold  (system stays IDLE)
///   - yellow zone: yellow_threshold .. red_threshold  (NORMAL load)
///   - red zone:    red_threshold .. max  (BUSY/HEAVY load)
pub fn compute_thresholds(base_max: u32, info: &SystemInfo) -> (u32, u32) {
    let headroom = 1.0 - info.load_ratio();
    let headroom = headroom.max(0.1).min(1.0);

    // Yellow starts where ~30% of capacity is used
    let yellow = ((base_max as f64) * 0.30 * (1.0 + headroom)).min(base_max as f64 * 0.6) as u32;
    // Red starts where ~60% of capacity is used
    let red = ((base_max as f64) * 0.60 * (1.0 + headroom * 0.5)).min(base_max as f64 * 0.85) as u32;

    (yellow.max(1).min(base_max - 1), red.max(yellow + 1).min(base_max))
}
