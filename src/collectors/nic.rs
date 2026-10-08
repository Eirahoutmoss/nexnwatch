use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetIfTable2, FreeMibTable, MIB_IF_ROW2, MIB_IF_TABLE2,
};

#[derive(Debug, Clone)]
pub struct AdapterInfo {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub mac: String,
    pub ipv4: String,
    pub ipv6: String,
    pub status: AdapterStatus,
    pub rx_link_bps: u64,
    pub tx_link_bps: u64,
    pub mtu: u32,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
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
            Self::Disconnected => "Disconnected",
        }
    }
}

fn wide_string(value: &[u16]) -> String {
    let len = value.iter().position(|c| *c == 0).unwrap_or(value.len());
    OsString::from_wide(&value[..len]).to_string_lossy().into_owned()
}

fn mac(row: &MIB_IF_ROW2) -> String {
    let len = row.PhysicalAddressLength as usize;
    let len = len.min(row.PhysicalAddress.len());
    row.PhysicalAddress[..len]
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn status(row: &MIB_IF_ROW2) -> AdapterStatus {
    // NET_IF_OPER_STATUS_UP = 1, DORMANT = 5, DOWN = 2.
    match row.OperStatus {
        1 => AdapterStatus::Up,
        2 => AdapterStatus::Down,
        _ => AdapterStatus::Disconnected,
    }
}

fn should_include(row: &MIB_IF_ROW2) -> bool {
    // Exclude loopback and tunnel interfaces. Physical-medium filtering is
    // intentionally conservative so useful Wi-Fi/Ethernet/VPN adapters are
    // not accidentally hidden from the user.
    row.InterfaceIndex != 1 && row.TunnelType == 0
}

pub fn list_adapters() -> Result<Vec<AdapterInfo>, String> {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();

    let result = unsafe { GetIfTable2(&mut table) };
    if result != 0 {
        return Err(format!("GetIfTable2 failed with Win32 error {result}"));
    }

    let adapters = unsafe {
        let count = (*table).NumEntries as usize;
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), count);
        let result = rows
            .iter()
            .filter(|row| should_include(row))
            .map(|row| AdapterInfo {
                index: row.InterfaceIndex,
                name: wide_string(&row.Alias),
                description: wide_string(&row.Description),
                mac: mac(row),
                ipv4: "IPv4: discovery in RC2".into(),
                ipv6: "IPv6: discovery in RC2".into(),
                status: status(row),
                rx_link_bps: row.ReceiveLinkSpeed,
                tx_link_bps: row.TransmitLinkSpeed,
                mtu: row.Mtu,
                rx_bytes: row.InOctets,
                tx_bytes: row.OutOctets,
            })
            .collect::<Vec<_>>();
        FreeMibTable(table as _);
        result
    };

    Ok(adapters)
}

pub fn all_adapter_counters() -> Result<(u64, u64), String> {
    let adapters = list_adapters()?;
    Ok(adapters.into_iter().fold((0_u64, 0_u64), |(rx, tx), a| {
        (rx.saturating_add(a.rx_bytes), tx.saturating_add(a.tx_bytes))
    }))
}

pub fn adapter_counters(index: u32) -> Result<(u64, u64), String> {
    let adapters = list_adapters()?;
    adapters
        .into_iter()
        .find(|a| a.index == index)
        .map(|a| (a.rx_bytes, a.tx_bytes))
        .ok_or_else(|| format!("Adapter index {index} not found"))
}
