//! A colour wheel: hue around the circle, saturation from the white centre to the rim. Click or drag
//! anywhere on it to pick; the brightness of the colour stays as it was (effects set their own).

use iced::mouse::{self, Cursor};
use iced::widget::canvas::{self, Action, Cache, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme};

use crate::theme;

/// Rings and segments the wheel is drawn from; enough that no step shows at its size.
const RINGS: usize = 12;
const SEGMENTS: usize = 120;

pub struct Wheel<'a, Message> {
    /// The colour picked now, as hue (0–360), saturation and value (0–1).
    pub hsv: (f32, f32, f32),
    pub on_pick: Box<dyn Fn([u8; 3]) -> Message + 'a>,
}

#[derive(Default)]
pub struct State {
    cache: Cache,
    dragging: bool,
    /// The rim's colour in the cached drawing: another theme draws the wheel again.
    rim: std::cell::Cell<Option<Color>>,
}

fn rgb(h: f32, s: f32, v: f32) -> Color {
    let c = ak980_core::effects::hsv(h / 360.0, s, v);
    Color::from_rgb(c[0], c[1], c[2])
}

/// The wheel's centre and radius in `size`, with room for the marker at the rim.
fn geometry(size: iced::Size) -> (Point, f32) {
    let r = size.width.min(size.height) / 2.0 - 6.0;
    (Point::new(size.width / 2.0, size.height / 2.0), r.max(1.0))
}

/// Hue and saturation under `p`; outside the wheel it is the nearest point of the rim.
pub fn pick_at(size: iced::Size, p: Point) -> (f32, f32) {
    let (c, r) = geometry(size);
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    let hue = (dy.atan2(dx).to_degrees() + 360.0) % 360.0;
    (hue, (dx.hypot(dy) / r).min(1.0))
}

impl<Message> Wheel<'_, Message> {
    fn pick(&self, size: iced::Size, p: Point) -> Message {
        let (h, s) = pick_at(size, p);
        // A black colour has no brightness to keep: picking makes it full
        let v = if self.hsv.2 < 0.05 { 1.0 } else { self.hsv.2 };
        let c = ak980_core::effects::hsv(h / 360.0, s, v);
        (self.on_pick)(c.map(|x| (x * 255.0).round().clamp(0.0, 255.0) as u8))
    }
}

impl<Message> canvas::Program<Message> for Wheel<'_, Message> {
    type State = State;

    fn update(&self, state: &mut State, event: &canvas::Event, bounds: Rectangle, cursor: Cursor) -> Option<Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = cursor.position_in(bounds)?;
                let (c, r) = geometry(bounds.size());
                if (p.x - c.x).hypot(p.y - c.y) > r + 4.0 {
                    return None;
                }
                state.dragging = true;
                Some(Action::publish(self.pick(bounds.size(), p)).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
                // Dragging past the rim keeps picking along it
                let p = cursor.position_from(bounds.position())?;
                Some(Action::publish(self.pick(bounds.size(), p)).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
                state.dragging = false;
                Some(Action::capture())
            }
            _ => None,
        }
    }

    fn mouse_interaction(&self, _state: &State, bounds: Rectangle, cursor: Cursor) -> mouse::Interaction {
        let over = cursor.position_in(bounds).is_some_and(|p| {
            let (c, r) = geometry(bounds.size());
            (p.x - c.x).hypot(p.y - c.y) <= r + 4.0
        });
        if over { mouse::Interaction::Crosshair } else { mouse::Interaction::default() }
    }

    fn draw(&self, state: &State, renderer: &Renderer, _theme: &Theme, bounds: Rectangle, _cursor: Cursor) -> Vec<Geometry> {
        let (c, r) = geometry(bounds.size());
        let rim = theme::alpha(theme::roles().overlay1, 0.8);
        if state.rim.replace(Some(rim)) != Some(rim) {
            state.cache.clear();
        }
        // The wheel itself changes only with the theme, the marker on it with every pick
        let wheel = state.cache.draw(renderer, bounds.size(), |frame| {
            let step = std::f32::consts::TAU / SEGMENTS as f32;
            for ring in 0..RINGS {
                let (r0, r1) = (r * ring as f32 / RINGS as f32, r * (ring + 1) as f32 / RINGS as f32);
                let s = (ring as f32 + 0.5) / RINGS as f32;
                for seg in 0..SEGMENTS {
                    // A little overlap hides the seams between the pieces
                    let (a0, a1) = (seg as f32 * step - 0.004, (seg + 1) as f32 * step + 0.004);
                    let at = |a: f32, rr: f32| Point::new(c.x + rr * a.cos(), c.y + rr * a.sin());
                    let piece = Path::new(|p| {
                        p.move_to(at(a0, r0));
                        p.line_to(at(a0, r1 + 0.5));
                        p.line_to(at(a1, r1 + 0.5));
                        p.line_to(at(a1, r0));
                        p.close();
                    });
                    frame.fill(&piece, rgb((seg as f32 + 0.5) * 360.0 / SEGMENTS as f32, s, 1.0));
                }
            }
            frame.stroke(&Path::circle(c, r), Stroke::default().with_color(rim).with_width(1.0));
        });
        let mut marker = Frame::new(renderer, bounds.size());
        let (h, s, _) = self.hsv;
        let a = h.to_radians();
        let at = Point::new(c.x + s * r * a.cos(), c.y + s * r * a.sin());
        marker.fill(&Path::circle(at, 6.0), rgb(h, s, 1.0));
        marker.stroke(&Path::circle(at, 6.0), Stroke::default().with_color(Color::WHITE).with_width(2.0));
        marker.stroke(&Path::circle(at, 7.5), Stroke::default().with_color(theme::alpha(Color::BLACK, 0.6)).with_width(1.0));
        vec![wheel, marker.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picking_reads_hue_around_and_saturation_outwards() {
        let size = iced::Size::new(112.0, 112.0);
        let (c, r) = geometry(size);
        let (h, s) = pick_at(size, c);
        assert!(s < 1e-3, "the centre is white (hue {h})");
        let (h, s) = pick_at(size, Point::new(c.x + r, c.y));
        assert!((h - 0.0).abs() < 0.5 && (s - 1.0).abs() < 1e-3, "right: red at full saturation");
        let (h, s) = pick_at(size, Point::new(c.x, c.y + r / 2.0));
        assert!((h - 90.0).abs() < 0.5 && (s - 0.5).abs() < 1e-3, "below: a quarter round, half way out");
        let (_, s) = pick_at(size, Point::new(c.x - 3.0 * r, c.y));
        assert_eq!(s, 1.0, "outside the wheel: its rim");
    }
}
