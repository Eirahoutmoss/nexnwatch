//! LAN testi — iki bilgisayar arasındaki ağ hızı (iPerf benzeri) ve ağ
//! paylaşımına (SMB) dosya yazma/okuma hızı.
//!
//! Protokol (NexNWatch ↔ NexNWatch, TCP 47210):
//!   istemci her bağlantıda tek satır JSON başlık gönderir:
//!   `{"nnw":1,"op":"up"|"down"|"ping"|"udp","secs":10,"rate_mbps":100,"size":1400}`
//!
//! * `up`: istemci `secs` boyunca veri yollar, yazma yönünü kapatır; sunucu
//!   aldığı baytı ve süreyi tek satır JSON ile bildirir (alıcı tarafı ölçümü,
//!   tampon etkisi yok).
//! * `down`: sunucu `secs` boyunca veri yollar ve kapatır; istemci sayar.
//! * `ping`: 8 baytlık yankı, 20 tur → RTT.
//! * `udp`: sunucu geçici bir UDP portu açar ve bildirir; istemci belirtilen
//!   hızda sıra numaralı, zaman damgalı datagram yollar; sonunda TCP'den
//!   `{"end":true,"sent":n}` yazar, sunucu kayıp / sıra dışı / jitter
//!   (RFC 3550) döndürür.
//!
//! Keşif: sunucu modu açıkken UDP 47211'e 2 sn'de bir yayın (broadcast) yapılır.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::paths;

pub const PORT: u16 = 47210;
pub const DISCOVERY_PORT: u16 = 47211;
const BUF: usize = 128 * 1024;
const MAX_SECS: u64 = 120;
const MAX_STREAMS: usize = 16;
const MAX_SESSIONS: usize = 64;
const UDP_MAGIC: u32 = 0x4E4E_5755; // "NNWU"

// ---------------------------------------------------------------------------
// Veri tipleri
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LanMode {
    /// Bu bilgisayardan karşıya.
    Upload,
    /// Karşıdan bu bilgisayara.
    Download,
    /// Aynı anda iki yön.
    Bidir,
    /// UDP: belirli hızda gönderim, jitter ve kayıp.
    Udp,
}

impl LanMode {
    pub const ALL: [LanMode; 4] = [Self::Upload, Self::Download, Self::Bidir, Self::Udp];

    pub fn label(self) -> &'static str {
        match self {
            Self::Upload => "↑ Yükleme",
            Self::Download => "↓ İndirme",
            Self::Bidir => "⇅ Çift yön",
            Self::Udp => "UDP jitter/kayıp",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanResult {
    pub timestamp: i64,
    /// "tcp" testlerinde karşı uç (ip:port), "smb" testinde paylaşım yolu.
    pub target: String,
    #[serde(default)]
    pub peer_name: String,
    pub kind: String,
    pub secs: f64,
    pub streams: usize,
    pub up_mbps: Option<f64>,
    pub down_mbps: Option<f64>,
    pub rtt_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub loss_pct: Option<f64>,
    pub udp_rate_mbps: Option<f64>,
    /// Paylaşım testi: yazma / okuma MB/s.
    pub write_mbs: Option<f64>,
    pub read_mbs: Option<f64>,
    pub bytes: u64,
}

impl LanResult {
    fn new(target: String, kind: &str) -> Self {
        Self {
            timestamp: chrono::Local::now().timestamp(),
            target,
            peer_name: String::new(),
            kind: kind.into(),
            secs: 0.0,
            streams: 0,
            up_mbps: None,
            down_mbps: None,
            rtt_ms: None,
            jitter_ms: None,
            loss_pct: None,
            udp_rate_mbps: None,
            write_mbs: None,
            read_mbs: None,
            bytes: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub ip: IpAddr,
    pub port: u16,
    pub name: String,
    pub version: String,
    pub last_seen: Instant,
}

#[derive(Debug, Clone, Default)]
pub struct LanState {
    pub phase: String,
    pub progress: f32,
    pub live_up: f64,
    pub live_down: f64,
    /// Test sırasında saniyelik (yukarı, aşağı) Mbps.
    pub live_series: Vec<(f64, f64)>,
    pub history: Vec<LanResult>,
    pub last_error: Option<String>,
    pub server_sessions: usize,
    pub server_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Header {
    #[serde(default)]
    nnw: u32,
    op: String,
    #[serde(default)]
    secs: u64,
    #[serde(default)]
    rate_mbps: f64,
    #[serde(default)]
    size: usize,
}

#[derive(Clone)]
pub struct LanTester {
    pub state: Arc<Mutex<LanState>>,
    pub active: Arc<AtomicBool>,
    server_on: Arc<AtomicBool>,
    peers: Arc<Mutex<HashMap<IpAddr, Peer>>>,
    discovery_started: Arc<AtomicBool>,
    sessions: Arc<AtomicUsize>,
    server_bytes: Arc<AtomicU64>,
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(|| {
            std::fs::read_to_string("/etc/hostname")
                .ok()
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "bilinmeyen".into())
}

fn mbps(bytes: u64, secs: f64) -> f64 {
    bytes as f64 * 8.0 / secs.max(0.001) / 1_000_000.0
}

fn read_line(r: &mut impl BufRead) -> std::io::Result<String> {
    let mut line = String::new();
    let n = r.read_line(&mut line)?;
    if n == 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "bağlantı kapandı",
        ));
    }
    Ok(line)
}

fn payload() -> Vec<u8> {
    (0..BUF).map(|i| (i * 7 % 251) as u8).collect()
}

impl LanTester {
    pub fn new() -> Self {
        let history: Vec<LanResult> = std::fs::read(paths::app_dir().join("lan_history.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            state: Arc::new(Mutex::new(LanState {
                history,
                ..Default::default()
            })),
            active: Arc::new(AtomicBool::new(false)),
            server_on: Arc::new(AtomicBool::new(false)),
            peers: Arc::new(Mutex::new(HashMap::new())),
            discovery_started: Arc::new(AtomicBool::new(false)),
            sessions: Arc::new(AtomicUsize::new(0)),
            server_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Sunucu şu an bir teste hizmet veriyor mu?
    pub fn server_busy(&self) -> bool {
        self.sessions.load(Ordering::Relaxed) > 0
    }

    pub fn server_running(&self) -> bool {
        self.server_on.load(Ordering::SeqCst)
    }

    pub fn snapshot(&self) -> LanState {
        let mut s = self.state.lock().map(|s| s.clone()).unwrap_or_default();
        s.server_sessions = self.sessions.load(Ordering::Relaxed);
        s.server_bytes = self.server_bytes.load(Ordering::Relaxed);
        s
    }

    /// Son 30 sn içinde duyulan NexNWatch sunucuları.
    pub fn peers(&self) -> Vec<Peer> {
        let mut v: Vec<Peer> = self
            .peers
            .lock()
            .map(|m| {
                m.values()
                    .filter(|p| p.last_seen.elapsed() < Duration::from_secs(30))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    fn set_phase(&self, phase: &str, progress: f32) {
        if let Ok(mut s) = self.state.lock() {
            s.phase = phase.into();
            s.progress = progress;
        }
    }

    fn set_live(&self, up: f64, down: f64, push: bool) {
        if let Ok(mut s) = self.state.lock() {
            s.live_up = up;
            s.live_down = down;
            if push {
                s.live_series.push((up, down));
            }
        }
    }

    fn finish(&self, result: Result<LanResult, String>) {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        st.phase.clear();
        st.progress = 0.0;
        match result {
            Ok(r) => {
                tracing::info!("LAN testi: {:?}", r);
                st.history.push(r);
                let len = st.history.len();
                if len > 300 {
                    st.history.drain(..len - 300);
                }
                st.last_error = None;
                if let Ok(b) = serde_json::to_vec_pretty(&st.history) {
                    let _ = paths::write_atomic(&paths::app_dir().join("lan_history.json"), &b);
                }
            }
            Err(e) => {
                tracing::warn!("LAN testi başarısız: {e}");
                st.last_error = Some(e);
            }
        }
        drop(st);
        self.active.store(false, Ordering::SeqCst);
    }

    // -----------------------------------------------------------------------
    // Sunucu
    // -----------------------------------------------------------------------

    /// Sunucu modunu aç/kapat. Açılınca dinleme + keşif yayını başlar.
    pub fn set_server(&self, on: bool) -> Result<(), String> {
        if !on {
            self.server_on.store(false, Ordering::SeqCst);
            return Ok(());
        }
        if self.server_on.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        firewall_rule();
        let listener = match TcpListener::bind(("0.0.0.0", PORT)) {
            Ok(l) => l,
            Err(e) => {
                self.server_on.store(false, Ordering::SeqCst);
                return Err(format!("TCP {PORT} dinlenemedi: {e}"));
            }
        };
        let _ = listener.set_nonblocking(true);
        let me = self.clone();
        std::thread::Builder::new()
            .name("lan-server".into())
            .spawn(move || me.accept_loop(listener))
            .map_err(|e| e.to_string())?;
        let me = self.clone();
        std::thread::Builder::new()
            .name("lan-beacon".into())
            .spawn(move || me.beacon_loop())
            .map_err(|e| e.to_string())?;
        tracing::info!("LAN sunucusu açıldı: TCP {PORT}");
        Ok(())
    }

    fn accept_loop(&self, listener: TcpListener) {
        while self.server_on.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, addr)) => {
                    if self.sessions.load(Ordering::SeqCst) >= MAX_SESSIONS {
                        continue;
                    }
                    let _ = stream.set_nonblocking(false);
                    let me = self.clone();
                    self.sessions.fetch_add(1, Ordering::SeqCst);
                    std::thread::spawn(move || {
                        if let Err(e) = me.serve(stream, addr) {
                            tracing::debug!("LAN oturumu ({addr}) bitti: {e}");
                        }
                        me.sessions.fetch_sub(1, Ordering::SeqCst);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => {
                    tracing::warn!("LAN accept hatası: {e}");
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        }
        tracing::info!("LAN sunucusu kapatıldı");
    }

    fn serve(&self, stream: TcpStream, addr: SocketAddr) -> std::io::Result<()> {
        stream.set_nodelay(true)?;
        stream.set_read_timeout(Some(Duration::from_secs(MAX_SECS + 15)))?;
        let mut reader = BufReader::with_capacity(BUF, stream.try_clone()?);
        let mut writer = stream;
        let line = read_line(&mut reader)?;
        let h: Header = serde_json::from_str(line.trim())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if h.nnw != 1 {
            return Ok(());
        }
        let secs = h.secs.clamp(1, MAX_SECS);
        match h.op.as_str() {
            "up" => {
                let mut buf = vec![0u8; BUF];
                let mut total = 0u64;
                let mut first: Option<Instant> = None;
                let mut last = Instant::now();
                loop {
                    let n = reader.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    if first.is_none() {
                        first = Some(Instant::now());
                    }
                    last = Instant::now();
                    total += n as u64;
                    self.server_bytes.fetch_add(n as u64, Ordering::Relaxed);
                }
                let el = first
                    .map(|f| last.duration_since(f).as_secs_f64())
                    .unwrap_or(0.0);
                writeln!(
                    writer,
                    "{}",
                    serde_json::json!({ "bytes": total, "secs": el })
                )?;
            }
            "down" => {
                let data = payload();
                let deadline = Instant::now() + Duration::from_secs(secs);
                while Instant::now() < deadline {
                    writer.write_all(&data)?;
                    self.server_bytes
                        .fetch_add(data.len() as u64, Ordering::Relaxed);
                }
                writer.flush()?;
                let _ = writer.shutdown(Shutdown::Write);
                // istemcinin kapatmasını bekle (RST yerine düzgün kapanış)
                let mut sink = [0u8; 64];
                let _ = reader.read(&mut sink);
            }
            "ping" => {
                let mut b = [0u8; 8];
                for _ in 0..64 {
                    if reader.read_exact(&mut b).is_err() {
                        break;
                    }
                    writer.write_all(&b)?;
                }
            }
            "udp" => self.serve_udp(&mut reader, &mut writer, addr)?,
            _ => {}
        }
        Ok(())
    }

    fn serve_udp(
        &self,
        reader: &mut BufReader<TcpStream>,
        writer: &mut TcpStream,
        _addr: SocketAddr,
    ) -> std::io::Result<()> {
        let sock = UdpSocket::bind(("0.0.0.0", 0))?;
        sock.set_read_timeout(Some(Duration::from_millis(200)))?;
        let port = sock.local_addr()?.port();
        writeln!(writer, "{}", serde_json::json!({ "port": port }))?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let server_bytes = self.server_bytes.clone();
        let rx = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let (mut recv, mut bytes, mut ooo, mut max_seq) = (0u64, 0u64, 0u64, 0u64);
            let mut jitter = 0.0f64;
            let mut prev_transit: Option<f64> = None;
            let base = Instant::now();
            let (mut first, mut last) = (None::<Instant>, Instant::now());
            while !stop2.load(Ordering::SeqCst) {
                let Ok(n) = sock.recv(&mut buf) else { continue };
                if n < 20
                    || u32::from_be_bytes(buf[0..4].try_into().unwrap_or_default()) != UDP_MAGIC
                {
                    continue;
                }
                let seq = u64::from_be_bytes(buf[4..12].try_into().unwrap_or_default());
                let sent_ns = u64::from_be_bytes(buf[12..20].try_into().unwrap_or_default());
                let now = Instant::now();
                first.get_or_insert(now);
                last = now;
                recv += 1;
                bytes += n as u64;
                server_bytes.fetch_add(n as u64, Ordering::Relaxed);
                if seq < max_seq {
                    ooo += 1;
                } else {
                    max_seq = seq;
                }
                // RFC 3550 jitter: iki saatin farkı sabit olduğundan transit farkı yeterli.
                let transit =
                    now.duration_since(base).as_secs_f64() * 1000.0 - sent_ns as f64 / 1e6;
                if let Some(p) = prev_transit {
                    jitter += ((transit - p).abs() - jitter) / 16.0;
                }
                prev_transit = Some(transit);
            }
            let secs = first
                .map(|f| last.duration_since(f).as_secs_f64())
                .unwrap_or(0.0);
            (recv, bytes, ooo, jitter, secs)
        });

        // istemcinin "end" satırını bekle
        let line = read_line(reader);
        std::thread::sleep(Duration::from_millis(300)); // yoldaki son paketler
        stop.store(true, Ordering::SeqCst);
        let (recv, bytes, ooo, jitter, secs) = rx.join().unwrap_or_default();
        let sent = line
            .ok()
            .and_then(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
            .and_then(|v| v["sent"].as_u64())
            .unwrap_or(recv);
        writeln!(
            writer,
            "{}",
            serde_json::json!({ "recv": recv, "sent": sent, "bytes": bytes, "ooo": ooo, "jitter_ms": jitter, "secs": secs })
        )?;
        Ok(())
    }

    fn beacon_loop(&self) {
        let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)) else {
            return;
        };
        let _ = sock.set_broadcast(true);
        let msg = serde_json::json!({
            "app": "NexNWatch",
            "name": hostname(),
            "port": PORT,
            "ver": env!("CARGO_PKG_VERSION"),
        })
        .to_string();
        while self.server_on.load(Ordering::SeqCst) {
            let _ = sock.send_to(msg.as_bytes(), ("255.255.255.255", DISCOVERY_PORT));
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    /// Keşif dinleyicisi (bir kez başlatılır).
    pub fn start_discovery(&self) {
        if self.discovery_started.swap(true, Ordering::SeqCst) {
            return;
        }
        firewall_rule();
        let peers = self.peers.clone();
        std::thread::Builder::new()
            .name("lan-discovery".into())
            .spawn(move || {
                let sock = match UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("Keşif portu {DISCOVERY_PORT} açılamadı: {e}");
                        return;
                    }
                };
                let mut buf = [0u8; 1500];
                let me = hostname();
                loop {
                    let Ok((n, from)) = sock.recv_from(&mut buf) else {
                        continue;
                    };
                    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf[..n]) else {
                        continue;
                    };
                    if v["app"] != "NexNWatch" {
                        continue;
                    }
                    let name = v["name"].as_str().unwrap_or("?").to_string();
                    if name == me && is_local_ip(from.ip()) {
                        continue; // kendi yayınımız
                    }
                    if let Ok(mut m) = peers.lock() {
                        m.insert(
                            from.ip(),
                            Peer {
                                ip: from.ip(),
                                port: v["port"].as_u64().unwrap_or(PORT as u64) as u16,
                                name,
                                version: v["ver"].as_str().unwrap_or("").to_string(),
                                last_seen: Instant::now(),
                            },
                        );
                    }
                }
            })
            .ok();
    }

    // -----------------------------------------------------------------------
    // İstemci
    // -----------------------------------------------------------------------

    /// Testi arka planda başlat.
    pub fn start_test(
        &self,
        target: SocketAddr,
        peer_name: String,
        mode: LanMode,
        secs: u64,
        streams: usize,
        udp_rate: f64,
    ) {
        if self.active.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Ok(mut s) = self.state.lock() {
            s.live_series.clear();
            s.last_error = None;
        }
        let me = self.clone();
        std::thread::Builder::new()
            .name("lan-test".into())
            .spawn(move || {
                let r = me
                    .run_test(target, mode, secs, streams, udp_rate)
                    .map(|mut r| {
                        r.peer_name = peer_name;
                        r
                    });
                me.finish(r);
            })
            .ok();
    }

    /// Senkron test (komut satırı ve iş parçacığı).
    pub fn run_test(
        &self,
        target: SocketAddr,
        mode: LanMode,
        secs: u64,
        streams: usize,
        udp_rate: f64,
    ) -> Result<LanResult, String> {
        let secs = secs.clamp(1, MAX_SECS);
        let streams = streams.clamp(1, MAX_STREAMS);
        let mut r = LanResult::new(target.to_string(), "tcp");
        r.streams = streams;

        self.set_phase("Gecikme ölçülüyor…", 0.02);
        r.rtt_ms = Some(self.ping(target)?);

        match mode {
            LanMode::Upload | LanMode::Download | LanMode::Bidir => {
                let ups = if mode != LanMode::Download {
                    streams
                } else {
                    0
                };
                let downs = if mode != LanMode::Upload { streams } else { 0 };
                self.set_phase(mode.label(), 0.05);
                let (up, down, bytes, el) = self.tcp_streams(target, secs, ups, downs)?;
                r.up_mbps = up;
                r.down_mbps = down;
                r.bytes = bytes;
                r.secs = el;
            }
            LanMode::Udp => {
                r.kind = "udp".into();
                self.set_phase("UDP gönderiliyor…", 0.05);
                let (rate, loss, jitter, bytes, el) = self.udp(target, secs, udp_rate)?;
                r.up_mbps = Some(rate);
                r.loss_pct = Some(loss);
                r.jitter_ms = Some(jitter);
                r.udp_rate_mbps = Some(udp_rate);
                r.bytes = bytes;
                r.secs = el;
            }
        }
        Ok(r)
    }

    fn connect(&self, target: SocketAddr, h: &Header) -> Result<TcpStream, String> {
        let s = TcpStream::connect_timeout(&target, Duration::from_secs(4)).map_err(|e| {
            format!(
                "{target} adresine bağlanılamadı ({e}). Karşı bilgisayarda NexNWatch → LAN Testi → \"Sunucu modu\" açık mı, güvenlik duvarı izin veriyor mu?"
            )
        })?;
        let _ = s.set_nodelay(true);
        let _ = s.set_read_timeout(Some(Duration::from_secs(h.secs.max(5) + 20)));
        let mut w = s.try_clone().map_err(|e| e.to_string())?;
        writeln!(w, "{}", serde_json::to_string(h).unwrap_or_default())
            .map_err(|e| e.to_string())?;
        Ok(s)
    }

    fn ping(&self, target: SocketAddr) -> Result<f64, String> {
        let h = Header {
            nnw: 1,
            op: "ping".into(),
            secs: 5,
            ..Default::default()
        };
        let mut s = self.connect(target, &h)?;
        let mut samples = Vec::new();
        let mut b = [0u8; 8];
        for i in 0..20u64 {
            let t = Instant::now();
            s.write_all(&i.to_be_bytes()).map_err(|e| e.to_string())?;
            s.read_exact(&mut b)
                .map_err(|e| format!("Yankı alınamadı: {e}"))?;
            samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Ok(samples[samples.len() / 2])
    }

    /// N yükleme + M indirme akışı. Dönüş: (yukarı Mbps, aşağı Mbps, toplam bayt, süre).
    fn tcp_streams(
        &self,
        target: SocketAddr,
        secs: u64,
        ups: usize,
        downs: usize,
    ) -> Result<(Option<f64>, Option<f64>, u64, f64), String> {
        let up_live = Arc::new(AtomicU64::new(0));
        let down_live = Arc::new(AtomicU64::new(0));
        let start = Instant::now();
        let deadline = start + Duration::from_secs(secs);

        let mut up_handles = Vec::new();
        for _ in 0..ups {
            let h = Header {
                nnw: 1,
                op: "up".into(),
                secs,
                ..Default::default()
            };
            let s = self.connect(target, &h)?;
            let live = up_live.clone();
            up_handles.push(std::thread::spawn(move || -> Result<(u64, f64), String> {
                let mut w = s.try_clone().map_err(|e| e.to_string())?;
                let data = payload();
                while Instant::now() < deadline {
                    w.write_all(&data).map_err(|e| e.to_string())?;
                    live.fetch_add(data.len() as u64, Ordering::Relaxed);
                }
                let _ = w.shutdown(Shutdown::Write);
                let mut r = BufReader::new(s);
                let line =
                    read_line(&mut r).map_err(|e| format!("Sunucu sonucu alınamadı: {e}"))?;
                let v: serde_json::Value =
                    serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
                Ok((
                    v["bytes"].as_u64().unwrap_or(0),
                    v["secs"].as_f64().unwrap_or(0.0),
                ))
            }));
        }
        let mut down_handles = Vec::new();
        for _ in 0..downs {
            let h = Header {
                nnw: 1,
                op: "down".into(),
                secs,
                ..Default::default()
            };
            let mut s = self.connect(target, &h)?;
            let live = down_live.clone();
            down_handles.push(std::thread::spawn(move || -> Result<(u64, f64), String> {
                let mut buf = vec![0u8; BUF];
                let (mut total, mut first, mut last) = (0u64, None::<Instant>, Instant::now());
                loop {
                    let n = s.read(&mut buf).map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    first.get_or_insert_with(Instant::now);
                    last = Instant::now();
                    total += n as u64;
                    live.fetch_add(n as u64, Ordering::Relaxed);
                }
                Ok((
                    total,
                    first
                        .map(|f| last.duration_since(f).as_secs_f64())
                        .unwrap_or(0.0),
                ))
            }));
        }

        // Canlı gösterge: saniyelik
        let (mut pu, mut pd, mut pt) = (0u64, 0u64, Instant::now());
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(250));
            if pt.elapsed() >= Duration::from_secs(1) {
                let (u, d) = (
                    up_live.load(Ordering::Relaxed),
                    down_live.load(Ordering::Relaxed),
                );
                let el = pt.elapsed().as_secs_f64();
                self.set_live(mbps(u - pu, el), mbps(d - pd, el), true);
                (pu, pd, pt) = (u, d, Instant::now());
            }
            let frac = (start.elapsed().as_secs_f32() / secs as f32).min(1.0);
            self.set_phase(
                if ups > 0 && downs > 0 {
                    "Çift yön ölçülüyor…"
                } else if ups > 0 {
                    "Yükleme ölçülüyor…"
                } else {
                    "İndirme ölçülüyor…"
                },
                0.05 + 0.9 * frac,
            );
        }
        self.set_phase("Sonuçlar alınıyor…", 0.97);

        let collect = |hs: Vec<std::thread::JoinHandle<Result<(u64, f64), String>>>| -> Result<Option<(u64, f64)>, String> {
            if hs.is_empty() {
                return Ok(None);
            }
            let (mut bytes, mut secs) = (0u64, 0.0f64);
            for h in hs {
                let (b, s) = h.join().map_err(|_| "akış iş parçacığı çöktü".to_string())??;
                bytes += b;
                secs = secs.max(s);
            }
            Ok(Some((bytes, secs)))
        };
        let up = collect(up_handles)?;
        let down = collect(down_handles)?;
        let total = up.map(|u| u.0).unwrap_or(0) + down.map(|d| d.0).unwrap_or(0);
        Ok((
            up.map(|(b, s)| mbps(b, s)),
            down.map(|(b, s)| mbps(b, s)),
            total,
            start.elapsed().as_secs_f64(),
        ))
    }

    /// Dönüş: (alınan Mbps, kayıp %, jitter ms, bayt, süre)
    fn udp(
        &self,
        target: SocketAddr,
        secs: u64,
        rate_mbps: f64,
    ) -> Result<(f64, f64, f64, u64, f64), String> {
        const SIZE: usize = 1400;
        let h = Header {
            nnw: 1,
            op: "udp".into(),
            secs,
            rate_mbps,
            size: SIZE,
        };
        let s = self.connect(target, &h)?;
        let mut reader = BufReader::new(s.try_clone().map_err(|e| e.to_string())?);
        let mut w = s;
        let line = read_line(&mut reader).map_err(|e| e.to_string())?;
        let port = serde_json::from_str::<serde_json::Value>(line.trim())
            .ok()
            .and_then(|v| v["port"].as_u64())
            .ok_or("Sunucu UDP portu bildirmedi")? as u16;
        let sock = UdpSocket::bind(("0.0.0.0", 0)).map_err(|e| e.to_string())?;
        let dest = SocketAddr::new(target.ip(), port);

        let interval =
            Duration::from_secs_f64(SIZE as f64 * 8.0 / (rate_mbps.max(0.1) * 1_000_000.0));
        let mut pkt = vec![0u8; SIZE];
        pkt[0..4].copy_from_slice(&UDP_MAGIC.to_be_bytes());
        let start = Instant::now();
        let deadline = start + Duration::from_secs(secs);
        let (mut seq, mut next) = (0u64, start);
        let mut last_live = Instant::now();
        let mut sent_at_live = 0u64;
        while Instant::now() < deadline {
            let now = Instant::now();
            if now < next {
                let gap = next - now;
                if gap > Duration::from_micros(1500) {
                    std::thread::sleep(gap - Duration::from_micros(1000));
                } else {
                    std::hint::spin_loop();
                }
                continue;
            }
            pkt[4..12].copy_from_slice(&seq.to_be_bytes());
            pkt[12..20].copy_from_slice(&(start.elapsed().as_nanos() as u64).to_be_bytes());
            let _ = sock.send_to(&pkt, dest);
            seq += 1;
            next += interval;
            // çok gerideysek yetişmeye çalışıp patlama yapma
            if Instant::now() > next + Duration::from_millis(50) {
                next = Instant::now();
            }
            if last_live.elapsed() >= Duration::from_secs(1) {
                let el = last_live.elapsed().as_secs_f64();
                self.set_live(mbps((seq - sent_at_live) * SIZE as u64, el), 0.0, true);
                sent_at_live = seq;
                last_live = Instant::now();
                self.set_phase(
                    "UDP gönderiliyor…",
                    0.05 + 0.9 * (start.elapsed().as_secs_f32() / secs as f32).min(1.0),
                );
            }
        }
        writeln!(w, "{}", serde_json::json!({ "end": true, "sent": seq }))
            .map_err(|e| e.to_string())?;
        let line = read_line(&mut reader).map_err(|e| format!("UDP sonucu alınamadı: {e}"))?;
        let v: serde_json::Value = serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
        let recv = v["recv"].as_u64().unwrap_or(0);
        let bytes = v["bytes"].as_u64().unwrap_or(0);
        let el = v["secs"].as_f64().unwrap_or(secs as f64).max(0.001);
        let loss = if seq > 0 {
            (seq.saturating_sub(recv)) as f64 / seq as f64 * 100.0
        } else {
            0.0
        };
        Ok((
            mbps(bytes, el),
            loss,
            v["jitter_ms"].as_f64().unwrap_or(0.0),
            bytes,
            el,
        ))
    }

    // -----------------------------------------------------------------------
    // Paylaşım (SMB) testi
    // -----------------------------------------------------------------------

    pub fn start_share_test(&self, dir: String, size_mb: u64) {
        if self.active.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Ok(mut s) = self.state.lock() {
            s.live_series.clear();
            s.last_error = None;
        }
        let me = self.clone();
        std::thread::Builder::new()
            .name("lan-share".into())
            .spawn(move || {
                let r = me.run_share_test(&dir, size_mb);
                me.finish(r);
            })
            .ok();
    }

    pub fn run_share_test(&self, dir: &str, size_mb: u64) -> Result<LanResult, String> {
        let dir = dir.trim();
        if dir.is_empty() {
            return Err("Paylaşım yolu boş (ör. \\\\SUNUCU\\Paylasim)".into());
        }
        let path = std::path::Path::new(dir).join(format!(
            "nexnwatch-lan-test-{}.tmp",
            chrono::Local::now().format("%Y%m%d%H%M%S")
        ));
        let chunk_len = 4 * 1024 * 1024usize;
        let chunks = (size_mb.max(16) * 1024 * 1024 / chunk_len as u64).max(1);
        let total = chunks * chunk_len as u64;
        let mut aligned = AlignedBuf::new(chunk_len);
        for (i, b) in aligned.as_mut().iter_mut().enumerate() {
            *b = (i * 13 % 251) as u8;
        }

        // Yazma
        self.set_phase("Paylaşıma yazılıyor…", 0.02);
        let t = Instant::now();
        {
            let mut f = open_unbuffered(&path, true)
                .map_err(|e| format!("Dosya oluşturulamadı ({}): {e}", path.display()))?;
            let mut last = Instant::now();
            let mut prev = 0u64;
            for i in 0..chunks {
                f.write_all(aligned.as_ref()).map_err(|e| {
                    let _ = std::fs::remove_file(&path);
                    format!("Yazma hatası: {e}")
                })?;
                let done = (i + 1) * chunk_len as u64;
                if last.elapsed() >= Duration::from_millis(500) {
                    self.set_live(mbps(done - prev, last.elapsed().as_secs_f64()), 0.0, true);
                    prev = done;
                    last = Instant::now();
                }
                self.set_phase(
                    "Paylaşıma yazılıyor…",
                    0.02 + 0.48 * (done as f32 / total as f32),
                );
            }
            f.sync_all()
                .map_err(|e| format!("Yazma tamamlanamadı: {e}"))?;
        }
        let write_secs = t.elapsed().as_secs_f64();

        // Okuma
        self.set_phase("Paylaşımdan okunuyor…", 0.5);
        let t = Instant::now();
        let read_res = (|| -> Result<u64, String> {
            let mut f =
                open_unbuffered(&path, false).map_err(|e| format!("Dosya açılamadı: {e}"))?;
            let mut got = 0u64;
            let mut last = Instant::now();
            let mut prev = 0u64;
            loop {
                let n = f
                    .read(aligned.as_mut())
                    .map_err(|e| format!("Okuma hatası: {e}"))?;
                if n == 0 {
                    break;
                }
                got += n as u64;
                if last.elapsed() >= Duration::from_millis(500) {
                    self.set_live(0.0, mbps(got - prev, last.elapsed().as_secs_f64()), true);
                    prev = got;
                    last = Instant::now();
                }
                self.set_phase(
                    "Paylaşımdan okunuyor…",
                    0.5 + 0.48 * (got as f32 / total as f32).min(1.0),
                );
            }
            Ok(got)
        })();
        let read_secs = t.elapsed().as_secs_f64();
        let _ = std::fs::remove_file(&path);
        let got = read_res?;

        let mut r = LanResult::new(dir.to_string(), "smb");
        r.secs = write_secs + read_secs;
        r.bytes = total + got;
        r.write_mbs = Some(total as f64 / write_secs.max(0.001) / 1_000_000.0);
        r.read_mbs = Some(got as f64 / read_secs.max(0.001) / 1_000_000.0);
        r.up_mbps = r.write_mbs.map(|v| v * 8.0);
        r.down_mbps = r.read_mbs.map(|v| v * 8.0);
        Ok(r)
    }
}

fn is_local_ip(ip: IpAddr) -> bool {
    ip.is_loopback() || UdpSocket::bind((ip, 0)).is_ok() // bu makineye ait bir adres
}

/// Sayfa hizalı tampon (önbelleksiz G/Ç için 4096 hizalama gerekir).
struct AlignedBuf {
    raw: Vec<u8>,
    off: usize,
    len: usize,
}

impl AlignedBuf {
    fn new(len: usize) -> Self {
        let raw = vec![0u8; len + 4096];
        let off = (4096 - (raw.as_ptr() as usize % 4096)) % 4096;
        Self { raw, off, len }
    }
    fn as_ref(&self) -> &[u8] {
        &self.raw[self.off..self.off + self.len]
    }
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.raw[self.off..self.off + self.len]
    }
}

/// Windows: önbelleği atlayan (NO_BUFFERING + WRITE_THROUGH) dosya — aksi halde
/// okuma testi yerel önbellekten gelir ve gerçek ağ hızını göstermez.
fn open_unbuffered(path: &std::path::Path, write: bool) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    if write {
        o.write(true).create(true).truncate(true);
    } else {
        o.read(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
        const FILE_FLAG_WRITE_THROUGH: u32 = 0x8000_0000;
        o.custom_flags(FILE_FLAG_NO_BUFFERING | if write { FILE_FLAG_WRITE_THROUGH } else { 0 });
    }
    o.open(path)
}

/// Windows Güvenlik Duvarı: uygulamaya özel/etki alanı profillerinde gelen
/// bağlantı izni (bir kez eklenir).
fn firewall_rule() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        static DONE: std::sync::Once = std::sync::Once::new();
        DONE.call_once(|| {
            let Ok(exe) = std::env::current_exe() else {
                return;
            };
            let name = "NexNWatch LAN Testi";
            let exists = std::process::Command::new("netsh")
                .args([
                    "advfirewall",
                    "firewall",
                    "show",
                    "rule",
                    &format!("name={name}"),
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !exists {
                let r = std::process::Command::new("netsh")
                    .args([
                        "advfirewall",
                        "firewall",
                        "add",
                        "rule",
                        &format!("name={name}"),
                        "dir=in",
                        "action=allow",
                        &format!("program={}", exe.display()),
                        "enable=yes",
                        "profile=private,domain",
                    ])
                    .creation_flags(CREATE_NO_WINDOW)
                    .output();
                tracing::info!("Güvenlik duvarı kuralı eklendi: {:?}", r.map(|o| o.status));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_tcp_udp() {
        let server = LanTester::new();
        server.set_server(true).expect("sunucu");
        std::thread::sleep(Duration::from_millis(200));
        let client = LanTester::new();
        let target: SocketAddr = ([127, 0, 0, 1], PORT).into();

        let r = client
            .run_test(target, LanMode::Bidir, 2, 2, 0.0)
            .expect("tcp");
        assert!(
            r.up_mbps.unwrap() > 1.0 && r.down_mbps.unwrap() > 1.0,
            "{r:?}"
        );
        assert!(r.rtt_ms.unwrap() >= 0.0);

        let r = client
            .run_test(target, LanMode::Udp, 2, 1, 20.0)
            .expect("udp");
        let got = r.up_mbps.unwrap();
        assert!(
            got > 10.0 && got < 30.0,
            "UDP hızı hedefe yakın olmalı: {r:?}"
        );
        assert!(r.loss_pct.unwrap() < 5.0, "{r:?}");

        let dir = std::env::temp_dir();
        let r = client
            .run_share_test(dir.to_str().unwrap(), 16)
            .expect("paylaşım");
        assert!(r.write_mbs.unwrap() > 0.0 && r.read_mbs.unwrap() > 0.0);
        server.set_server(false).ok();
    }
}
