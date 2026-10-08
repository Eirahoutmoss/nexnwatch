//! Ağ Adaptörleri sayfası — fiziksel ve sanal gruplar, tüm ayrıntılar.

use iced::widget::{Space, column, container, row, text};
use iced::{Alignment, Element, Length};

use crate::app::{App, Message};
use crate::collectors::nic::{AdapterInfo, AdapterStatus};
use crate::theme::p;
use crate::ui::{card_with, info_row, panel_style, small_button};
use crate::units;

pub fn view(app: &App) -> Element<'_, Message> {
    let physical: Vec<&AdapterInfo> = app
        .adapters
        .iter()
        .filter(|a| !a.kind.is_virtual())
        .collect();
    let mut virtuals: Vec<&AdapterInfo> = app
        .adapters
        .iter()
        .filter(|a| a.kind.is_virtual())
        .collect();
    virtuals.sort_by_key(|a| {
        (
            a.kind.label(),
            a.status != AdapterStatus::Up,
            a.name.clone(),
        )
    });

    let phys = card_with(
        "Fiziksel Adaptörler",
        "Ethernet, Wi-Fi, Bluetooth, mobil — \"Tüm Adaptörler\" toplamı bunlardan oluşur",
        small_button("↻ Yenile", Message::RefreshAdapters, false),
        grid(app, &physical),
    );

    let open = app.cfg.virtual_group_open;
    let up = virtuals
        .iter()
        .filter(|a| a.status == AdapterStatus::Up)
        .count();
    let virt_body: Element<'_, Message> = if open {
        grid(app, &virtuals)
    } else {
        text(format!(
            "{} sanal adaptör ({} aktif): {}",
            virtuals.len(),
            up,
            kinds_summary(&virtuals)
        ))
        .size(11)
        .color(p().muted)
        .into()
    };
    let virt = card_with(
        &format!("Sanal Adaptörler ({})", virtuals.len()),
        "VPN, Hyper-V / WSL, VMware, VirtualBox, Wi-Fi Direct… — trafikleri fiziksel adaptör üzerinden de geçtiği için toplama katılmaz",
        small_button(
            if open { "Gizle" } else { "Göster" },
            Message::ToggleVirtualGroup,
            false,
        ),
        virt_body,
    );

    column![phys, virt].spacing(10).into()
}

fn kinds_summary(list: &[&AdapterInfo]) -> String {
    let mut kinds: Vec<&str> = list.iter().map(|a| a.kind.label()).collect();
    kinds.sort();
    kinds.dedup();
    if kinds.is_empty() {
        "—".into()
    } else {
        kinds.join(", ")
    }
}

fn grid<'a>(app: &'a App, list: &[&'a AdapterInfo]) -> Element<'a, Message> {
    if list.is_empty() {
        return text("Adaptör bulunamadı.").size(11).color(p().muted).into();
    }
    let mut col = column![].spacing(10);
    for pair in list.chunks(2) {
        let mut r = row![].spacing(10);
        for a in pair {
            r = r.push(adapter_card(app, a));
        }
        if pair.len() == 1 {
            r = r.push(Space::new().width(Length::Fill));
        }
        col = col.push(r);
    }
    col.into()
}

fn adapter_card<'a>(app: &'a App, a: &'a AdapterInfo) -> Element<'a, Message> {
    let up = a.status == AdapterStatus::Up;
    let rate = app.rates.get(&a.index).copied().unwrap_or_default();
    let unit = app.cfg.unit;
    let duplex = match app.duplex.get(&a.index) {
        Some(true) => "Full",
        Some(false) => "Half",
        None => "—",
    };
    let list = |v: &Vec<String>| {
        if v.is_empty() {
            "—".to_string()
        } else {
            v.join("\n")
        }
    };
    let selected = app.selected == Some(a.index);

    container(
        column![
            row![
                text(a.kind.icon()).size(18).color(p().accent),
                column![
                    text(a.name.clone()).size(14).color(p().title),
                    text(a.description.clone()).size(10).color(p().muted),
                ]
                .spacing(1),
                Space::new().width(Length::Fill),
                text(format!("● {}", a.status.label()))
                    .size(11)
                    .color(if up { p().good } else { p().bad }),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            row![
                text(format!("↓ {}", units::speed(rate.rx, unit)))
                    .size(14)
                    .color(p().rx),
                text(format!("↑ {}", units::speed(rate.tx, unit)))
                    .size(14)
                    .color(p().tx),
                Space::new().width(Length::Fill),
                if selected {
                    Element::from(text("İzleniyor").size(11).color(p().accent))
                } else {
                    small_button("İzle", Message::SelectAdapter(Some(a.index)), false)
                },
            ]
            .spacing(14)
            .align_y(Alignment::Center),
            row![
                column![
                    info_row("Tür", a.kind.label().to_string()),
                    info_row("MAC", a.mac.clone()),
                    info_row(
                        "Link (↓/↑)",
                        format!(
                            "{} / {}",
                            units::link(a.rx_link_bps),
                            units::link(a.tx_link_bps)
                        )
                    ),
                    info_row("Duplex", duplex.to_string()),
                    info_row("MTU", a.mtu.to_string()),
                    info_row("Arayüz No", a.index.to_string()),
                ]
                .spacing(4)
                .width(Length::FillPortion(1)),
                column![
                    info_row("IPv4", list(&a.ipv4)),
                    info_row("IPv6", list(&a.ipv6)),
                    info_row("Ağ Geçidi", list(&a.gateways)),
                    info_row("DNS", list(&a.dns)),
                    info_row(
                        "Açılıştan beri",
                        format!(
                            "↓ {}  ↑ {}",
                            units::bytes(a.rx_bytes),
                            units::bytes(a.tx_bytes)
                        )
                    ),
                ]
                .spacing(4)
                .width(Length::FillPortion(1)),
            ]
            .spacing(14),
        ]
        .spacing(10),
    )
    .padding(12)
    .width(Length::Fill)
    .style(move |_| {
        panel_style(
            p().panel_alt,
            if selected { p().accent } else { p().border },
            8.0,
        )
    })
    .into()
}
