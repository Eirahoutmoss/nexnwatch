use std::time::{Duration, Instant};

use iced::widget::{button, canvas, column, container, pick_list, row, scrollable, text, Space};
use iced::{application, window, Alignment, Background, Border, Color, Element, Length, Padding, Subscription, Task, Theme};

use crate::collectors::{nic, process};
use crate::state::rolling_window::{RollingWindow, Sample};
use crate::ui::chart::TrafficChart;

// NexNWatch visual language: deep navy, electric cyan, clean white, orange TX.
const BG: Color = Color { r: 5.0 / 255.0, g: 15.0 / 255.0, b: 28.0 / 255.0, a: 1.0 };
const HEADER: Color = Color { r: 7.0 / 255.0, g: 21.0 / 255.0, b: 38.0 / 255.0, a: 1.0 };
const PANEL: Color = Color { r: 10.0 / 255.0, g: 28.0 / 255.0, b: 48.0 / 255.0, a: 1.0 };
const PANEL_ALT: Color = Color { r: 12.0 / 255.0, g: 34.0 / 255.0, b: 58.0 / 255.0, a: 1.0 };
const BORDER: Color = Color { r: 24.0 / 255.0, g: 68.0 / 255.0, b: 105.0 / 255.0, a: 1.0 };
const CYAN: Color = Color { r: 15.0 / 255.0, g: 198.0 / 255.0, b: 255.0 / 255.0, a: 1.0 };
const BLUE: Color = Color { r: 50.0 / 255.0, g: 132.0 / 255.0, b: 255.0 / 255.0, a: 1.0 };
const RX: Color = Color { r: 18.0 / 255.0, g: 184.0 / 255.0, b: 255.0 / 255.0, a: 1.0 };
const TX: Color = Color { r: 255.0 / 255.0, g: 151.0 / 255.0, b: 58.0 / 255.0, a: 1.0 };
const GREEN: Color = Color { r: 74.0 / 255.0, g: 227.0 / 255.0, b: 139.0 / 255.0, a: 1.0 };
const WHITE: Color = Color { r: 236.0 / 255.0, g: 246.0 / 255.0, b: 255.0 / 255.0, a: 1.0 };
const TEXT: Color = Color { r: 186.0 / 255.0, g: 207.0 / 255.0, b: 229.0 / 255.0, a: 1.0 };
const MUTED: Color = Color { r: 91.0 / 255.0, g: 125.0 / 255.0, b: 158.0 / 255.0, a: 1.0 };

#[derive(Debug, Clone)]
pub enum Message {
    Tick(Instant),
    AdapterSelected(String),
    Refresh,
    SpeedTest,
}

pub struct App {
    adapters: Vec<nic::AdapterInfo>,
    selected: String,
    rolling: RollingWindow,
    rx_history: Vec<f64>,
    tx_history: Vec<f64>,
    rx_speed: f64,
    tx_speed: f64,
    last_tick: Option<Instant>,
    process_system: sysinfo::System,
    processes: Vec<process::ProcessInfo>,
    speedtest_status: String,
}

impl App {
    pub fn new() -> (Self, Task<Message>) {
        let adapters = nic::list_adapters().unwrap_or_default();
        let selected = adapters
            .iter()
            .find(|a| a.status == nic::AdapterStatus::Up)
            .or_else(|| adapters.first())
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Tüm Adaptörler".into());
        let mut process_system = sysinfo::System::new();
        let processes = process::snapshot(&mut process_system);

        (
            Self {
                adapters,
                selected,
                rolling: RollingWindow::new(Duration::from_secs(600)),
                rx_history: Vec::with_capacity(60),
                tx_history: Vec::with_capacity(60),
                rx_speed: 0.0,
                tx_speed: 0.0,
                last_tick: None,
                process_system,
                processes,
                speedtest_status: "Hazır".into(),
            },
            Task::none(),
        )
    }
}

pub fn run() -> iced::Result {
    application(App::new, update, view)
        .title("NexNWatch | Gerçek Zamanlı Ağ İzleme")
        .subscription(subscription)
        .theme(|_: &App| Theme::Dark)
        .window(window::Settings {
            size: iced::Size::new(1536.0, 960.0),
            min_size: Some((1180, 760).into()),
            ..Default::default()
        })
        .run()
}

fn subscription(_app: &App) -> Subscription<Message> {
    iced::time::every(Duration::from_secs(1)).map(Message::Tick)
}

fn update(app: &mut App, message: Message) {
    match message {
        Message::Tick(now) => {
            let counters = if app.selected == "Tüm Adaptörler" {
                nic::all_adapter_counters()
            } else {
                app.adapters
                    .iter()
                    .find(|a| a.name == app.selected)
                    .map(|a| nic::adapter_counters(a.index))
                    .unwrap_or_else(|| Err("Adapter not selected".into()))
            };

            if let Ok((rx, tx)) = counters {
                if let Some(last) = app.rolling.latest() {
                    let elapsed = now.duration_since(last.at).as_secs_f64().max(0.001);
                    app.rx_speed = rx.saturating_sub(last.rx_bytes) as f64 / elapsed;
                    app.tx_speed = tx.saturating_sub(last.tx_bytes) as f64 / elapsed;
                }
                app.rolling.push(Sample { at: now, rx_bytes: rx, tx_bytes: tx });
                app.rx_history.push(app.rx_speed);
                app.tx_history.push(app.tx_speed);
                if app.rx_history.len() > 60 {
                    app.rx_history.remove(0);
                }
                if app.tx_history.len() > 60 {
                    app.tx_history.remove(0);
                }
            }

            if app
                .last_tick
                .map(|t| now.duration_since(t) >= Duration::from_secs(2))
                .unwrap_or(true)
            {
                app.processes = process::snapshot(&mut app.process_system);
                app.last_tick = Some(now);
            }
        }
        Message::AdapterSelected(name) => {
            app.selected = name;
            app.rolling.clear();
            app.rx_history.clear();
            app.tx_history.clear();
            app.rx_speed = 0.0;
            app.tx_speed = 0.0;
        }
        Message::Refresh => {
            app.adapters = nic::list_adapters().unwrap_or_default();
            if !app.adapters.iter().any(|a| a.name == app.selected) && app.selected != "Tüm Adaptörler" {
                app.selected = app
                    .adapters
                    .iter()
                    .find(|a| a.status == nic::AdapterStatus::Up)
                    .or_else(|| app.adapters.first())
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| "Tüm Adaptörler".into());
            }
        }
        Message::SpeedTest => {
            app.speedtest_status = "Speedtest RC2'de etkinleştirilecek".into();
        }
    }
}

fn view(app: &App) -> Element<'_, Message> {
    let names: Vec<String> = std::iter::once("Tüm Adaptörler".to_string())
        .chain(app.adapters.iter().map(|a| a.name.clone()))
        .collect();

    let selected_adapter = app.adapters.iter().find(|a| a.name == app.selected);
    let link = selected_adapter
        .map(|a| format_link(a.rx_link_bps.min(a.tx_link_bps)))
        .unwrap_or_else(|| "Toplam Trafik".into());
    let status = selected_adapter.map(|a| a.status.label()).unwrap_or("Hazır");
    let status_color = if selected_adapter.map(|a| a.status == nic::AdapterStatus::Up).unwrap_or(false) {
        GREEN
    } else {
        MUTED
    };

    let header = container(
        row![
            row![
                text("◉").size(30).color(CYAN),
                column![
                    row![text("NexN").size(29).color(WHITE), text("Watch").size(29).color(CYAN)].spacing(0),
                    text("AĞINI GÖR, KONTROL SENDE").size(8).color(MUTED),
                ].spacing(1),
            ]
            .spacing(9)
            .align_y(Alignment::Center),
            Space::new().width(Length::Fill),
            column![
                text("GERÇEK ZAMANLI AĞ İZLEME").size(12).color(TEXT),
                text("AĞ ADAPTÖRLERİ   •   PROCESS TRAFİĞİ   •   DETAYLI ANALİZ   •   HIZ TESTİ")
                    .size(8)
                    .color(CYAN),
            ]
            .spacing(5)
            .align_x(iced::alignment::Horizontal::Right),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::from([13_u16, 20_u16]))
    .style(|_| panel_style(HEADER, BORDER, 0.0));

    let nav = container(
        row![
            nav_button("⌂  Ana Ekran", true),
            nav_button("▣  Ağ Adaptörleri", false),
            nav_button("⌘  Process İzleme", false),
            nav_button("◔  Hız Testi", false),
            nav_button("▤  Raporlar", false),
            nav_button("⚙  Ayarlar", false),
        ]
        .spacing(5),
    )
    .padding(Padding::from([7_u16, 20_u16]));

    let adapter_bar = card(
        row![
            text("Ağ Adaptörü").size(11).color(MUTED),
            pick_list(names, Some(app.selected.clone()), Message::AdapterSelected).width(330),
            text(format!("Link: {link}")).size(11).color(TEXT),
            text(format!("● {status}")).size(11).color(status_color),
            Space::new().width(Length::Fill),
            button(text("↻  Yenile").size(11)).on_press(Message::Refresh),
        ]
        .align_y(Alignment::Center)
        .spacing(10),
    );

    let sidebar = adapter_sidebar(app, selected_adapter);

    let realtime = row![
        metric_card("↓  İNDİRME (RX)", format_speed(app.rx_speed), RX, "GERÇEK ZAMANLI"),
        metric_card("↑  YÜKLEME (TX)", format_speed(app.tx_speed), TX, "GERÇEK ZAMANLI"),
    ]
    .spacing(10);

    let chart = card(
        column![
            row![
                column![
                    text("Gerçek Zamanlı Ağ Trafiği").size(14).color(WHITE),
                    text(if app.selected == "Tüm Adaptörler" { "Tüm adaptörlerin birleşik trafiği" } else { "Seçili ağ adaptörü • son 60 saniye" })
                        .size(9)
                        .color(MUTED),
                ]
                .spacing(2),
                Space::new().width(Length::Fill),
                unit_chip("B/s", false),
                unit_chip("KB/s", false),
                unit_chip("MB/s", true),
                unit_chip("Gbps", false),
            ]
            .align_y(Alignment::Center),
            canvas(TrafficChart { rx: app.rx_history.clone(), tx: app.tx_history.clone() }).height(220),
            row![
                text("■  İndirme (RX)").size(9).color(RX),
                text("■  Yükleme (TX)").size(9).color(TX),
                Space::new().width(Length::Fill),
                text("Son 60 saniye").size(9).color(MUTED),
            ]
            .spacing(15),
        ]
        .spacing(8),
    );

    let (rx1, tx1) = app.rolling.total_since(Duration::from_secs(60));
    let (rx5, tx5) = app.rolling.total_since(Duration::from_secs(300));
    let (rx10, tx10) = app.rolling.total_since(Duration::from_secs(600));
    let totals = card(
        column![
            text("Toplam Kullanım").size(14).color(WHITE),
            row![
                usage_card("Son 1 Dakika", rx1, tx1),
                usage_card("Son 5 Dakika", rx5, tx5),
                usage_card("Son 10 Dakika", rx10, tx10),
            ]
            .spacing(8),
        ]
        .spacing(8),
    );

    let speedtest = card(
        row![
            column![
                text("Hız Testi").size(14).color(WHITE),
                text("İnternet bağlantı kalitesini ölç").size(9).color(MUTED),
            ]
            .spacing(2),
            Space::new().width(Length::Fill),
            text("↓  -- Mbps").size(17).color(RX),
            text("↑  -- Mbps").size(17).color(TX),
            text("Ping  -- ms").size(10).color(TEXT),
            button(text("⚡  Test Başlat").size(11)).on_press(Message::SpeedTest),
        ]
        .align_y(Alignment::Center)
        .spacing(16),
    );

    let process_panel = process_panel(app);
    let tree_panel = process_tree_panel(app);

    let main = column![adapter_bar, realtime, chart, totals, speedtest, row![process_panel, tree_panel].spacing(10)]
        .spacing(10)
        .width(Length::Fill);

    let body = row![sidebar, main].spacing(10).padding(Padding::from([0_u16, 18_u16]));

    container(scrollable(column![header, nav, body].spacing(0)))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(BG)),
            text_color: Some(TEXT),
            border: Border::default(),
            ..Default::default()
        })
        .into()
}

fn adapter_sidebar<'a>(app: &'a App, selected: Option<&'a nic::AdapterInfo>) -> Element<'a, Message> {
    let items = app.adapters.iter().map(|a| adapter_item(a, app.selected == a.name)).collect::<Vec<_>>();

    let list = scrollable(column(items).spacing(5)).height(Length::Fixed(290.0));

    let info = if let Some(a) = selected {
        column![
            text("ADAPTÖR BİLGİLERİ").size(10).color(CYAN),
            info_row("MAC Adresi", a.mac.clone()),
            info_row("IPv4", a.ipv4.clone()),
            info_row("IPv6", a.ipv6.clone()),
            info_row("Link Hızı", format_link(a.rx_link_bps.min(a.tx_link_bps))),
            info_row("Duplex", "Full".to_string()),
            info_row("MTU", a.mtu.to_string()),
        ]
        .spacing(6)
    } else {
        column![
            text("ADAPTÖR BİLGİLERİ").size(10).color(CYAN),
            text("Bir adaptör seçin").size(11).color(MUTED),
        ]
        .spacing(6)
    };

    container(
        column![
            text("AĞ ADAPTÖRLERİ").size(13).color(WHITE),
            text("Aktif bağlantılar ve fiziksel arabirimler").size(8).color(MUTED),
            container(row![text("▣").size(13).color(CYAN), text("Tüm Adaptörler").size(10).color(TEXT), Space::new().width(Length::Fill), text("●").size(9).color(GREEN)].spacing(7).align_y(Alignment::Center))
                .padding(8)
                .style(|_| panel_style(PANEL_ALT, BORDER, 6.0)),
            list,
            container(info).padding(11).style(|_| panel_style(PANEL_ALT, BORDER, 7.0)),
        ]
        .spacing(7),
    )
    .width(Length::Fixed(275.0))
    .padding(11)
    .style(|_| panel_style(PANEL, BORDER, 8.0))
    .into()
}

fn adapter_item(a: &nic::AdapterInfo, selected: bool) -> Element<'_, Message> {
    let accent = if a.status == nic::AdapterStatus::Up { GREEN } else { MUTED };
    let bg = if selected { Color::from_rgb8(9, 61, 99) } else { PANEL_ALT };
    let display = short_adapter_name(&a.name);
    container(
        button(
            row![
                text(if a.status == nic::AdapterStatus::Up { "◉" } else { "○" }).size(10).color(accent),
                column![
                    text(display).size(9).color(if selected { WHITE } else { TEXT }),
                    text(format_link(a.rx_link_bps.min(a.tx_link_bps))).size(8).color(MUTED),
                ]
                .spacing(1),
                Space::new().width(Length::Fill),
                text(if a.status == nic::AdapterStatus::Up { "Up" } else { "Down" }).size(8).color(accent),
            ]
            .spacing(7)
            .align_y(Alignment::Center),
        )
        .padding(8)
        .width(Length::Fill)
        .on_press(Message::AdapterSelected(a.name.clone()))
        .style(move |_, _| iced::widget::button::Style {
            background: Some(Background::Color(bg)),
            text_color: TEXT,
            border: Border { color: if selected { CYAN } else { BORDER }, width: if selected { 1.0 } else { 0.5 }, radius: 6.0.into() },
            ..Default::default()
        }),
    )
    .into()
}

fn process_panel(app: &App) -> Element<'_, Message> {
    let mut rows = Vec::new();
    for p in app.processes.iter().take(8) {
        rows.push(
            container(
                row![
                    text(p.name.clone()).width(Length::Fill),
                    text(p.pid.to_string()).width(Length::Fixed(65.0)),
                    text(format!("{:.1}%", p.cpu)).width(Length::Fixed(65.0)),
                    text(format_bytes(p.memory)).width(Length::Fixed(90.0)),
                    text("--").width(Length::Fixed(70.0)).color(RX),
                    text("--").width(Length::Fixed(70.0)).color(TX),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .padding(Padding::from([6_u16, 4_u16]))
            .style(|_| container::Style { background: Some(Background::Color(PANEL_ALT)), border: Border::default(), ..Default::default() })
            .into(),
        );
    }

    card(
        column![
            row![
                column![
                    text("En Çok Trafik Kullanan Process'ler").size(14).color(WHITE),
                    text("CPU • RAM ve ETW ağ trafiği").size(8).color(MUTED),
                ]
                .spacing(2),
                Space::new().width(Length::Fill),
                text("PROCESS").size(7).color(MUTED),
                text("PID").size(7).color(MUTED),
                text("CPU").size(7).color(MUTED),
                text("RAM").size(7).color(MUTED),
                text("RX").size(7).color(RX),
                text("TX").size(7).color(TX),
            ]
            .align_y(Alignment::End),
            scrollable(column(rows).spacing(3)).height(Length::Fixed(190.0)),
        ]
        .spacing(8),
    )
}

fn process_tree_panel(app: &App) -> Element<'_, Message> {
    let root = app
        .processes
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("chrome.exe"))
        .or_else(|| app.processes.first());

    let mut lines = Vec::new();
    if let Some(root) = root {
        lines.push(tree_line("▾", root.name.clone(), root.pid, WHITE));
        let children = app.processes.iter().filter(|p| p.parent_pid == Some(root.pid)).take(6);
        for child in children {
            lines.push(tree_line("├", child.name.clone(), child.pid, TEXT));
        }
    } else {
        lines.push(text("Process bulunamadı").size(10).color(MUTED).into());
    }

    card(
        column![
            row![
                column![
                    text("Process Ağacı").size(14).color(WHITE),
                    text("Seçili process ve child process'leri").size(8).color(MUTED),
                ]
                .spacing(2),
                Space::new().width(Length::Fill),
                text("ETW • RC2").size(8).color(CYAN),
            ]
            .align_y(Alignment::Center),
            column(lines).spacing(5),
            Space::new().height(Length::Fill),
            container(
                row![
                    text("TOPLAM AĞAÇ").size(8).color(MUTED),
                    Space::new().width(Length::Fill),
                    text("↓ --").size(11).color(RX),
                    text("↑ --").size(11).color(TX),
                ]
                .spacing(10),
            )
            .padding(9)
            .style(|_| panel_style(PANEL_ALT, BORDER, 6.0)),
        ]
        .spacing(8),
    )
}

fn tree_line(prefix: &str, name: String, pid: u32, color: Color) -> Element<'static, Message> {
    text(format!("{prefix}  {name}  ({pid})")).size(10).color(color).into()
}

fn info_row(label: &str, value: String) -> Element<'static, Message> {
    row![
        text(label.to_string()).size(8).color(MUTED).width(Length::Fixed(68.0)),
        text(value).size(8).color(TEXT),
    ]
    .spacing(5)
    .into()
}

fn short_adapter_name(name: &str) -> String {
    let lowered = name.to_ascii_lowercase();
    if lowered.contains("wi-fi") || lowered.contains("wifi") {
        "Wi-Fi".into()
    } else if lowered.contains("ethernet") && lowered.contains("fortinet") {
        "FortiClient / VPN".into()
    } else if lowered.contains("ethernet") {
        "Ethernet".into()
    } else if lowered.contains("bluetooth") {
        "Bluetooth".into()
    } else if lowered.contains("virtual") || lowered.contains("hyper-v") {
        "Virtual Adapter".into()
    } else {
        let compact = name.trim();
        if compact.chars().count() > 25 {
            compact.chars().take(25).collect::<String>() + "…"
        } else {
            compact.to_string()
        }
    }
}

fn nav_button(label: &str, active: bool) -> Element<'_, Message> {
    button(text(label).size(9))
        .padding(Padding::from([8_u16, 12_u16]))
        .style(move |_, _| iced::widget::button::Style {
            background: Some(Background::Color(if active { BLUE } else { PANEL_ALT })),
            text_color: if active { WHITE } else { TEXT },
            border: Border { color: if active { CYAN } else { BORDER }, width: 0.6, radius: 4.0.into() },
            ..Default::default()
        })
        .into()
}

fn unit_chip(label: &str, active: bool) -> Element<'_, Message> {
    container(text(label).size(8).color(if active { WHITE } else { MUTED }))
        .padding(Padding::from([5_u16, 8_u16]))
        .style(move |_| panel_style(if active { BLUE } else { PANEL_ALT }, if active { CYAN } else { BORDER }, 4.0))
        .into()
}

fn card<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(12)
        .width(Length::Fill)
        .style(|_| panel_style(PANEL, BORDER, 8.0))
        .into()
}

fn metric_card<'a>(label: &'a str, value: String, color: Color, badge: &'a str) -> Element<'a, Message> {
    container(
        column![
            row![text(label).size(9).color(MUTED), Space::new().width(Length::Fill), text(badge).size(6).color(CYAN)].align_y(Alignment::Center),
            text(value).size(28).color(color),
        ]
        .spacing(4),
    )
    .padding(13)
    .width(Length::Fill)
    .style(move |_| panel_style(PANEL, color, 8.0))
    .into()
}

fn usage_card<'a>(label: &'a str, rx: u64, tx: u64) -> Element<'a, Message> {
    container(
        column![
            text(label).size(9).color(MUTED),
            row![text("↓").size(10).color(RX), text(format_bytes(rx)).size(14).color(RX)].spacing(5),
            row![text("↑").size(10).color(TX), text(format_bytes(tx)).size(14).color(TX)].spacing(5),
            text(format!("Toplam {}", format_bytes(rx.saturating_add(tx)))).size(8).color(TEXT),
        ]
        .spacing(4),
    )
    .padding(10)
    .width(Length::Fill)
    .style(|_| panel_style(PANEL_ALT, BORDER, 7.0))
    .into()
}

fn panel_style(background: Color, border_color: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(TEXT),
        border: Border { color: border_color, width: 1.0, radius: radius.into() },
        ..Default::default()
    }
}

fn format_speed(bytes_per_second: f64) -> String {
    let units = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut value = bytes_per_second;
    let mut i = 0usize;
    while value >= 1024.0 && i < units.len() - 1 {
        value /= 1024.0;
        i += 1;
    }
    format!("{value:.1} {}", units[i])
}

fn format_bytes(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut i = 0usize;
    while value >= 1024.0 && i < units.len() - 1 {
        value /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{} {}", bytes, units[i]) } else { format!("{value:.2} {}", units[i]) }
}

fn format_link(bps: u64) -> String {
    let gbps = bps as f64 / 1_000_000_000.0;
    if gbps >= 1.0 {
        format!("{gbps:.1} Gbps")
    } else {
        format!("{:.0} Mbps", bps as f64 / 1_000_000.0)
    }
}
