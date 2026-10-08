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
    /// start_time'ı hiç bilinmeyen PID'lerin son etkinlik zamanı.
    idle: HashMap<u32, Instant>,
}

impl TrafficMap {
    /// `alive`: PID → start_time (son process taraması).
    /// Dönüş: (artık yaşamayan PID'ler, bu tick'teki PID başına rx/tx farkı).
    pub fn update(
        &mut self,
        now: Instant,
        etw: &HashMap<u32, PidBytes>,
        alive: &HashMap<u32, u64>,
    ) -> (Vec<u32>, Vec<(u32, u64, u64)>) {
        let mut deltas = Vec::new();
        let elapsed = self
            .last
            .map(|l| now.duration_since(l).as_secs_f64())
            .unwrap_or(0.0);
        self.last = Some(now);

        for (&pid, &cur) in etw {
            let start = alive.get(&pid).copied().unwrap_or(0);
            let entry = self.stats.entry(pid).or_insert(TrafficStats {
                start_time: start,
                ..Default::default()
            });

            // Aynı PID, farklı başlangıç zamanı → yeni process.
            if start != 0 && entry.start_time != 0 && entry.start_time != start {
                *entry = TrafficStats {
                    start_time: start,
                    ..Default::default()
                };
                self.base
                    .insert(pid, self.prev.get(&pid).copied().unwrap_or_default());
            } else if entry.start_time == 0 {
                entry.start_time = start;
            }

            let base = self.base.get(&pid).copied().unwrap_or_default();
            let prev = self.prev.get(&pid).copied().unwrap_or(cur);
            let drx = cur.rx.saturating_sub(prev.rx);
            let dtx = cur.tx.saturating_sub(prev.tx);
            if drx > 0 || dtx > 0 {
                deltas.push((pid, drx, dtx));
            }
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
            return (Vec::new(), deltas);
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

        // Hiç canlı görülmemiş (çok kısa ömürlü) PID kayıtları: 10 dk sessizse sil.
        self.idle.retain(|pid, _| self.stats.contains_key(pid));
        for (pid, s) in &self.stats {
            if s.start_time == 0 {
                let t = self.idle.entry(*pid).or_insert(now);
                if s.rx_per_sec + s.tx_per_sec > 0.0 {
                    *t = now;
                }
            }
        }
        let stale: Vec<u32> = self
            .idle
            .iter()
            .filter(|(_, t)| now.duration_since(**t).as_secs() > 600)
            .map(|(p, _)| *p)
            .collect();
        let mut dead = dead;
        for pid in stale {
            self.stats.remove(&pid);
            self.prev.remove(&pid);
            self.base.remove(&pid);
            self.idle.remove(&pid);
            dead.push(pid);
        }
        (dead, deltas)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn pb(rx: u64, tx: u64) -> PidBytes {
        PidBytes { rx, tx }
    }

    #[test]
    fn rates_deltas_and_death() {
        let t0 = Instant::now();
        let mut m = TrafficMap::default();
        let alive: HashMap<u32, u64> = [(100, 1000), (200, 1000)].into();

        let etw: HashMap<u32, PidBytes> = [(100, pb(1000, 100))].into();
        let (dead, deltas) = m.update(t0, &etw, &alive);
        assert!(dead.is_empty());
        assert!(deltas.is_empty(), "ilk örnek taban olmalı");

        let etw: HashMap<u32, PidBytes> = [(100, pb(3000, 300))].into();
        let (_, deltas) = m.update(t0 + Duration::from_secs(2), &etw, &alive);
        assert_eq!(deltas, vec![(100, 2000, 200)]);
        let s = m.get(100);
        assert_eq!((s.rx_bytes, s.tx_bytes), (3000, 300));
        assert!((s.rx_per_sec - 1000.0).abs() < 1e-6);

        // 100 öldü → kaldırılmalı
        let alive2: HashMap<u32, u64> = [(200, 1000)].into();
        let (dead, _) = m.update(t0 + Duration::from_secs(3), &etw, &alive2);
        assert_eq!(dead, vec![100]);
        assert_eq!(m.get(100).total(), 0);
    }

    #[test]
    fn pid_reuse_resets_totals() {
        let t0 = Instant::now();
        let mut m = TrafficMap::default();
        let etw: HashMap<u32, PidBytes> = [(7, pb(500, 0))].into();
        m.update(t0, &etw, &[(7, 10)].into());
        assert_eq!(m.get(7).rx_bytes, 500);
        // Aynı PID, farklı başlangıç zamanı (ETW haritası henüz silinmemiş)
        let etw: HashMap<u32, PidBytes> = [(7, pb(800, 0))].into();
        m.update(t0 + Duration::from_secs(1), &etw, &[(7, 99)].into());
        assert_eq!(
            m.get(7).rx_bytes,
            300,
            "yeni process yalnızca kendi baytlarını görmeli"
        );
    }
}
