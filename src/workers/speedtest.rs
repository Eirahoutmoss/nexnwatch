//! İnternet hız testi — iki sağlayıcı:
//!
//! * **Cloudflare** (speed.cloudflare.com): `__down?bytes=`, `__up` (POST);
//!   konum/IP `cdn-cgi/trace`'ten. (`/meta` uç noktası 2026'dan beri herkese
//!   403 veriyor — kullanılmıyor.)
//! * **Speedtest.net** (Ookla): `api/js/servers` ile coğrafi olarak en yakın
//!   sunucular, aralarından TCP gecikmesi en düşük olan; `/download?size=`,
//!   `/upload` (POST).
//!
//! "Otomatik" önce Cloudflare'i dener; indirme/yükleme başarısız olursa
//! Speedtest.net'e geçer. ISS bilgisi isteğe bağlıdır (ipinfo.io), alınamazsa
//! test yine sürer.
//!
//! Ping / jitter: sunucuya 10 kez TCP bağlantı kurulum süresi (saf RTT).
//! Download / upload: 4 paralel akış, her biri ~8 sn.
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

const CF_HOST: &str = "speed.cloudflare.com";
const UA: &str = concat!("NexNWatch/", env!("CARGO_PKG_VERSION"));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SpeedProvider {
    /// Önce Cloudflare, olmazsa Speedtest.net.
    #[default]
    Auto,
    Cloudflare,
    Ookla,
}

impl SpeedProvider {
    pub const ALL: [SpeedProvider; 3] = [Self::Auto, Self::Cloudflare, Self::Ookla];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Otomatik",
            Self::Cloudflare => "Cloudflare",
            Self::Ookla => "Speedtest.net",
        }
    }
}

/// Seçilen test sunucusu.
struct Server {
    label: String,
    /// TCP ping hedefi (ana bilgisayar, port).
    host: String,
    port: u16,
    kind: SpeedProvider,
}

impl Server {
    fn download_url(&self, n: u64) -> String {
        match self.kind {
            SpeedProvider::Ookla => format!(
                "https://{}:{}/download?nocache={}&size=25000000",
                self.host,
                self.port,
                nonce(n)
            ),
            _ => format!("https://{CF_HOST}/__down?bytes=25000000&n={}", nonce(n)),
        }
    }

    fn upload_url(&self, n: u64) -> String {
        match self.kind {
            SpeedProvider::Ookla => format!(
                "https://{}:{}/upload?nocache={}",
                self.host,
                self.port,
                nonce(n)
            ),
            _ => format!("https://{CF_HOST}/__up?n={}", nonce(n)),
        }
    }
}

fn nonce(n: u64) -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{t:x}{n}")
}

/// ureq hatasını kullanıcıya anlaşılır biçimde açıkla.
fn explain(step: &str, e: &ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(403) => format!(
            "{step}: sunucu isteği reddetti (HTTP 403). Kurum güvenlik duvarı / web filtresi hız testi sitelerini engelliyor olabilir."
        ),
        ureq::Error::StatusCode(429) => {
            format!("{step}: sunucu çok sık test yapıldığı için geçici olarak sınırladı (HTTP 429)")
        }
        ureq::Error::StatusCode(c) => format!("{step}: sunucu HTTP {c} döndü"),
        ureq::Error::Timeout(_) => format!("{step}: zaman aşımı"),
        ureq::Error::HostNotFound => format!("{step}: DNS çözümlenemedi"),
        ureq::Error::ConnectionFailed => format!("{step}: bağlantı kurulamadı"),
        other => format!("{step}: {other}"),
    }
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
        self.state
            .lock()
            .ok()
            .and_then(|s| s.history.last().cloned())
    }

    pub fn start(&self, auto: bool, provider: SpeedProvider) {
        if self.active.swap(true, Ordering::SeqCst) {
            return; // zaten çalışıyor
        }
        let me = self.clone();
        std::thread::Builder::new()
            .name("speedtest".into())
            .spawn(move || {
                let result = me.run(auto, provider);
                {
                    let mut st = me.state.lock().unwrap_or_else(|e| e.into_inner());
                    st.phase = Phase::Idle;
                    st.live_mbps = 0.0;
                    st.progress = 0.0;
                    match result {
                        Ok(r) => {
                            tracing::info!(
                                "Hız testi ({}): ↓{:.1} ↑{:.1} Mbps, ping {:.1} ms",
                                r.server,
                                r.download_mbps,
                                r.upload_mbps,
                                r.ping_ms
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

    /// Arayüzsüz tek test (komut satırı: `nexnwatch --speedtest [auto|cloudflare|ookla]`).
    pub fn run_blocking(&self, provider: SpeedProvider) -> Result<SpeedResult, String> {
        self.run(false, provider)
    }

    fn agent(&self) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_global(Some(Duration::from_secs(PHASE_SECS + 15)))
            .user_agent(UA)
            .build()
            .into()
    }

    fn run(&self, auto: bool, provider: SpeedProvider) -> Result<SpeedResult, String> {
        let agent = self.agent();
        let used = Arc::new(AtomicU64::new(0));

        match provider {
            SpeedProvider::Cloudflare => self.run_with(&agent, cloudflare(&agent), auto, &used),
            SpeedProvider::Ookla => {
                self.set(Phase::Meta, 0.0, 0.0);
                let server = ookla(&agent)?;
                self.run_with(&agent, server, auto, &used)
            }
            SpeedProvider::Auto => match self.run_with(&agent, cloudflare(&agent), auto, &used) {
                Ok(r) => Ok(r),
                Err(cf_err) => {
                    tracing::warn!(
                        "Cloudflare hız testi başarısız ({cf_err}); Speedtest.net deneniyor"
                    );
                    self.set(Phase::Meta, 0.0, 0.0);
                    let server = ookla(&agent)
                        .map_err(|e| format!("Cloudflare: {cf_err} · Speedtest.net: {e}"))?;
                    self.run_with(&agent, server, auto, &used)
                        .map_err(|e| format!("Cloudflare: {cf_err} · Speedtest.net: {e}"))
                }
            },
        }
    }

    fn run_with(
        &self,
        agent: &ureq::Agent,
        server: Server,
        auto: bool,
        used: &Arc<AtomicU64>,
    ) -> Result<SpeedResult, String> {
        // Genel IP / ISS (isteğe bağlı; başarısız olursa test sürer)
        let (ip, isp) = client_info(agent);

        self.set(Phase::Ping, 0.0, 0.05);
        let (ping_ms, jitter_ms) = ping(&server.host, server.port)?;

        let download_mbps = self.download(agent, &server, used.clone())?;
        let upload_mbps = self.upload(agent, &server, used.clone())?;

        Ok(SpeedResult {
            timestamp: chrono::Local::now().timestamp(),
            download_mbps,
            upload_mbps,
            ping_ms,
            jitter_ms,
            server: server.label,
            isp,
            ip,
            bytes_used: used.load(Ordering::Relaxed),
            auto,
        })
    }

    fn download(
        &self,
        agent: &ureq::Agent,
        server: &Server,
        used: Arc<AtomicU64>,
    ) -> Result<f64, String> {
        self.set(Phase::Download, 0.0, 0.1);
        let counter = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        let deadline = start + Duration::from_secs(PHASE_SECS);
        let last_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let urls: Vec<String> = (0..64).map(|i| server.download_url(i)).collect();
        let urls = Arc::new(urls);

        let workers: Vec<_> = (0..STREAMS)
            .map(|w| {
                let agent = agent.clone();
                let counter = counter.clone();
                let last_err = last_err.clone();
                let urls = urls.clone();
                std::thread::spawn(move || {
                    let mut buf = vec![0u8; 64 * 1024];
                    let mut i = w;
                    while Instant::now() < deadline {
                        let url = &urls[i % urls.len()];
                        i += STREAMS;
                        let mut resp = match agent.get(url).call() {
                            Ok(r) => r,
                            Err(e) => {
                                if let Ok(mut l) = last_err.lock() {
                                    *l = Some(explain("İndirme", &e));
                                }
                                std::thread::sleep(Duration::from_millis(300));
                                continue;
                            }
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
                                Err(_) => break,
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
            let detail = last_err
                .lock()
                .ok()
                .and_then(|l| l.clone())
                .unwrap_or_else(|| "İndirme testi veri alamadı".into());
            return Err(detail);
        }
        Ok(bytes as f64 * 8.0 / start.elapsed().as_secs_f64().max(0.1) / 1_000_000.0)
    }

    fn upload(
        &self,
        agent: &ureq::Agent,
        server: &Server,
        used: Arc<AtomicU64>,
    ) -> Result<f64, String> {
        self.set(Phase::Upload, 0.0, 0.55);
        let counter = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        let deadline = start + Duration::from_secs(PHASE_SECS);
        let last_err: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let chunk: Arc<Vec<u8>> =
            Arc::new((0..2 * 1024 * 1024).map(|i| (i * 31 % 251) as u8).collect());
        let urls = Arc::new((0..64).map(|i| server.upload_url(i)).collect::<Vec<_>>());

        let workers: Vec<_> = (0..STREAMS)
            .map(|w| {
                let agent = agent.clone();
                let counter = counter.clone();
                let chunk = chunk.clone();
                let last_err = last_err.clone();
                let urls = urls.clone();
                std::thread::spawn(move || {
                    let mut i = w;
                    while Instant::now() < deadline {
                        let url = &urls[i % urls.len()];
                        i += STREAMS;
                        match agent
                            .post(url)
                            .header("Content-Type", "application/octet-stream")
                            .send(&chunk[..])
                        {
                            Ok(_) => {
                                counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                            }
                            Err(e) => {
                                if let Ok(mut l) = last_err.lock() {
                                    *l = Some(explain("Yükleme", &e));
                                }
                                std::thread::sleep(Duration::from_millis(300));
                            }
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
            let detail = last_err
                .lock()
                .ok()
                .and_then(|l| l.clone())
                .unwrap_or_else(|| "Yükleme testi veri gönderemedi".into());
            return Err(detail);
        }
        Ok(bytes as f64 * 8.0 / elapsed / 1_000_000.0)
    }
    /// Aşama sürerken canlı Mbps ve ilerlemeyi günceller.
    fn monitor(
        &self,
        phase: Phase,
        counter: &AtomicU64,
        start: Instant,
        deadline: Instant,
        p0: f32,
        p1: f32,
    ) {
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

/// Cloudflare sunucusu; konum `cdn-cgi/trace`'ten (başarısızsa yalnızca "Cloudflare").
fn cloudflare(agent: &ureq::Agent) -> Server {
    let trace = agent
        .get(format!("https://{CF_HOST}/cdn-cgi/trace"))
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_to_string().ok())
        .unwrap_or_default();
    let field = |k: &str| {
        trace
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")).map(str::to_string))
            .unwrap_or_default()
    };
    let colo = field("colo");
    let loc = field("loc");
    let label = match (colo.is_empty(), loc.is_empty()) {
        (false, false) => format!("Cloudflare {colo} · {loc}"),
        (false, true) => format!("Cloudflare {colo}"),
        _ => "Cloudflare".to_string(),
    };
    Server {
        label,
        host: CF_HOST.into(),
        port: 443,
        kind: SpeedProvider::Cloudflare,
    }
}

/// Speedtest.net: coğrafi olarak en yakın 5 sunucudan TCP gecikmesi en düşük olan.
fn ookla(agent: &ureq::Agent) -> Result<Server, String> {
    let list: serde_json::Value = agent
        .get("https://www.speedtest.net/api/js/servers?engine=js&limit=5&https_functional=true")
        .call()
        .map_err(|e| explain("Speedtest.net sunucu listesi", &e))?
        .body_mut()
        .read_json()
        .map_err(|e| format!("Speedtest.net sunucu listesi okunamadı: {e}"))?;
    let mut best: Option<(f64, Server)> = None;
    for s in list.as_array().into_iter().flatten() {
        let Some(hostport) = s["host"].as_str() else {
            continue;
        };
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse().unwrap_or(8080)),
            None => (hostport.to_string(), 8080),
        };
        let Some(rtt) = tcp_rtt(&host, port) else {
            continue;
        };
        let label = format!(
            "Speedtest.net · {} · {}",
            s["sponsor"].as_str().unwrap_or("?"),
            s["name"].as_str().unwrap_or("?")
        );
        if best.as_ref().is_none_or(|(b, _)| rtt < *b) {
            best = Some((
                rtt,
                Server {
                    label,
                    host,
                    port,
                    kind: SpeedProvider::Ookla,
                },
            ));
        }
    }
    best.map(|(_, s)| s)
        .ok_or_else(|| "Speedtest.net: erişilebilir sunucu bulunamadı".into())
}

/// Genel IP ve ISS (ipinfo.io; isteğe bağlı).
fn client_info(agent: &ureq::Agent) -> (String, String) {
    let v: serde_json::Value = agent
        .get("https://ipinfo.io/json")
        .call()
        .ok()
        .and_then(|mut r| r.body_mut().read_json().ok())
        .unwrap_or(serde_json::Value::Null);
    let ip = v["ip"].as_str().unwrap_or("").to_string();
    // "AS9121 Turk Telekom" → "Turk Telekom (AS9121)"
    let isp = match v["org"].as_str() {
        Some(org) => match org.split_once(' ') {
            Some((asn, name)) if asn.starts_with("AS") => format!("{name} ({asn})"),
            _ => org.to_string(),
        },
        None => String::new(),
    };
    (ip, isp)
}

fn tcp_rtt(host: &str, port: u16) -> Option<f64> {
    let addr: SocketAddr = (host, port).to_socket_addrs().ok()?.next()?;
    let t = Instant::now();
    TcpStream::connect_timeout(&addr, Duration::from_secs(2)).ok()?;
    Some(t.elapsed().as_secs_f64() * 1000.0)
}

fn ping(host: &str, port: u16) -> Result<(f64, f64), String> {
    let addr: SocketAddr = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("DNS çözümlenemedi ({host}): {e}"))?
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
