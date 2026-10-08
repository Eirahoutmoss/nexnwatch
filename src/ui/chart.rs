use iced::widget::canvas::{self, Path, Program, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme};

#[derive(Debug, Clone)]
pub struct TrafficChart {
    pub rx: Vec<f64>,
    pub tx: Vec<f64>,
}

impl<Message> Program<Message> for TrafficChart {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let width = bounds.width;
        let height = bounds.height;

        for i in 1..=4 {
            let y = height * i as f32 / 5.0;
            frame.stroke(&Path::line(Point::new(0.0, y), Point::new(width, y)), Stroke::default().with_color(Color::from_rgb8(48, 54, 74)).with_width(1.0));
        }

        let max_value = self.rx.iter().chain(self.tx.iter()).copied().fold(1.0_f64, f64::max);
        draw_line(&mut frame, &self.rx, max_value, width, height, Color::from_rgb8(122, 162, 247));
        draw_line(&mut frame, &self.tx, max_value, width, height, Color::from_rgb8(255, 158, 100));

        vec![frame.into_geometry()]
    }
}

fn draw_line(
    frame: &mut canvas::Frame,
    values: &[f64],
    max_value: f64,
    width: f32,
    height: f32,
    color: Color,
) {
    if values.len() < 2 { return; }
    let step = width / (values.len() - 1) as f32;
    let path = Path::new(|builder| {
        for (i, value) in values.iter().enumerate() {
            let x = i as f32 * step;
            let y = height - ((*value / max_value) as f32 * (height - 10.0)) - 5.0;
            if i == 0 { builder.move_to(Point::new(x, y)); } else { builder.line_to(Point::new(x, y)); }
        }
    });
    frame.stroke(&path, Stroke::default().with_color(color).with_width(2.0));
}
