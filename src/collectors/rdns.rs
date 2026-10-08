//! Uzak adresler için arka planda ters DNS çözümleme (önbellekli) ve bilinen
//! port → servis adı eşlemesi.

use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

const MAX_CACHE: usize = 4096;

#[derive(Clone)]
pub struct Resolver {
    cache: Arc<Mutex<HashMap<IpAddr, Option<String>>>>,
    pending: Arc<Mutex<HashSet<IpAddr>>>,
    tx: Sender<IpAddr>,
}

impl Resolver {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<IpAddr>();
        let cache: Arc<Mutex<HashMap<IpAddr, Option<String>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending = Arc::new(Mutex::new(HashSet::new()));
        let (c, p) = (cache.clone(), pending.clone());
        std::thread::Builder::new()
            .name("rdns".into())
            .spawn(move || {
                while let Ok(ip) = rx.recv() {
                    let name = dns_lookup::lookup_addr(&ip)
                        .ok()
                        .filter(|n| n.parse::<IpAddr>().is_err());
                    if let Ok(mut m) = c.lock() {
                        if m.len() >= MAX_CACHE {
                            m.clear();
                        }
                        m.insert(ip, name);
                    }
                    if let Ok(mut s) = p.lock() {
                        s.remove(&ip);
                    }
                }
            })
            .ok();
        Self { cache, pending, tx }
    }

    /// Önbellekte varsa adı döndürür; yoksa çözümleme kuyruğuna ekler.
    pub fn lookup(&self, ip: IpAddr) -> Option<String> {
        if ip.is_unspecified() || ip.is_multicast() {
            return None;
        }
        if let Some(v) = self.cache.lock().ok().and_then(|m| m.get(&ip).cloned()) {
            return v;
        }
        if let Ok(mut p) = self.pending.lock()
            && p.len() < 256
            && p.insert(ip)
        {
            let _ = self.tx.send(ip);
        }
        None
    }
}

/// Bilinen portlar için kısa servis adı.
pub fn service(port: u16) -> Option<&'static str> {
    Some(match port {
        20 | 21 => "ftp",
        22 => "ssh",
        23 => "telnet",
        25 | 587 | 465 => "smtp",
        53 => "dns",
        67 | 68 => "dhcp",
        80 => "http",
        110 | 995 => "pop3",
        123 => "ntp",
        137..=139 => "netbios",
        143 | 993 => "imap",
        161 | 162 => "snmp",
        389 | 636 => "ldap",
        443 => "https",
        445 => "smb",
        500 | 4500 => "ipsec",
        1194 => "openvpn",
        1433 => "mssql",
        1900 => "ssdp",
        3306 => "mysql",
        3389 => "rdp",
        3478 | 3479 => "stun",
        5060 | 5061 => "sip",
        5353 => "mdns",
        5355 => "llmnr",
        5432 => "postgres",
        5900 => "vnc",
        6379 => "redis",
        8080 | 8008 => "http-alt",
        8443 => "https-alt",
        27017 => "mongodb",
        51820 => "wireguard",
        _ => return None,
    })
}
