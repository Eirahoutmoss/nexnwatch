//! İnternet hız testi — Cloudflare hız testi uç noktaları (speed.cloudflare.com).
//!
//! * Ping / jitter : sunucuya 10 kez TCP bağlantı kurulum süresi (saf RTT)
//! * Download      : 4 paralel akış, ~8 sn, `__down?bytes=`
//! * Upload        : 4 paralel akış, ~8 sn, `__up` (POST)
//! * Sunucu / ISS  : `/meta` (colo, şehir, AS organizasyonu, IP)
//!
//! Test sürerken `active` bayrağı true olur; NIC rolling window bu süredeki
//! trafiği pencere toplamlarına katmaz.

use std::io::Read;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::paths;

const HOST: &str = "speed.cloudflare.com";
const STREAMS: usize = 4;
const PHASE_SECS: u64 = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedResult {
    /// UNIX zaman damgası (sn).
    pub timestamp: i64,
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub ping_ms: f64,
    pub jitter_ms: f64,
    pub server: String,
    pub isp: String,
    pub ip: String,
    /// Testin harcadığı toplam veri (bayt).
    pub bytes_used: u64,
    #[serde(default)]
    pub auto: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Phase {
    Idle,
    Meta,
    Ping,
    Download,
    Upload,
}

impl Phase {
    pub fn label(&self) -> &'static str {
        match self {
            Phase::Idle => "Hazır",
            Phase::Meta => "Sunucu bulunuyor…",
            Phase::Ping => "Ping ölçülüyor…",
            Phase::Download => "İndirme ölçülüyor…",
            Phase::Upload => "Yükleme ölçülüyor…",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpeedState {
    pub phase: Phase,
    /// Aşama içi canlı hız (Mbps).
    pub live_mbps: f64,
    /// 0..1 ilerleme.
    pub progress: f32,
    pub history: Vec<SpeedResult>,
    pub last_error: Option<String>,
}

#[derive(Clone)]
pub struct SpeedTester {
    pub state: Arc<Mutex<SpeedState>>,
    pub active: Arc<AtomicBool>,
}

impl SpeedTester {
    pub fn new() -> Self {
        let history: Vec<SpeedResult> = std::fs::read(paths::speedtest_file())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            state: Arc::new(Mutex::new(SpeedState {
                phase: Phase::Idle,
                live_mbps: 0.0,
                progress: 0.0,
                history,
                last_error: None,
            })),
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    pub fn snapshot(&self) -> SpeedState {
        self.state.lock().map(|s| s.clone()).unwrap_or(SpeedState {
            phase: Phase::Idle,
            live_mbps: 0.0,
            progress: 0.0,
            history: vec![],
            last_error: None,
        })
    }

    pub fn last(&self) -> Option<SpeedResult> {
        self.state.lock().ok().and_then(|s| s.history.last().cloned())
    }

    pub fn start(&self, auto: bool) {
        if self.active.swap(true, Ordering::SeqCst) {
            return; // zaten çalışıyor
        }
        let me = self.clone();
        std::thread::Builder::new()
            .name("speedtest".into())
            .spawn(move || {
                let result = me.run(auto);
                {
                    let mut st = me.state.lock().unwrap_or_else(|e| e.into_inner());
                    st.phase = Phase::Idle;
                    st.live_mbps = 0.0;
                    st.progress = 0.0;
                    match result {
                        Ok(r) => {
                            tracing::info!(
                                "Hız testi: ↓{:.1} ↑{:.1} Mbps, ping {:.1} ms",
                                r.download_mbps, r.upload_mbps, r.ping_ms
                            );
                            st.history.push(r);
                            let len = st.history.len();
                            if len > 500 {
                                st.history.drain(..len - 500);
                            }
                            st.last_error = None;
                            if let Ok(bytes) = serde_json::to_vec_pretty(&st.history) {
                                let _ = paths::write_atomic(&paths::speedtest_file(), &bytes);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Hız testi başarısız: {e}");
                            st.last_error = Some(e);
                        }
                    }
                }
                me.active.store(false, Ordering::SeqCst);
            })
            .ok();
    }

    fn set(&self, phase: Phase, live: f64, progress: f32) {
        if let Ok(mut st) = self.state.lock() {
            st.phase = phase;
            st.live_mbps = live;
            st.progress = progress;
        }
    }

    fn run(&self, auto: bool) -> Result<SpeedResult, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_global(Some(Duration::from_secs(PHASE_SECS + 15)))
            .user_agent("NexNWatch/0.2")
            .build()
            .into();

        // 1) Meta
        self.set(Phase::Meta, 0.0, 0.0);
        let meta: serde_json::Value = agent
            .get(format!("https://{HOST}/meta"))
            .call()
            .map_err(|e| format!("Sunucuya ulaşılamadı: {e}"))?
            .body_mut()
            .read_json()
            .unwrap_or(serde_json::Value::Null);
        let s = |k: &str| meta[k].as_str().unwrap_or("").to_string();
        let server = {
            let colo = s("colo");
            let city = s("city");
            match (colo.is_empty(), city.is_empty()) {
                (false, false) => format!("Cloudflare {colo} · {city}"),
                (false, true) => format!("Cloudflare {colo}"),
                _ => "Cloudflare".to_string(),
            }
        };
        let isp = s("asOrganization");
        let ip = s("clientIp");

        // 2) Ping / jitter
        self.set(Phase::Ping, 0.0, 0.05);
        let (ping_ms, jitter_ms) = ping()?;

        // 3) Download
        let used = Arc::new(AtomicU64::new(0));
        let download_mbps = self.download(&agent, used.clone())?;

        // 4) Upload
        let upload_mbps = self.upload(&agent, used.clone())?;

        Ok(SpeedResult {
            timestamp: chrono::Local::now().timestamp(),
            download_mbps,
            upload_mbps,
            ping_ms,
            jitter_ms,
            server,
            isp,
            ip,
            bytes_used: used.load(Ordering::Relaxed),
            auto,
        })
    }

    fn download(&self, agent: &ureq::Agent, used: Arc<AtomicU64>) -> Result<f64, String> {
        self.set(Phase::Download, 0.0, 0.1);
        let counter = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        let deadline = start + Duration::from_secs(PHASE_SECS);
        let errors = Arc::new(AtomicU64::new(0));

        let workers: Vec<_> = (0..STREAMS)
            .map(|_| {
                let agent = agent.clone();
                let counter = counter.clone();
                let errors = errors.clone();
                std::thread::spawn(move || {
                    let mut buf = vec![0u8; 64 * 1024];
                    while Instant::now() < deadline {
                        let resp = agent.get(format!("https://{HOST}/__down?bytes=100000000")).call();
                        let Ok(mut resp) = resp else {
                            errors.fetch_add(1, Ordering::Relaxed);
                            std::thread::sleep(Duration::from_millis(200));
                            continue;
                        };
                        let mut reader = resp.body_mut().as_reader();
                        loop {
                            if Instant::now() >= deadline {
                                return;
                            }
                            match reader.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    counter.fetch_add(n as u64, Ordering::Relaxed);
                                }
                                Err(_) => {
                                    errors.fetch_add(1, Ordering::Relaxed);
                                    break;
                                }
                            }
                        }
                    }
                })
            })
            .collect();

        self.monitor(Phase::Download, &counter, start, deadline, 0.1, 0.55);
        for w in workers {
            let _ = w.join();
        }
        let bytes = counter.load(Ordering::Relaxed);
        used.fetch_add(bytes, Ordering::Relaxed);
        if bytes == 0 {
            return Err("İndirme testi veri alamadı".into());
        }
        Ok(bytes as f64 * 8.0 / start.elapsed().as_secs_f64().max(0.1) / 1_000_000.0)
    }

    fn upload(&self, agent: &ureq::Agent, used: Arc<AtomicU64>) -> Result<f64, String> {
        self.set(Phase::Upload, 0.0, 0.55);
        let counter = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        let deadline = start + Duration::from_secs(PHASE_SECS);
        let chunk: Arc<Vec<u8>> = Arc::new((0..2 * 1024 * 1024).map(|i| (i * 31 % 251) as u8).collect());

        let workers: Vec<_> = (0..STREAMS)
            .map(|_| {
                let agent = agent.clone();
                let counter = counter.clone();
                let chunk = chunk.clone();
                std::thread::spawn(move || {
                    while Instant::now() < deadline {
                        match agent.post(format!("https://{HOST}/__up")).send(&chunk[..]) {
                            Ok(_) => {
                                counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                            }
                            Err(_) => std::thread::sleep(Duration::from_millis(200)),
                        }
                    }
                })
            })
            .collect();

        self.monitor(Phase::Upload, &counter, start, deadline, 0.55, 1.0);
        for w in workers {
            let _ = w.join();
        }
        let elapsed = start.elapsed().as_secs_f64().max(0.1);
        let bytes = counter.load(Ordering::Relaxed);
        used.fetch_add(bytes, Ordering::Relaxed);
        if bytes == 0 {
            return Err("Yükleme testi veri gönderemedi".into());
        }
        Ok(bytes as f64 * 8.0 / elapsed / 1_000_000.0)
    }

    /// Aşama sürerken canlı Mbps ve ilerlemeyi günceller.
    fn monitor(&self, phase: Phase, counter: &AtomicU64, start: Instant, deadline: Instant, p0: f32, p1: f32) {
        let total = deadline.duration_since(start).as_secs_f32();
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(250));
            let el = start.elapsed().as_secs_f64().max(0.05);
            let mbps = counter.load(Ordering::Relaxed) as f64 * 8.0 / el / 1_000_000.0;
            let frac = (start.elapsed().as_secs_f32() / total).min(1.0);
            self.set(phase.clone(), mbps, p0 + (p1 - p0) * frac);
        }
    }
}

fn ping() -> Result<(f64, f64), String> {
    let addr: SocketAddr = (HOST, 443)
        .to_socket_addrs()
        .map_err(|e| format!("DNS çözümlenemedi: {e}"))?
        .next()
        .ok_or("DNS sonucu yok")?;
    let mut samples = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        if TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok() {
            samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    if samples.is_empty() {
        return Err("Ping ölçülemedi".into());
    }
    let jitter = if samples.len() > 1 {
        samples.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (samples.len() - 1) as f64
    } else {
        0.0
    };
    let mut sorted = samples.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted[sorted.len() / 2];
    Ok((median, jitter))
}
