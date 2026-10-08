//! Raporlar — kalıcı günlük / haftalık / aylık kullanım ve oturum özeti.

use chrono::Datelike;
use iced::widget::{canvas, column, container, row, scrollable, text};
use iced::{Element, Length};

use crate::app::{App, Message};
use crate::theme::p;
use crate::ui::chart::BarChart;
use crate::ui::{bar, card, panel_style, th};
use crate::units;

pub fn view(app: &App) -> Element<'_, Message> {
    let today = app.usage.today();
    let week = app.usage.this_week();
    let month = app.usage.this_month();

    let summary_box = |label: &'static str, rx: u64, tx: u64| -> Element<'_, Message> {
        container(
            column![
                text(label).size(12).color(p().muted),
                text(units::bytes(rx.saturating_add(tx)))
                    .size(26)
                    .color(p().title),
                row![
                    text(format!("↓ {}", units::bytes(rx)))
                        .size(12)
                        .color(p().rx),
                    text(format!("↑ {}", units::bytes(tx)))
                        .size(12)
                        .color(p().tx),
                ]
                .spacing(12),
            ]
            .spacing(4),
        )
        .padding(14)
        .width(Length::Fill)
        .style(|_| panel_style(p().panel_alt, p().border, 8.0))
        .into()
    };

    let persist_note = if app.cfg.persist_usage {
        "Fiziksel adaptörlerin toplamı · %APPDATA%\\NexNWatch\\usage.json"
    } else {
        "Kalıcı sayaç Ayarlar'da kapalı — yeni veri kaydedilmiyor"
    };

    let summary = card(
        "Kullanım Özeti",
        persist_note,
        row![
            summary_box("Bugün", today.rx, today.tx),
            summary_box("Bu Hafta", week.rx, week.tx),
            summary_box("Bu Ay", month.rx, month.tx),
        ]
        .spacing(10),
    );

    // Son 30 gün grafiği (eskiden yeniye)
    let days = app.usage.last_days(30);
    let bars: Vec<(String, f64, f64)> = days
        .iter()
        .rev()
        .map(|(d, u)| (d.format("%d.%m").to_string(), u.rx as f64, u.tx as f64))
        .collect();
    let chart = card(
        "Son 30 Gün",
        "■ mavi: indirme · ■ turuncu: yükleme",
        canvas(BarChart {
            bars,
            fmt: Box::new(|v| units::bytes(v as u64)),
        })
        .width(Length::Fill)
        .height(220),
    );

    // Günlük tablo
    let max_day = days
        .iter()
        .map(|(_, u)| u.total())
        .max()
        .unwrap_or(1)
        .max(1);
    let w = |v: f32| Length::Fixed(v);
    let mut day_rows = column![
        row![
            th("GÜN", w(110.0)),
            th("İNDİRME", w(90.0)),
            th("YÜKLEME", w(90.0)),
            th("TOPLAM", w(90.0)),
            th("", Length::Fill)
        ]
        .spacing(6)
    ]
    .spacing(4);
    for (d, u) in &days {
        day_rows = day_rows.push(
            row![
                text(format!("{} {}", d.format("%d.%m.%Y"), gun(d.weekday())))
                    .size(11)
                    .color(p().text)
                    .width(w(110.0)),
                text(units::bytes(u.rx))
                    .size(11)
                    .color(p().rx)
                    .width(w(90.0)),
                text(units::bytes(u.tx))
                    .size(11)
                    .color(p().tx)
                    .width(w(90.0)),
                text(units::bytes(u.total()))
                    .size(11)
                    .color(p().title)
                    .width(w(90.0)),
                iced::widget::container(bar(u.total() as f32 / max_day as f32, p().accent_strong))
                    .width(Length::Fill)
                    .padding(iced::Padding {
                        top: 6.0,
                        ..iced::Padding::ZERO
                    }),
            ]
            .spacing(6),
        );
    }
    let daily = card(
        "Günlük Döküm",
        "",
        scrollable(day_rows).height(Length::Fixed(300.0)),
    );

    // Aylık
    let mut month_rows = column![
        row![
            th("AY", w(90.0)),
            th("İNDİRME", w(90.0)),
            th("YÜKLEME", w(90.0)),
            th("TOPLAM", w(90.0))
        ]
        .spacing(6)
    ]
    .spacing(4);
    for (m, u) in app.usage.months(12) {
        month_rows = month_rows.push(
            row![
                text(m).size(11).color(p().text).width(w(90.0)),
                text(units::bytes(u.rx))
                    .size(11)
                    .color(p().rx)
                    .width(w(90.0)),
                text(units::bytes(u.tx))
                    .size(11)
                    .color(p().tx)
                    .width(w(90.0)),
                text(units::bytes(u.total()))
                    .size(11)
                    .color(p().title)
                    .width(w(90.0)),
            ]
            .spacing(6),
        );
    }
    let monthly = card("Aylık", "Son 12 ay", month_rows);

    // Oturum: en çok trafik yapan process'ler
    let mut procs: Vec<_> = app.traffic.iter().filter(|(_, s)| s.total() > 0).collect();
    procs.sort_by_key(|b| std::cmp::Reverse(b.1.total()));
    let mut proc_rows = column![
        row![
            th("PROCESS", Length::Fill),
            th("İNDİRME", w(84.0)),
            th("YÜKLEME", w(84.0))
        ]
        .spacing(6)
    ]
    .spacing(4);
    for (pid, s) in procs.iter().take(15) {
        let name = app
            .process(**pid)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| format!("PID {pid}"));
        proc_rows = proc_rows.push(
            row![
                text(name).size(11).color(p().text).width(Length::Fill),
                text(units::bytes(s.rx_bytes))
                    .size(11)
                    .color(p().rx)
                    .width(w(84.0)),
                text(units::bytes(s.tx_bytes))
                    .size(11)
                    .color(p().tx)
                    .width(w(84.0)),
            ]
            .spacing(6),
        );
    }
    let uptime = app.started.elapsed().as_secs();
    let session = card(
        "Bu Oturum — Process Bazında",
        &format!("Uygulama {} dk'dır açık", uptime / 60),
        proc_rows,
    );

    let apps_card = |title: &str,
                     list: Vec<(String, crate::state::usage_store::DayUsage)>|
     -> Element<'_, Message> {
        let max = list.first().map(|(_, u)| u.total()).unwrap_or(1).max(1);
        let mut rows = column![
            row![
                th("UYGULAMA", Length::Fill),
                th("İNDİRME", w(84.0)),
                th("YÜKLEME", w(84.0)),
                th("TOPLAM", w(84.0))
            ]
            .spacing(6)
        ]
        .spacing(4);
        if list.is_empty() {
            rows = rows.push(text("Henüz kayıt yok.").size(11).color(p().muted));
        }
        for (name, u) in list.iter().take(20) {
            rows = rows.push(
                column![
                    row![
                        text(name.clone())
                            .size(11)
                            .color(p().text)
                            .width(Length::Fill),
                        text(units::bytes(u.rx))
                            .size(11)
                            .color(p().rx)
                            .width(w(84.0)),
                        text(units::bytes(u.tx))
                            .size(11)
                            .color(p().tx)
                            .width(w(84.0)),
                        text(units::bytes(u.total()))
                            .size(11)
                            .color(p().title)
                            .width(w(84.0)),
                    ]
                    .spacing(6),
                    bar(u.total() as f32 / max as f32, p().accent_strong),
                ]
                .spacing(2),
            );
        }
        card(title, "Uygulama adına göre · kalıcı", rows)
    };
    let apps_row = row![
        apps_card("Uygulamalar — Bugün", app.usage.apps_today()),
        apps_card("Uygulamalar — Bu Ay", app.usage.apps_this_month()),
    ]
    .spacing(10);

    column![
        summary,
        chart,
        apps_row,
        row![
            iced::widget::container(daily).width(Length::FillPortion(3)),
            column![monthly, session]
                .spacing(10)
                .width(Length::FillPortion(2)),
        ]
        .spacing(10),
    ]
    .spacing(10)
    .into()
}

fn gun(w: chrono::Weekday) -> &'static str {
    use chrono::Weekday::*;
    match w {
        Mon => "Pzt",
        Tue => "Sal",
        Wed => "Çar",
        Thu => "Per",
        Fri => "Cum",
        Sat => "Cmt",
        Sun => "Paz",
    }
}
