//! NexNWatch renk paleti (koyu / açık). Konsept görseldeki lacivert + elektrik
//! mavisi dil esas alınmıştır; RX mavi, TX turuncu.

use std::sync::atomic::{AtomicBool, Ordering};

use iced::Color;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Color,
    pub header: Color,
    pub panel: Color,
    pub panel_alt: Color,
    pub selected: Color,
    pub border: Color,
    pub accent: Color,
    pub accent_strong: Color,
    pub rx: Color,
    pub tx: Color,
    pub good: Color,
    pub bad: Color,
    pub warn: Color,
    pub title: Color,
    pub text: Color,
    pub muted: Color,
    pub grid: Color,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

pub const DARK: Palette = Palette {
    bg: rgb(5, 15, 28),
    header: rgb(7, 21, 38),
    panel: rgb(10, 28, 48),
    panel_alt: rgb(12, 34, 58),
    selected: rgb(9, 61, 99),
    border: rgb(24, 68, 105),
    accent: rgb(15, 198, 255),
    accent_strong: rgb(50, 132, 255),
    rx: rgb(18, 184, 255),
    tx: rgb(255, 151, 58),
    good: rgb(74, 227, 139),
    bad: rgb(247, 118, 142),
    warn: rgb(255, 199, 90),
    title: rgb(236, 246, 255),
    text: rgb(186, 207, 229),
    muted: rgb(91, 125, 158),
    grid: rgb(30, 54, 82),
};

pub const LIGHT: Palette = Palette {
    bg: rgb(236, 242, 249),
    header: rgb(255, 255, 255),
    panel: rgb(255, 255, 255),
    panel_alt: rgb(243, 247, 252),
    selected: rgb(214, 234, 255),
    border: rgb(200, 214, 232),
    accent: rgb(0, 133, 204),
    accent_strong: rgb(34, 110, 230),
    rx: rgb(0, 124, 214),
    tx: rgb(224, 112, 20),
    good: rgb(22, 150, 80),
    bad: rgb(206, 50, 80),
    warn: rgb(190, 130, 0),
    title: rgb(12, 26, 44),
    text: rgb(40, 58, 80),
    muted: rgb(110, 130, 155),
    grid: rgb(220, 229, 240),
};

static LIGHT_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_light(light: bool) {
    LIGHT_MODE.store(light, Ordering::Relaxed);
}

pub fn is_light() -> bool {
    LIGHT_MODE.load(Ordering::Relaxed)
}

/// Aktif palet.
pub fn p() -> &'static Palette {
    if is_light() { &LIGHT } else { &DARK }
}
