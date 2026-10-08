//! Ayarlar + Hakkında.

use iced::widget::{Space, column, row, text, toggler};
use iced::{Alignment, Element, Length};

use crate::app::{App, Message};
use crate::config::ThemeMode;
use crate::theme::p;
use crate::ui::{card, chip, small_button};
use crate::units::DisplayUnit;

fn setting<'a>(
    title: &str,
    desc: &str,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![
        column![
            text(title.to_string()).size(13).color(p().title),
            text(desc.to_string()).size(10).color(p().muted),
        ]
        .spacing(2)
        .width(Length::Fill),
        control.into(),
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let c = &app.cfg;

    let ticks = row([500u64, 1000, 2000, 5000].iter().map(|ms| {
        let label = if *ms < 1000 {
            format!("{ms} ms")
        } else {
            format!("{} sn", ms / 1000)
        };
        chip(label, c.tick_ms == *ms, Message::SetTick(*ms))
    }))
    .spacing(4);

    let intervals = row([5u64, 10, 15, 30, 60].iter().map(|m| {
        chip(
            format!("{m} dk"),
            c.speedtest_interval_min == *m,
            Message::SetSpeedInterval(*m),
        )
    }))
    .spacing(4);

    let windows = row([10u64, 30, 60].iter().map(|m| {
        chip(
            format!("{m} dk"),
            c.window_minutes == *m,
            Message::SetWindow(*m),
        )
    }))
    .spacing(4);

    let units = row(DisplayUnit::ALL
        .iter()
        .map(|u| chip(u.label().to_string(), c.unit == *u, Message::SetUnit(*u))))
    .spacing(4);

    let themes = row![
        chip(
            "Koyu".into(),
            c.theme == ThemeMode::Dark,
            Message::SetTheme(ThemeMode::Dark)
        ),
        chip(
            "Açık".into(),
            c.theme == ThemeMode::Light,
            Message::SetTheme(ThemeMode::Light)
        ),
    ]
    .spacing(4);

    let general = card(
        "Ölçüm",
        "",
        column![
            setting(
                "Ölçüm aralığı",
                "Sayaçların okunma ve ekranın güncellenme sıklığı",
                ticks
            ),
            setting(
                "Rolling window",
                "Bellekte tutulan trafik geçmişi (grafik ve pencere toplamları)",
                windows
            ),
            setting("Birim", "Hız gösterimi: bayt/sn veya bit/sn", units),
            setting("Tema", "Arayüz renkleri", themes),
        ]
        .spacing(14),
    );

    let speed = card(
        "Hız Testi",
        "",
        column![
            setting(
                "Otomatik hız testi",
                "Belirli aralıklarla arka planda ölç. Her test bağlantı hızına göre birkaç yüz MB veri harcayabilir.",
                toggler(c.speedtest_auto).on_toggle(Message::SetSpeedAuto),
            ),
            setting("Test aralığı", "Otomatik testler arasındaki süre", intervals),
        ]
        .spacing(14),
    );

    let quota = row([0.0f64, 1.0, 5.0, 10.0, 25.0, 50.0].iter().map(|g| {
        let label = if *g == 0.0 {
            "Kapalı".to_string()
        } else {
            format!("{g:.0} GB")
        };
        chip(
            label,
            (c.daily_quota_gb - g).abs() < 0.01,
            Message::SetQuota(*g),
        )
    }))
    .spacing(4);
    let speed_alert = row([0.0f64, 10.0, 25.0, 50.0, 100.0, 250.0].iter().map(|m| {
        let label = if *m == 0.0 {
            "Kapalı".to_string()
        } else {
            format!("< {m:.0} Mbps")
        };
        chip(
            label,
            (c.speed_alert_mbps - m).abs() < 0.01,
            Message::SetSpeedAlert(*m),
        )
    }))
    .spacing(4);
    let tray_note = if crate::tray::available() {
        "Kapat düğmesi uygulamayı tepsiye gizler; çıkmak için tepsi menüsünden \"Çıkış\"."
    } else {
        "Sistem tepsisi bu platformda yok; kapat düğmesi uygulamadan çıkar."
    };

    let alerts = card(
        "Tepsi ve Bildirimler",
        "",
        column![
            setting(
                "Kapatınca tepsiye küçült",
                tray_note,
                toggler(c.close_to_tray).on_toggle(Message::SetCloseToTray)
            ),
            setting(
                "Windows bildirimleri",
                "Kota aşımı ve düşük hız uyarıları",
                toggler(c.notifications).on_toggle(Message::SetNotifications),
            ),
            setting(
                "Günlük kota",
                "Bugünkü toplam bu değerin %80'ine ve %100'üne ulaşınca bildir",
                quota
            ),
            setting(
                "Düşük hız uyarısı",
                "Hız testinde indirme bu değerin altında kalırsa bildir",
                speed_alert
            ),
        ]
        .spacing(14),
    );

    let data = card(
        "Veri ve Sistem",
        "",
        column![
            setting(
                "Kalıcı kullanım sayacı",
                "Bugün / bu hafta / bu ay toplamlarını diske kaydet",
                toggler(c.persist_usage).on_toggle(Message::SetPersist),
            ),
            setting(
                "Windows açılışında başlat",
                "Görev Zamanlayıcı ile oturum açılışında yönetici olarak başlatır",
                toggler(c.autostart).on_toggle(Message::SetAutostart),
            ),
            setting(
                "Veri klasörü",
                "config.toml, usage.json, speedtest_history.json ve logs\\",
                small_button("Klasörü Aç", Message::OpenDataFolder, false),
            ),
        ]
        .spacing(14),
    );

    let about = card(
        "Hakkında",
        "",
        column![
            row![
                text("NexN").size(22).color(p().title),
                text("Watch").size(22).color(p().accent),
                Space::new().width(10),
                text(format!("v{}", env!("CARGO_PKG_VERSION"))).size(13).color(p().muted),
            ]
            .align_y(Alignment::Center),
            text("Gerçek zamanlı ağ izleme ve process bazlı trafik analizi — Nex ailesi.").size(12).color(p().text),
            text("Kaynaklar: GetIfTable2 / GetAdaptersAddresses (IP Helper), ETW Microsoft-Windows-Kernel-Network, sysinfo, Cloudflare hız testi.")
                .size(10)
                .color(p().muted),
            text("© Hasan Güler · MIT").size(10).color(p().muted),
        ]
        .spacing(6),
    );

    column![general, speed, alerts, data, about]
        .spacing(10)
        .into()
}
