//! Painting slides, charts and layout elements.

use crate::model::*;
use crate::canvas::Canvas;
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Shape, Stroke, StrokeKind, Vec2};
use std::f32::consts::{PI, TAU};

/// Maps slide units to screen points.
#[derive(Clone, Copy)]
pub struct Xf {
    pub origin: Pos2,
    pub scale: f32,
}

impl Xf {
    pub fn pos(&self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.scale
    }
    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(self.pos(x, y), Vec2::new(w, h) * self.scale)
    }
    pub fn el_rect(&self, e: &Element) -> Rect {
        self.rect(e.x, e.y, e.w, e.h)
    }
    pub fn to_slide(&self, p: Pos2) -> Pos2 {
        let v = (p - self.origin) / self.scale;
        Pos2::new(v.x, v.y)
    }
}

pub fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

pub const INK: Color32 = Color32::from_rgb(38, 40, 48);
pub const MUTED: Color32 = Color32::from_rgb(110, 114, 124);
pub const GRID: Color32 = Color32::from_rgb(222, 224, 230);
const UP: Color32 = Color32::from_rgb(70, 150, 96);
const DOWN: Color32 = Color32::from_rgb(200, 78, 66);

fn text_on(fill: Color32) -> Color32 {
    let l = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if l < 150.0 {
        Color32::WHITE
    } else {
        INK
    }
}

/// Draws text if it is big enough to read; `size` is in screen points.
fn text(p: &Canvas, pos: Pos2, align: Align2, s: &str, size: f32, color: Color32) -> Rect {
    if size < 2.6 || s.is_empty() {
        return Rect::from_center_size(pos, Vec2::ZERO);
    }
    p.text_at(pos, align, s, size, color)
}

fn text_width(p: &Canvas, s: &str, size: f32) -> f32 {
    if size < 2.6 {
        return s.len() as f32 * size * 0.5;
    }
    p.measure(s, size).x
}

pub static DECIMAL_COMMA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn fmt_num(v: f64, decimals: usize, unit: &str) -> String {
    let s = fmt_num_dot(v, decimals, unit);
    if DECIMAL_COMMA.load(std::sync::atomic::Ordering::Relaxed) {
        s.chars()
            .map(|c| match c {
                '.' => ',',
                ',' => '.',
                c => c,
            })
            .collect()
    } else {
        s
    }
}

fn fmt_num_dot(v: f64, decimals: usize, unit: &str) -> String {
    let s = format!("{:.*}", decimals, v.abs());
    // thousands separators
    let (int, frac) = match s.find('.') {
        Some(i) => (&s[..i], &s[i..]),
        None => (&s[..], ""),
    };
    let mut out = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    let sign = if v < 0.0 && v.abs() >= 0.5 * 10f64.powi(-(decimals as i32)) { "−" } else { "" };
    format!("{sign}{out}{frac}{unit}")
}

fn series_color(chart: &Chart, i: usize, style: &Style) -> Color32 {
    match chart.series.get(i).and_then(|s| s.color) {
        Some(c) => rgb(c),
        None => rgb(style.colors[i % style.colors.len()]),
    }
}

// ---------------------------------------------------------------------------

pub fn draw_slide(p: &Canvas, slide: &Slide, xf: &Xf, style: &Style, number: usize) {
    let page = xf.rect(0.0, 0.0, SLIDE_W, SLIDE_H);
    if !p.recording() {
        p.rect_filled(page, 0.0, Color32::WHITE);
    }
    let s = xf.scale;
    // title area
    text(p, xf.pos(40.0, 30.0), Align2::LEFT_CENTER, &slide.title, 26.0 * s, INK);
    p.line_segment([xf.pos(40.0, 60.0), xf.pos(920.0, 60.0)], Stroke::new((1.5 * s).max(0.5), rgb(style.accent)));
    // footer
    text(p, xf.pos(40.0, 518.0), Align2::LEFT_CENTER, &slide.footnote, 10.0 * s, MUTED);
    text(p, xf.pos(920.0, 518.0), Align2::RIGHT_CENTER, &number.to_string(), 10.0 * s, MUTED);
    for e in &slide.elements {
        draw_element(p, e, xf, style);
    }
}

pub fn draw_element(p: &Canvas, e: &Element, xf: &Xf, style: &Style) {
    let r = xf.el_rect(e);
    let s = xf.scale;
    let p = &p.with_clip_rect(r.expand(30.0 * s).intersect(p.clip_rect()));
    p.begin_group(e.kind.label());
    match &e.kind {
        Kind::Chart(c) => draw_chart(p, r, s, c, style),
        Kind::Gantt(g) => draw_gantt(p, r, s, g, style),
        Kind::Text(t) => draw_text(p, r, s, t),
        Kind::Harvey(h) => draw_harvey(p, r, s, h, style),
        Kind::Check(c) => draw_check(p, r, s, c, style),
        Kind::Process(pr) => draw_process(p, r, s, pr, style),
        Kind::Agenda(a) => draw_agenda(p, r, s, a, style),
        Kind::Shape(sh) => draw_shape(p, r, s, sh),
        Kind::Table(t) => draw_table(p, r, s, t, style),
    }
    p.end_group();
}

// ---------------------------------------------------------------------------
// Charts

fn nice_step(range: f64, ticks: f64) -> f64 {
    if range <= 0.0 {
        return 1.0;
    }
    let raw = range / ticks;
    let mag = 10f64.powf(raw.log10().floor());
    let n = raw / mag;
    let m = if n < 1.5 {
        1.0
    } else if n < 3.0 {
        2.0
    } else if n < 7.0 {
        5.0
    } else {
        10.0
    };
    m * mag
}

struct Plot {
    rect: Rect,
    lo: f64,
    hi: f64,
}

impl Plot {
    fn y(&self, v: f64) -> f32 {
        let t = ((v - self.lo) / (self.hi - self.lo).max(1e-9)) as f32;
        self.rect.bottom() - t * self.rect.height()
    }
}

fn draw_legend(p: &Canvas, r: Rect, s: f32, c: &Chart, style: &Style) -> f32 {
    let size = 11.0 * s;
    let mut x = r.left();
    let y = r.top() + 8.0 * s;
    for (i, ser) in c.series.iter().enumerate() {
        let sw = Rect::from_center_size(Pos2::new(x + 5.0 * s, y), Vec2::splat(9.0 * s));
        p.rect_filled(sw, 1.0, series_color(c, i, style));
        let tr = text(p, Pos2::new(x + 13.0 * s, y), Align2::LEFT_CENTER, &ser.name, size, INK);
        x = tr.right().max(x + 13.0 * s + text_width(p, &ser.name, size)) + 14.0 * s;
    }
    18.0 * s
}

pub fn draw_chart(p: &Canvas, r: Rect, s: f32, c: &Chart, style: &Style) {
    let mut area = r;
    if !c.title.is_empty() {
        text(p, Pos2::new(area.left(), area.top() + 9.0 * s), Align2::LEFT_CENTER, &c.title, 15.0 * s, INK);
        area.min.y += 24.0 * s;
    }
    let legend_ok = c.legend && !matches!(c.kind, ChartKind::Waterfall) && !c.kind.is_xy();
    if legend_ok {
        let h = draw_legend(p, area, s, c, style);
        area.min.y += h + 4.0 * s;
    }
    if c.categories.is_empty() || c.series.is_empty() {
        text(p, area.center(), Align2::CENTER_CENTER, "No data — open the datasheet", 12.0 * s, MUTED);
        return;
    }
    match c.kind {
        ChartKind::Stacked | ChartKind::Clustered | ChartKind::Line | ChartKind::Area | ChartKind::Waterfall => {
            column_like(p, area, s, c, style, None)
        }
        ChartKind::Combination => {
            // all series but the last as stacked columns, the last as a line on top
            let mut bars = c.clone();
            bars.kind = ChartKind::Stacked;
            let line = bars.series.pop();
            match line {
                Some(l) if !bars.series.is_empty() => {
                    let col = series_color(c, c.series.len() - 1, style);
                    column_like(p, area, s, &bars, style, Some((&l, col)))
                }
                _ => column_like(p, area, s, c, style, None),
            }
        }
        ChartKind::AreaPercent => {
            let mut pc = c.clone();
            pc.kind = ChartKind::Area;
            pc.unit = "%".into();
            pc.decimals = 0;
            pc.totals = false;
            pc.cagr = None;
            pc.diff = None;
            for i in 0..pc.categories.len() {
                let total: f64 = c.series.iter().map(|s| s.values.get(i).copied().unwrap_or(0.0).max(0.0)).sum();
                for ser in pc.series.iter_mut() {
                    if let Some(v) = ser.values.get_mut(i) {
                        *v = if total > 0.0 { v.max(0.0) / total * 100.0 } else { 0.0 };
                    }
                }
            }
            column_like(p, area, s, &pc, style, None)
        }
        ChartKind::Scatter | ChartKind::Bubble => xy_chart(p, area, s, c, style),
        ChartKind::Percent => percent_chart(p, area, s, c, style),
        ChartKind::Bar => bar_chart(p, area, s, c, style),
        ChartKind::Mekko => mekko_chart(p, area, s, c, style),
        ChartKind::Pie | ChartKind::Doughnut => pie_chart(p, area, s, c, style),
    }
}

/// Per-category (bottom, top) for waterfall bars, plus the displayed value.
fn waterfall_bars(c: &Chart) -> Vec<(f64, f64, f64, bool)> {
    let mut run = 0.0;
    let vals = c.series.first().map(|s| s.values.clone()).unwrap_or_default();
    let mut out = Vec::new();
    for (i, v) in vals.iter().enumerate() {
        let sub = c.subtotal.get(i).copied().unwrap_or(false);
        if sub {
            out.push((0.0, run, run, true));
        } else if i == 0 {
            run = *v;
            out.push((0.0, run, run, true));
        } else {
            let start = run;
            run += v;
            out.push((start, run, *v, false));
        }
    }
    out
}

fn column_like(p: &Canvas, area: Rect, s: f32, c: &Chart, style: &Style, overlay: Option<(&Series, Color32)>) {
    let n = c.categories.len();
    let lbl = 11.0 * s;
    let annotate_top = if c.cagr.is_some() { 34.0 * s } else { 0.0 };
    let diff_right = if c.diff.is_some() { 56.0 * s } else { 0.0 };
    let axis_w = if c.axis { 34.0 * s } else { 0.0 };
    let mut plot_rect = Rect::from_min_max(
        Pos2::new(area.left() + axis_w, area.top() + annotate_top + 16.0 * s),
        Pos2::new(area.right() - diff_right, area.bottom() - 20.0 * s),
    );
    if plot_rect.height() < 10.0 || plot_rect.width() < 10.0 {
        plot_rect = area;
    }

    // value range
    let (mut lo, mut hi) = (0.0f64, 0.0f64);
    match c.kind {
        ChartKind::Stacked | ChartKind::Area => {
            for i in 0..n {
                let (mut pos, mut neg) = (0.0, 0.0);
                for ser in &c.series {
                    let v = ser.values.get(i).copied().unwrap_or(0.0);
                    if v >= 0.0 {
                        pos += v
                    } else {
                        neg += v
                    }
                }
                hi = hi.max(pos);
                lo = lo.min(neg);
            }
        }
        ChartKind::Waterfall => {
            for (a, b, _, _) in waterfall_bars(c) {
                hi = hi.max(a.max(b));
                lo = lo.min(a.min(b));
            }
        }
        _ => {
            for ser in &c.series {
                for v in &ser.values {
                    hi = hi.max(*v);
                    lo = lo.min(*v);
                }
            }
        }
    }
    if let Some((l, _)) = overlay {
        for v in &l.values {
            hi = hi.max(*v);
            lo = lo.min(*v);
        }
    }
    if hi - lo < 1e-9 {
        hi = lo + 1.0;
    }
    let pad = (hi - lo) * 0.04;
    let plot = Plot { rect: plot_rect, lo: if lo < 0.0 { lo - pad } else { 0.0 }, hi: hi + pad };

    // axis + gridlines
    if c.axis {
        let step = nice_step(plot.hi - plot.lo, 5.0);
        let mut v = (plot.lo / step).ceil() * step;
        while v <= plot.hi + 1e-9 {
            let y = plot.y(v);
            p.line_segment([Pos2::new(plot_rect.left(), y), Pos2::new(plot_rect.right(), y)], Stroke::new(0.6 * s, GRID));
            text(p, Pos2::new(plot_rect.left() - 5.0 * s, y), Align2::RIGHT_CENTER, &fmt_num(v, 0, ""), 10.0 * s, MUTED);
            v += step;
        }
    }

    let slot = plot_rect.width() / n as f32;
    let bar_w = slot * (1.0 - c.gap.clamp(0.05, 0.9));
    let cx = |i: usize| plot_rect.left() + slot * (i as f32 + 0.5);
    let base_y = plot.y(0.0);
    let mut tops: Vec<f32> = vec![base_y; n]; // y of highest point per category, for annotations

    match c.kind {
        ChartKind::Stacked => {
            let mut seg_tops: Vec<Vec<(f32, f32)>> = vec![vec![]; c.series.len()];
            for i in 0..n {
                let (mut pos, mut neg) = (0.0, 0.0);
                for (si, ser) in c.series.iter().enumerate() {
                    let v = ser.values.get(i).copied().unwrap_or(0.0);
                    let (a, b) = if v >= 0.0 {
                        let a = pos;
                        pos += v;
                        (a, pos)
                    } else {
                        let a = neg;
                        neg += v;
                        (a, neg)
                    };
                    let y0 = plot.y(a);
                    let y1 = plot.y(b);
                    let rect = Rect::from_x_y_ranges(cx(i) - bar_w / 2.0..=cx(i) + bar_w / 2.0, y0.min(y1)..=y0.max(y1));
                    let col = series_color(c, si, style);
                    p.rect_filled(rect, 0.0, col);
                    p.rect_stroke(rect, 0.0, Stroke::new(0.5 * s, Color32::WHITE), StrokeKind::Inside);
                    seg_tops[si].push((rect.left(), y1));
                    if c.labels && rect.height() > lbl * 1.1 && v != 0.0 {
                        text(p, rect.center(), Align2::CENTER_CENTER, &fmt_num(v, c.decimals, &c.unit), lbl, text_on(col));
                    }
                }
                let top = plot.y(pos);
                tops[i] = top;
                if c.totals {
                    text(p, Pos2::new(cx(i), top - 3.0 * s), Align2::CENTER_BOTTOM, &fmt_num(c.total(i), c.decimals, &c.unit), lbl * 1.05, INK);
                    tops[i] = top - lbl * 1.3;
                }
            }
            if c.connectors {
                for st in &seg_tops {
                    for w in st.windows(2) {
                        p.line_segment(
                            [Pos2::new(w[0].0 + bar_w, w[0].1), Pos2::new(w[1].0, w[1].1)],
                            Stroke::new(0.6 * s, MUTED),
                        );
                    }
                }
            }
        }
        ChartKind::Clustered => {
            let k = c.series.len().max(1) as f32;
            let sub = bar_w / k;
            for i in 0..n {
                for (si, ser) in c.series.iter().enumerate() {
                    let v = ser.values.get(i).copied().unwrap_or(0.0);
                    let x0 = cx(i) - bar_w / 2.0 + sub * si as f32;
                    let y1 = plot.y(v);
                    let rect = Rect::from_x_y_ranges(x0 + 0.5 * s..=x0 + sub - 0.5 * s, y1.min(base_y)..=y1.max(base_y));
                    p.rect_filled(rect, 0.0, series_color(c, si, style));
                    if c.labels {
                        let (pos, al) = if v >= 0.0 {
                            (Pos2::new(rect.center().x, rect.top() - 2.0 * s), Align2::CENTER_BOTTOM)
                        } else {
                            (Pos2::new(rect.center().x, rect.bottom() + 2.0 * s), Align2::CENTER_TOP)
                        };
                        text(p, pos, al, &fmt_num(v, c.decimals, &c.unit), lbl * 0.9, INK);
                    }
                    tops[i] = tops[i].min(rect.top() - if c.labels { lbl * 1.2 } else { 0.0 });
                }
            }
        }
        ChartKind::Line => {
            for (si, ser) in c.series.iter().enumerate() {
                let col = series_color(c, si, style);
                let pts: Vec<Pos2> = (0..n).map(|i| Pos2::new(cx(i), plot.y(ser.values.get(i).copied().unwrap_or(0.0)))).collect();
                p.add(Shape::line(pts.clone(), Stroke::new(2.2 * s, col)));
                for (i, pt) in pts.iter().enumerate() {
                    p.circle_filled(*pt, 3.4 * s, col);
                    p.circle_filled(*pt, 1.6 * s, Color32::WHITE);
                    if c.labels {
                        let v = ser.values.get(i).copied().unwrap_or(0.0);
                        text(p, *pt - Vec2::new(0.0, 6.0 * s), Align2::CENTER_BOTTOM, &fmt_num(v, c.decimals, &c.unit), lbl * 0.9, col);
                    }
                    tops[i] = tops[i].min(pt.y - lbl * 1.6);
                }
            }
        }
        ChartKind::Area => {
            let mut lower: Vec<f64> = vec![0.0; n];
            for (si, ser) in c.series.iter().enumerate() {
                let col = series_color(c, si, style);
                let upper: Vec<f64> = (0..n).map(|i| lower[i] + ser.values.get(i).copied().unwrap_or(0.0).max(0.0)).collect();
                for i in 0..n.saturating_sub(1) {
                    let quad = vec![
                        Pos2::new(cx(i), plot.y(upper[i])),
                        Pos2::new(cx(i + 1), plot.y(upper[i + 1])),
                        Pos2::new(cx(i + 1), plot.y(lower[i + 1])),
                        Pos2::new(cx(i), plot.y(lower[i])),
                    ];
                    p.add(Shape::convex_polygon(quad, col, Stroke::NONE));
                }
                let line: Vec<Pos2> = (0..n).map(|i| Pos2::new(cx(i), plot.y(upper[i]))).collect();
                p.add(Shape::line(line, Stroke::new(0.8 * s, Color32::WHITE)));
                if c.labels {
                    for i in 0..n {
                        let v = ser.values.get(i).copied().unwrap_or(0.0);
                        let ym = (plot.y(upper[i]) + plot.y(lower[i])) / 2.0;
                        if (plot.y(lower[i]) - plot.y(upper[i])) > lbl {
                            text(p, Pos2::new(cx(i), ym), Align2::CENTER_CENTER, &fmt_num(v, c.decimals, &c.unit), lbl * 0.9, text_on(col));
                        }
                    }
                }
                lower = upper;
            }
            for i in 0..n {
                tops[i] = plot.y(lower[i]);
                if c.totals {
                    text(p, Pos2::new(cx(i), tops[i] - 3.0 * s), Align2::CENTER_BOTTOM, &fmt_num(lower[i], c.decimals, &c.unit), lbl, INK);
                    tops[i] -= lbl * 1.3;
                }
            }
        }
        ChartKind::Waterfall => {
            let bars = waterfall_bars(c);
            let total_col = series_color(c, 0, style);
            for (i, (a, b, shown, is_total)) in bars.iter().enumerate() {
                let y0 = plot.y(*a);
                let y1 = plot.y(*b);
                let rect = Rect::from_x_y_ranges(cx(i) - bar_w / 2.0..=cx(i) + bar_w / 2.0, y0.min(y1)..=y0.max(y1).max(y0.min(y1) + 1.0));
                let col = if *is_total {
                    total_col
                } else if b >= a {
                    UP
                } else {
                    DOWN
                };
                p.rect_filled(rect, 0.0, col);
                if c.connectors && i + 1 < bars.len() {
                    let y = plot.y(*b);
                    p.line_segment(
                        [Pos2::new(rect.right(), y), Pos2::new(cx(i + 1) - bar_w / 2.0, y)],
                        Stroke::new(0.7 * s, MUTED),
                    );
                }
                if c.labels {
                    let label = if *is_total {
                        fmt_num(*shown, c.decimals, &c.unit)
                    } else if *shown >= 0.0 {
                        format!("+{}", fmt_num(*shown, c.decimals, &c.unit))
                    } else {
                        fmt_num(*shown, c.decimals, &c.unit)
                    };
                    let up = *b >= *a || *is_total;
                    let (pos, al) = if up {
                        (Pos2::new(cx(i), rect.top() - 3.0 * s), Align2::CENTER_BOTTOM)
                    } else {
                        (Pos2::new(cx(i), rect.bottom() + 3.0 * s), Align2::CENTER_TOP)
                    };
                    text(p, pos, al, &label, lbl * 1.05, INK);
                }
                tops[i] = rect.top();
            }
        }
        _ => {}
    }

    if let Some((l, col)) = overlay {
        let pts: Vec<Pos2> = (0..n).map(|i| Pos2::new(cx(i), plot.y(l.values.get(i).copied().unwrap_or(0.0)))).collect();
        p.add(Shape::line(pts.clone(), Stroke::new(2.2 * s, col)));
        for (i, pt) in pts.iter().enumerate() {
            p.circle_filled(*pt, 3.6 * s, col);
            p.circle_filled(*pt, 1.7 * s, Color32::WHITE);
            if c.labels {
                let v = l.values.get(i).copied().unwrap_or(0.0);
                let tr = Rect::from_center_size(*pt - Vec2::new(0.0, 12.0 * s), Vec2::new(30.0 * s, lbl * 1.4));
                p.rect_filled(tr, 2.0 * s, Color32::WHITE);
                text(p, tr.center(), Align2::CENTER_CENTER, &fmt_num(v, c.decimals, &c.unit), lbl * 0.9, col);
            }
        }
    }

    // baseline
    p.line_segment([Pos2::new(plot_rect.left(), base_y), Pos2::new(plot_rect.right(), base_y)], Stroke::new(0.9 * s, MUTED));
    // category labels
    let cat_size = if slot < 40.0 * s { lbl * 0.85 } else { lbl };
    for (i, cat) in c.categories.iter().enumerate() {
        text(p, Pos2::new(cx(i), plot_rect.bottom() + 5.0 * s), Align2::CENTER_TOP, cat, cat_size, INK);
    }

    // growth annotation (CAGR)
    if let Some((a, b)) = c.cagr {
        if a < n && b < n && a != b {
            let (a, b) = (a.min(b), a.max(b));
            let va = c.total(a);
            let vb = c.total(b);
            let years = (b - a) as f64;
            let rate = if va > 0.0 && vb > 0.0 { (vb / va).powf(1.0 / years) - 1.0 } else { f64::NAN };
            let top = area.top() + 18.0 * s;
            let xa = cx(a);
            let xb = cx(b);
            let stroke = Stroke::new(0.9 * s, INK);
            p.line_segment([Pos2::new(xa, tops[a] - 2.0 * s), Pos2::new(xa, top)], stroke);
            p.line_segment([Pos2::new(xa, top), Pos2::new(xb, top)], stroke);
            p.arrow(Pos2::new(xb, top), Vec2::new(0.0, (tops[b] - 2.0 * s - top).max(4.0 * s)), stroke);
            let label = if rate.is_nan() { "n/a".to_string() } else { format!("{:+.1}%", rate * 100.0) };
            let mid = Pos2::new((xa + xb) / 2.0, top);
            let w = text_width(p, &label, lbl) + 14.0 * s;
            let bubble = Rect::from_center_size(mid, Vec2::new(w, lbl * 1.7));
            p.add(egui::epaint::RectShape::new(bubble, bubble.height() / 2.0, Color32::WHITE, stroke, StrokeKind::Inside));
            text(p, mid, Align2::CENTER_CENTER, &label, lbl, INK);
            text(p, Pos2::new(mid.x, bubble.top() - 1.0 * s), Align2::CENTER_BOTTOM, "CAGR", 8.5 * s, MUTED);
        }
    }
    // difference arrow
    if let Some((a, b)) = c.diff {
        if a < n && b < n && a != b {
            let va = c.total(a);
            let vb = c.total(b);
            let ya = plot.y(va);
            let yb = plot.y(vb);
            let x = plot_rect.right() + 18.0 * s;
            let stroke = Stroke::new(0.9 * s, INK);
            let dash = |from: Pos2, to: Pos2| p.dashed(from, to, Stroke::new(0.7 * s, MUTED), 3.0 * s, 2.5 * s);
            dash(Pos2::new(cx(a) + bar_w / 2.0, ya), Pos2::new(x, ya));
            dash(Pos2::new(cx(b) + bar_w / 2.0, yb), Pos2::new(x, yb));
            p.arrow(Pos2::new(x, ya), Vec2::new(0.0, yb - ya), stroke);
            let pct = if va.abs() > 1e-9 { (vb - va) / va.abs() * 100.0 } else { 0.0 };
            let label = format!("{:+.0}%", pct);
            let mid = Pos2::new(x, (ya + yb) / 2.0);
            let w = text_width(p, &label, lbl) + 10.0 * s;
            let bubble = Rect::from_min_size(Pos2::new(x + 4.0 * s, mid.y - lbl * 0.85), Vec2::new(w, lbl * 1.7));
            p.add(egui::epaint::RectShape::new(bubble, 3.0 * s, Color32::WHITE, stroke, StrokeKind::Inside));
            text(p, bubble.center(), Align2::CENTER_CENTER, &label, lbl, INK);
        }
    }
}

fn percent_chart(p: &Canvas, area: Rect, s: f32, c: &Chart, style: &Style) {
    let n = c.categories.len();
    let lbl = 11.0 * s;
    let plot = Rect::from_min_max(Pos2::new(area.left(), area.top() + 16.0 * s), Pos2::new(area.right(), area.bottom() - 20.0 * s));
    let slot = plot.width() / n as f32;
    let bar_w = slot * (1.0 - c.gap.clamp(0.05, 0.9));
    for i in 0..n {
        let total: f64 = c.series.iter().map(|s| s.values.get(i).copied().unwrap_or(0.0).max(0.0)).sum();
        let cx = plot.left() + slot * (i as f32 + 0.5);
        let mut acc = 0.0;
        for (si, ser) in c.series.iter().enumerate() {
            let v = ser.values.get(i).copied().unwrap_or(0.0).max(0.0);
            let share = if total > 0.0 { v / total } else { 0.0 };
            let y0 = plot.bottom() - (acc as f32) * plot.height();
            acc += share;
            let y1 = plot.bottom() - (acc as f32) * plot.height();
            let rect = Rect::from_x_y_ranges(cx - bar_w / 2.0..=cx + bar_w / 2.0, y1..=y0);
            let col = series_color(c, si, style);
            p.rect_filled(rect, 0.0, col);
            p.rect_stroke(rect, 0.0, Stroke::new(0.5 * s, Color32::WHITE), StrokeKind::Inside);
            if c.labels && rect.height() > lbl * 1.1 {
                text(p, rect.center(), Align2::CENTER_CENTER, &format!("{:.0}%", share * 100.0), lbl, text_on(col));
            }
        }
        if c.totals {
            text(p, Pos2::new(cx, plot.top() - 3.0 * s), Align2::CENTER_BOTTOM, &fmt_num(total, c.decimals, &c.unit), lbl, INK);
        }
        text(p, Pos2::new(cx, plot.bottom() + 5.0 * s), Align2::CENTER_TOP, &c.categories[i], lbl, INK);
    }
}

fn bar_chart(p: &Canvas, area: Rect, s: f32, c: &Chart, style: &Style) {
    let n = c.categories.len();
    let lbl = 11.0 * s;
    let label_w = c.categories.iter().map(|t| text_width(p, t, lbl)).fold(0.0, f32::max) + 10.0 * s;
    let total_w = if c.totals { 44.0 * s } else { 0.0 };
    let plot = Rect::from_min_max(Pos2::new(area.left() + label_w, area.top() + 4.0 * s), Pos2::new(area.right() - total_w, area.bottom()));
    let max: f64 = (0..n).map(|i| c.series.iter().map(|s| s.values.get(i).copied().unwrap_or(0.0).max(0.0)).sum::<f64>()).fold(1e-9, f64::max);
    let slot = plot.height() / n as f32;
    let bar_h = slot * (1.0 - c.gap.clamp(0.05, 0.9));
    for i in 0..n {
        let cy = plot.top() + slot * (i as f32 + 0.5);
        let mut acc = 0.0;
        for (si, ser) in c.series.iter().enumerate() {
            let v = ser.values.get(i).copied().unwrap_or(0.0).max(0.0);
            let x0 = plot.left() + (acc / max) as f32 * plot.width();
            acc += v;
            let x1 = plot.left() + (acc / max) as f32 * plot.width();
            let rect = Rect::from_x_y_ranges(x0..=x1, cy - bar_h / 2.0..=cy + bar_h / 2.0);
            let col = series_color(c, si, style);
            p.rect_filled(rect, 0.0, col);
            p.rect_stroke(rect, 0.0, Stroke::new(0.5 * s, Color32::WHITE), StrokeKind::Inside);
            let txt = fmt_num(v, c.decimals, &c.unit);
            if c.labels && rect.width() > text_width(p, &txt, lbl) + 4.0 * s && rect.height() > lbl {
                text(p, rect.center(), Align2::CENTER_CENTER, &txt, lbl, text_on(col));
            }
        }
        let end = plot.left() + (acc / max) as f32 * plot.width();
        if c.totals {
            text(p, Pos2::new(end + 4.0 * s, cy), Align2::LEFT_CENTER, &fmt_num(acc, c.decimals, &c.unit), lbl, INK);
        }
        text(p, Pos2::new(plot.left() - 6.0 * s, cy), Align2::RIGHT_CENTER, &c.categories[i], lbl, INK);
    }
    p.line_segment([plot.left_top(), plot.left_bottom()], Stroke::new(0.9 * s, MUTED));
}

fn mekko_chart(p: &Canvas, area: Rect, s: f32, c: &Chart, style: &Style) {
    let n = c.categories.len();
    let lbl = 10.5 * s;
    let plot = Rect::from_min_max(Pos2::new(area.left() + 28.0 * s, area.top() + 16.0 * s), Pos2::new(area.right(), area.bottom() - 20.0 * s));
    let totals: Vec<f64> = (0..n).map(|i| c.series.iter().map(|s| s.values.get(i).copied().unwrap_or(0.0).max(0.0)).sum()).collect();
    let grand: f64 = totals.iter().sum::<f64>().max(1e-9);
    let gap = 1.5 * s;
    let mut x = plot.left();
    for i in 0..n {
        let w = (totals[i] / grand) as f32 * plot.width();
        let mut acc = 0.0;
        for (si, ser) in c.series.iter().enumerate() {
            let v = ser.values.get(i).copied().unwrap_or(0.0).max(0.0);
            let share = if totals[i] > 0.0 { v / totals[i] } else { 0.0 };
            let y0 = plot.bottom() - acc as f32 * plot.height();
            acc += share;
            let y1 = plot.bottom() - acc as f32 * plot.height();
            let rect = Rect::from_x_y_ranges(x..=(x + w - gap).max(x), y1..=y0);
            let col = series_color(c, si, style);
            p.rect_filled(rect, 0.0, col);
            p.rect_stroke(rect, 0.0, Stroke::new(0.5 * s, Color32::WHITE), StrokeKind::Inside);
            let t = format!("{:.0}%", share * 100.0);
            if c.labels && rect.height() > lbl * 1.1 && rect.width() > text_width(p, &t, lbl) + 4.0 * s {
                text(p, rect.center(), Align2::CENTER_CENTER, &t, lbl, text_on(col));
            }
        }
        let cx = x + w / 2.0;
        if c.totals {
            text(p, Pos2::new(cx, plot.top() - 3.0 * s), Align2::CENTER_BOTTOM, &fmt_num(totals[i], c.decimals, &c.unit), lbl, INK);
        }
        text(p, Pos2::new(cx, plot.bottom() + 5.0 * s), Align2::CENTER_TOP, &c.categories[i], lbl, INK);
        x += w;
    }
    // percentage axis
    for k in 0..=4 {
        let y = plot.bottom() - plot.height() * k as f32 / 4.0;
        text(p, Pos2::new(plot.left() - 4.0 * s, y), Align2::RIGHT_CENTER, &format!("{}%", k * 25), 9.0 * s, MUTED);
    }
}

fn sector(center: Pos2, r_in: f32, r_out: f32, a0: f32, a1: f32, col: Color32, p: &Canvas) {
    let steps = (((a1 - a0).abs() / TAU) * 96.0).ceil().max(1.0) as usize;
    if p.record_sector(center, r_in, r_out, a0, a1, col) {
        return;
    }
    // one filled mesh (no seams), plus a thin line along the arcs for smooth edges
    let dir = |k: usize| Vec2::angled(a0 + (a1 - a0) * k as f32 / steps as f32);
    let outer: Vec<Pos2> = (0..=steps).map(|k| center + dir(k) * r_out).collect();
    let mut mesh = egui::Mesh::default();
    if r_in <= 0.0 {
        mesh.colored_vertex(center, col);
        for pt in &outer {
            mesh.colored_vertex(*pt, col);
        }
        for k in 0..steps as u32 {
            mesh.add_triangle(0, k + 1, k + 2);
        }
    } else {
        for k in 0..=steps {
            mesh.colored_vertex(outer[k], col);
            mesh.colored_vertex(center + dir(k) * r_in, col);
        }
        for k in 0..steps as u32 {
            let (o0, i0, o1, i1) = (2 * k, 2 * k + 1, 2 * k + 2, 2 * k + 3);
            mesh.add_triangle(o0, i0, o1);
            mesh.add_triangle(i0, i1, o1);
        }
        let inner: Vec<Pos2> = (0..=steps).map(|k| center + dir(k) * r_in).collect();
        p.add(Shape::line(inner, Stroke::new(0.8, col)));
    }
    p.add(Shape::mesh(mesh));
    p.add(Shape::line(outer, Stroke::new(0.8, col)));
}

fn pie_chart(p: &Canvas, area: Rect, s: f32, c: &Chart, style: &Style) {
    let cat = c.pie_category.min(c.categories.len().saturating_sub(1));
    let vals: Vec<f64> = c.series.iter().map(|s| s.values.get(cat).copied().unwrap_or(0.0).max(0.0)).collect();
    let total: f64 = vals.iter().sum::<f64>().max(1e-9);
    let radius = (area.width().min(area.height()) / 2.0 - 22.0 * s).max(4.0);
    let center = area.center();
    let r_in = if c.kind == ChartKind::Doughnut { radius * 0.55 } else { 0.0 };
    let lbl = 11.0 * s;
    let mut a = -PI / 2.0;
    for (i, v) in vals.iter().enumerate() {
        let sweep = (*v / total) as f32 * TAU;
        if sweep <= 0.0 {
            continue;
        }
        let col = series_color(c, i, style);
        sector(center, r_in, radius, a, a + sweep, col, p);
        // white separators
        p.line_segment([center + Vec2::angled(a) * r_in, center + Vec2::angled(a) * radius], Stroke::new(1.2 * s, Color32::WHITE));
        if c.labels {
            let mid = a + sweep / 2.0;
            let t = format!("{:.0}%", v / total * 100.0);
            let inside_r = if r_in > 0.0 { (r_in + radius) / 2.0 } else { radius * 0.62 };
            if sweep > 0.35 {
                text(p, center + Vec2::angled(mid) * inside_r, Align2::CENTER_CENTER, &t, lbl, text_on(col));
            } else {
                text(p, center + Vec2::angled(mid) * (radius + 12.0 * s), Align2::CENTER_CENTER, &t, lbl, INK);
            }
        }
        a += sweep;
    }
    if c.kind == ChartKind::Doughnut && c.totals {
        text(p, center, Align2::CENTER_CENTER, &fmt_num(total, c.decimals, &c.unit), 16.0 * s, INK);
    }
}

fn xy_chart(p: &Canvas, area: Rect, s: f32, c: &Chart, style: &Style) {
    let lbl = 10.5 * s;
    let axis_w = 34.0 * s;
    let plot = Rect::from_min_max(
        Pos2::new(area.left() + axis_w, area.top() + 10.0 * s),
        Pos2::new(area.right() - 10.0 * s, area.bottom() - 22.0 * s),
    );
    if plot.width() < 10.0 || plot.height() < 10.0 || c.series.is_empty() {
        return;
    }
    let get = |ser: &Series, k: usize| ser.values.get(k).copied().unwrap_or(0.0);
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for ser in &c.series {
        x0 = x0.min(get(ser, 0));
        x1 = x1.max(get(ser, 0));
        y0 = y0.min(get(ser, 1));
        y1 = y1.max(get(ser, 1));
    }
    let xs = nice_step((x1 - x0.min(0.0)).max(1e-9), 5.0);
    let ys = nice_step((y1 - y0.min(0.0)).max(1e-9), 5.0);
    // leave room so points (and bubbles) at the edge stay inside
    let room = |max: f64, step: f64| {
        let top = (max / step).ceil() * step;
        if top - max < step * 0.35 { top + step } else { top }.max(step)
    };
    let (x0, x1) = ((x0.min(0.0) / xs).floor() * xs, room(x1, xs));
    let (y0, y1) = ((y0.min(0.0) / ys).floor() * ys, room(y1, ys));
    let px = |v: f64| plot.left() + ((v - x0) / (x1 - x0).max(1e-9)) as f32 * plot.width();
    let py = |v: f64| plot.bottom() - ((v - y0) / (y1 - y0).max(1e-9)) as f32 * plot.height();
    if c.axis {
        let mut v = y0;
        while v <= y1 + 1e-9 {
            p.line_segment([Pos2::new(plot.left(), py(v)), Pos2::new(plot.right(), py(v))], Stroke::new(0.6 * s, GRID));
            text(p, Pos2::new(plot.left() - 5.0 * s, py(v)), Align2::RIGHT_CENTER, &fmt_num(v, 0, ""), 9.5 * s, MUTED);
            v += ys;
        }
        let mut v = x0;
        while v <= x1 + 1e-9 {
            text(p, Pos2::new(px(v), plot.bottom() + 5.0 * s), Align2::CENTER_TOP, &fmt_num(v, 0, ""), 9.5 * s, MUTED);
            v += xs;
        }
    }
    p.line_segment([plot.left_bottom(), plot.right_bottom()], Stroke::new(0.9 * s, MUTED));
    p.line_segment([plot.left_top(), plot.left_bottom()], Stroke::new(0.9 * s, MUTED));
    let max_size = c.series.iter().map(|ser| get(ser, 2).abs()).fold(1e-9, f64::max);
    let max_r = plot.width().min(plot.height()) * 0.09;
    for (i, ser) in c.series.iter().enumerate() {
        let pt = Pos2::new(px(get(ser, 0)), py(get(ser, 1)));
        let col = if ser.color.is_some() {
            series_color(c, i, style)
        } else {
            rgb(style.colors[if c.kind == ChartKind::Bubble { 1 } else { 0 }])
        };
        let r = if c.kind == ChartKind::Bubble {
            ((get(ser, 2).abs() / max_size).sqrt() as f32 * max_r).max(3.0 * s)
        } else {
            4.0 * s
        };
        if c.kind == ChartKind::Bubble {
            p.circle_filled(pt, r, col.gamma_multiply(0.75));
            p.circle_stroke(pt, r, Stroke::new(0.8 * s, col));
        } else {
            p.circle_filled(pt, r, col);
        }
        if c.labels {
            text(p, pt + Vec2::new(r + 3.0 * s, 0.0), Align2::LEFT_CENTER, &ser.name, lbl, INK);
        }
    }
}

fn draw_shape(p: &Canvas, r: Rect, s: f32, sh: &ShapeBox) {
    let col = rgb(sh.fill);
    let d = (r.height() * 0.45).min(r.width() * 0.4);
    let (t, b, m) = (r.top(), r.bottom(), r.center().y);
    let (x, w) = (r.left(), r.width());
    let mut text_rect = r;
    let adj = ((d / r.height().min(w).max(0.01)) * 100000.0).round() as i64;
    match sh.shape {
        ShapeKind::Rounded => p.rect_filled(r, (8.0 * s).min(r.height() / 2.0), col),
        ShapeKind::Pentagon => {
            if !p.preset(r, "homePlate", Some(adj), col) {
                p.rect_filled(Rect::from_x_y_ranges(x..=x + w - d, t..=b), 0.0, col);
                p.add(Shape::convex_polygon(vec![Pos2::new(x + w - d, t), Pos2::new(x + w, m), Pos2::new(x + w - d, b)], col, Stroke::NONE));
            }
            text_rect.max.x -= d * 0.5;
        }
        ShapeKind::Chevron => {
            if !p.preset(r, "chevron", Some(adj), col) {
                p.add(Shape::convex_polygon(vec![Pos2::new(x, t), Pos2::new(x + w - d, t), Pos2::new(x + w, m), Pos2::new(x + d, m)], col, Stroke::NONE));
                p.add(Shape::convex_polygon(vec![Pos2::new(x + d, m), Pos2::new(x + w, m), Pos2::new(x + w - d, b), Pos2::new(x, b)], col, Stroke::NONE));
            }
            text_rect.min.x += d * 0.6;
            text_rect.max.x -= d * 0.4;
        }
    }
    let size = sh.size * s;
    let lines: Vec<&str> = sh.text.lines().collect();
    let lh = size * 1.25;
    let top = text_rect.center().y - lh * lines.len() as f32 / 2.0 + lh / 2.0;
    for (i, line) in lines.iter().enumerate() {
        text(p, Pos2::new(text_rect.center().x, top + lh * i as f32), Align2::CENTER_CENTER, line, size, rgb(sh.text_color));
    }
}

fn draw_table(p: &Canvas, r: Rect, s: f32, t: &Table, style: &Style) {
    let rows = t.rows.len().max(1);
    let cols = t.rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let rh = r.height() / rows as f32;
    let first_w = r.width() * if cols > 1 { 0.34 } else { 1.0 };
    let other_w = if cols > 1 { (r.width() - first_w) / (cols - 1) as f32 } else { 0.0 };
    let col_x = |c: usize| if c == 0 { r.left() } else { r.left() + first_w + other_w * (c - 1) as f32 };
    let size = (13.0 * s).min(rh * 0.5);
    for (ri, row) in t.rows.iter().enumerate() {
        let y = r.top() + rh * ri as f32;
        let head = t.header && ri == 0;
        let band = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), rh));
        if head {
            p.rect_filled(band, 0.0, rgb(style.colors[0]));
        } else if ri % 2 == 0 {
            p.rect_filled(band, 0.0, Color32::from_rgb(244, 246, 250));
        }
        for (ci, cell) in row.iter().enumerate() {
            let x = col_x(ci);
            let (pos, al) = if ci == 0 {
                (Pos2::new(x + 8.0 * s, y + rh / 2.0), Align2::LEFT_CENTER)
            } else {
                (Pos2::new(x + other_w / 2.0, y + rh / 2.0), Align2::CENTER_CENTER)
            };
            text(p, pos, al, cell, size, if head { Color32::WHITE } else { INK });
        }
        if !head {
            p.line_segment([Pos2::new(r.left(), y + rh), Pos2::new(r.right(), y + rh)], Stroke::new(0.6 * s, GRID));
        }
    }
}

// ---------------------------------------------------------------------------
// Gantt

/// Where things sit inside a Gantt chart drawn in `r` at scale `s`.
pub struct GanttGeo {
    pub grid: Rect,
    pub col_w: f32,
    pub row_h: f32,
    pub bottom: f32,
    pub header_h: f32,
    pub label_w: f32,
}

impl GanttGeo {
    pub fn new(r: Rect, s: f32, g: &Gantt) -> Self {
        let label_w = (r.width() * 0.2).max(60.0 * s);
        let header_h = if g.start.is_some() { 20.0 * s * g.scales.iter().filter(|b| **b).count().max(1) as f32 } else { 24.0 * s };
        let grid = Rect::from_min_max(Pos2::new(r.left() + label_w, r.top() + header_h), Pos2::new(r.right(), r.bottom() - 18.0 * s));
        let col_w = grid.width() / g.columns() as f32;
        let rows = g.tasks.len().max(1);
        let row_h = (grid.height() / rows as f32).min(44.0 * s);
        GanttGeo { grid, col_w, row_h, bottom: grid.top() + row_h * rows as f32, header_h, label_w }
    }
    pub fn x(&self, t: f32) -> f32 {
        self.grid.left() + t * self.col_w
    }
    pub fn t(&self, x: f32) -> f32 {
        (x - self.grid.left()) / self.col_w.max(1e-3)
    }
    pub fn row_center(&self, i: usize) -> f32 {
        self.grid.top() + self.row_h * (i as f32 + 0.5)
    }
    /// Which row a screen height falls in (clamped to the rows that exist).
    pub fn row_at(&self, y: f32, rows: usize) -> usize {
        (((y - self.grid.top()) / self.row_h.max(1.0)).floor().max(0.0) as usize).min(rows.saturating_sub(1))
    }
    /// The screen rectangle of a bar (or milestone diamond) on row `i`.
    pub fn bar_rect(&self, t: &Bar, i: usize) -> Rect {
        let cy = self.row_center(i);
        if t.milestone {
            Rect::from_center_size(Pos2::new(self.x(t.start), cy), Vec2::splat(self.row_h * 0.56))
        } else {
            Rect::from_x_y_ranges(self.x(t.start)..=self.x(t.end.max(t.start + 0.05)), cy - self.row_h * 0.22..=cy + self.row_h * 0.22)
        }
    }
}

pub fn draw_gantt(p: &Canvas, r: Rect, s: f32, g: &Gantt, style: &Style) {
    let lbl = 11.0 * s;
    let calendar = g.start.map(|[y, m, d]| days_from_civil(y, m.max(1) as u32, d.max(1) as u32));
    let geo = GanttGeo::new(r, s, g);
    let (header_h, grid, col_w, row_h, bottom) = (geo.header_h, geo.grid, geo.col_w, geo.row_h, geo.bottom);
    let _ = geo.label_w;
    let cols = g.columns();
    let rows = g.tasks.len().max(1);
    let x_of = |t: f32| grid.left() + t * col_w;
    let strong = Stroke::new(0.9 * s, INK);

    match calendar {
        Some(start) => {
            // header rows chosen in the scales: years, quarters, months, weeks
            let active: Vec<usize> = (0..4).filter(|k| g.scales[*k]).collect();
            let active = if active.is_empty() { vec![3] } else { active };
            let row = header_h / active.len() as f32;
            let key = |k: usize, i: usize| -> i64 {
                let (yy, mm, _) = civil_from_days(start + 7 * i as i64);
                match k {
                    0 => yy as i64,
                    1 => yy as i64 * 10 + (mm as i64 - 1) / 3,
                    2 => yy as i64 * 100 + mm as i64,
                    _ => i as i64,
                }
            };
            for i in 0..cols {
                let monday = start + 7 * i as i64;
                let (yy, mm, _) = civil_from_days(monday);
                let x = grid.left() + col_w * i as f32;
                let mut separated = false;
                for (ri, &k) in active.iter().enumerate() {
                    if !(i == 0 || key(k, i) != key(k, i - 1)) {
                        continue;
                    }
                    let y = r.top() + row * (ri as f32 + 0.5);
                    let label = match k {
                        0 => yy.to_string(),
                        1 => format!("Q{}", (mm - 1) / 3 + 1),
                        2 => MONTHS[(mm as usize + 11) % 12].to_string(),
                        _ => format!("{:02}", iso_week(monday).1),
                    };
                    if k == 3 {
                        text(p, Pos2::new(x + col_w / 2.0, y), Align2::CENTER_CENTER, &label, lbl, INK);
                    } else {
                        text(p, Pos2::new(x + 2.0 * s, y), Align2::LEFT_CENTER, &label, lbl * if k == 0 { 1.05 } else { 1.0 }, INK);
                    }
                    // one separator per boundary: the biggest unit that starts here wins
                    if !separated && g.separators[k] && i > 0 {
                        let top = Pos2::new(x, r.top() + row * ri as f32);
                        let end = Pos2::new(x, bottom);
                        match k {
                            0 => p.line_segment([top, end], strong),
                            1 | 2 => p.line_segment([top, end], Stroke::new(0.7 * s, MUTED)),
                            _ => p.dashed(Pos2::new(x, grid.top()), end, Stroke::new(0.8 * s, Color32::from_rgb(160, 166, 178)), 2.0 * s, 2.0 * s),
                        }
                        separated = true;
                    }
                }
            }
            for ri in 1..active.len() {
                let y = r.top() + row * ri as f32;
                p.line_segment([Pos2::new(grid.left(), y), Pos2::new(r.right(), y)], Stroke::new(0.6 * s, MUTED));
            }
            text(p, Pos2::new(r.left(), r.top() + row * (active.len() as f32 - 0.5)), Align2::LEFT_CENTER, &g.header, lbl * 1.05, INK);
            p.line_segment([Pos2::new(grid.left(), grid.top()), Pos2::new(grid.left(), bottom)], strong);
            p.line_segment([Pos2::new(r.right(), grid.top()), Pos2::new(r.right(), bottom)], strong);
        }
        None => {
            for (i, name) in g.periods.iter().enumerate() {
                let x = grid.left() + col_w * i as f32;
                if i % 2 == 0 {
                    p.rect_filled(Rect::from_min_size(Pos2::new(x, r.top()), Vec2::new(col_w, header_h + row_h * rows as f32)), 0.0, Color32::from_rgb(246, 247, 250));
                }
                text(p, Pos2::new(x + col_w / 2.0, r.top() + header_h / 2.0), Align2::CENTER_CENTER, name, lbl, MUTED);
                p.line_segment([Pos2::new(x, r.top() + header_h * 0.7), Pos2::new(x, bottom)], Stroke::new(0.4 * s, GRID));
            }
            text(p, Pos2::new(r.left(), r.top() + header_h / 2.0), Align2::LEFT_CENTER, &g.header, lbl, MUTED);
        }
    }
    p.line_segment([Pos2::new(r.left(), grid.top()), Pos2::new(r.right(), grid.top())], strong);
    p.line_segment([Pos2::new(r.left(), bottom), Pos2::new(r.right(), bottom)], strong);

    for (i, row) in g.tasks.iter().enumerate() {
        let cy = grid.top() + row_h * (i as f32 + 0.5);
        text(p, Pos2::new(r.left(), cy), Align2::LEFT_CENTER, &row.label, lbl, INK);
        if i + 1 < rows {
            p.line_segment([Pos2::new(r.left(), cy + row_h / 2.0), Pos2::new(r.right(), cy + row_h / 2.0)], Stroke::new(0.4 * s, GRID));
        }
        for t in &row.bars {
        let col = rgb(style.colors[t.color % style.colors.len()]);
        if t.milestone {
            let c = Pos2::new(x_of(t.start), cy);
            let d = row_h * 0.28;
            if p.preset(Rect::from_center_size(c, Vec2::splat(2.0 * d)), "diamond", None, col) {
                continue;
            }
            p.add(Shape::convex_polygon(
                vec![c + Vec2::new(0.0, -d), c + Vec2::new(d, 0.0), c + Vec2::new(0.0, d), c + Vec2::new(-d, 0.0)],
                col,
                Stroke::NONE,
            ));
        } else {
            let bar = Rect::from_x_y_ranges(x_of(t.start)..=x_of(t.end.max(t.start + 0.05)), cy - row_h * 0.22..=cy + row_h * 0.22);
            if t.dashed {
                // planned or optional work: dashed outline
                let st = Stroke::new(1.2 * s, col);
                let (d, gp) = (4.0 * s, 3.0 * s);
                p.dashed(bar.left_top(), bar.right_top(), st, d, gp);
                p.dashed(bar.left_bottom(), bar.right_bottom(), st, d, gp);
                p.dashed(bar.left_top(), bar.left_bottom(), st, d, gp);
                p.dashed(bar.right_top(), bar.right_bottom(), st, d, gp);
            } else {
                p.rect_filled(bar, 2.0 * s, col);
            }
        }
        }
    }

    // date line: follows the real date in calendar mode until it is dragged
    if let Some((t, label)) = g.today_line() {
        if t >= 0.0 && t <= cols as f32 {
            let x = x_of(t);
            p.dashed(Pos2::new(x, grid.top() - 4.0 * s), Pos2::new(x, bottom + 4.0 * s), Stroke::new(1.3 * s, INK), 4.0 * s, 3.0 * s);
            let tip = Pos2::new(x, bottom + 4.0 * s);
            p.add(Shape::convex_polygon(vec![tip, tip + Vec2::new(4.0 * s, 7.0 * s), tip + Vec2::new(-4.0 * s, 7.0 * s)], INK, Stroke::NONE));
            text(p, Pos2::new(x, bottom + 12.0 * s), Align2::CENTER_TOP, &label, 10.5 * s, INK);
        }
    }
}

// ---------------------------------------------------------------------------
// Layout elements

fn draw_text(p: &Canvas, r: Rect, s: f32, t: &TextBox) {
    if let Some(f) = t.fill {
        p.rect_filled(r, 3.0 * s, rgb(f));
    }
    let size = t.size * s;
    if size < 2.6 {
        return;
    }
    let pad = 8.0 * s;
    let inner = r.shrink(pad);
    p.text_block(inner, &t.text, size, rgb(t.color), t.align);
}

fn draw_harvey(p: &Canvas, r: Rect, s: f32, h: &Harvey, style: &Style) {
    let d = r.height().min(r.width()) * 0.8;
    let c = Pos2::new(r.left() + r.height() / 2.0, r.center().y);
    let col = rgb(style.colors[0]);
    p.circle_filled(c, d / 2.0, Color32::WHITE);
    let q = h.quarters.min(4) as f32;
    if q > 0.0 {
        sector(c, 0.0, d / 2.0, -PI / 2.0, -PI / 2.0 + q / 4.0 * TAU, col, p);
    }
    p.circle_stroke(c, d / 2.0, Stroke::new(1.2 * s, col));
    text(p, Pos2::new(r.left() + r.height() + 6.0 * s, c.y), Align2::LEFT_CENTER, &h.label, 14.0 * s, INK);
}

fn draw_check(p: &Canvas, r: Rect, s: f32, cm: &CheckMark, style: &Style) {
    let d = r.height().min(r.width()) * 0.7;
    let c = Pos2::new(r.left() + r.height() / 2.0, r.center().y);
    let bx = Rect::from_center_size(c, Vec2::splat(d));
    let accent = rgb(style.colors[0]);
    match cm.state {
        CheckState::Done => {
            p.rect_filled(bx, 3.0 * s, UP);
            let pts = vec![c + Vec2::new(-d * 0.28, 0.0), c + Vec2::new(-d * 0.06, d * 0.22), c + Vec2::new(d * 0.3, -d * 0.24)];
            p.add(Shape::line(pts, Stroke::new(2.2 * s, Color32::WHITE)));
        }
        CheckState::Cross => {
            p.rect_filled(bx, 3.0 * s, DOWN);
            let k = d * 0.24;
            let st = Stroke::new(2.2 * s, Color32::WHITE);
            p.line_segment([c + Vec2::new(-k, -k), c + Vec2::new(k, k)], st);
            p.line_segment([c + Vec2::new(-k, k), c + Vec2::new(k, -k)], st);
        }
        CheckState::Open => {
            p.rect_stroke(bx, 3.0 * s, Stroke::new(1.4 * s, accent), StrokeKind::Inside);
        }
    }
    text(p, Pos2::new(r.left() + r.height() + 6.0 * s, c.y), Align2::LEFT_CENTER, &cm.label, 14.0 * s, INK);
}

fn draw_process(p: &Canvas, r: Rect, s: f32, pr: &Process, style: &Style) {
    let n = pr.steps.len().max(1) as f32;
    let gap = 4.0 * s;
    let d = (r.height() * 0.4).min(r.width() / n * 0.3);
    // chevrons nest into each other: each starts where the previous tip begins
    let w = (r.width() + (n - 1.0) * (d - gap)) / n;
    let (t, b, m) = (r.top(), r.bottom(), r.center().y);
    for (i, step) in pr.steps.iter().enumerate() {
        let x = r.left() + i as f32 * (w - d + gap);
        let notch = if i == 0 { 0.0 } else { d };
        let active = pr.highlight == Some(i);
        let col = if active { rgb(style.accent) } else { rgb(style.colors[3]) };
        let tc = if active { Color32::WHITE } else { INK };
        let label_pos = Pos2::new(x + (notch + w - d) / 2.0 + d * 0.25, m);
        let adj = ((d / r.height().min(w).max(0.01)) * 100000.0).round() as i64;
        if p.preset(Rect::from_x_y_ranges(x..=x + w, t..=b), if i == 0 { "homePlate" } else { "chevron" }, Some(adj), col) {
            text(p, label_pos, Align2::CENTER_CENTER, step, 14.0 * s, tc);
            continue;
        }
        let top = vec![Pos2::new(x, t), Pos2::new(x + w - d, t), Pos2::new(x + w, m), Pos2::new(x + notch, m)];
        let bottom = vec![Pos2::new(x + notch, m), Pos2::new(x + w, m), Pos2::new(x + w - d, b), Pos2::new(x, b)];
        p.add(Shape::convex_polygon(top, col, Stroke::NONE));
        p.add(Shape::convex_polygon(bottom, col, Stroke::NONE));
        text(p, label_pos, Align2::CENTER_CENTER, step, 14.0 * s, tc);
    }
}

fn draw_agenda(p: &Canvas, r: Rect, s: f32, a: &Agenda, style: &Style) {
    let n = a.items.len().max(1);
    let row = (r.height() / n as f32).min(56.0 * s);
    for (i, item) in a.items.iter().enumerate() {
        let y = r.top() + row * i as f32;
        let active = i == a.current;
        let num = Rect::from_min_size(Pos2::new(r.left(), y + row * 0.15), Vec2::splat(row * 0.7));
        let col = if active { rgb(style.accent) } else { rgb(style.colors[3]) };
        p.rect_filled(num, 3.0 * s, col);
        text(p, num.center(), Align2::CENTER_CENTER, &(i + 1).to_string(), 15.0 * s, if active { Color32::WHITE } else { INK });
        text(
            p,
            Pos2::new(num.right() + 12.0 * s, num.center().y),
            Align2::LEFT_CENTER,
            item,
            17.0 * s,
            if active { INK } else { MUTED },
        );
        if active {
            p.line_segment(
                [Pos2::new(num.right() + 12.0 * s, num.bottom() + 1.0 * s), Pos2::new(r.right(), num.bottom() + 1.0 * s)],
                Stroke::new(1.0 * s, rgb(style.accent)),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Small icons for the ribbon

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Chart(ChartKind),
    Gantt,
    Text,
    Harvey,
    Check,
    Process,
    Agenda,
    Shape(ShapeKind),
    Table,
    Elements,
    BuildDown,
}

pub fn draw_icon(p: &Canvas, r: Rect, icon: Icon, col: Color32) {
    let light = col.gamma_multiply(0.45);
    let b = r.shrink(2.0);
    let st = Stroke::new(1.4, col);
    match icon {
        Icon::Chart(k) => match k {
            ChartKind::Stacked | ChartKind::Percent => {
                let hs: [f32; 3] = if k == ChartKind::Percent { [1.0, 1.0, 1.0] } else { [0.5, 0.75, 1.0] };
                for (i, h) in hs.iter().enumerate() {
                    let x = b.left() + b.width() * (0.08 + 0.32 * i as f32);
                    let w = b.width() * 0.24;
                    let top = b.bottom() - b.height() * h;
                    let mid = b.bottom() - b.height() * h * 0.5;
                    p.rect_filled(Rect::from_x_y_ranges(x..=x + w, mid..=b.bottom()), 0.0, col);
                    p.rect_filled(Rect::from_x_y_ranges(x..=x + w, top..=mid), 0.0, light);
                }
            }
            ChartKind::Clustered => {
                for i in 0..3 {
                    let x = b.left() + b.width() * (0.04 + 0.33 * i as f32);
                    let w = b.width() * 0.13;
                    let h1 = [0.5, 0.8, 0.65][i];
                    let h2 = [0.35, 0.6, 1.0][i];
                    p.rect_filled(Rect::from_x_y_ranges(x..=x + w, b.bottom() - b.height() * h1..=b.bottom()), 0.0, col);
                    p.rect_filled(Rect::from_x_y_ranges(x + w..=x + 2.0 * w, b.bottom() - b.height() * h2..=b.bottom()), 0.0, light);
                }
            }
            ChartKind::Bar => {
                for (i, w) in [0.9f32, 0.65, 0.4].iter().enumerate() {
                    let y = b.top() + b.height() * (0.08 + 0.32 * i as f32);
                    p.rect_filled(Rect::from_x_y_ranges(b.left()..=b.left() + b.width() * w, y..=y + b.height() * 0.22), 0.0, col);
                }
            }
            ChartKind::Line => {
                let pts = vec![
                    Pos2::new(b.left(), b.bottom() - b.height() * 0.2),
                    Pos2::new(b.left() + b.width() * 0.35, b.bottom() - b.height() * 0.55),
                    Pos2::new(b.left() + b.width() * 0.65, b.bottom() - b.height() * 0.45),
                    Pos2::new(b.right(), b.top()),
                ];
                p.add(Shape::line(pts, Stroke::new(2.0, col)));
            }
            ChartKind::Area => {
                let xs = [0.0, 0.33, 0.66, 1.0];
                let ys = [0.3, 0.55, 0.5, 0.9];
                for i in 0..3 {
                    let q = vec![
                        Pos2::new(b.left() + b.width() * xs[i], b.bottom() - b.height() * ys[i]),
                        Pos2::new(b.left() + b.width() * xs[i + 1], b.bottom() - b.height() * ys[i + 1]),
                        Pos2::new(b.left() + b.width() * xs[i + 1], b.bottom()),
                        Pos2::new(b.left() + b.width() * xs[i], b.bottom()),
                    ];
                    p.add(Shape::convex_polygon(q, col, Stroke::NONE));
                }
            }
            ChartKind::Waterfall => {
                let segs = [(0.0, 0.6), (0.6, 0.85), (0.7, 0.85), (0.0, 0.7)];
                for (i, (a, bb)) in segs.iter().enumerate() {
                    let x = b.left() + b.width() * (0.02 + 0.25 * i as f32);
                    let w = b.width() * 0.2;
                    let c = if i == 0 || i == 3 { col } else { light };
                    p.rect_filled(Rect::from_x_y_ranges(x..=x + w, b.bottom() - b.height() * bb..=b.bottom() - b.height() * a), 0.0, c);
                }
            }
            ChartKind::Mekko => {
                let ws = [0.45, 0.33, 0.22];
                let mut x = b.left();
                for (i, w) in ws.iter().enumerate() {
                    let ww = b.width() * w;
                    let split = b.top() + b.height() * [0.4, 0.6, 0.3][i];
                    p.rect_filled(Rect::from_x_y_ranges(x..=x + ww - 1.0, split..=b.bottom()), 0.0, col);
                    p.rect_filled(Rect::from_x_y_ranges(x..=x + ww - 1.0, b.top()..=split), 0.0, light);
                    x += ww;
                }
            }
            ChartKind::Pie | ChartKind::Doughnut => {
                let c = b.center();
                let rad = b.height().min(b.width()) / 2.0;
                let inner = if k == ChartKind::Doughnut { rad * 0.5 } else { 0.0 };
                sector(c, inner, rad, -PI / 2.0, PI * 0.7, col, p);
                sector(c, inner, rad, PI * 0.7, PI * 1.5, light, p);
            }
            ChartKind::Combination => {
                for (i, h) in [0.45f32, 0.6, 0.8].iter().enumerate() {
                    let x = b.left() + b.width() * (0.06 + 0.33 * i as f32);
                    p.rect_filled(Rect::from_x_y_ranges(x..=x + b.width() * 0.22, b.bottom() - b.height() * h..=b.bottom()), 0.0, light);
                }
                let pts = vec![
                    Pos2::new(b.left() + b.width() * 0.17, b.bottom() - b.height() * 0.65),
                    Pos2::new(b.left() + b.width() * 0.5, b.bottom() - b.height() * 0.8),
                    Pos2::new(b.left() + b.width() * 0.83, b.top()),
                ];
                p.add(Shape::line(pts, Stroke::new(2.0, col)));
            }
            ChartKind::AreaPercent => {
                let xs = [0.0, 0.5, 1.0];
                let ys = [0.45, 0.65, 0.4];
                for i in 0..2 {
                    let x0 = b.left() + b.width() * xs[i];
                    let x1 = b.left() + b.width() * xs[i + 1];
                    p.add(Shape::convex_polygon(
                        vec![Pos2::new(x0, b.bottom() - b.height() * ys[i]), Pos2::new(x1, b.bottom() - b.height() * ys[i + 1]), Pos2::new(x1, b.bottom()), Pos2::new(x0, b.bottom())],
                        col,
                        Stroke::NONE,
                    ));
                    p.add(Shape::convex_polygon(
                        vec![Pos2::new(x0, b.top()), Pos2::new(x1, b.top()), Pos2::new(x1, b.bottom() - b.height() * ys[i + 1]), Pos2::new(x0, b.bottom() - b.height() * ys[i])],
                        light,
                        Stroke::NONE,
                    ));
                }
            }
            ChartKind::Scatter | ChartKind::Bubble => {
                let pts = [(0.15, 0.7, 2.0, 0.6), (0.4, 0.4, 2.0, 1.0), (0.65, 0.55, 2.0, 0.5), (0.85, 0.2, 2.0, 0.8)];
                for (x, y, r, z) in pts {
                    let c = Pos2::new(b.left() + b.width() * x, b.top() + b.height() * y);
                    let rr = if k == ChartKind::Bubble { 2.0 + 5.0 * z } else { r + 0.5 };
                    p.circle_filled(c, rr, if k == ChartKind::Bubble { light } else { col });
                    if k == ChartKind::Bubble {
                        p.circle_stroke(c, rr, Stroke::new(1.0, col));
                    }
                }
            }
        },
        Icon::BuildDown => {
            let segs = [(0.0, 1.0), (0.65, 1.0), (0.4, 0.65), (0.0, 0.4)];
            for (i, (a, bb)) in segs.iter().enumerate() {
                let x = b.left() + b.width() * (0.02 + 0.25 * i as f32);
                let c = if i == 0 || i == 3 { col } else { light };
                p.rect_filled(Rect::from_x_y_ranges(x..=x + b.width() * 0.2, b.bottom() - b.height() * bb..=b.bottom() - b.height() * a), 0.0, c);
            }
        }
        Icon::Shape(k) => {
            let bx = Rect::from_center_size(b.center(), Vec2::new(b.width(), b.height() * 0.7));
            let (t, bt, m, l, r2) = (bx.top(), bx.bottom(), bx.center().y, bx.left(), bx.right());
            let d = bx.height() * 0.5;
            match k {
                ShapeKind::Rounded => p.rect_filled(bx, 5.0, col),
                ShapeKind::Pentagon => {
                    p.rect_filled(Rect::from_x_y_ranges(l..=r2 - d, t..=bt), 0.0, col);
                    p.add(Shape::convex_polygon(vec![Pos2::new(r2 - d, t), Pos2::new(r2, m), Pos2::new(r2 - d, bt)], col, Stroke::NONE));
                }
                ShapeKind::Chevron => {
                    p.add(Shape::convex_polygon(vec![Pos2::new(l, t), Pos2::new(r2 - d, t), Pos2::new(r2, m), Pos2::new(l + d, m)], col, Stroke::NONE));
                    p.add(Shape::convex_polygon(vec![Pos2::new(l + d, m), Pos2::new(r2, m), Pos2::new(r2 - d, bt), Pos2::new(l, bt)], col, Stroke::NONE));
                }
            }
        }
        Icon::Table => {
            let rh = b.height() / 4.0;
            p.rect_filled(Rect::from_min_size(b.min, Vec2::new(b.width(), rh)), 0.0, col);
            for i in 1..4 {
                let y = b.top() + rh * (i as f32 + 1.0);
                p.line_segment([Pos2::new(b.left(), y), Pos2::new(b.right(), y)], Stroke::new(1.0, light));
            }
            p.line_segment([Pos2::new(b.left() + b.width() * 0.4, b.top()), Pos2::new(b.left() + b.width() * 0.4, b.bottom())], Stroke::new(1.0, light));
        }
        Icon::Elements => {
            for i in 0..3 {
                for j in 0..3 {
                    let c = Pos2::new(b.left() + b.width() * (0.18 + 0.32 * i as f32), b.top() + b.height() * (0.18 + 0.32 * j as f32));
                    p.rect_filled(Rect::from_center_size(c, Vec2::splat(b.height() * 0.24)), 1.5, if (i + j) % 2 == 0 { col } else { light });
                }
            }
        }
        Icon::Gantt => {
            for i in 0..3 {
                let y = b.top() + b.height() * (0.1 + 0.32 * i as f32);
                let x0 = b.left() + b.width() * [0.0, 0.25, 0.55][i];
                let x1 = b.left() + b.width() * [0.45, 0.75, 1.0][i];
                p.rect_filled(Rect::from_x_y_ranges(x0..=x1, y..=y + b.height() * 0.2), 1.5, col);
            }
        }
        Icon::Text => {
            p.text_at(b.center(), Align2::CENTER_CENTER, "T", b.height() * 1.1, col);
        }
        Icon::Harvey => {
            let c = b.center();
            let rad = b.height() / 2.0;
            sector(c, 0.0, rad, -PI / 2.0, PI * 1.0, col, p);
            p.circle_stroke(c, rad, st);
        }
        Icon::Check => {
            let bx = Rect::from_center_size(b.center(), Vec2::splat(b.height()));
            p.rect_stroke(bx, 2.0, st, StrokeKind::Inside);
            let c = bx.center();
            let d = bx.height();
            p.add(Shape::line(
                vec![c + Vec2::new(-d * 0.28, 0.0), c + Vec2::new(-d * 0.06, d * 0.22), c + Vec2::new(d * 0.3, -d * 0.24)],
                Stroke::new(2.0, col),
            ));
        }
        Icon::Process => {
            for i in 0..3 {
                let x = b.left() + b.width() * 0.33 * i as f32;
                let w = b.width() * 0.3;
                let (t, bt, m) = (b.top() + b.height() * 0.25, b.bottom() - b.height() * 0.25, b.center().y);
                p.add(Shape::convex_polygon(vec![Pos2::new(x, t), Pos2::new(x + w * 0.7, t), Pos2::new(x + w, m), Pos2::new(x + w * 0.7, bt), Pos2::new(x, bt)], if i == 1 { col } else { light }, Stroke::NONE));
            }
        }
        Icon::Agenda => {
            for i in 0..3 {
                let y = b.top() + b.height() * (0.08 + 0.32 * i as f32);
                let sq = b.height() * 0.22;
                p.rect_filled(Rect::from_min_size(Pos2::new(b.left(), y), Vec2::splat(sq)), 1.0, if i == 1 { col } else { light });
                p.rect_filled(Rect::from_min_size(Pos2::new(b.left() + sq * 1.5, y + sq * 0.3), Vec2::new(b.width() - sq * 1.6, sq * 0.4)), 0.0, light);
            }
        }
    }
}
