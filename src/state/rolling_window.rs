use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub at: Instant,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Kümülatif sayaç örneklerini tutar; pencere toplamı = son − pencere başı.
#[derive(Debug, Default)]
pub struct RollingWindow {
    samples: VecDeque<Sample>,
    capacity: Duration,
}

impl RollingWindow {
    pub fn new(capacity: Duration) -> Self {
        Self {
            samples: VecDeque::new(),
            capacity,
        }
    }

    pub fn set_capacity(&mut self, capacity: Duration) {
        self.capacity = capacity;
        if let Some(last) = self.latest() {
            self.prune(last.at);
        }
    }

    pub fn push(&mut self, sample: Sample) {
        self.samples.push_back(sample);
        self.prune(sample.at);
    }

    fn prune(&mut self, now: Instant) {
        while let Some(first) = self.samples.front() {
            if now.duration_since(first.at) > self.capacity + Duration::from_secs(2) {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Son `duration` içindeki toplam (rx, tx) bayt.
    pub fn total_since(&self, duration: Duration) -> (u64, u64) {
        let Some(last) = self.samples.back() else {
            return (0, 0);
        };
        let cutoff = last.at.checked_sub(duration).unwrap_or(last.at);
        // Pencere başına en yakın (cutoff'tan önceki son) örnek: tam pencereyi kapsar.
        let first = self
            .samples
            .iter()
            .rev()
            .find(|s| s.at <= cutoff)
            .or_else(|| self.samples.front())
            .copied()
            .unwrap_or(*last);
        (
            last.rx_bytes.saturating_sub(first.rx_bytes),
            last.tx_bytes.saturating_sub(first.tx_bytes),
        )
    }

    /// Pencerenin kapsadığı gerçek süre (uygulama yeni açıldıysa < istenen).
    pub fn covered(&self) -> Duration {
        match (self.samples.front(), self.samples.back()) {
            (Some(a), Some(b)) => b.at.duration_since(a.at),
            _ => Duration::ZERO,
        }
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn latest(&self) -> Option<Sample> {
        self.samples.back().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_totals() {
        let t0 = Instant::now();
        let mut w = RollingWindow::new(Duration::from_secs(600));
        for i in 0..=120u64 {
            w.push(Sample {
                at: t0 + Duration::from_secs(i),
                rx_bytes: i * 100,
                tx_bytes: i * 10,
            });
        }
        assert_eq!(w.total_since(Duration::from_secs(60)), (6000, 600));
        assert_eq!(w.total_since(Duration::from_secs(600)), (12000, 1200));
    }
}
