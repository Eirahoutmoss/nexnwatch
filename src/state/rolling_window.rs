use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub at: Instant,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Default)]
pub struct RollingWindow {
    samples: VecDeque<Sample>,
    capacity: Duration,
}

impl RollingWindow {
    pub fn new(capacity: Duration) -> Self {
        Self { samples: VecDeque::new(), capacity }
    }

    pub fn push(&mut self, sample: Sample) {
        self.samples.push_back(sample);
        self.prune(sample.at);
    }

    fn prune(&mut self, now: Instant) {
        while let Some(first) = self.samples.front() {
            if now.duration_since(first.at) > self.capacity {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn total_since(&self, duration: Duration) -> (u64, u64) {
        let Some(last) = self.samples.back() else { return (0, 0); };
        let cutoff = last.at.checked_sub(duration).unwrap_or(last.at);
        let first = self
            .samples
            .iter()
            .find(|sample| sample.at >= cutoff)
            .copied()
            .unwrap_or(*last);
        (
            last.rx_bytes.saturating_sub(first.rx_bytes),
            last.tx_bytes.saturating_sub(first.tx_bytes),
        )
    }

    pub fn clear(&mut self) { self.samples.clear(); }

    pub fn latest(&self) -> Option<Sample> { self.samples.back().copied() }
}
