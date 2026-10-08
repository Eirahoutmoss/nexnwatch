//! Process ağacı: kök seçimi, recursive child toplama, ağaç toplamı.

use std::collections::{HashMap, HashSet};

use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

use crate::app::{App, Message};
use crate::state::traffic_map::TrafficStats;
use crate::theme::p;
use crate::ui::panel_style;
use crate::units;

pub struct TreeRow {
    pub depth: usize,
    pub pid: u32,
    pub name: String,
    pub stats: TrafficStats,
    /// Kardeşleri arasında sonuncu mu (└ / ├ çizimi için).
    pub last_child: bool,
}

pub struct ProcessTree {
    pub rows: Vec<TreeRow>,
    pub total: TrafficStats,
    pub auto: bool,
}

/// parent → children haritası; PID yeniden kullanımına karşı çocuğun başlangıç
/// zamanı ebeveyninkinden önce olamaz.
pub fn children_map(app: &App) -> HashMap<u32, Vec<u32>> {
    let mut map: HashMap<u32, Vec<u32>> = HashMap::new();
    for proc_ in &app.processes {
        let Some(ppid) = proc_.parent_pid else { continue };
        if ppid == proc_.pid {
            continue;
        }
        if let Some(parent) = app.process(ppid) {
            if proc_.start_time + 1 < parent.start_time {
                continue; // ebeveyn PID'i başka bir process'e geçmiş
            }
            map.entry(ppid).or_default().push(proc_.pid);
        }
    }
    for v in map.values_mut() {
        v.sort_by(|a, b| {
            let ta = app.traffic.get(*a).total();
            let tb = app.traffic.get(*b).total();
            tb.cmp(&ta).then(a.cmp(b))
        });
    }
    map
}

/// Bir process'in "aile kökü": aynı adlı ebeveynlere doğru tırman (chrome.exe gibi).
pub fn family_root(app: &App, pid: u32) -> u32 {
    let mut cur = pid;
    let mut guard = 0;
    while let Some(p) = app.process(cur) {
        let Some(ppid) = p.parent_pid else { break };
        match app.process(ppid) {
            Some(parent) if parent.name.eq_ignore_ascii_case(&p.name) && parent.start_time <= p.start_time + 1 => {
                cur = ppid;
            }
            _ => break,
        }
        guard += 1;
        if guard > 32 {
            break;
        }
    }
    cur
}

/// Kök yoksa: en çok trafik yapan process'in ailesi.
pub fn effective_root(app: &App) -> Option<(u32, bool)> {
    if let Some((pid, _)) = app.root {
        return Some((pid, false));
    }
    let top = app
        .traffic
        .iter()
        .filter(|(pid, s)| **pid > 4 && s.total() > 0 && app.process(**pid).is_some())
        .max_by_key(|(_, s)| s.total())
        .map(|(pid, _)| *pid)?;
    Some((family_root(app, top), true))
}

pub fn build(app: &App, children: &HashMap<u32, Vec<u32>>) -> Option<ProcessTree> {
    let (root_pid, auto) = effective_root(app)?;
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    walk(app, children, root_pid, 0, true, &mut rows, &mut seen);
    let mut total = TrafficStats::default();
    for r in &rows {
        total.rx_bytes += r.stats.rx_bytes;
        total.tx_bytes += r.stats.tx_bytes;
        total.rx_per_sec += r.stats.rx_per_sec;
        total.tx_per_sec += r.stats.tx_per_sec;
    }
    Some(ProcessTree { rows, total, auto })
}

fn walk(
    app: &App,
    children: &HashMap<u32, Vec<u32>>,
    pid: u32,
    depth: usize,
    last: bool,
    out: &mut Vec<TreeRow>,
    seen: &mut HashSet<u32>,
) {
    if !seen.insert(pid) || depth > 16 {
        return;
    }
    let name = app.process(pid).map(|p| p.name.clone()).unwrap_or_else(|| format!("PID {pid}"));
    out.push(TreeRow { depth, pid, name, stats: app.traffic.get(pid), last_child: last });
    if let Some(kids) = children.get(&pid) {
        let n = kids.len();
        for (i, k) in kids.iter().enumerate() {
            walk(app, children, *k, depth + 1, i + 1 == n, out, seen);
        }
    }
}

/// Ağaç paneli (Ana Ekran ve Process sayfası ortak).
pub fn panel(app: &App, max_height: f32) -> Element<'_, Message> {
    let unit = app.cfg.unit;
    let children = children_map(app);
    let Some(tree) = build(app, &children) else {
        return container(
            text("Henüz ağ trafiği yapan process yok. Process sayfasından bir uygulama seçebilirsiniz.")
                .size(11)
                .color(p().muted),
        )
        .padding(10)
        .into();
    };

    let header = row![
        text("PROCESS").size(10).color(p().muted).width(Length::Fill),
        text("↓ HIZ").size(10).color(p().rx).width(Length::Fixed(82.0)),
        text("↑ HIZ").size(10).color(p().tx).width(Length::Fixed(82.0)),
        text("↓ TOPLAM").size(10).color(p().rx).width(Length::Fixed(78.0)),
        text("↑ TOPLAM").size(10).color(p().tx).width(Length::Fixed(78.0)),
    ]
    .spacing(6);

    let mut lines = column![].spacing(2);
    for (i, r) in tree.rows.iter().enumerate().take(300) {
        let prefix = if r.depth == 0 {
            "▾ ".to_string()
        } else {
            format!("{}{} ", "   ".repeat(r.depth - 1), if r.last_child { "└" } else { "├" })
        };
        let active = r.stats.rx_per_sec + r.stats.tx_per_sec > 0.0;
        let name_color = if r.depth == 0 { p().title } else if active { p().text } else { p().muted };
        let pid = r.pid;
        let line = button(
            row![
                text(format!("{prefix}{}  ({})", r.name, r.pid)).size(11).color(name_color).width(Length::Fill),
                text(units::speed(r.stats.rx_per_sec, unit)).size(11).color(p().rx).width(Length::Fixed(82.0)),
                text(units::speed(r.stats.tx_per_sec, unit)).size(11).color(p().tx).width(Length::Fixed(82.0)),
                text(units::bytes(r.stats.rx_bytes)).size(11).color(p().text).width(Length::Fixed(78.0)),
                text(units::bytes(r.stats.tx_bytes)).size(11).color(p().text).width(Length::Fixed(78.0)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([3_u16, 6_u16]))
        .width(Length::Fill)
        .on_press(Message::SelectRoot(pid))
        .style(move |_, status| button::Style {
            background: Some(Background::Color(if i == 0 {
                p().selected
            } else if matches!(status, button::Status::Hovered) {
                p().panel_alt
            } else {
                Color::TRANSPARENT
            })),
            text_color: p().text,
            border: Border { radius: 4.0.into(), ..Default::default() },
            ..Default::default()
        });
        lines = lines.push(line);
    }

    let footer = container(
        row![
            text(format!("TOPLAM AĞAÇ · {} process", tree.rows.len())).size(11).color(p().muted).width(Length::Fill),
            text(format!("↓ {}", units::speed(tree.total.rx_per_sec, unit))).size(12).color(p().rx).width(Length::Fixed(82.0)),
            text(format!("↑ {}", units::speed(tree.total.tx_per_sec, unit))).size(12).color(p().tx).width(Length::Fixed(82.0)),
            text(units::bytes(tree.total.rx_bytes)).size(12).color(p().rx).width(Length::Fixed(78.0)),
            text(units::bytes(tree.total.tx_bytes)).size(12).color(p().tx).width(Length::Fixed(78.0)),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([8_u16, 6_u16]))
    .style(|_| panel_style(p().panel_alt, p().border, 6.0));

    let mode: Element<'_, Message> = if tree.auto {
        text("Otomatik: en çok trafik yapan uygulama. Başka bir satıra tıklayarak kökü değiştirin.")
            .size(10)
            .color(p().muted)
            .into()
    } else {
        row![
            text("Seçili kök sabitlendi.").size(10).color(p().muted),
            Space::new().width(Length::Fill),
            crate::ui::small_button("Otomatiğe dön", Message::AutoRoot, false),
        ]
        .align_y(Alignment::Center)
        .into()
    };

    column![
        header,
        scrollable(lines).height(Length::Fixed(max_height)),
        footer,
        mode,
    ]
    .spacing(6)
    .into()
}
