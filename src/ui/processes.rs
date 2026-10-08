//! Process İzleme sayfası — arama, sıralama, tüm process'ler + ağaç.

use iced::widget::{button, column, row, scrollable, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

use crate::app::{App, Message, ProcSort};
use crate::collectors::etw::EtwStatus;
use crate::theme::p;
use crate::ui::{card, chip, tree};
use crate::units;

pub fn view(app: &App) -> Element<'_, Message> {
    let unit = app.cfg.unit;
    let q = app.search.to_lowercase();

    let mut list: Vec<_> = app
        .processes
        .iter()
        .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q) || p.pid.to_string().contains(&q))
        .map(|p| (p, app.traffic.get(p.pid)))
        .collect();
    match app.sort {
        ProcSort::Traffic => list.sort_by(|a, b| b.1.total().cmp(&a.1.total()).then(a.0.name.cmp(&b.0.name))),
        ProcSort::Rate => list.sort_by(|a, b| {
            (b.1.rx_per_sec + b.1.tx_per_sec)
                .partial_cmp(&(a.1.rx_per_sec + a.1.tx_per_sec))
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        ProcSort::Name => list.sort_by(|a, b| a.0.name.to_lowercase().cmp(&b.0.name.to_lowercase())),
        ProcSort::Cpu => list.sort_by(|a, b| b.0.cpu.partial_cmp(&a.0.cpu).unwrap_or(std::cmp::Ordering::Equal)),
        ProcSort::Memory => list.sort_by(|a, b| b.0.memory.cmp(&a.0.memory)),
    }

    let controls = row![
        text_input("Process adı veya PID ara…", &app.search)
            .on_input(Message::ProcessSearch)
            .size(12)
            .padding(8)
            .width(Length::Fixed(280.0)),
        text("Sırala:").size(11).color(p().muted),
        chip("Trafik".into(), app.sort == ProcSort::Traffic, Message::SetProcSort(ProcSort::Traffic)),
        chip("Anlık hız".into(), app.sort == ProcSort::Rate, Message::SetProcSort(ProcSort::Rate)),
        chip("Ad".into(), app.sort == ProcSort::Name, Message::SetProcSort(ProcSort::Name)),
        chip("CPU".into(), app.sort == ProcSort::Cpu, Message::SetProcSort(ProcSort::Cpu)),
        chip("RAM".into(), app.sort == ProcSort::Memory, Message::SetProcSort(ProcSort::Memory)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let w = |v: f32| Length::Fixed(v);
    let header = row![
        text("PROCESS").size(10).color(p().muted).width(Length::Fill),
        text("PID").size(10).color(p().muted).width(w(60.0)),
        text("CPU").size(10).color(p().muted).width(w(56.0)),
        text("RAM").size(10).color(p().muted).width(w(76.0)),
        text("↓ HIZ").size(10).color(p().rx).width(w(84.0)),
        text("↑ HIZ").size(10).color(p().tx).width(w(84.0)),
        text("↓ TOPLAM").size(10).color(p().rx).width(w(78.0)),
        text("↑ TOPLAM").size(10).color(p().tx).width(w(78.0)),
    ]
    .spacing(6)
    .padding(Padding::from([0_u16, 6_u16]));

    let root_pid = app.root.map(|r| r.0);
    let mut rows = column![].spacing(1);
    for (proc_, s) in list.iter().take(250) {
        let pid = proc_.pid;
        let is_root = root_pid == Some(pid);
        let dim = s.total() == 0;
        rows = rows.push(
            button(
                row![
                    text(proc_.name.clone()).size(11).color(if dim { p().muted } else { p().text }).width(Length::Fill),
                    text(pid.to_string()).size(11).color(p().muted).width(w(60.0)),
                    text(format!("{:.1}%", proc_.cpu)).size(11).color(p().text).width(w(56.0)),
                    text(units::bytes(proc_.memory)).size(11).color(p().text).width(w(76.0)),
                    text(units::speed(s.rx_per_sec, unit)).size(11).color(p().rx).width(w(84.0)),
                    text(units::speed(s.tx_per_sec, unit)).size(11).color(p().tx).width(w(84.0)),
                    text(units::bytes(s.rx_bytes)).size(11).color(p().text).width(w(78.0)),
                    text(units::bytes(s.tx_bytes)).size(11).color(p().text).width(w(78.0)),
                ]
                .spacing(6),
            )
            .padding(Padding::from([4_u16, 6_u16]))
            .width(Length::Fill)
            .on_press(Message::SelectRoot(pid))
            .style(move |_, st| button::Style {
                background: Some(Background::Color(if is_root {
                    p().selected
                } else if matches!(st, button::Status::Hovered) {
                    p().panel_alt
                } else {
                    Color::TRANSPARENT
                })),
                text_color: p().text,
                border: Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            }),
        );
    }

    let etw_line = if app.etw.source() == crate::collectors::etw::Source::IpHelper {
        format!("IP Helper yedeği aktif (yalnızca TCP) · {} process trafik yaptı", app.traffic.len())
    } else {
        match app.etw.status() {
            EtwStatus::Running if app.etw.event_count() == 0 && app.started.elapsed().as_secs() > 20 => {
                "ETW oturumu açık ama henüz olay gelmedi — yönetici olarak çalıştığından ve güvenlik yazılımının ETW'yi engellemediğinden emin olun.".into()
            }
            EtwStatus::Running => format!(
                "ETW Kernel-Network aktif · {} olay işlendi · {} process trafik yaptı",
                app.etw.event_count(),
                app.traffic.len()
            ),
            EtwStatus::Starting => "ETW oturumu başlatılıyor…".into(),
            EtwStatus::Failed(e) => format!("ETW kullanılamıyor: {e}. Uygulamayı yönetici olarak çalıştırın."),
        }
    };

    let table = card(
        &format!("Tüm Process'ler ({})", app.processes.len()),
        &etw_line,
        column![controls, header, scrollable(rows).height(Length::Fixed(540.0))].spacing(8),
    );

    let tree = card(
        "Process Ağacı",
        "Bir satıra tıklayın: o process ve tüm child'ları izlenir",
        tree::panel(app, 520.0),
    );

    column![
        row![
            iced::widget::container(table).width(Length::FillPortion(11)),
            iced::widget::container(tree).width(Length::FillPortion(9)),
        ]
        .spacing(10),
        crate::ui::connections::view(app),
    ]
    .spacing(10)
    .into()
}
