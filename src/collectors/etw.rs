//! Process bazlı trafik kaynağı.
//!
//! Birincil: ETW — Microsoft-Windows-Kernel-Network. Callback yalnızca topluyor:
//! (PID → rx/tx) ve (bağlantı → rx/tx) yerel haritalarda birikir, ~250 ms'de bir
//! paylaşılan haritalara aktarılır. Hesaplama UI tick'inde, callback dışında.
//!
//! Yedek: ETW başlatılamazsa ya da olay gelmezse `iphelper` modülü aynı
//! haritaları GetExtendedTcpTable + GetPerTcpConnectionEStats ile doldurur.
//!
//! Olay kimlikleri (Kernel-Network manifesti):
//!   10 TCPv4 gönder · 11 TCPv4 al · 26 TCPv6 gönder · 27 TCPv6 al
//!   42 UDPv4 gönder · 43 UDPv4 al · 58 UDPv6 gönder · 59 UDPv6 al
//! Alanlar: PID (u32), size (u32), daddr, saddr, dport, sport (ağ bayt sırası) ...

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[cfg_attr(not(windows), allow(dead_code))]
pub const SESSION_NAME: &str = "NexNWatch-KernelNetwork";

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum EtwStatus {
    Starting,
    Running,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Etw,
    IpHelper,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PidBytes {
    pub rx: u64,
    pub tx: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Proto {
    Tcp,
    Udp,
}

impl Proto {
    pub fn label(self) -> &'static str {
        match self {
            Proto::Tcp => "TCP",
            Proto::Udp => "UDP",
        }
    }
}

/// Bağlantı anahtarı. `a` = olaydaki daddr:dport, `b` = saddr:sport. Hangisinin
/// uzak uç olduğu UI tarafında yerel IP listesine göre belirlenir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnKey {
    pub pid: u32,
    pub proto: Proto,
    pub a: SocketAddr,
    pub b: SocketAddr,
}

#[derive(Debug, Clone, Copy)]
pub struct ConnBytes {
    pub rx: u64,
    pub tx: u64,
    pub last_seen: Instant,
}

const MAX_CONNS: usize = 8000;

#[derive(Clone)]
pub struct EtwHandle {
    pub totals: Arc<Mutex<HashMap<u32, PidBytes>>>,
    pub conns: Arc<Mutex<HashMap<ConnKey, ConnBytes>>>,
    pub status: Arc<Mutex<EtwStatus>>,
    pub events: Arc<AtomicU64>,
    pub fallback: Arc<AtomicBool>,
}

impl EtwHandle {
    fn new() -> Self {
        Self {
            totals: Arc::new(Mutex::new(HashMap::new())),
            conns: Arc::new(Mutex::new(HashMap::new())),
            status: Arc::new(Mutex::new(EtwStatus::Starting)),
            events: Arc::new(AtomicU64::new(0)),
            fallback: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn status(&self) -> EtwStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or(EtwStatus::Starting)
    }

    pub fn source(&self) -> Source {
        if self.fallback.load(Ordering::Relaxed) { Source::IpHelper } else { Source::Etw }
    }

    pub fn event_count(&self) -> u64 {
        self.events.load(Ordering::Relaxed)
    }

    /// Kümülatif PID sayaçlarının anlık kopyası.
    pub fn snapshot(&self) -> HashMap<u32, PidBytes> {
        self.totals.lock().map(|m| m.clone()).unwrap_or_default()
    }

    pub fn connections(&self) -> Vec<(ConnKey, ConnBytes)> {
        self.conns.lock().map(|m| m.iter().map(|(k, v)| (*k, *v)).collect()).unwrap_or_default()
    }

    /// Ölen process'lerin sayaçlarını ve bağlantılarını sil; eski bağlantıları buda.
    pub fn forget(&self, pids: &[u32]) {
        if let Ok(mut m) = self.totals.lock() {
            for pid in pids {
                m.remove(pid);
            }
        }
        if let Ok(mut c) = self.conns.lock() {
            if !pids.is_empty() {
                c.retain(|k, _| !pids.contains(&k.pid));
            }
        }
    }

    /// `max_age` boyunca etkinlik görmeyen bağlantıları sil; üst sınırı koru.
    pub fn prune_connections(&self, max_age: std::time::Duration) {
        if let Ok(mut c) = self.conns.lock() {
            let now = Instant::now();
            c.retain(|_, v| now.duration_since(v.last_seen) <= max_age);
            if c.len() > MAX_CONNS {
                let mut items: Vec<_> = c.iter().map(|(k, v)| (*k, v.last_seen)).collect();
                items.sort_by_key(|(_, t)| *t);
                let drop = c.len() - MAX_CONNS;
                for (k, _) in items.into_iter().take(drop) {
                    c.remove(&k);
                }
            }
        }
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn set_status(&self, s: EtwStatus) {
        if let Ok(mut st) = self.status.lock() {
            *st = s;
        }
    }

    /// Yerel birikimi paylaşılan haritalara aktar (ETW ve IP Helper ortak).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn merge(&self, pids: &mut HashMap<u32, PidBytes>, conns: &mut HashMap<ConnKey, ConnBytes>) {
        if !pids.is_empty() {
            if let Ok(mut m) = self.totals.lock() {
                for (pid, b) in pids.drain() {
                    let t = m.entry(pid).or_default();
                    t.rx = t.rx.wrapping_add(b.rx);
                    t.tx = t.tx.wrapping_add(b.tx);
                }
            }
        }
        if !conns.is_empty() {
            if let Ok(mut m) = self.conns.lock() {
                for (k, b) in conns.drain() {
                    match m.get_mut(&k) {
                        Some(t) => {
                            t.rx = t.rx.wrapping_add(b.rx);
                            t.tx = t.tx.wrapping_add(b.tx);
                            t.last_seen = b.last_seen;
                        }
                        None => {
                            m.insert(k, b);
                        }
                    }
                }
            }
        }
    }
}

/// Yerel birikime bir olay ekle.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn accumulate(
    pids: &mut HashMap<u32, PidBytes>,
    conns: &mut HashMap<ConnKey, ConnBytes>,
    key: Option<ConnKey>,
    pid: u32,
    rx: u64,
    tx: u64,
    now: Instant,
) {
    let e = pids.entry(pid).or_default();
    e.rx = e.rx.wrapping_add(rx);
    e.tx = e.tx.wrapping_add(tx);
    if let Some(k) = key {
        let c = conns.entry(k).or_insert(ConnBytes { rx: 0, tx: 0, last_seen: now });
        c.rx = c.rx.wrapping_add(rx);
        c.tx = c.tx.wrapping_add(tx);
        c.last_seen = now;
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    use ferrisetw::parser::Parser;
    use ferrisetw::provider::{EventFilter, Provider};
    use ferrisetw::schema_locator::SchemaLocator;
    use ferrisetw::trace::{stop_trace_by_name, UserTrace};
    use ferrisetw::EventRecord;

    const KERNEL_NETWORK_GUID: &str = "7DD42A49-5329-4832-8DFD-43D979153A88";
    const SEND_IDS: [u16; 4] = [10, 26, 42, 58];
    const RECV_IDS: [u16; 4] = [11, 27, 43, 59];
    const UDP_IDS: [u16; 4] = [42, 43, 58, 59];

    static TRACE: Mutex<Option<UserTrace>> = Mutex::new(None);

    pub fn start() -> EtwHandle {
        let handle = EtwHandle::new();
        let h = handle.clone();
        std::thread::Builder::new()
            .name("etw-kernel-network".into())
            .spawn(move || run(h))
            .ok();
        handle
    }

    fn ip(parser: &Parser, name: &str) -> Option<IpAddr> {
        if let Ok(ip) = parser.try_parse::<IpAddr>(name) {
            return Some(ip);
        }
        parser
            .try_parse::<u32>(name)
            .ok()
            .map(|raw| IpAddr::V4(Ipv4Addr::from(raw.to_ne_bytes())))
    }

    fn port(parser: &Parser, name: &str) -> u16 {
        // Kernel-Network portları ağ bayt sırasında (win:Port).
        parser.try_parse::<u16>(name).map(u16::from_be).unwrap_or(0)
    }

    fn run(handle: EtwHandle) {
        // Önceki çökmeden kalan aynı adlı oturumu kapat.
        let _ = stop_trace_by_name(SESSION_NAME);

        let h = handle.clone();
        let mut pids: HashMap<u32, PidBytes> = HashMap::new();
        let mut conns: HashMap<ConnKey, ConnBytes> = HashMap::new();
        let mut last_flush = Instant::now();

        let callback = move |record: &EventRecord, locator: &SchemaLocator| {
            let id = record.event_id();
            let is_send = SEND_IDS.contains(&id);
            if !is_send && !RECV_IDS.contains(&id) {
                return;
            }
            let Ok(schema) = locator.event_schema(record) else { return };
            let parser = Parser::create(record, &schema);
            let pid: u32 = parser.try_parse("PID").unwrap_or_else(|_| record.process_id());
            let size: u32 = parser.try_parse("size").or_else(|_| parser.try_parse("Size")).unwrap_or(0);
            if size == 0 {
                return;
            }
            h.events.fetch_add(1, Ordering::Relaxed);

            let key = match (ip(&parser, "daddr"), ip(&parser, "saddr")) {
                (Some(d), Some(s)) => Some(ConnKey {
                    pid,
                    proto: if UDP_IDS.contains(&id) { Proto::Udp } else { Proto::Tcp },
                    a: SocketAddr::new(d, port(&parser, "dport")),
                    b: SocketAddr::new(s, port(&parser, "sport")),
                }),
                _ => None,
            };
            let now = Instant::now();
            let size = size as u64;
            if is_send {
                accumulate(&mut pids, &mut conns, key, pid, 0, size, now);
            } else {
                accumulate(&mut pids, &mut conns, key, pid, size, 0, now);
            }
            if now.duration_since(last_flush) >= Duration::from_millis(250) {
                h.merge(&mut pids, &mut conns);
                last_flush = now;
            }
        };

        let ids: Vec<u16> = SEND_IDS.iter().chain(RECV_IDS.iter()).copied().collect();
        // Tüm anahtar kelimeler açık (IPv4 0x10, IPv6 0x20 dahil); olay kimliği
        // filtresi zaten yalnızca gönder/al olaylarını geçiriyor.
        let provider = Provider::by_guid(KERNEL_NETWORK_GUID)
            .any(u64::MAX)
            .add_filter(EventFilter::ByEventIds(ids))
            .add_callback(callback)
            .build();

        match UserTrace::new()
            .named(SESSION_NAME.to_string())
            .enable(provider)
            .start_and_process()
        {
            Ok(trace) => {
                tracing::info!("ETW oturumu başladı: {SESSION_NAME}");
                if let Ok(mut t) = TRACE.lock() {
                    *t = Some(trace);
                }
                handle.set_status(EtwStatus::Running);
            }
            Err(err) => {
                let msg = format!("{err:?}");
                tracing::error!("ETW başlatılamadı: {msg}");
                let hint = if msg.contains("AccessDenied") || msg.contains("5)") {
                    "Yönetici yetkisi gerekli".to_string()
                } else {
                    msg
                };
                handle.set_status(EtwStatus::Failed(hint));
            }
        }
    }

    /// ETW oturumunu kapat (yedek kaynağa geçerken ya da çıkışta).
    pub fn shutdown() {
        if let Ok(mut t) = TRACE.lock() {
            if let Some(trace) = t.take() {
                let _ = trace.stop();
                tracing::info!("ETW oturumu kapatıldı");
            }
        }
        let _ = stop_trace_by_name(SESSION_NAME);
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn start() -> EtwHandle {
        let h = EtwHandle::new();
        h.set_status(EtwStatus::Failed("ETW yalnızca Windows'ta kullanılabilir".into()));
        h
    }

    pub fn shutdown() {}
}

pub use imp::{shutdown, start};
