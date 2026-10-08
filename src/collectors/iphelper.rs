//! ETW yedeği: IP Helper ile TCP bağlantı bazlı bayt sayımı.
//!
//! Saniyede bir `GetExtendedTcpTable` (IPv4 + IPv6, OWNER_PID) okunur; her
//! ESTABLISHED bağlantı için `SetPerTcpConnectionEStats` ile veri istatistiği
//! açılır ve `GetPerTcp(6)ConnectionEStats` → DataBytesIn/Out farkları PID'e
//! yazılır. Sınırlamalar: UDP trafiği ve bir saniyeden kısa ömürlü bağlantılar
//! görünmez — planda belirtildiği gibi ETW birincil kaynaktır.

use crate::collectors::etw::EtwHandle;

#[cfg(windows)]
mod imp {
    use std::collections::{HashMap, HashSet};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, GetPerTcp6ConnectionEStats, GetPerTcpConnectionEStats, MIB_TCP6ROW,
        MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_LH, MIB_TCPROW_OWNER_PID, SetPerTcp6ConnectionEStats,
        SetPerTcpConnectionEStats, TCP_ESTATS_DATA_ROD_v0, TCP_ESTATS_DATA_RW_v0,
        TCP_TABLE_OWNER_PID_ALL, TcpConnectionEstatsData,
    };
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};

    use crate::collectors::etw::{accumulate, ConnBytes, ConnKey, EtwHandle, PidBytes, Proto};

    const ESTABLISHED: u32 = 5;

    pub fn start(handle: EtwHandle) {
        if handle.fallback.swap(true, Ordering::SeqCst) {
            return;
        }
        tracing::warn!("ETW kullanılamıyor; IP Helper (TCP EStats) yedeğine geçildi");
        std::thread::Builder::new()
            .name("iphelper-fallback".into())
            .spawn(move || {
                let mut prev: HashMap<ConnKey, (u64, u64)> = HashMap::new();
                loop {
                    poll(&handle, &mut prev);
                    std::thread::sleep(Duration::from_secs(1));
                }
            })
            .ok();
    }

    /// GetExtendedTcpTable çağrısını gerekli tampon boyutuyla yap.
    fn table(af: u16) -> Option<Vec<u64>> {
        let mut size: u32 = 0;
        unsafe {
            GetExtendedTcpTable(std::ptr::null_mut(), &mut size, 0, af as u32, TCP_TABLE_OWNER_PID_ALL, 0);
        }
        for _ in 0..3 {
            let mut buf = vec![0u64; (size as usize).div_ceil(8) + 64];
            let mut sz = (buf.len() * 8) as u32;
            let rc = unsafe {
                GetExtendedTcpTable(buf.as_mut_ptr().cast(), &mut sz, 0, af as u32, TCP_TABLE_OWNER_PID_ALL, 0)
            };
            if rc == 0 {
                return Some(buf);
            }
            size = sz;
        }
        None
    }

    fn port(p: u32) -> u16 {
        u16::from_be(p as u16)
    }

    fn poll(handle: &EtwHandle, prev: &mut HashMap<ConnKey, (u64, u64)>) {
        let now = Instant::now();
        let mut pids: HashMap<u32, PidBytes> = HashMap::new();
        let mut conns: HashMap<ConnKey, ConnBytes> = HashMap::new();
        let mut seen: HashSet<ConnKey> = HashSet::new();

        // ---- IPv4
        if let Some(buf) = table(AF_INET) {
            unsafe {
                let count = *(buf.as_ptr() as *const u32) as usize;
                let rows = std::slice::from_raw_parts(
                    (buf.as_ptr() as *const u8).add(4) as *const MIB_TCPROW_OWNER_PID,
                    count,
                );
                for r in rows.iter().filter(|r| r.dwState == ESTABLISHED) {
                    let mut row: MIB_TCPROW_LH = std::mem::zeroed();
                    row.Anonymous.dwState = r.dwState;
                    row.dwLocalAddr = r.dwLocalAddr;
                    row.dwLocalPort = r.dwLocalPort;
                    row.dwRemoteAddr = r.dwRemoteAddr;
                    row.dwRemotePort = r.dwRemotePort;
                    let key = ConnKey {
                        pid: r.dwOwningPid,
                        proto: Proto::Tcp,
                        a: SocketAddr::new(IpAddr::V4(Ipv4Addr::from(r.dwRemoteAddr.to_ne_bytes())), port(r.dwRemotePort)),
                        b: SocketAddr::new(IpAddr::V4(Ipv4Addr::from(r.dwLocalAddr.to_ne_bytes())), port(r.dwLocalPort)),
                    };
                    if !prev.contains_key(&key) {
                        let rw = TCP_ESTATS_DATA_RW_v0 { EnableCollection: true };
                        SetPerTcpConnectionEStats(
                            &row,
                            TcpConnectionEstatsData,
                            (&rw as *const TCP_ESTATS_DATA_RW_v0).cast(),
                            0,
                            std::mem::size_of::<TCP_ESTATS_DATA_RW_v0>() as u32,
                            0,
                        );
                    }
                    let mut rod: TCP_ESTATS_DATA_ROD_v0 = std::mem::zeroed();
                    let rc = GetPerTcpConnectionEStats(
                        &row,
                        TcpConnectionEstatsData,
                        std::ptr::null_mut(),
                        0,
                        0,
                        std::ptr::null_mut(),
                        0,
                        0,
                        (&mut rod as *mut TCP_ESTATS_DATA_ROD_v0).cast(),
                        0,
                        std::mem::size_of::<TCP_ESTATS_DATA_ROD_v0>() as u32,
                    );
                    if rc == 0 {
                        record(prev, &mut seen, &mut pids, &mut conns, key, rod.DataBytesIn, rod.DataBytesOut, now);
                    }
                }
            }
        }

        // ---- IPv6
        if let Some(buf) = table(AF_INET6) {
            unsafe {
                let count = *(buf.as_ptr() as *const u32) as usize;
                let rows = std::slice::from_raw_parts(
                    (buf.as_ptr() as *const u8).add(4) as *const MIB_TCP6ROW_OWNER_PID,
                    count,
                );
                for r in rows.iter().filter(|r| r.dwState == ESTABLISHED) {
                    let mut row: MIB_TCP6ROW = std::mem::zeroed();
                    row.State = r.dwState as _;
                    row.LocalAddr.u.Byte = r.ucLocalAddr;
                    row.dwLocalScopeId = r.dwLocalScopeId;
                    row.dwLocalPort = r.dwLocalPort;
                    row.RemoteAddr.u.Byte = r.ucRemoteAddr;
                    row.dwRemoteScopeId = r.dwRemoteScopeId;
                    row.dwRemotePort = r.dwRemotePort;
                    let key = ConnKey {
                        pid: r.dwOwningPid,
                        proto: Proto::Tcp,
                        a: SocketAddr::new(IpAddr::V6(Ipv6Addr::from(r.ucRemoteAddr)), port(r.dwRemotePort)),
                        b: SocketAddr::new(IpAddr::V6(Ipv6Addr::from(r.ucLocalAddr)), port(r.dwLocalPort)),
                    };
                    if !prev.contains_key(&key) {
                        let rw = TCP_ESTATS_DATA_RW_v0 { EnableCollection: true };
                        SetPerTcp6ConnectionEStats(
                            &row,
                            TcpConnectionEstatsData,
                            (&rw as *const TCP_ESTATS_DATA_RW_v0).cast(),
                            0,
                            std::mem::size_of::<TCP_ESTATS_DATA_RW_v0>() as u32,
                            0,
                        );
                    }
                    let mut rod: TCP_ESTATS_DATA_ROD_v0 = std::mem::zeroed();
                    let rc = GetPerTcp6ConnectionEStats(
                        &row,
                        TcpConnectionEstatsData,
                        std::ptr::null_mut(),
                        0,
                        0,
                        std::ptr::null_mut(),
                        0,
                        0,
                        (&mut rod as *mut TCP_ESTATS_DATA_ROD_v0).cast(),
                        0,
                        std::mem::size_of::<TCP_ESTATS_DATA_ROD_v0>() as u32,
                    );
                    if rc == 0 {
                        record(prev, &mut seen, &mut pids, &mut conns, key, rod.DataBytesIn, rod.DataBytesOut, now);
                    }
                }
            }
        }

        prev.retain(|k, _| seen.contains(k));
        handle.events.fetch_add(seen.len() as u64, Ordering::Relaxed);
        handle.merge(&mut pids, &mut conns);
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        prev: &mut HashMap<ConnKey, (u64, u64)>,
        seen: &mut HashSet<ConnKey>,
        pids: &mut HashMap<u32, PidBytes>,
        conns: &mut HashMap<ConnKey, ConnBytes>,
        key: ConnKey,
        bytes_in: u64,
        bytes_out: u64,
        now: Instant,
    ) {
        seen.insert(key);
        let (pin, pout) = prev.get(&key).copied().unwrap_or((0, 0));
        let din = bytes_in.saturating_sub(pin);
        let dout = bytes_out.saturating_sub(pout);
        prev.insert(key, (bytes_in, bytes_out));
        if din > 0 || dout > 0 {
            accumulate(pids, conns, Some(key), key.pid, din, dout, now);
        }
    }
}

#[cfg(windows)]
pub use imp::start;

#[cfg(not(windows))]
pub fn start(_handle: EtwHandle) {
    tracing::info!("IP Helper yedeği yalnızca Windows'ta");
}
