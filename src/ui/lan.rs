//! LAN Testi — iki bilgisayar arası hız (iPerf benzeri) ve ağ paylaşımı (SMB) testi.

use iced::widget::{
    Space, button, canvas, column, container, progress_bar, row, scrollable, text, text_input,
    toggler,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

use crate::app::{App, Message};
use crate::collectors::nic::AdapterStatus;
use crate::theme::p;
use crate::ui::chart::LineChart;
use crate::ui::{card, card_with, chip, info_row, panel_style, small_button, th};
use crate::units;
use crate::workers::lan::{DISCOVERY_PORT, LanMode, LanResult, PORT};

pub fn view(app: &App) -> Element<'_, Message> {
    let st = app.lan.snapshot();
    let running = app.lan.is_running();

    let top = row![server_card(app, &st), target_card(app, running)].spacing(10);

    let mut col = column![top].spacing(10);
    if running || !st.live_series.is_empty() {
        col = col.push(live_card(app, &st, running));
    }
    if let Some(e) = &st.last_error {
        col = col.push(
            container(
                text(format!("Son test başarısız: {e}"))
                    .size(12)
                    .color(p().bad),
            )
            .padding(12)
            .width(Length::Fill)
            .style(|_| panel_style(p().panel, p().bad, 8.0)),
        );
    }
    if let Some(r) = st.history.last() {
        col = col.push(result_card(app, r));
    }
    col = col.push(share_card(app, running));
    col = col.push(history_card(&st.history));
    col.into()
}

/// Bu makinenin fiziksel, bağlı IPv4 adresleri.
fn my_ips(app: &App) -> Vec<String> {
    app.adapters
        .iter()
        .filter(|a| a.status == AdapterStatus::Up && !a.kind.is_virtual())
        .flat_map(|a| {
            a.ipv4
                .iter()
                .map(move |ip| format!("{} ({})", ip.split('/').next().unwrap_or(ip), a.name))
        })
        .collect()
}

fn server_card<'a>(app: &'a App, st: &crate::workers::lan::LanState) -> Element<'a, Message> {
    let on = app.lan.server_running();
    let ips = my_ips(app);
    let ip_text = if ips.is_empty() {
        "—".to_string()
    } else {
        ips.join("\n")
    };
    let status = if on {
        if st.server_sessions > 0 {
            format!("● Test yapılıyor ({} bağlantı)", st.server_sessions)
        } else {
            "● Bekliyor — diğer bilgisayarlar bu makineyi listede görür".to_string()
        }
    } else {
        "○ Kapalı".to_string()
    };
    container(card(
        "Bu Bilgisayar — Sunucu",
        "Karşı bilgisayar buna bağlanarak ölçüm yapar",
        column![
            row![
                text("Sunucu modu").size(13).color(p().title),
                Space::new().width(Length::Fill),
                toggler(on).on_toggle(Message::LanServer),
            ]
            .align_y(Alignment::Center),
            text(status).size(11).color(if on { p().good } else { p().muted }),
            info_row("IP adresleri", ip_text),
            info_row("Portlar", format!("TCP {PORT} (test) · UDP {DISCOVERY_PORT} (keşif)")),
            info_row("Sunulan veri", units::bytes(st.server_bytes)),
            text("Windows Güvenlik Duvarı'na yalnızca Özel/Etki alanı ağları için izin kuralı otomatik eklenir. Genel (Public) ağda bağlantılar engellenebilir.")
                .size(10)
                .color(p().muted),
        ]
        .spacing(8),
    ))
    .width(Length::FillPortion(2))
    .into()
}

fn target_card(app: &App, running: bool) -> Element<'_, Message> {
    let peers = app.lan.peers();
    let mut peer_list = column![].spacing(4);
    if peers.is_empty() {
        peer_list = peer_list.push(
            text("Ağda sunucu modu açık NexNWatch bulunamadı. Karşı bilgisayarda LAN Testi → Sunucu modu'nu açın ya da IP'yi elle yazın.")
                .size(11)
                .color(p().muted),
        );
    }
    for peer in peers {
        let ip = if peer.port == PORT {
            peer.ip.to_string()
        } else {
            format!("{}:{}", peer.ip, peer.port)
        };
        let selected = app.cfg.lan_target.trim() == ip;
        let ago = peer.last_seen.elapsed().as_secs();
        peer_list = peer_list.push(
            button(
                row![
                    text("🖥").size(14).color(p().accent),
                    column![
                        text(peer.name.clone()).size(12).color(p().title),
                        text(format!("{ip} · NexNWatch {}", peer.version))
                            .size(10)
                            .color(p().muted),
                    ]
                    .spacing(1),
                    Space::new().width(Length::Fill),
                    text(if ago < 5 {
                        "çevrimiçi".to_string()
                    } else {
                        units::ago(ago)
                    })
                    .size(10)
                    .color(p().good),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding(Padding::from([6_u16, 8_u16]))
            .width(Length::Fill)
            .on_press(Message::LanPickPeer(ip))
            .style(move |_, s| button::Style {
                background: Some(Background::Color(if selected {
                    p().selected
                } else if matches!(s, button::Status::Hovered) {
                    p().panel_alt
                } else {
                    Color::TRANSPARENT
                })),
                text_color: p().text,
                border: Border {
                    color: if selected { p().accent } else { p().border },
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            }),
        );
    }

    let modes = row(LanMode::ALL.iter().map(|m| {
        chip(
            m.label().to_string(),
            app.lan_mode == *m,
            Message::LanSetMode(*m),
        )
    }))
    .spacing(4);
    let secs = row([5u64, 10, 30, 60]
        .iter()
        .map(|s| chip(format!("{s} sn"), app.lan_secs == *s, Message::LanSecs(*s))))
    .spacing(4);
    let extra: Element<'_, Message> = if app.lan_mode == LanMode::Udp {
        row![
            text("Hedef hız")
                .size(11)
                .color(p().muted)
                .width(Length::Fixed(80.0))
        ]
        .push(
            row([10.0f64, 50.0, 100.0, 500.0, 1000.0].iter().map(|r| {
                chip(
                    format!("{r:.0} Mbps"),
                    (app.lan_udp_rate - r).abs() < 0.1,
                    Message::LanUdpRate(*r),
                )
            }))
            .spacing(4),
        )
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    } else {
        row![
            text("Paralel akış")
                .size(11)
                .color(p().muted)
                .width(Length::Fixed(80.0))
        ]
        .push(
            row([1usize, 2, 4, 8].iter().map(|n| {
                chip(
                    n.to_string(),
                    app.lan_streams == *n,
                    Message::LanStreams(*n),
                )
            }))
            .spacing(4),
        )
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    };

    let start: Element<'_, Message> = if running {
        text("⇄ Test sürüyor…").size(12).color(p().warn).into()
    } else {
        small_button("⇄ Testi Başlat", Message::LanStart, true)
    };

    container(card_with(
        "Hedef Bilgisayar",
        "Ağda bulunan NexNWatch sunucuları (otomatik keşif)",
        start,
        column![
            scrollable(peer_list).height(Length::Fixed(110.0)),
            row![
                text("IP / ad")
                    .size(11)
                    .color(p().muted)
                    .width(Length::Fixed(80.0)),
                text_input("ör. 192.168.1.20 veya PC-ADI", &app.cfg.lan_target)
                    .on_input(Message::LanTarget)
                    .on_submit(Message::LanStart)
                    .size(12)
                    .padding(7),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text("Test")
                    .size(11)
                    .color(p().muted)
                    .width(Length::Fixed(80.0)),
                modes
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                text("Süre")
                    .size(11)
                    .color(p().muted)
                    .width(Length::Fixed(80.0)),
                secs
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            extra,
        ]
        .spacing(8),
    ))
    .width(Length::FillPortion(3))
    .into()
}

fn live_card<'a>(
    app: &'a App,
    st: &crate::workers::lan::LanState,
    running: bool,
) -> Element<'a, Message> {
    let fmt_mbps = |v: f64| {
        if v >= 1000.0 {
            format!("{:.2} Gbps", v / 1000.0).replace('.', ",")
        } else {
            units::mbps(v)
        }
    };
    let series = &st.live_series;
    let chart = canvas(LineChart {
        series: vec![
            (series.iter().map(|s| s.0).collect(), p().tx),
            (series.iter().map(|s| s.1).collect(), p().rx),
        ],
        fmt: Box::new(move |v| {
            if v >= 1000.0 {
                format!("{:.1} Gbps", v / 1000.0)
            } else {
                format!("{v:.0} Mbps")
            }
        }),
        span_secs: None,
        capacity: series.len().max(app.lan_secs as usize).max(2),
    })
    .width(Length::Fill)
    .height(170);

    let head: Element<'_, Message> = if running {
        column![
            row![
                text(st.phase.clone()).size(13).color(p().warn),
                Space::new().width(Length::Fill),
                text(format!("↑ {}", fmt_mbps(st.live_up)))
                    .size(20)
                    .color(p().tx),
                text(format!("↓ {}", fmt_mbps(st.live_down)))
                    .size(20)
                    .color(p().rx),
            ]
            .spacing(16)
            .align_y(Alignment::Center),
            progress_bar(0.0..=1.0, st.progress).girth(6),
        ]
        .spacing(6)
        .into()
    } else {
        text("Son testin saniyelik grafiği")
            .size(11)
            .color(p().muted)
            .into()
    };

    card(
        "Canlı Ölçüm",
        "■ turuncu: bu bilgisayardan giden · ■ mavi: gelen",
        column![head, chart].spacing(8),
    )
}

/// Karşılaştırma için bağlantı hızı: seçili adaptör ya da bağlı ilk fiziksel adaptör.
fn link_mbps(app: &App) -> Option<f64> {
    let a = app.selected_adapter().or_else(|| {
        app.adapters
            .iter()
            .find(|a| a.status == AdapterStatus::Up && !a.kind.is_virtual() && a.link_bps() > 0)
    })?;
    let bps = a.link_bps();
    (bps > 0 && bps != u64::MAX).then(|| bps as f64 / 1_000_000.0)
}

fn result_card<'a>(app: &'a App, r: &LanResult) -> Element<'a, Message> {
    let big = |label: String, value: String, sub: String, color: Color| -> Element<'a, Message> {
        container(
            column![
                text(label).size(12).color(p().muted),
                text(value).size(28).color(color),
                text(sub).size(10).color(p().muted),
            ]
            .spacing(3)
            .align_x(Alignment::Center),
        )
        .padding(14)
        .width(Length::Fill)
        .style(|_| panel_style(p().panel_alt, p().border, 8.0))
        .into()
    };
    let fmt = |v: f64| {
        if v >= 1000.0 {
            format!("{:.2} Gbps", v / 1000.0).replace('.', ",")
        } else {
            units::mbps(v)
        }
    };
    let link = link_mbps(app);
    let pct = |v: f64| match link {
        Some(l) => format!("bağlantı hızının %{:.0}'i ({})", v / l * 100.0, fmt(l)),
        None => String::new(),
    };

    let mut boxes = row![].spacing(10);
    match r.kind.as_str() {
        "smb" => {
            if let Some(w) = r.write_mbs {
                boxes = boxes.push(big(
                    "Yazma".into(),
                    format!("{w:.1} MB/s").replace('.', ","),
                    format!("≈ {}", fmt(w * 8.0)),
                    p().tx,
                ));
            }
            if let Some(rd) = r.read_mbs {
                boxes = boxes.push(big(
                    "Okuma".into(),
                    format!("{rd:.1} MB/s").replace('.', ","),
                    format!("≈ {}", fmt(rd * 8.0)),
                    p().rx,
                ));
            }
        }
        "udp" => {
            boxes = boxes.push(big(
                "Alınan".into(),
                fmt(r.up_mbps.unwrap_or(0.0)),
                format!("hedef {}", fmt(r.udp_rate_mbps.unwrap_or(0.0))),
                p().tx,
            ));
            let loss = r.loss_pct.unwrap_or(0.0);
            boxes = boxes.push(big(
                "Paket kaybı".into(),
                format!("%{loss:.2}").replace('.', ","),
                if loss < 0.1 {
                    "çok iyi".into()
                } else if loss < 1.0 {
                    "kabul edilebilir".into()
                } else {
                    "sorunlu".into()
                },
                if loss < 1.0 { p().good } else { p().bad },
            ));
            boxes = boxes.push(big(
                "Jitter".into(),
                format!("{:.2} ms", r.jitter_ms.unwrap_or(0.0)).replace('.', ","),
                "RFC 3550".into(),
                p().title,
            ));
        }
        _ => {
            if let Some(u) = r.up_mbps {
                boxes = boxes.push(big("↑ Bu bilgisayardan".into(), fmt(u), pct(u), p().tx));
            }
            if let Some(d) = r.down_mbps {
                boxes = boxes.push(big("↓ Bu bilgisayara".into(), fmt(d), pct(d), p().rx));
            }
        }
    }
    if let Some(rtt) = r.rtt_ms {
        boxes = boxes.push(big(
            "Gecikme (RTT)".into(),
            format!("{rtt:.2} ms").replace('.', ","),
            "medyan, 20 ölçüm".into(),
            p().title,
        ));
    }

    let when = chrono::DateTime::from_timestamp(r.timestamp, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%d.%m.%Y %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    let who = if r.peer_name.is_empty() {
        r.target.clone()
    } else {
        format!("{} ({})", r.peer_name, r.target)
    };
    let detail = match r.kind.as_str() {
        "smb" => format!(
            "{when} · {who} · {} yazıldı ve okundu",
            units::bytes(r.bytes / 2)
        ),
        _ => format!(
            "{when} · {who} · {} akış · {:.0} sn · {} aktarıldı",
            r.streams,
            r.secs,
            units::bytes(r.bytes)
        ),
    };
    card("Son Sonuç", &detail, boxes)
}

fn share_card(app: &App, running: bool) -> Element<'_, Message> {
    let sizes = row([128u64, 512, 1024, 4096].iter().map(|mb| {
        let label = if *mb >= 1024 {
            format!("{} GB", mb / 1024)
        } else {
            format!("{mb} MB")
        };
        chip(label, app.lan_share_size == *mb, Message::LanShareSize(*mb))
    }))
    .spacing(4);
    let start: Element<'_, Message> = if running {
        text("Test sürüyor…").size(12).color(p().warn).into()
    } else {
        small_button("Paylaşımı Test Et", Message::LanShareStart, true)
    };
    card_with(
        "Ağ Paylaşımı (SMB) Testi",
        "Paylaşıma geçici bir dosya yazar, geri okur ve siler — dosya sunucusu / NAS performansı",
        start,
        column![
            row![
                text("Paylaşım").size(11).color(p().muted).width(Length::Fixed(80.0)),
                text_input("\\\\SUNUCU\\Paylasim  veya  Z:\\", &app.cfg.lan_share)
                    .on_input(Message::LanSharePath)
                    .on_submit(Message::LanShareStart)
                    .size(12)
                    .padding(7),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![text("Dosya boyutu").size(11).color(p().muted).width(Length::Fixed(80.0)), sizes].spacing(8).align_y(Alignment::Center),
            text("Windows önbelleği devre dışı bırakılarak okunur/yazılır; sonuç diskin değil ağ + sunucunun gerçek hızıdır. Paylaşımda yazma izniniz olmalı.")
                .size(10)
                .color(p().muted),
        ]
        .spacing(8),
    )
}

fn history_card<'a>(history: &[LanResult]) -> Element<'a, Message> {
    let w = |v: f32| Length::Fixed(v);
    let fmt = |v: Option<f64>| {
        v.map(|x| {
            if x >= 1000.0 {
                format!("{:.2} Gbps", x / 1000.0)
            } else {
                format!("{x:.1} Mbps")
            }
        })
        .unwrap_or_else(|| "—".into())
    };
    let mut rows = column![
        row![
            th("ZAMAN", w(120.0)),
            th("TÜR", w(70.0)),
            th("HEDEF", Length::Fill),
            th("↑ GİDEN", w(100.0)),
            th("↓ GELEN", w(100.0)),
            th("RTT", w(70.0)),
            th("KAYIP / JITTER", w(120.0)),
        ]
        .spacing(6)
    ]
    .spacing(3);
    for r in history.iter().rev().take(100) {
        let when = chrono::DateTime::from_timestamp(r.timestamp, 0)
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%d.%m %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        let kind = match r.kind.as_str() {
            "smb" => "SMB",
            "udp" => "UDP",
            _ => "TCP",
        };
        let target = if r.peer_name.is_empty() {
            r.target.clone()
        } else {
            format!("{} · {}", r.peer_name, r.target)
        };
        let extra = match (r.loss_pct, r.jitter_ms) {
            (Some(l), Some(j)) => format!("%{l:.2} / {j:.2} ms"),
            _ => "—".into(),
        };
        rows = rows.push(
            row![
                text(when).size(11).color(p().text).width(w(120.0)),
                text(kind).size(11).color(p().muted).width(w(70.0)),
                text(target).size(11).color(p().text).width(Length::Fill),
                text(fmt(r.up_mbps)).size(11).color(p().tx).width(w(100.0)),
                text(fmt(r.down_mbps))
                    .size(11)
                    .color(p().rx)
                    .width(w(100.0)),
                text(
                    r.rtt_ms
                        .map(|v| format!("{v:.2} ms"))
                        .unwrap_or_else(|| "—".into())
                )
                .size(11)
                .color(p().text)
                .width(w(70.0)),
                text(extra).size(11).color(p().muted).width(w(120.0)),
            ]
            .spacing(6),
        );
    }
    card(
        &format!("LAN Test Geçmişi ({})", history.len()),
        "SMB testinde ↑ = yazma, ↓ = okuma (Mbps karşılığı)",
        scrollable(rows).height(Length::Fixed(240.0)),
    )
}
