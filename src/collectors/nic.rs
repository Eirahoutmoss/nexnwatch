//! Ağ adaptörü keşfi ve sayaç okuma.
//!
//! Windows: `GetIfTable2` (MIB_IF_ROW2) + `GetAdaptersAddresses` (IP, ağ geçidi,
//! DNS). Linux (geliştirme / ileride port): /sys/class/net + /proc/net/dev.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdapterKind {
    Ethernet,
    WiFi,
    Bluetooth,
    Cellular,
    Vpn,
    HyperV,
    Container,
    VmWare,
    VirtualBox,
    WifiDirect,
    OtherVirtual,
}

impl AdapterKind {
    pub fn is_virtual(self) -> bool {
        !matches!(
            self,
            Self::Ethernet | Self::WiFi | Self::Bluetooth | Self::Cellular
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ethernet => "Ethernet",
            Self::WiFi => "Wi-Fi",
            Self::Bluetooth => "Bluetooth",
            Self::Cellular => "Mobil",
            Self::Vpn => "VPN",
            Self::HyperV => "Hyper-V",
            Self::Container => "WSL / Container",
            Self::VmWare => "VMware",
            Self::VirtualBox => "VirtualBox",
            Self::WifiDirect => "Wi-Fi Direct",
            Self::OtherVirtual => "Sanal",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Ethernet => "🖧",
            Self::WiFi => "📶",
            Self::Bluetooth => "ᛒ",
            Self::Cellular => "📱",
            Self::Vpn => "🛡",
            _ => "◇",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterStatus {
    Up,
    Down,
    Disconnected,
}

impl AdapterStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::Disconnected => "Bağlı değil",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdapterInfo {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub mac: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub gateways: Vec<String>,
    pub dns: Vec<String>,
    pub status: AdapterStatus,
    pub kind: AdapterKind,
    pub rx_link_bps: u64,
    pub tx_link_bps: u64,
    pub mtu: u32,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

impl AdapterInfo {
    pub fn link_bps(&self) -> u64 {
        match (self.rx_link_bps, self.tx_link_bps) {
            (0, t) => t,
            (r, 0) => r,
            (r, t) => r.min(t),
        }
    }
}

/// Her tick'te okunan hafif sayaç satırı.
#[derive(Debug, Clone, Copy)]
pub struct CounterRow {
    pub index: u32,
    pub rx: u64,
    pub tx: u64,
    pub is_virtual: bool,
}

/// Sanal adaptör sınıflandırması — ad / açıklama / tür bilgisinden.
pub fn classify(alias: &str, description: &str, if_type: u32, hardware: bool) -> AdapterKind {
    let a = alias.to_lowercase();
    let d = description.to_lowercase();
    let any = |keys: &[&str]| keys.iter().any(|k| a.contains(k) || d.contains(k));

    if any(&[
        "vpn",
        "tap-windows",
        "wireguard",
        "wintun",
        "openvpn",
        "fortinet",
        "forticlient",
        "anyconnect",
        "globalprotect",
        "pangp",
        "juniper",
        "pulse secure",
        "zerotier",
        "tailscale",
        "nordlynx",
        "protonvpn",
        "sonicwall",
        "check point",
        "cisco",
        "softether",
        "hamachi",
        "radmin",
    ]) {
        return AdapterKind::Vpn;
    }
    if any(&["wsl", "docker", "podman", "containers"]) {
        return AdapterKind::Container;
    }
    if any(&["hyper-v", "vethernet"]) {
        return AdapterKind::HyperV;
    }
    if any(&["vmware"]) {
        return AdapterKind::VmWare;
    }
    if any(&["virtualbox", "vboxnet"]) {
        return AdapterKind::VirtualBox;
    }
    if any(&[
        "wi-fi direct",
        "wifi direct",
        "local area connection*",
        "yerel ağ bağlantısı*",
    ]) {
        return AdapterKind::WifiDirect;
    }
    if any(&["bluetooth"]) {
        return AdapterKind::Bluetooth;
    }
    match if_type {
        71 => AdapterKind::WiFi,               // IF_TYPE_IEEE80211
        243 | 244 => AdapterKind::Cellular,    // WWAN PP / PP2
        53 | 131 => AdapterKind::OtherVirtual, // PROP_VIRTUAL / TUNNEL
        _ if !hardware => AdapterKind::OtherVirtual,
        _ if any(&["virtual", "loopback", "miniport", "pseudo"]) => AdapterKind::OtherVirtual,
        _ => AdapterKind::Ethernet,
    }
}

/// Açıklama/ad tabanlı gizleme: filtre sürücüsü kopyaları, WAN Miniport'lar,
/// tünel ve çekirdek hata ayıklama arayüzleri listede gürültü yapar.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_noise(alias: &str, description: &str) -> bool {
    let a = alias.to_lowercase();
    let d = description.to_lowercase();
    const NOISE: &[&str] = &[
        "wan miniport",
        "kernel debug",
        "teredo",
        "6to4",
        "isatap",
        "ip-https",
        "lightweight filter",
        "packet scheduler",
        "wfp native",
        "wfp 802.3",
        "npcap packet",
        "native wifi filter",
        "virtual wifi filter",
        "-0000",
        "-0001",
        "-0002",
        "-0003",
        "loopback pseudo-interface",
    ];
    NOISE.iter().any(|k| a.contains(k) || d.contains(k))
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------
#[cfg(windows)]
mod imp {
    use super::*;
    use std::ffi::OsString;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::os::windows::ffi::OsStringExt;

    use windows_sys::Win32::Foundation::ERROR_BUFFER_OVERFLOW;
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        FreeMibTable, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST,
        GetAdaptersAddresses, GetIfTable2, IP_ADAPTER_ADDRESSES_LH, MIB_IF_ROW2, MIB_IF_TABLE2,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, SOCKET_ADDRESS};

    const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
    const FLAG_HARDWARE: u8 = 0x01;
    const FLAG_FILTER: u8 = 0x02;

    fn wide(value: &[u16]) -> String {
        let len = value.iter().position(|c| *c == 0).unwrap_or(value.len());
        OsString::from_wide(&value[..len])
            .to_string_lossy()
            .into_owned()
    }

    unsafe fn pwstr(p: *const u16) -> String {
        if p.is_null() {
            return String::new();
        }
        let mut len = 0usize;
        unsafe {
            while *p.add(len) != 0 && len < 4096 {
                len += 1;
            }
            wide(std::slice::from_raw_parts(p, len))
        }
    }

    fn mac(row: &MIB_IF_ROW2) -> String {
        let len = (row.PhysicalAddressLength as usize).min(row.PhysicalAddress.len());
        if len == 0 {
            return "—".into();
        }
        row.PhysicalAddress[..len]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }

    fn status(row: &MIB_IF_ROW2) -> AdapterStatus {
        // IfOperStatusUp = 1, Down = 2; MediaConnectStateDisconnected = 2
        match row.OperStatus as i32 {
            1 => AdapterStatus::Up,
            2 if row.MediaConnectState as i32 == 2 => AdapterStatus::Disconnected,
            2 => AdapterStatus::Down,
            _ => AdapterStatus::Disconnected,
        }
    }

    fn include(row: &MIB_IF_ROW2, alias: &str, desc: &str) -> bool {
        let flags = row.InterfaceAndOperStatusFlags._bitfield;
        row.Type != IF_TYPE_SOFTWARE_LOOPBACK
            && row.TunnelType as i32 == 0
            && flags & FLAG_FILTER == 0
            && !is_noise(alias, desc)
    }

    fn kind_of(row: &MIB_IF_ROW2, alias: &str, desc: &str) -> AdapterKind {
        let hardware = row.InterfaceAndOperStatusFlags._bitfield & FLAG_HARDWARE != 0;
        classify(alias, desc, row.Type, hardware)
    }

    fn with_table<R>(f: impl FnOnce(&[MIB_IF_ROW2]) -> R) -> Result<R, String> {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        let rc = unsafe { GetIfTable2(&mut table) };
        if rc != 0 || table.is_null() {
            return Err(format!("GetIfTable2 başarısız (Win32 hata {rc})"));
        }
        let out = unsafe {
            let count = (*table).NumEntries as usize;
            let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), count);
            let r = f(rows);
            FreeMibTable(table as _);
            r
        };
        Ok(out)
    }

    pub fn list_adapters() -> Result<Vec<AdapterInfo>, String> {
        let addrs = addresses();
        let mut seen = std::collections::HashSet::new();
        with_table(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let alias = wide(&row.Alias);
                    let desc = wide(&row.Description);
                    if !include(row, &alias, &desc) || !seen.insert(row.InterfaceIndex) {
                        return None;
                    }
                    let a = addrs.get(&row.InterfaceIndex).cloned().unwrap_or_default();
                    Some(AdapterInfo {
                        index: row.InterfaceIndex,
                        kind: kind_of(row, &alias, &desc),
                        name: if alias.is_empty() {
                            desc.clone()
                        } else {
                            alias
                        },
                        description: desc,
                        mac: mac(row),
                        ipv4: a.ipv4,
                        ipv6: a.ipv6,
                        gateways: a.gateways,
                        dns: a.dns,
                        status: status(row),
                        rx_link_bps: row.ReceiveLinkSpeed,
                        tx_link_bps: row.TransmitLinkSpeed,
                        mtu: row.Mtu,
                        rx_bytes: row.InOctets,
                        tx_bytes: row.OutOctets,
                    })
                })
                .collect()
        })
    }

    pub fn counters() -> Result<Vec<CounterRow>, String> {
        with_table(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let alias = wide(&row.Alias);
                    let desc = wide(&row.Description);
                    if !include(row, &alias, &desc) {
                        return None;
                    }
                    Some(CounterRow {
                        index: row.InterfaceIndex,
                        rx: row.InOctets,
                        tx: row.OutOctets,
                        is_virtual: kind_of(row, &alias, &desc).is_virtual(),
                    })
                })
                .collect()
        })
    }

    #[derive(Default, Clone)]
    struct Addr {
        ipv4: Vec<String>,
        ipv6: Vec<String>,
        gateways: Vec<String>,
        dns: Vec<String>,
    }

    unsafe fn sock_ip(sa: &SOCKET_ADDRESS) -> Option<IpAddr> {
        if sa.lpSockaddr.is_null() {
            return None;
        }
        unsafe {
            let family = (*sa.lpSockaddr).sa_family;
            let base = sa.lpSockaddr as *const u8;
            if family == AF_INET {
                let b = std::slice::from_raw_parts(base.add(4), 4);
                Some(IpAddr::V4(Ipv4Addr::new(b[0], b[1], b[2], b[3])))
            } else if family == AF_INET6 {
                let mut o = [0u8; 16];
                o.copy_from_slice(std::slice::from_raw_parts(base.add(8), 16));
                Some(IpAddr::V6(Ipv6Addr::from(o)))
            } else {
                None
            }
        }
    }

    fn addresses() -> HashMap<u32, Addr> {
        let mut map = HashMap::new();
        let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
        let mut size: u32 = 32 * 1024;
        let mut buf: Vec<u64> = Vec::new();
        let mut rc = ERROR_BUFFER_OVERFLOW;
        for _ in 0..4 {
            buf = vec![0u64; (size as usize).div_ceil(8)];
            rc = unsafe {
                GetAdaptersAddresses(
                    AF_UNSPEC as u32,
                    flags,
                    std::ptr::null(),
                    buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                    &mut size,
                )
            };
            if rc != ERROR_BUFFER_OVERFLOW {
                break;
            }
        }
        if rc != 0 {
            tracing::warn!("GetAdaptersAddresses başarısız: {rc}");
            return map;
        }

        unsafe {
            let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
            while !cur.is_null() {
                let a = &*cur;
                let mut entry = Addr::default();

                let mut u = a.FirstUnicastAddress;
                while !u.is_null() {
                    if let Some(ip) = sock_ip(&(*u).Address) {
                        let s = format!("{ip}/{}", (*u).OnLinkPrefixLength);
                        match ip {
                            IpAddr::V4(_) => entry.ipv4.push(s),
                            IpAddr::V6(_) => entry.ipv6.push(s),
                        }
                    }
                    u = (*u).Next;
                }
                let mut g = a.FirstGatewayAddress;
                while !g.is_null() {
                    if let Some(ip) = sock_ip(&(*g).Address) {
                        entry.gateways.push(ip.to_string());
                    }
                    g = (*g).Next;
                }
                let mut d = a.FirstDnsServerAddress;
                while !d.is_null() {
                    if let Some(ip) = sock_ip(&(*d).Address) {
                        entry.dns.push(ip.to_string());
                    }
                    d = (*d).Next;
                }
                // IPv6 için ayrı sıra kullanılabiliyor; IfIndex ve Ipv6IfIndex ikisine de yaz.
                let idx4 = a.Anonymous1.Anonymous.IfIndex;
                let idx6 = a.Ipv6IfIndex;
                if idx4 != 0 {
                    map.insert(idx4, entry.clone());
                }
                if idx6 != 0 && idx6 != idx4 {
                    map.entry(idx6).or_insert(entry);
                }
                let _ = pwstr(a.FriendlyName);
                cur = a.Next;
            }
        }
        map
    }

    /// Duplex bilgisi MIB_IF_ROW2'de yok; MSFT_NetAdapter.FullDuplex alanını
    /// PowerShell üzerinden bir kez (arka planda) okuyoruz.
    pub fn duplex_map() -> HashMap<u32, bool> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-NetAdapter -IncludeHidden | Select-Object ifIndex,FullDuplex | ConvertTo-Json -Compress",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let mut map = HashMap::new();
        let Ok(out) = out else { return map };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
            return map;
        };
        let items = match value {
            serde_json::Value::Array(v) => v,
            other => vec![other],
        };
        for item in items {
            if let (Some(idx), Some(full)) =
                (item["ifIndex"].as_u64(), item["FullDuplex"].as_bool())
            {
                map.insert(idx as u32, full);
            }
        }
        map
    }
}

// ---------------------------------------------------------------------------
// Linux (geliştirme ortamı ve ileride port için)
// ---------------------------------------------------------------------------
#[cfg(not(windows))]
mod imp {
    use super::*;
    use std::fs;

    fn read(path: String) -> String {
        fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    }

    fn proc_net_dev() -> HashMap<String, (u64, u64)> {
        let mut map = HashMap::new();
        let text = fs::read_to_string("/proc/net/dev").unwrap_or_default();
        for line in text.lines().skip(2) {
            let Some((name, rest)) = line.split_once(':') else {
                continue;
            };
            let cols: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|c| c.parse().ok())
                .collect();
            if cols.len() >= 9 {
                map.insert(name.trim().to_string(), (cols[0], cols[8]));
            }
        }
        map
    }

    pub fn list_adapters() -> Result<Vec<AdapterInfo>, String> {
        let counters = proc_net_dev();
        let mut out = Vec::new();
        for (name, (rx, tx)) in counters {
            if name == "lo" {
                continue;
            }
            let base = format!("/sys/class/net/{name}");
            let index = read(format!("{base}/ifindex")).parse().unwrap_or(0);
            let oper = read(format!("{base}/operstate"));
            let speed: u64 = read(format!("{base}/speed"))
                .parse::<i64>()
                .unwrap_or(0)
                .max(0) as u64;
            let hardware = fs::metadata(format!("{base}/device")).is_ok();
            let wireless = fs::metadata(format!("{base}/wireless")).is_ok();
            let if_type = if wireless { 71 } else { 6 };
            out.push(AdapterInfo {
                index,
                kind: classify(&name, &name, if_type, hardware),
                description: name.clone(),
                mac: read(format!("{base}/address")).to_uppercase(),
                ipv4: vec![],
                ipv6: vec![],
                gateways: vec![],
                dns: vec![],
                status: match oper.as_str() {
                    "up" | "unknown" => AdapterStatus::Up,
                    "down" => AdapterStatus::Down,
                    _ => AdapterStatus::Disconnected,
                },
                rx_link_bps: speed * 1_000_000,
                tx_link_bps: speed * 1_000_000,
                mtu: read(format!("{base}/mtu")).parse().unwrap_or(0),
                rx_bytes: rx,
                tx_bytes: tx,
                name,
            });
        }
        out.sort_by_key(|a| a.index);
        Ok(out)
    }

    pub fn counters() -> Result<Vec<CounterRow>, String> {
        Ok(list_adapters()?
            .into_iter()
            .map(|a| CounterRow {
                index: a.index,
                rx: a.rx_bytes,
                tx: a.tx_bytes,
                is_virtual: a.kind.is_virtual(),
            })
            .collect())
    }

    pub fn duplex_map() -> HashMap<u32, bool> {
        let mut map = HashMap::new();
        if let Ok(adapters) = list_adapters() {
            for a in adapters {
                let d = read(format!("/sys/class/net/{}/duplex", a.name));
                if !d.is_empty() {
                    map.insert(a.index, d == "full");
                }
            }
        }
        map
    }
}

pub use imp::{counters, duplex_map, list_adapters};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        assert_eq!(
            classify("Wi-Fi", "Intel(R) Wi-Fi 6 AX201", 71, true),
            AdapterKind::WiFi
        );
        assert_eq!(
            classify("Ethernet", "Intel(R) Ethernet Connection", 6, true),
            AdapterKind::Ethernet
        );
        assert_eq!(
            classify(
                "vEthernet (WSL)",
                "Hyper-V Virtual Ethernet Adapter",
                6,
                false
            ),
            AdapterKind::Container
        );
        assert_eq!(
            classify(
                "vEthernet (Default Switch)",
                "Hyper-V Virtual Ethernet Adapter",
                6,
                false
            ),
            AdapterKind::HyperV
        );
        assert_eq!(
            classify(
                "Ethernet 3",
                "Fortinet Virtual Ethernet Adapter (NDIS 6.30)",
                6,
                false
            ),
            AdapterKind::Vpn
        );
        assert_eq!(
            classify(
                "VMware Network Adapter VMnet8",
                "VMware Virtual Ethernet Adapter for VMnet8",
                6,
                false
            ),
            AdapterKind::VmWare
        );
        assert_eq!(
            classify(
                "Local Area Connection* 1",
                "Microsoft Wi-Fi Direct Virtual Adapter",
                71,
                false
            ),
            AdapterKind::WifiDirect
        );
        assert!(is_noise(
            "Wi-Fi-WFP Native MAC Layer LightWeight Filter-0000",
            "x"
        ));
        assert!(!is_noise("Wi-Fi", "Intel(R) Wi-Fi 6 AX201"));
    }
}
