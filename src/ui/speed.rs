//! Hız Testi sayfası — anlık test, son sonuç, geçmiş ve grafik.

use iced::widget::{Space, canvas, column, container, progress_bar, row, scrollable, text};
use iced::{Alignment, Element, Length};

use crate::app::{App, Message};
use crate::theme::p;
use crate::ui::chart::LineChart;
use crate::ui::{card, card_with, panel_style, small_button, th};
use crate::units;

pub fn view(app: &App) -> Element<'_, Message> {
    let st = app.speed.snapshot();
    let running = app.speed.is_running();

    let big = |label: &str, value: String, color: iced::Color| -> Element<'_, Message> {
        container(
            column![
                text(label.to_string()).size(12).color(p().muted),
                text(value).size(30).color(color)
            ]
            .spacing(4)
            .align_x(Alignment::Center),
        )
        .padding(16)
        .width(Length::Fill)
        .style(move |_| panel_style(p().panel_alt, p().border, 8.0))
        .into()
    };

    let last = st.history.last();
    let current: Element<'_, Message> = if running {
        column![
            row![
                text(st.phase.label()).size(14).color(p().warn),
                Space::new().width(Length::Fill),
                text(units::mbps(st.live_mbps)).size(26).color(p().title),
            ]
            .align_y(Alignment::Center),
            progress_bar(0.0..=1.0, st.progress).girth(8),
            text("Test sırasında oluşan trafik pencere toplamlarına (1/5/10 dk) katılmaz.")
                .size(10)
                .color(p().muted),
        ]
        .spacing(8)
        .into()
    } else {
        match last {
            Some(r) => {
                let when = chrono::DateTime::from_timestamp(r.timestamp, 0)
                    .map(|d| {
                        d.with_timezone(&chrono::Local)
                            .format("%d.%m.%Y %H:%M")
                            .to_string()
                    })
                    .unwrap_or_default();
                column![
                    row![
                        big("İndirme", units::mbps(r.download_mbps), p().rx),
                        big("Yükleme", units::mbps(r.upload_mbps), p().tx),
                        big("Ping", format!("{:.0} ms", r.ping_ms), p().title),
                        big("Jitter", format!("{:.1} ms", r.jitter_ms), p().title),
                    ]
                    .spacing(10),
                    text(format!(
                        "{when} · {} · ISS: {} · IP: {} · bu test {} veri kullandı",
                        r.server,
                        if r.isp.is_empty() { "—" } else { &r.isp },
                        if r.ip.is_empty() { "—" } else { &r.ip },
                        units::bytes(r.bytes_used)
                    ))
                    .size(11)
                    .color(p().muted),
                ]
                .spacing(8)
                .into()
            }
            None => text("Henüz test yapılmadı. \"Test Başlat\" ile ilk ölçümü alın.")
                .size(12)
                .color(p().muted)
                .into(),
        }
    };

    let err: Element<'_, Message> = match &st.last_error {
        Some(e) => text(format!("Son hata: {e}"))
            .size(11)
            .color(p().bad)
            .into(),
        None => Space::new().into(),
    };

    let trailing: Element<'_, Message> = if running {
        text("⚡ Ölçülüyor…").size(12).color(p().warn).into()
    } else {
        small_button("⚡ Test Başlat", Message::SpeedTest, true)
    };
    let auto = if app.cfg.speedtest_auto {
        format!(
            "Sunucu: {} · otomatik her {} dk (Ayarlar'dan değiştirilebilir)",
            app.cfg.speed_provider.label(),
            app.cfg.speedtest_interval_min
        )
    } else {
        format!(
            "Sunucu: {} · otomatik test kapalı",
            app.cfg.speed_provider.label()
        )
    };
    let top = card_with(
        "Hız Testi",
        &auto,
        trailing,
        column![current, err].spacing(8),
    );

    // Geçmiş grafiği
    let hist: Vec<_> = st.history.iter().rev().take(60).rev().collect();
    let chart_body: Element<'_, Message> = if hist.len() < 2 {
        text("Grafik için en az iki test sonucu gerekiyor.")
            .size(11)
            .color(p().muted)
            .into()
    } else {
        canvas(LineChart {
            series: vec![
                (hist.iter().map(|r| r.download_mbps).collect(), p().rx),
                (hist.iter().map(|r| r.upload_mbps).collect(), p().tx),
            ],
            fmt: Box::new(|v| {
                if v < 10.0 {
                    format!("{v:.1} Mbps")
                } else {
                    format!("{v:.0} Mbps")
                }
            }),
            span_secs: None,
            capacity: hist.len().max(2),
        })
        .width(Length::Fill)
        .height(180)
        .into()
    };
    let chart = card(
        "Geçmiş (son 60 test)",
        "■ mavi: indirme · ■ turuncu: yükleme (Mbps)",
        chart_body,
    );

    // Geçmiş tablosu
    let w = |v: f32| Length::Fixed(v);
    let mut rows = column![
        row![
            th("ZAMAN", w(130.0)),
            th("İNDİRME", w(100.0)),
            th("YÜKLEME", w(100.0)),
            th("PING", w(64.0)),
            th("JITTER", w(64.0)),
            th("VERİ", w(76.0)),
            th("TÜR", w(64.0)),
            th("SUNUCU / ISS", Length::Fill),
        ]
        .spacing(6)
    ]
    .spacing(3);
    for r in st.history.iter().rev().take(200) {
        let when = chrono::DateTime::from_timestamp(r.timestamp, 0)
            .map(|d| {
                d.with_timezone(&chrono::Local)
                    .format("%d.%m.%Y %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        rows = rows.push(
            row![
                text(when).size(11).color(p().text).width(w(130.0)),
                text(units::mbps(r.download_mbps))
                    .size(11)
                    .color(p().rx)
                    .width(w(100.0)),
                text(units::mbps(r.upload_mbps))
                    .size(11)
                    .color(p().tx)
                    .width(w(100.0)),
                text(format!("{:.0} ms", r.ping_ms))
                    .size(11)
                    .color(p().text)
                    .width(w(64.0)),
                text(format!("{:.1} ms", r.jitter_ms))
                    .size(11)
                    .color(p().text)
                    .width(w(64.0)),
                text(units::bytes(r.bytes_used))
                    .size(11)
                    .color(p().muted)
                    .width(w(76.0)),
                text(if r.auto { "Otomatik" } else { "Manuel" })
                    .size(11)
                    .color(p().muted)
                    .width(w(64.0)),
                text(format!("{} · {}", r.server, r.isp))
                    .size(11)
                    .color(p().muted)
                    .width(Length::Fill),
            ]
            .spacing(6),
        );
    }
    let table = card(
        &format!("Test Geçmişi ({})", st.history.len()),
        "",
        scrollable(rows).height(Length::Fixed(320.0)),
    );

    column![top, chart, table].spacing(10).into()
}
