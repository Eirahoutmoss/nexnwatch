//! Bayt / bit dönüşüm ve biçimlendirme yardımcıları.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DisplayUnit {
    /// B/s → KB/s → MB/s → GB/s otomatik ölçekleme.
    #[default]
    Auto,
    KBs,
    MBs,
    /// bit/s → Kbps → Mbps → Gbps otomatik ölçekleme.
    Bits,
    Mbps,
    Gbps,
}

impl DisplayUnit {
    pub const ALL: [DisplayUnit; 6] = [
        DisplayUnit::Auto,
        DisplayUnit::KBs,
        DisplayUnit::MBs,
        DisplayUnit::Bits,
        DisplayUnit::Mbps,
        DisplayUnit::Gbps,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DisplayUnit::Auto => "Oto B/s",
            DisplayUnit::KBs => "KB/s",
            DisplayUnit::MBs => "MB/s",
            DisplayUnit::Bits => "Oto bit/s",
            DisplayUnit::Mbps => "Mbps",
            DisplayUnit::Gbps => "Gbps",
        }
    }
}

impl std::fmt::Display for DisplayUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Bayt/saniye değerini seçili birime göre biçimlendirir.
pub fn speed(bytes_per_second: f64, unit: DisplayUnit) -> String {
    let b = bytes_per_second.max(0.0);
    match unit {
        DisplayUnit::Auto => scaled(b, 1024.0, &["B/s", "KB/s", "MB/s", "GB/s"]),
        DisplayUnit::KBs => format!("{} KB/s", num(b / 1024.0)),
        DisplayUnit::MBs => format!("{} MB/s", num(b / 1024.0 / 1024.0)),
        DisplayUnit::Bits => scaled(b * 8.0, 1000.0, &["bit/s", "Kbps", "Mbps", "Gbps"]),
        DisplayUnit::Mbps => format!("{} Mbps", num(b * 8.0 / 1_000_000.0)),
        DisplayUnit::Gbps => format!("{} Gbps", num3(b * 8.0 / 1_000_000_000.0)),
    }
}

fn scaled(mut value: f64, step: f64, units: &[&str]) -> String {
    let mut i = 0usize;
    while value >= step && i < units.len() - 1 {
        value /= step;
        i += 1;
    }
    if i == 0 {
        format!("{value:.0} {}", units[i])
    } else {
        format!("{} {}", num(value), units[i])
    }
}

fn num(v: f64) -> String {
    // Türkçe ondalık ayıracı (virgül), konsept görseldeki gibi: 42,8 MB/s
    if v >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}").replace('.', ",")
    }
}

fn num3(v: f64) -> String {
    format!("{v:.3}").replace('.', ",")
}

/// Toplam bayt miktarı (MB / GB otomatik).
pub fn bytes(value: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = value as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{value} B")
    } else if v >= 100.0 {
        format!("{v:.0} {}", units[i])
    } else {
        format!("{} {}", format!("{v:.1}").replace('.', ","), units[i])
    }
}

/// Bağlantı hızı (link speed, bit/s cinsinden gelir).
pub fn link(bps: u64) -> String {
    if bps == 0 || bps == u64::MAX {
        return "—".into();
    }
    let gbps = bps as f64 / 1_000_000_000.0;
    if gbps >= 1.0 {
        format!("{} Gbps", format!("{gbps:.1}").replace('.', ","))
    } else {
        format!("{:.0} Mbps", bps as f64 / 1_000_000.0)
    }
}

/// Mbps değerini (hız testi) biçimlendirir.
pub fn mbps(v: f64) -> String {
    format!("{} Mbps", format!("{v:.1}").replace('.', ","))
}

pub fn ago(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} sn önce"),
        60..=3599 => format!("{} dk önce", secs / 60),
        3600..=86_399 => format!("{} sa önce", secs / 3600),
        _ => format!("{} gün önce", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(speed(0.0, DisplayUnit::Auto), "0 B/s");
        assert_eq!(speed(1536.0, DisplayUnit::Auto), "1,5 KB/s");
        assert_eq!(speed(125_000.0, DisplayUnit::Mbps), "1,0 Mbps");
        assert_eq!(bytes(1024 * 1024 * 3 / 2), "1,5 MB");
        assert_eq!(link(1_000_000_000), "1,0 Gbps");
        assert_eq!(link(100_000_000), "100 Mbps");
    }
}
