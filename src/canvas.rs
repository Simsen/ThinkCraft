//! A drawing surface that either paints on screen (egui) or records the
//! shapes so they can be written into a PowerPoint file.

use crate::model::Align;
use eframe::egui::{self, epaint::RectShape, Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};
use std::{cell::RefCell, rc::Rc};

pub enum Item {
    Shape(Shape),
    Text { rect: Rect, text: String, size: f32, color: Color32, align: Align, wrap: bool },
    Sector { center: Pos2, r_in: f32, r_out: f32, a0: f32, a1: f32, color: Color32 },
    Line { from: Pos2, to: Pos2, stroke: Stroke, dashed: bool, arrow: bool },
    Preset { rect: Rect, prst: &'static str, adj: Option<i64>, color: Color32 },
    Begin(String),
    End,
}

#[derive(Clone)]
pub struct Canvas {
    painter: Painter,
    rec: Option<Rc<RefCell<Vec<Item>>>>,
}

fn radius(r: f32) -> egui::CornerRadius {
    egui::CornerRadius::same(r.round().clamp(0.0, 255.0) as u8)
}

impl Canvas {
    pub fn paint(painter: Painter) -> Self {
        Canvas { painter, rec: None }
    }
    pub fn record(painter: Painter) -> Self {
        Canvas { painter, rec: Some(Rc::new(RefCell::new(Vec::new()))) }
    }
    pub fn recording(&self) -> bool {
        self.rec.is_some()
    }
    pub fn take(&self) -> Vec<Item> {
        match &self.rec {
            Some(r) => std::mem::take(&mut *r.borrow_mut()),
            None => vec![],
        }
    }
    pub fn clip_rect(&self) -> Rect {
        if self.recording() {
            Rect::EVERYTHING
        } else {
            self.painter.clip_rect()
        }
    }
    pub fn with_clip_rect(&self, r: Rect) -> Self {
        Canvas { painter: self.painter.with_clip_rect(r), rec: self.rec.clone() }
    }
    pub fn begin_group(&self, name: &str) {
        if let Some(r) = &self.rec {
            r.borrow_mut().push(Item::Begin(name.to_string()));
        }
    }
    pub fn end_group(&self) {
        if let Some(r) = &self.rec {
            r.borrow_mut().push(Item::End);
        }
    }

    pub fn add(&self, s: impl Into<Shape>) {
        match &self.rec {
            Some(r) => r.borrow_mut().push(Item::Shape(s.into())),
            None => {
                self.painter.add(s);
            }
        }
    }
    pub fn rect_filled(&self, rect: Rect, r: f32, col: Color32) {
        self.add(RectShape::filled(rect, radius(r), col));
    }
    pub fn rect_stroke(&self, rect: Rect, r: f32, stroke: Stroke, kind: StrokeKind) {
        self.add(RectShape::stroke(rect, radius(r), stroke, kind));
    }
    pub fn line_segment(&self, pts: [Pos2; 2], stroke: Stroke) {
        self.add(Shape::line_segment(pts, stroke));
    }
    pub fn circle_filled(&self, c: Pos2, r: f32, col: Color32) {
        self.add(Shape::circle_filled(c, r, col));
    }
    pub fn circle_stroke(&self, c: Pos2, r: f32, stroke: Stroke) {
        self.add(Shape::circle_stroke(c, r, stroke));
    }
    /// A line with a small filled arrow head at the end.
    pub fn arrow(&self, origin: Pos2, vec: Vec2, stroke: Stroke) {
        let len = vec.length();
        if len < 0.5 {
            return;
        }
        if let Some(r) = &self.rec {
            r.borrow_mut().push(Item::Line { from: origin, to: origin + vec, stroke, dashed: false, arrow: true });
            return;
        }
        let dir = vec / len;
        let head = (stroke.width * 5.0).max(3.0).min(len * 0.6);
        let tip = origin + vec;
        let back = tip - dir * head;
        self.line_segment([origin, back], stroke);
        let n = Vec2::new(-dir.y, dir.x) * head * 0.45;
        self.add(Shape::convex_polygon(vec![tip, back + n, back - n], stroke.color, Stroke::NONE));
    }

    pub fn dashed(&self, a: Pos2, b: Pos2, stroke: Stroke, dash: f32, gap: f32) {
        match &self.rec {
            Some(r) => r.borrow_mut().push(Item::Line { from: a, to: b, stroke, dashed: true, arrow: false }),
            None => {
                self.painter.add(Shape::dashed_line(&[a, b], stroke, dash, gap));
            }
        }
    }

    /// Records a PowerPoint preset shape (chevron, diamond...); returns false when painting on screen.
    pub fn preset(&self, rect: Rect, prst: &'static str, adj: Option<i64>, color: Color32) -> bool {
        match &self.rec {
            Some(r) => {
                r.borrow_mut().push(Item::Preset { rect, prst, adj, color });
                true
            }
            None => false,
        }
    }

    /// Records a pie slice or ring segment; returns false when painting on screen.
    pub fn record_sector(&self, center: Pos2, r_in: f32, r_out: f32, a0: f32, a1: f32, color: Color32) -> bool {
        match &self.rec {
            Some(r) => {
                r.borrow_mut().push(Item::Sector { center, r_in, r_out, a0, a1, color });
                true
            }
            None => false,
        }
    }

    pub fn measure(&self, s: &str, size: f32) -> Vec2 {
        if size < 1.0 || s.is_empty() {
            return Vec2::ZERO;
        }
        self.painter.layout_no_wrap(s.to_string(), FontId::proportional(size), Color32::BLACK).size()
    }

    pub fn text_at(&self, pos: Pos2, align: Align2, s: &str, size: f32, color: Color32) -> Rect {
        let galley = self.painter.layout_no_wrap(s.to_string(), FontId::proportional(size), color);
        let rect = align.anchor_size(pos, galley.size());
        match &self.rec {
            Some(r) => {
                let a = match align.x() {
                    egui::Align::Min => Align::Left,
                    egui::Align::Center => Align::Center,
                    egui::Align::Max => Align::Right,
                };
                r.borrow_mut().push(Item::Text { rect, text: s.to_string(), size, color, align: a, wrap: false });
            }
            None => self.painter.galley(rect.min, galley, color),
        }
        rect
    }

    pub fn text_block(&self, rect: Rect, s: &str, size: f32, color: Color32, align: Align) {
        match &self.rec {
            Some(r) => r.borrow_mut().push(Item::Text { rect, text: s.to_string(), size, color, align, wrap: true }),
            None => {
                let mut job = egui::text::LayoutJob::simple(s.to_string(), FontId::proportional(size), color, rect.width().max(1.0));
                job.halign = match align {
                    Align::Left => egui::Align::LEFT,
                    Align::Center => egui::Align::Center,
                    Align::Right => egui::Align::RIGHT,
                };
                let galley = self.painter.layout_job(job);
                let x = match align {
                    Align::Left => rect.left(),
                    Align::Center => rect.center().x,
                    Align::Right => rect.right(),
                };
                self.painter.galley(Pos2::new(x, rect.top()), galley, color);
            }
        }
    }
}
