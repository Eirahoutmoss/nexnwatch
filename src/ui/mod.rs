//! Arayüz: ortak bileşenler + sayfa yönlendirme.

pub mod adapters;
pub mod chart;
pub mod dashboard;
pub mod processes;
pub mod reports;
pub mod settings;
pub mod speed;
pub mod tree;

use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

use crate::app::{App, Message, Page};
use crate::collectors::etw::EtwStatus;
use crate::theme::p;

pub fn view(app: &App) -> Element<'_, Message> {
    let page: Element<'_, Message> = match app.page {
        Page::Dashboard => dashboard::view(app),
        Page::Adapters => adapters::view(app),
        Page::Processes => processes::view(app),
        Page::Speed => speed::view(app),
        Page::Reports => reports::view(app),
        Page::Settings => settings::view(app),
    };

    let mut body = column![header(app)].spacing(10);
    if let Some(n) = &app.notice {
        body = body.push(
            container(
                row![
                    text("ⓘ").size(13).color(p().warn),
                    text(n.clone()).size(12).color(p().text),
                    Space::new().width(Length::Fill),
                    small_button("Kapat", Message::DismissNotice, false),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding(Padding::from([8_u16, 14_u16]))
            .style(|_| panel_style(p().panel_alt, p().warn, 6.0)),
        );
    }
    body = body.push(scrollable(container(page).padding(Padding::from([0_u16, 4_u16]))).height(Length::Fill));

    container(body.padding(Padding::from([12_u16, 16_u16])))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(Background::Color(p().bg)),
            text_color: Some(p().text),
            ..Default::default()
        })
        .into()
}

fn header(app: &App) -> Element<'_, Message> {
    let logo = row![
        text("◖◉").size(26).color(p().accent),
        column![
            row![text("NexN").size(24).color(p().title), text("Watch").size(24).color(p().accent)],
            text("AĞINI GÖR, KONTROL SENDE").size(8).color(p().muted),
        ]
        .spacing(0),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let nav = row![
        nav_button("⌂  Ana Ekran", Page::Dashboard, app.page),
        nav_button("🖧  Ağ Adaptörleri", Page::Adapters, app.page),
        nav_button("⌘  Process İzleme", Page::Processes, app.page),
        nav_button("◔  Hız Testi", Page::Speed, app.page),
        nav_button("▤  Raporlar", Page::Reports, app.page),
        nav_button("⚙  Ayarlar", Page::Settings, app.page),
    ]
    .spacing(4);

    let (etw_label, etw_color) = match app.etw.status() {
        EtwStatus::Running => ("● ETW aktif", p().good),
        EtwStatus::Starting => ("● ETW başlıyor", p().warn),
        EtwStatus::Failed(_) => ("● ETW kapalı", p().bad),
    };
    let speed_badge: Element<'_, Message> = if app.speed.is_running() {
        text("⚡ Hız testi sürüyor").size(10).color(p().warn).into()
    } else {
        Space::new().into()
    };

    container(
        row![
            logo,
            Space::new().width(24),
            nav,
            Space::new().width(Length::Fill),
            column![speed_badge, text(etw_label).size(10).color(etw_color)]
                .spacing(2)
                .align_x(iced::alignment::Horizontal::Right),
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding(Padding::from([10_u16, 16_u16]))
    .style(|_| panel_style(p().header, p().border, 10.0))
    .into()
}

fn nav_button(label: &str, page: Page, current: Page) -> Element<'_, Message> {
    let active = page == current;
    button(text(label).size(12))
        .padding(Padding::from([8_u16, 12_u16]))
        .on_press(Message::Navigate(page))
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if active {
                    p().accent_strong
                } else if hovered {
                    p().panel_alt
                } else {
                    Color::TRANSPARENT
                })),
                text_color: if active { Color::WHITE } else { p().text },
                border: Border { color: if active { p().accent } else { Color::TRANSPARENT }, width: 1.0, radius: 6.0.into() },
                ..Default::default()
            }
        })
        .into()
}

// ---------------------------------------------------------------------------
// Ortak bileşenler
// ---------------------------------------------------------------------------

pub fn panel_style(background: Color, border_color: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(p().text),
        border: Border { color: border_color, width: 1.0, radius: radius.into() },
        ..Default::default()
    }
}

/// Başlıklı panel kartı.
pub fn card<'a>(title: &str, subtitle: &str, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    card_with(title, subtitle, Space::new(), content)
}

/// Başlıklı, sağ üstte ek kontrolü olan panel kartı.
pub fn card_with<'a>(
    title: &str,
    subtitle: &str,
    trailing: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    let mut head = column![text(title.to_string()).size(15).color(p().title)].spacing(2);
    if !subtitle.is_empty() {
        head = head.push(text(subtitle.to_string()).size(10).color(p().muted));
    }
    container(
        column![
            row![head, Space::new().width(Length::Fill), trailing.into()].align_y(Alignment::Center),
            content.into(),
        ]
        .spacing(10),
    )
    .padding(14)
    .width(Length::Fill)
    .style(|_| panel_style(p().panel, p().border, 10.0))
    .into()
}

pub fn chip<'a>(label: String, active: bool, msg: Message) -> Element<'a, Message> {
    button(text(label).size(11))
        .padding(Padding::from([5_u16, 10_u16]))
        .on_press(msg)
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if active {
                    p().accent_strong
                } else if hovered {
                    p().selected
                } else {
                    p().panel_alt
                })),
                text_color: if active { Color::WHITE } else { p().text },
                border: Border { color: if active { p().accent } else { p().border }, width: 1.0, radius: 5.0.into() },
                ..Default::default()
            }
        })
        .into()
}

pub fn small_button<'a>(label: &str, msg: Message, primary: bool) -> Element<'a, Message> {
    button(text(label.to_string()).size(12))
        .padding(Padding::from([6_u16, 14_u16]))
        .on_press(msg)
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            let bg = if primary {
                if hovered { p().accent } else { p().accent_strong }
            } else if hovered {
                p().selected
            } else {
                p().panel_alt
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color: if primary { Color::WHITE } else { p().text },
                border: Border { color: if primary { p().accent } else { p().border }, width: 1.0, radius: 6.0.into() },
                ..Default::default()
            }
        })
        .into()
}

/// Etiket : değer satırı.
pub fn info_row<'a>(label: &str, value: String) -> Element<'a, Message> {
    row![
        text(label.to_string()).size(11).color(p().muted).width(Length::Fixed(92.0)),
        text(value).size(11).color(p().text),
    ]
    .spacing(6)
    .into()
}

/// Büyük metrik kutusu (RX / TX).
pub fn metric<'a>(arrow: &str, label: &str, value: String, color: Color, foot: String) -> Element<'a, Message> {
    container(
        row![
            text(arrow.to_string()).size(34).color(color),
            column![
                text(label.to_string()).size(12).color(color),
                text(value).size(30).color(p().title),
                text(foot).size(10).color(p().muted),
            ]
            .spacing(2),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding(14)
    .width(Length::Fill)
    .style(move |_| panel_style(p().panel, color, 10.0))
    .into()
}

/// Tablo başlık hücresi.
pub fn th<'a>(label: &str, width: Length) -> Element<'a, Message> {
    text(label.to_string()).size(10).color(p().muted).width(width).into()
}

/// Oransal yatay çubuk (0..1).
pub fn bar<'a>(fraction: f32, color: Color) -> Element<'a, Message> {
    let f = fraction.clamp(0.0, 1.0);
    let filled = (f * 1000.0).round() as u16;
    let rest = 1000u16.saturating_sub(filled);
    let mut r = row![].height(3);
    if filled > 0 {
        r = r.push(
            container(Space::new())
                .width(Length::FillPortion(filled))
                .height(3)
                .style(move |_| container::Style { background: Some(Background::Color(color)), border: Border { radius: 2.0.into(), ..Default::default() }, ..Default::default() }),
        );
    }
    if rest > 0 {
        r = r.push(Space::new().width(Length::FillPortion(rest)));
    }
    r.width(Length::Fill).into()
}
