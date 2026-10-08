//! Canvas tabanlı çizgi ve çubuk grafikleri.

use iced::widget::canvas::{self, Path, Program, Stroke, Text};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, mouse};

use crate::theme::p;

const LEFT: f32 = 74.0;
const BOTTOM: f32 = 20.0;
const TOP: f32 = 8.0;

/// Çok serili çizgi grafiği. Değerler soldan sağa eskiden yeniye.
pub struct LineChart {
    pub series: Vec<(Vec<f64>, Color)>,
    /// Eksen etiketi biçimlendirici.
    pub fmt: Box<dyn Fn(f64) -> String>,
    /// Alt eksen: toplam süre (sn). None → etiket yok.
    pub span_secs: Option<u64>,
    /// Toplam nokta kapasitesi (veri azsa sağa yaslanır).
    pub capacity: usize,
}

impl<Message> Program<Message> for LineChart {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let plot_w = (bounds.width - LEFT - 6.0).max(10.0);
        let plot_h = (bounds.height - BOTTOM - TOP).max(10.0);

        let max_raw = self
            .series
            .iter()
            .flat_map(|(v, _)| v.iter().copied())
            .fold(0.0_f64, f64::max);
        let max = nice_max(max_raw);

        // Izgara + Y etiketleri
        for i in 0..=4 {
            let y = TOP + plot_h * i as f32 / 4.0;
            frame.stroke(
                &Path::line(Point::new(LEFT, y), Point::new(LEFT + plot_w, y)),
                Stroke::default().with_color(p().grid).with_width(1.0),
            );
            let value = max * (4 - i) as f64 / 4.0;
            frame.fill_text(Text {
                content: (self.fmt)(value),
                position: Point::new(4.0, y - 7.0),
                color: p().muted,
                size: 10.0.into(),
                ..Text::default()
            });
        }

        // X etiketleri
        if let Some(span) = self.span_secs {
            for i in 0..=5 {
                let x = LEFT + plot_w * i as f32 / 5.0;
                let secs = span - span * i as u64 / 5;
                let label = if secs >= 60 && secs.is_multiple_of(60) {
                    format!("{} dk", secs / 60)
                } else {
                    format!("{secs} sn")
                };
                frame.fill_text(Text {
                    content: label,
                    position: Point::new(
                        (x - 12.0).clamp(LEFT, LEFT + plot_w - 34.0),
                        TOP + plot_h + 5.0,
                    ),
                    color: p().muted,
                    size: 10.0.into(),
                    ..Text::default()
                });
            }
        }

        let cap = self.capacity.max(2);
        for (values, color) in &self.series {
            draw_series(&mut frame, values, cap, max, plot_w, plot_h, *color);
        }

        vec![frame.into_geometry()]
    }
}

fn draw_series(
    frame: &mut canvas::Frame,
    values: &[f64],
    cap: usize,
    max: f64,
    plot_w: f32,
    plot_h: f32,
    color: Color,
) {
    if values.len() < 2 {
        return;
    }
    let values = if values.len() > cap {
        &values[values.len() - cap..]
    } else {
        values
    };
    let step = plot_w / (cap - 1) as f32;
    let offset = (cap - values.len()) as f32 * step;
    let point = |i: usize, v: f64| {
        let x = LEFT + offset + i as f32 * step;
        let y = TOP + plot_h - (v / max) as f32 * plot_h;
        Point::new(x, y.clamp(TOP, TOP + plot_h))
    };

    let line = Path::new(|b| {
        for (i, v) in values.iter().enumerate() {
            if i == 0 {
                b.move_to(point(i, *v))
            } else {
                b.line_to(point(i, *v))
            }
        }
    });
    let area = Path::new(|b| {
        b.move_to(Point::new(LEFT + offset, TOP + plot_h));
        for (i, v) in values.iter().enumerate() {
            b.line_to(point(i, *v));
        }
        b.line_to(Point::new(
            LEFT + offset + (values.len() - 1) as f32 * step,
            TOP + plot_h,
        ));
        b.close();
    });
    frame.fill(&area, Color { a: 0.12, ..color });
    frame.stroke(&line, Stroke::default().with_color(color).with_width(1.8));
}

/// Eksen üst sınırını "güzel" bir sayıya yuvarlar (1-2-5 serisi).
fn nice_max(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let exp = v.log10().floor();
    let base = 10f64.powf(exp);
    let m = v / base;
    let nice = if m <= 1.0 {
        1.0
    } else if m <= 2.0 {
        2.0
    } else if m <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * base
}

/// Yığılmış (RX + TX) çubuk grafiği — günlük kullanım.
pub struct BarChart {
    /// (etiket, rx, tx) — soldan sağa.
    pub bars: Vec<(String, f64, f64)>,
    pub fmt: Box<dyn Fn(f64) -> String>,
}

impl<Message> Program<Message> for BarChart {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let plot_w = (bounds.width - LEFT - 6.0).max(10.0);
        let plot_h = (bounds.height - BOTTOM - TOP).max(10.0);
        let max = nice_max(self.bars.iter().map(|(_, r, t)| r + t).fold(0.0, f64::max));

        for i in 0..=4 {
            let y = TOP + plot_h * i as f32 / 4.0;
            frame.stroke(
                &Path::line(Point::new(LEFT, y), Point::new(LEFT + plot_w, y)),
                Stroke::default().with_color(p().grid).with_width(1.0),
            );
            frame.fill_text(Text {
                content: (self.fmt)(max * (4 - i) as f64 / 4.0),
                position: Point::new(4.0, y - 7.0),
                color: p().muted,
                size: 10.0.into(),
                ..Text::default()
            });
        }

        let n = self.bars.len().max(1);
        let slot = plot_w / n as f32;
        let w = (slot * 0.62).max(2.0);
        let label_every = (n / 10).max(1);
        for (i, (label, rx, tx)) in self.bars.iter().enumerate() {
            let x = LEFT + slot * i as f32 + (slot - w) / 2.0;
            let h_rx = (*rx / max) as f32 * plot_h;
            let h_tx = (*tx / max) as f32 * plot_h;
            let base = TOP + plot_h;
            frame.fill_rectangle(Point::new(x, base - h_rx), Size::new(w, h_rx), p().rx);
            frame.fill_rectangle(
                Point::new(x, base - h_rx - h_tx),
                Size::new(w, h_tx),
                p().tx,
            );
            if i % label_every == 0 || i == n - 1 {
                frame.fill_text(Text {
                    content: label.clone(),
                    position: Point::new(x - 4.0, base + 5.0),
                    color: p().muted,
                    size: 10.0.into(),
                    ..Text::default()
                });
            }
        }
        vec![frame.into_geometry()]
    }
}
