//! ETW — Microsoft-Windows-Kernel-Network ile PID bazlı trafik.
//!
//! Callback yalnızca topluyor: (PID → rx/tx bayt) yerel bir haritada birikir ve
//! ~250 ms'de bir paylaşılan haritaya aktarılır. Hesaplama (hız, ağaç toplamı)
//! UI tick'inde, callback dışında yapılır.
//!
//! Olay kimlikleri (Kernel-Network manifesti):
//!   10 TCPv4 gönder · 11 TCPv4 al · 26 TCPv6 gönder · 27 TCPv6 al
//!   42 UDPv4 gönder · 43 UDPv4 al · 58 UDPv6 gönder · 59 UDPv6 al
//! Alanlar: PID (u32), size (u32), daddr, saddr, dport, sport ...

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[cfg_attr(not(windows), allow(dead_code))]
pub const SESSION_NAME: &str = "NexNWatch-KernelNetwork";

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum EtwStatus {
    Starting,
    Running,
    Failed(String),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PidBytes {
    pub rx: u64,
    pub tx: u64,
}

#[derive(Clone)]
pub struct EtwHandle {
    pub totals: Arc<Mutex<HashMap<u32, PidBytes>>>,
    pub status: Arc<Mutex<EtwStatus>>,
    pub events: Arc<AtomicU64>,
}

impl EtwHandle {
    fn new() -> Self {
        Self {
            totals: Arc::new(Mutex::new(HashMap::new())),
            status: Arc::new(Mutex::new(EtwStatus::Starting)),
            events: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn status(&self) -> EtwStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or(EtwStatus::Starting)
    }

    pub fn event_count(&self) -> u64 {
        self.events.load(Ordering::Relaxed)
    }

    /// Kümülatif PID sayaçlarının anlık kopyası.
    pub fn snapshot(&self) -> HashMap<u32, PidBytes> {
        self.totals.lock().map(|m| m.clone()).unwrap_or_default()
    }

    /// Ölen process'in sayacını sil (PID yeniden kullanılırsa karışmasın).
    pub fn forget(&self, pids: &[u32]) {
        if let Ok(mut m) = self.totals.lock() {
            for pid in pids {
                m.remove(pid);
            }
        }
    }

    fn set_status(&self, s: EtwStatus) {
        if let Ok(mut st) = self.status.lock() {
            *st = s;
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::time::{Duration, Instant};

    use ferrisetw::parser::Parser;
    use ferrisetw::provider::{EventFilter, Provider};
    use ferrisetw::schema_locator::SchemaLocator;
    use ferrisetw::trace::{stop_trace_by_name, UserTrace};
    use ferrisetw::EventRecord;

    const KERNEL_NETWORK_GUID: &str = "7DD42A49-5329-4832-8DFD-43D979153A88";
    const SEND_IDS: [u16; 4] = [10, 26, 42, 58];
    const RECV_IDS: [u16; 4] = [11, 27, 43, 59];

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

    fn run(handle: EtwHandle) {
        // Önceki çökmeden kalan aynı adlı oturumu kapat.
        let _ = stop_trace_by_name(SESSION_NAME);

        let shared = handle.totals.clone();
        let events = handle.events.clone();
        let mut local: HashMap<u32, PidBytes> = HashMap::new();
        let mut last_flush = Instant::now();

        let callback = move |record: &EventRecord, locator: &SchemaLocator| {
            let id = record.event_id();
            let is_send = SEND_IDS.contains(&id);
            if !is_send && !RECV_IDS.contains(&id) {
                return;
            }
            let Ok(schema) = locator.event_schema(record) else { return };
            let parser = Parser::create(record, &schema);
            let pid: u32 = match parser.try_parse("PID") {
                Ok(p) => p,
                Err(_) => record.process_id(),
            };
            let size: u32 = parser.try_parse("size").or_else(|_| parser.try_parse("Size")).unwrap_or(0);
            if size == 0 {
                return;
            }
            events.fetch_add(1, Ordering::Relaxed);
            let e = local.entry(pid).or_default();
            if is_send {
                e.tx = e.tx.wrapping_add(size as u64);
            } else {
                e.rx = e.rx.wrapping_add(size as u64);
            }
            if last_flush.elapsed() >= Duration::from_millis(250) {
                if let Ok(mut m) = shared.lock() {
                    for (pid, b) in local.drain() {
                        let t = m.entry(pid).or_default();
                        t.rx = t.rx.wrapping_add(b.rx);
                        t.tx = t.tx.wrapping_add(b.tx);
                    }
                }
                last_flush = Instant::now();
            }
        };

        let ids: Vec<u16> = SEND_IDS.iter().chain(RECV_IDS.iter()).copied().collect();
        let provider = Provider::by_guid(KERNEL_NETWORK_GUID)
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

    /// Uygulama kapanırken oturumu düzgün kapat (StopTrace).
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
