//! PID → TrafficStats. ETW'nin kümülatif sayaçlarından anlık hız hesaplanır;
//! PID yeniden kullanımı start_time ile doğrulanır.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crate::collectors::etw::PidBytes;

#[derive(Debug, Clone, Copy, Default)]
pub struct TrafficStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_per_sec: f64,
    pub tx_per_sec: f64,
    pub start_time: u64,
}

impl TrafficStats {
    pub fn total(&self) -> u64 {
        self.rx_bytes.saturating_add(self.tx_bytes)
    }
}

#[derive(Debug, Default)]
pub struct TrafficMap {
    stats: HashMap<u32, TrafficStats>,
    prev: HashMap<u32, PidBytes>,
    /// ETW sayaçlarında PID yeniden kullanıldığında sıfırlama için taban.
    base: HashMap<u32, PidBytes>,
    last: Option<Instant>,
}

impl TrafficMap {
    /// `alive`: PID → start_time (son process taraması). Dönüş: artık yaşamayan,
    /// ETW haritasından silinmesi gereken PID'ler.
    pub fn update(
        &mut self,
        now: Instant,
        etw: &HashMap<u32, PidBytes>,
        alive: &HashMap<u32, u64>,
    ) -> Vec<u32> {
        let elapsed = self.last.map(|l| now.duration_since(l).as_secs_f64()).unwrap_or(0.0);
        self.last = Some(now);

        for (&pid, &cur) in etw {
            let start = alive.get(&pid).copied().unwrap_or(0);
            let entry = self.stats.entry(pid).or_insert(TrafficStats { start_time: start, ..Default::default() });

            // Aynı PID, farklı başlangıç zamanı → yeni process.
            if start != 0 && entry.start_time != 0 && entry.start_time != start {
                *entry = TrafficStats { start_time: start, ..Default::default() };
                self.base.insert(pid, self.prev.get(&pid).copied().unwrap_or_default());
            } else if entry.start_time == 0 {
                entry.start_time = start;
            }

            let base = self.base.get(&pid).copied().unwrap_or_default();
            let prev = self.prev.get(&pid).copied().unwrap_or(cur);
            let drx = cur.rx.saturating_sub(prev.rx);
            let dtx = cur.tx.saturating_sub(prev.tx);
            if elapsed > 0.0 {
                entry.rx_per_sec = drx as f64 / elapsed;
                entry.tx_per_sec = dtx as f64 / elapsed;
            }
            entry.rx_bytes = cur.rx.saturating_sub(base.rx);
            entry.tx_bytes = cur.tx.saturating_sub(base.tx);
            self.prev.insert(pid, cur);
        }

        // ETW'de görünmeyen PID'lerin hızı 0.
        for (pid, s) in self.stats.iter_mut() {
            if !etw.contains_key(pid) {
                s.rx_per_sec = 0.0;
                s.tx_per_sec = 0.0;
            }
        }

        // Ölen process'ler: process listesinde yoksa kaldır.
        if alive.is_empty() {
            return Vec::new();
        }
        let dead: Vec<u32> = self
            .stats
            .iter()
            // Yalnızca daha önce canlı görülmüş (start_time bilinen) PID'ler:
            // ETW, process taramasından önce yeni doğan PID'i bildirebilir.
            .filter(|(pid, s)| **pid > 4 && s.start_time != 0 && !alive.contains_key(pid))
            .map(|(pid, _)| *pid)
            .collect();
        let dead_set: HashSet<u32> = dead.iter().copied().collect();
        self.stats.retain(|pid, _| !dead_set.contains(pid));
        self.prev.retain(|pid, _| !dead_set.contains(pid));
        self.base.retain(|pid, _| !dead_set.contains(pid));
        dead
    }

    pub fn get(&self, pid: u32) -> TrafficStats {
        self.stats.get(&pid).copied().unwrap_or_default()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u32, &TrafficStats)> {
        self.stats.iter()
    }

    pub fn len(&self) -> usize {
        self.stats.len()
    }
}
