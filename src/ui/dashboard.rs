//! Ana Ekran — konsept görseldeki düzen.

use std::time::Duration;

use iced::widget::{button, canvas, column, container, progress_bar, row, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

use crate::app::{App, Message, Page, ALL_LABEL};
use crate::collectors::etw::EtwStatus;
use crate::collectors::nic::{AdapterInfo, AdapterStatus};
use crate::theme::p;
use crate::ui::chart::LineChart;
use crate::ui::{bar, card, card_with, chip, info_row, metric, panel_style, small_button, th, tree};
use crate::units::{self, DisplayUnit};

pub fn view(app: &App) -> Element<'_, Message> {
    let main = column![
        realtime(app),
        row![usage(app), speed_mini(app).width(Length::Fixed(330.0))].spacing(10),
        row![top_processes(app), tree_card(app)].spacing(10),
    ]
    .spacing(10)
    .width(Length::Fill);

    row![sidebar(app), main].spacing(10).into()
}

// ---------------------------------------------------------------------------
// Sol panel: adaptörler (fiziksel + gruplanmış sanal)
// ---------------------------------------------------------------------------

pub fn sidebar(app: &App) -> Element<'_, Message> {
    let physical: Vec<&AdapterInfo> = app.adapters.iter().filter(|a| !a.kind.is_virtual()).collect();
    let virtuals: Vec<&AdapterInfo> = app.adapters.iter().filter(|a| a.kind.is_virtual()).collect();

    let mut list = column![all_item(app)].spacing(5);
    for a in &physical {
        list = list.push(adapter_item(app, a, false));
    }

    // Sanal adaptör grubu
    let up = virtuals.iter().filter(|a| a.status == AdapterStatus::Up).count();
    let open = app.cfg.virtual_group_open;
    let group_head = button(
        row![
            text(if open { "▾" } else { "▸" }).size(12).color(p().accent),
            text(format!("Sanal Adaptörler ({})", virtuals.len())).size(12).color(p().text),
            Space::new().width(Length::Fill),
            text(format!("{up} aktif")).size(10).color(if up > 0 { p().good } else { p().muted }),
        ]
        .spacing(7)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([7_u16, 9_u16]))
    .width(Length::Fill)
    .on_press(Message::ToggleVirtualGroup)
    .style(|_, status| button::Style {
        background: Some(Background::Color(if matches!(status, button::Status::Hovered) { p().selected } else { p().panel_alt })),
        text_color: p().text,
        border: Border { color: p().border, width: 1.0, radius: 6.0.into() },
        ..Default::default()
    });
    if !virtuals.is_empty() {
        list = list.push(Space::new().height(4)).push(group_head);
        if open {
            let mut inner = column![].spacing(4);
            for a in &virtuals {
                inner = inner.push(adapter_item(app, a, true));
            }
            list = list.push(container(inner).padding(Padding { left: 10.0, ..Padding::ZERO }));
        }
    }

    let info: Element<'_, Message> = match app.selected_adapter() {
        Some(a) => adapter_info(app, a),
        None => column![
            text("ADAPTÖR BİLGİLERİ").size(11).color(p().accent),
            text("Tüm fiziksel adaptörlerin toplamı gösteriliyor. Sanal adaptörler (VPN, Hyper-V…) aynı trafiği ikinci kez saydığı için toplama katılmaz.")
                .size(10)
                .color(p().muted),
        ]
        .spacing(6)
        .into(),
    };

    container(
        column![
            row![
                text("Ağ Adaptörleri").size(15).color(p().title),
                Space::new().width(Length::Fill),
                small_button("↻", Message::RefreshAdapters, false),
            ]
            .align_y(Alignment::Center),
            list,
            container(info).padding(12).width(Length::Fill).style(|_| panel_style(p().panel_alt, p().border, 8.0)),
        ]
        .spacing(8),
    )
    .width(Length::Fixed(300.0))
    .padding(12)
    .style(|_| panel_style(p().panel, p().border, 10.0))
    .into()
}

fn all_item(app: &App) -> Element<'_, Message> {
    let selected = app.selected.is_none();
    item_button(
        "∑".into(),
        ALL_LABEL.to_string(),
        "Toplam fiziksel trafik".into(),
        "".into(),
        p().accent,
        selected,
        Message::SelectAdapter(None),
    )
}

fn adapter_item<'a>(app: &'a App, a: &'a AdapterInfo, compact: bool) -> Element<'a, Message> {
    let up = a.status == AdapterStatus::Up;
    let rate = app.rates.get(&a.index).copied().unwrap_or_default();
    let sub = if compact {
        format!("{} · {}", a.kind.label(), short(&a.description, 22))
    } else {
        short(&a.description, 30)
    };
    let status = if up {
        if rate.rx + rate.tx > 0.0 {
            format!("↓{}", units::speed(rate.rx, DisplayUnit::Auto))
        } else {
            "Up".into()
        }
    } else {
        a.status.label().to_string()
    };
    item_button(
        a.kind.icon().to_string(),
        short(&a.name, 24),
        sub,
        status,
        if up { p().good } else { p().bad },
        app.selected == Some(a.index),
        Message::SelectAdapter(Some(a.index)),
    )
}

fn item_button<'a>(
    icon: String,
    title: String,
    sub: String,
    status: String,
    status_color: Color,
    selected: bool,
    msg: Message,
) -> Element<'a, Message> {
    button(
        row![
            text(icon).size(15).color(p().accent).width(Length::Fixed(20.0)),
            column![
                text(title).size(12).color(if selected { p().title } else { p().text }),
                text(sub).size(9).color(p().muted),
            ]
            .spacing(1)
            .width(Length::Fill),
            text(status).size(10).color(status_color),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([7_u16, 9_u16]))
    .width(Length::Fill)
    .on_press(msg)
    .style(move |_, st| {
        let hovered = matches!(st, button::Status::Hovered);
        button::Style {
            background: Some(Background::Color(if selected {
                p().selected
            } else if hovered {
                p().panel_alt
            } else {
                Color::TRANSPARENT
            })),
            text_color: p().text,
            border: Border {
                color: if selected { p().accent } else { p().border },
                width: if selected { 1.0 } else { 0.5 },
                radius: 6.0.into(),
            },
            ..Default::default()
        }
    })
    .into()
}

pub fn adapter_info<'a>(app: &'a App, a: &'a AdapterInfo) -> Element<'a, Message> {
    let duplex = match app.duplex.get(&a.index) {
        Some(true) => "Full",
        Some(false) => "Half",
        None => "—",
    };
    let link = if a.rx_link_bps != a.tx_link_bps && a.rx_link_bps != 0 && a.tx_link_bps != 0 {
        format!("↓{} / ↑{}", units::link(a.rx_link_bps), units::link(a.tx_link_bps))
    } else {
        units::link(a.link_bps())
    };
    column![
        text("ADAPTÖR BİLGİLERİ").size(11).color(p().accent),
        info_row("Tür", a.kind.label().to_string()),
        info_row("MAC Adresi", a.mac.clone()),
        info_row("IPv4", first_or_dash(&a.ipv4)),
        info_row("IPv6", first_or_dash(&a.ipv6)),
        info_row("Ağ Geçidi", first_or_dash(&a.gateways)),
        info_row("Link Hızı", link),
        info_row("Duplex", duplex.to_string()),
        info_row("MTU", a.mtu.to_string()),
    ]
    .spacing(5)
    .into()
}

fn first_or_dash(v: &[String]) -> String {
    match v.len() {
        0 => "—".into(),
        1 => v[0].clone(),
        n => format!("{} (+{})", v[0], n - 1),
    }
}

pub fn short(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Gerçek zamanlı trafik
// ---------------------------------------------------------------------------

fn realtime(app: &App) -> Element<'_, Message> {
    let unit = app.cfg.unit;
    let chips = row(DisplayUnit::ALL.iter().map(|u| chip(u.label().to_string(), *u == unit, Message::SetUnit(*u))))
        .spacing(4);

    let link = app.selected_adapter().map(|a| a.link_bps()).unwrap_or(0);
    let util = |bps: f64| {
        if link > 0 {
            format!("Link kullanımı %{:.1}", bps * 8.0 / link as f64 * 100.0)
        } else {
            "Gerçek zamanlı".to_string()
        }
    };

    let metrics = row![
        metric("↓", "İndirme (RX)", units::speed(app.rx_speed, unit), p().rx, util(app.rx_speed)),
        metric("↑", "Yükleme (TX)", units::speed(app.tx_speed, unit), p().tx, util(app.tx_speed)),
    ]
    .spacing(10);

    let points = app.chart_points();
    let data: Vec<(f64, f64)> = app.history.iter().rev().take(points).rev().copied().collect();
    let chart = canvas(LineChart {
        series: vec![
            (data.iter().map(|d| d.0).collect(), p().rx),
            (data.iter().map(|d| d.1).collect(), p().tx),
        ],
        fmt: Box::new(move |v| units::speed(v, unit)),
        span_secs: Some(app.chart_secs),
        capacity: points,
    })
    .width(Length::Fill)
    .height(230);

    let ranges = row![
        text("■ İndirme (RX)").size(11).color(p().rx),
        text("■ Yükleme (TX)").size(11).color(p().tx),
        Space::new().width(Length::Fill),
        chip("Son 60 sn".into(), app.chart_secs == 60, Message::SetChartRange(60)),
        chip("5 dk".into(), app.chart_secs == 300, Message::SetChartRange(300)),
        chip("10 dk".into(), app.chart_secs == 600, Message::SetChartRange(600)),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let title = format!("Gerçek Zamanlı Ağ Trafiği — {}", app.selected_label());
    card_with(&title, "", chips, column![metrics, ranges, chart].spacing(10))
}

// ---------------------------------------------------------------------------
// Toplam kullanım
// ---------------------------------------------------------------------------

fn usage(app: &App) -> Element<'_, Message> {
    let covered = app.rolling.covered();
    let win = |label: &'static str, secs: u64| {
        let (rx, tx) = app.rolling.total_since(Duration::from_secs(secs));
        let partial = covered < Duration::from_secs(secs.saturating_sub(2));
        usage_box(label, rx, tx, if partial { Some(covered.as_secs()) } else { None })
    };
    let today = app.usage.today();
    let week = app.usage.this_week();
    let month = app.usage.this_month();

    let note = if app.speed.is_running() {
        "Hız testi sürüyor — bu trafik pencere toplamlarına katılmıyor."
    } else if app.excluded_total > 0 {
        "Hız testi trafiği pencere toplamlarından hariç tutuldu."
    } else {
        ""
    };

    card(
        "Toplam Kullanım",
        note,
        column![
            row![win("Son 1 Dakika", 60), win("Son 5 Dakika", 300), win("Son 10 Dakika", 600)].spacing(8),
            row![
                usage_box("Bugün", today.rx, today.tx, None),
                usage_box("Bu Hafta", week.rx, week.tx, None),
                usage_box("Bu Ay", month.rx, month.tx, None),
            ]
            .spacing(8),
        ]
        .spacing(8),
    )
}

fn usage_box<'a>(label: &'static str, rx: u64, tx: u64, partial_secs: Option<u64>) -> Element<'a, Message> {
    let mut head = row![text(label).size(11).color(p().muted)].spacing(6);
    if let Some(s) = partial_secs {
        head = head.push(text(format!("({} sn veri)", s)).size(9).color(p().muted));
    }
    container(
        column![
            head,
            row![text("↓").size(13).color(p().rx), text(units::bytes(rx)).size(16).color(p().rx)].spacing(5),
            row![text("↑").size(13).color(p().tx), text(units::bytes(tx)).size(16).color(p().tx)].spacing(5),
            text(format!("Toplam {}", units::bytes(rx.saturating_add(tx)))).size(11).color(p().text),
        ]
        .spacing(3),
    )
    .padding(10)
    .width(Length::Fill)
    .style(|_| panel_style(p().panel_alt, p().border, 8.0))
    .into()
}

// ---------------------------------------------------------------------------
// Hız testi (küçük kart)
// ---------------------------------------------------------------------------

pub fn speed_mini(app: &App) -> container::Container<'_, Message> {
    let st = app.speed.snapshot();
    let running = app.speed.is_running();
    let last = st.history.last();

    let body: Element<'_, Message> = if running {
        column![
            text(st.phase.label()).size(12).color(p().warn),
            text(units::mbps(st.live_mbps)).size(26).color(p().title),
            progress_bar(0.0..=1.0, st.progress).girth(6),
        ]
        .spacing(6)
        .into()
    } else if let Some(r) = last {
        let ago = (chrono::Local::now().timestamp() - r.timestamp).max(0) as u64;
        column![
            row![
                text(format!("↓ {}", units::mbps(r.download_mbps))).size(18).color(p().rx),
                text(format!("↑ {}", units::mbps(r.upload_mbps))).size(18).color(p().tx),
            ]
            .spacing(14),
            row![
                text(format!("Ping {:.0} ms", r.ping_ms)).size(12).color(p().text),
                text(format!("Jitter {:.1} ms", r.jitter_ms)).size(12).color(p().text),
            ]
            .spacing(14),
            text(format!("{} · {}", r.server, units::ago(ago))).size(10).color(p().muted),
        ]
        .spacing(6)
        .into()
    } else {
        text("Henüz test yapılmadı.").size(11).color(p().muted).into()
    };

    let err: Element<'_, Message> = match &st.last_error {
        Some(e) => text(format!("Son hata: {e}")).size(10).color(p().bad).into(),
        None => Space::new().into(),
    };
    let auto = if app.cfg.speedtest_auto {
        format!("Otomatik: her {} dk", app.cfg.speedtest_interval_min)
    } else {
        "Otomatik: kapalı".to_string()
    };

    let trailing: Element<'_, Message> = if running {
        text("⚡ Ölçülüyor").size(11).color(p().warn).into()
    } else {
        small_button("⚡ Test Başlat", Message::SpeedTest, true)
    };

    container(card_with("Hız Testi", &auto, trailing, column![body, err].spacing(6)))
}

// ---------------------------------------------------------------------------
// Process'ler
// ---------------------------------------------------------------------------

fn top_processes(app: &App) -> Element<'_, Message> {
    let unit = app.cfg.unit;
    let mut list: Vec<_> = app.traffic.iter().filter(|(_, s)| s.total() > 0).collect();
    list.sort_by(|a, b| b.1.total().cmp(&a.1.total()));
    let max = list.first().map(|(_, s)| s.total()).unwrap_or(1).max(1);

    let mut rows = column![
        row![
            th("PROCESS", Length::Fill),
            th("PID", Length::Fixed(56.0)),
            th("↓ HIZ", Length::Fixed(80.0)),
            th("↓ TOPLAM", Length::Fixed(74.0)),
            th("↑ TOPLAM", Length::Fixed(74.0)),
        ]
        .spacing(6)
    ]
    .spacing(4);

    for (pid, s) in list.iter().take(7) {
        let name = app.process(**pid).map(|p| p.name.clone()).unwrap_or_else(|| "(sonlandı)".into());
        let frac = s.total() as f32 / max as f32;
        let pid = **pid;
        rows = rows.push(
            button(
                column![
                    row![
                        text(name).size(12).color(p().text).width(Length::Fill),
                        text(pid.to_string()).size(11).color(p().muted).width(Length::Fixed(56.0)),
                        text(units::speed(s.rx_per_sec, unit)).size(11).color(p().rx).width(Length::Fixed(80.0)),
                        text(units::bytes(s.rx_bytes)).size(11).color(p().text).width(Length::Fixed(74.0)),
                        text(units::bytes(s.tx_bytes)).size(11).color(p().text).width(Length::Fixed(74.0)),
                    ]
                    .spacing(6),
                    bar(frac, p().accent_strong),
                ]
                .spacing(3),
            )
            .padding(Padding::from([4_u16, 4_u16]))
            .width(Length::Fill)
            .on_press(Message::SelectRoot(pid))
            .style(|_, st| button::Style {
                background: Some(Background::Color(if matches!(st, button::Status::Hovered) { p().panel_alt } else { Color::TRANSPARENT })),
                text_color: p().text,
                border: Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            }),
        );
    }

    let empty: Element<'_, Message> = match app.etw.status() {
        EtwStatus::Failed(e) => text(format!("ETW kullanılamıyor: {e}")).size(11).color(p().bad).into(),
        _ if list.is_empty() => text("Trafik bekleniyor…").size(11).color(p().muted).into(),
        _ => Space::new().into(),
    };

    card_with(
        "En Çok Trafik Kullanan Process'ler",
        "ETW · uygulama açıldığından beri",
        small_button("Tümünü Gör", Message::Navigate(Page::Processes), false),
        column![rows, empty].spacing(6),
    )
}

fn tree_card(app: &App) -> Element<'_, Message> {
    card("Process Ağacı", "Seçili uygulama ve tüm child process'leri", tree::panel(app, 210.0))
}
