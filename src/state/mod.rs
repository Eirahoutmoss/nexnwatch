pub mod rolling_window;

#[derive(Debug, Clone, Copy, Default)]
pub struct TrafficSample {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_bps: f64,
    pub tx_bps: f64,
}
