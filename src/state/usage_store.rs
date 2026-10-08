//! Kalıcı günlük kullanım sayacı (bugün / bu hafta / bu ay).
//! %APPDATA%\NexNWatch\usage.json — gün anahtarı YYYY-MM-DD (yerel saat).

use std::collections::BTreeMap;

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct DayUsage {
    pub rx: u64,
    pub tx: u64,
}

impl DayUsage {
    pub fn total(&self) -> u64 {
        self.rx.saturating_add(self.tx)
    }
    fn add(&mut self, other: DayUsage) {
        self.rx = self.rx.saturating_add(other.rx);
        self.tx = self.tx.saturating_add(other.tx);
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UsageStore {
    pub days: BTreeMap<String, DayUsage>,
    /// Gün → uygulama adı → kullanım (ETW/IP Helper process trafiğinden).
    #[serde(default)]
    pub apps: BTreeMap<String, BTreeMap<String, DayUsage>>,
    #[serde(skip)]
    dirty: bool,
}

fn key(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

impl UsageStore {
    pub fn load() -> Self {
        std::fs::read(paths::usage_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        // 400 günden eskiyi buda.
        let cutoff = key(Local::now().date_naive() - ChronoDuration::days(400));
        self.days.retain(|k, _| *k >= cutoff);
        let app_cutoff = key(Local::now().date_naive() - ChronoDuration::days(92));
        self.apps.retain(|k, _| *k >= app_cutoff);
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            if let Err(e) = paths::write_atomic(&paths::usage_file(), &bytes) {
                tracing::warn!("usage.json yazılamadı: {e}");
            } else {
                self.dirty = false;
            }
        }
    }

    pub fn add(&mut self, rx: u64, tx: u64) {
        if rx == 0 && tx == 0 {
            return;
        }
        let k = key(Local::now().date_naive());
        self.days.entry(k).or_default().add(DayUsage { rx, tx });
        self.dirty = true;
    }

    /// Uygulama bazlı kullanım ekle (ad küçük harfe çevrilmez; görünen ad korunur).
    pub fn add_app(&mut self, name: &str, rx: u64, tx: u64) {
        if rx == 0 && tx == 0 {
            return;
        }
        let k = key(Local::now().date_naive());
        self.apps
            .entry(k)
            .or_default()
            .entry(name.to_string())
            .or_default()
            .add(DayUsage { rx, tx });
        self.dirty = true;
    }

    /// `from` gününden bugüne uygulama toplamları, en çoktan aza.
    fn apps_from(&self, from: NaiveDate) -> Vec<(String, DayUsage)> {
        let mut map: BTreeMap<String, DayUsage> = BTreeMap::new();
        for (_, apps) in self.apps.range(key(from)..) {
            for (name, u) in apps {
                map.entry(name.clone()).or_default().add(*u);
            }
        }
        let mut v: Vec<_> = map.into_iter().collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.1.total()));
        v
    }

    pub fn apps_today(&self) -> Vec<(String, DayUsage)> {
        self.apps_from(Local::now().date_naive())
    }

    pub fn apps_this_month(&self) -> Vec<(String, DayUsage)> {
        let today = Local::now().date_naive();
        self.apps_from(today.with_day(1).unwrap_or(today))
    }

    fn sum_from(&self, from: NaiveDate) -> DayUsage {
        let from = key(from);
        let mut out = DayUsage::default();
        for (_, d) in self.days.range(from..) {
            out.add(*d);
        }
        out
    }

    pub fn today(&self) -> DayUsage {
        self.sum_from(Local::now().date_naive())
    }

    pub fn this_week(&self) -> DayUsage {
        let today = Local::now().date_naive();
        let monday = today - ChronoDuration::days(today.weekday().num_days_from_monday() as i64);
        self.sum_from(monday)
    }

    pub fn this_month(&self) -> DayUsage {
        let today = Local::now().date_naive();
        self.sum_from(today.with_day(1).unwrap_or(today))
    }

    /// Son `n` gün (bugün dahil), en yeni en üstte; kaydı olmayan gün 0.
    pub fn last_days(&self, n: i64) -> Vec<(NaiveDate, DayUsage)> {
        let today = Local::now().date_naive();
        (0..n)
            .map(|i| {
                let d = today - ChronoDuration::days(i);
                (d, self.days.get(&key(d)).copied().unwrap_or_default())
            })
            .collect()
    }

    /// Son `n` ay toplamı (YYYY-MM).
    pub fn months(&self, n: usize) -> Vec<(String, DayUsage)> {
        let mut map: BTreeMap<String, DayUsage> = BTreeMap::new();
        for (k, d) in &self.days {
            map.entry(k[..7].to_string()).or_default().add(*d);
        }
        map.into_iter().rev().take(n).collect()
    }
}
