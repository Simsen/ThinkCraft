//! The ThinkCraft window: ribbon, slide list, slide canvas, properties and datasheet.

use crate::canvas::Canvas;
use crate::draw::{self, Icon, Xf};
use crate::model::*;
use crate::office;
use eframe::egui::{
    self, Align2, Color32, ColorImage, CursorIcon, FontId, Key, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What Copy puts on the clipboard: one Gantt bar or milestone, or whole elements.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
enum Clip {
    GanttItem { row: usize, bar: Bar },
    Elements(Vec<Element>),
}

/// The round menu around a Gantt chart's corner (right-click the chart).
struct Radial {
    id: u64,
    slide: usize,
    /// The empty timeline spot that was right-clicked: (chart id, activity, time).
    cell: Option<(u64, usize, f32)>,
    opened: u64,
}

/// The buttons of the round menu, clockwise from the top.
#[derive(Clone, Copy, PartialEq, Debug)]
enum RadialCmd {
    Delete,
    SameWidth,
    SameHeight,
    Activity,
    Responsible,
    Remark,
    Fit,
    Calendar,
    AddBar,
    AddMilestone,
    PasteHere,
    Excel,
    Datasheet,
    Duplicate,
}

const RADIAL: [RadialCmd; 14] = [
    RadialCmd::Delete,
    RadialCmd::SameWidth,
    RadialCmd::SameHeight,
    RadialCmd::Activity,
    RadialCmd::Responsible,
    RadialCmd::Remark,
    RadialCmd::Fit,
    RadialCmd::Calendar,
    RadialCmd::AddBar,
    RadialCmd::AddMilestone,
    RadialCmd::PasteHere,
    RadialCmd::Excel,
    RadialCmd::Datasheet,
    RadialCmd::Duplicate,
];

/// The calendar window for a Gantt chart: which chart and which month is shown.
struct CalendarDlg {
    id: u64,
    /// The month picked at the top, scrolled into view when `scroll` is set.
    year: i32,
    month: u32,
    scroll: bool,
    /// The first day clicked, waiting for the last day (days since 1970).
    pending: Option<i64>,
    /// The exact first and last day picked last time.
    chosen: Option<(i64, i64)>,
    /// The day under the pointer in the last frame.
    hover: Option<i64>,
    /// Just opened: place the window in the middle of the screen.
    fresh: bool,
}

/// First line of ThinkCraft's clipboard text, so a paste can tell it from other text.
const CLIP_MARK: &str = "ThinkCraft clipboard v1\n";

impl Clip {
    fn to_text(&self) -> String {
        format!("{CLIP_MARK}{}", serde_json::to_string(self).unwrap_or_default())
    }
    fn from_text(text: &str) -> Option<Clip> {
        serde_json::from_str(text.strip_prefix(CLIP_MARK)?).ok()
    }
}

/// Puts a copy of `bar` into a Gantt chart and returns its (row, index).
/// It goes into `row` (an empty chart gets a first activity), keeps its own dates unless
/// `at` gives a new start, and lands last in the row so it is drawn on top and grabbed first.
fn paste_bar(g: &mut Gantt, row: usize, mut bar: Bar, at: Option<f32>) -> (usize, usize) {
    if g.tasks.is_empty() {
        g.tasks.push(Task::new("New activity", vec![]));
    }
    let row = row.min(g.tasks.len() - 1);
    let cols = g.columns() as f32;
    let len = if bar.milestone { 0.0 } else { (bar.end - bar.start).max(0.0).min(cols) };
    let start = match at {
        Some(t) => (t / g.snap_step()).round() * g.snap_step(),
        None => bar.start,
    };
    bar.start = start.clamp(0.0, (cols - len).max(0.0));
    bar.end = bar.start + len;
    g.tasks[row].bars.push(bar);
    (row, g.tasks[row].bars.len() - 1)
}

/// Moves a bar by `dt` (in columns) and `drow` activities; returns its new (row, index).
fn nudge_bar(g: &mut Gantt, row: usize, bi: usize, dt: f32, drow: i32) -> (usize, usize) {
    let cols = g.columns() as f32;
    let b = &mut g.tasks[row].bars[bi];
    let len = (b.end - b.start).max(0.0);
    b.start = (b.start + dt).clamp(0.0, (cols - len).max(0.0));
    b.end = if b.milestone { b.start } else { b.start + len };
    let target = (row as i32 + drow).clamp(0, g.tasks.len() as i32 - 1) as usize;
    if target == row {
        return (row, bi);
    }
    let moved = g.tasks[row].bars.remove(bi);
    g.tasks[target].bars.push(moved);
    (target, g.tasks[target].bars.len() - 1)
}

const ACCENT: Color32 = Color32::from_rgb(196, 84, 36);
const PANEL_BG: Color32 = Color32::from_rgb(246, 246, 248);
const CANVAS_BG: Color32 = Color32::from_rgb(214, 217, 223);
const GUIDE: Color32 = Color32::from_rgb(230, 40, 140);

#[derive(Clone, Copy, PartialEq)]
enum AlignOp {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
    SameWidth,
    SameHeight,
    SpreadH,
    SpreadV,
}

#[derive(Clone, Copy, PartialEq)]
enum Insert {
    Chart(ChartKind),
    Gantt,
    Text,
    Harvey,
    Check,
    Process,
    Agenda,
    Shape(ShapeKind),
    Table,
    BuildDown,
    Timeline,
}

/// Everything in the Elements pop-up, in sections.
const ELEMENT_SECTIONS: [(&str, &[(Icon, &str, Insert)]); 4] = [
    (
        "Text and shapes",
        &[
            (Icon::Text, "Text box", Insert::Text),
            (Icon::Shape(ShapeKind::Pentagon), "Pentagon", Insert::Shape(ShapeKind::Pentagon)),
            (Icon::Shape(ShapeKind::Chevron), "Chevron", Insert::Shape(ShapeKind::Chevron)),
            (Icon::Shape(ShapeKind::Rounded), "Rounded box", Insert::Shape(ShapeKind::Rounded)),
            (Icon::Process, "Process", Insert::Process),
        ],
    ),
    (
        "Status and agenda",
        &[
            (Icon::Harvey, "Harvey ball", Insert::Harvey),
            (Icon::Check, "Checkbox", Insert::Check),
            (Icon::Agenda, "Agenda", Insert::Agenda),
            (Icon::Table, "Table", Insert::Table),
        ],
    ),
    (
        "Charts",
        &[
            (Icon::Chart(ChartKind::Stacked), "Stacked", Insert::Chart(ChartKind::Stacked)),
            (Icon::Chart(ChartKind::Percent), "100%", Insert::Chart(ChartKind::Percent)),
            (Icon::Chart(ChartKind::Clustered), "Clustered", Insert::Chart(ChartKind::Clustered)),
            (Icon::Chart(ChartKind::Bar), "Bar", Insert::Chart(ChartKind::Bar)),
            (Icon::Chart(ChartKind::Waterfall), "Build-up", Insert::Chart(ChartKind::Waterfall)),
            (Icon::BuildDown, "Build-down", Insert::BuildDown),
            (Icon::Chart(ChartKind::Mekko), "Mekko", Insert::Chart(ChartKind::Mekko)),
            (Icon::Chart(ChartKind::Combination), "Combination", Insert::Chart(ChartKind::Combination)),
            (Icon::Chart(ChartKind::Line), "Line", Insert::Chart(ChartKind::Line)),
            (Icon::Chart(ChartKind::Area), "Area", Insert::Chart(ChartKind::Area)),
            (Icon::Chart(ChartKind::AreaPercent), "100% Area", Insert::Chart(ChartKind::AreaPercent)),
            (Icon::Chart(ChartKind::Pie), "Pie", Insert::Chart(ChartKind::Pie)),
            (Icon::Chart(ChartKind::Doughnut), "Doughnut", Insert::Chart(ChartKind::Doughnut)),
            (Icon::Chart(ChartKind::Scatter), "Scatter", Insert::Chart(ChartKind::Scatter)),
            (Icon::Chart(ChartKind::Bubble), "Bubble", Insert::Chart(ChartKind::Bubble)),
        ],
    ),
    (
        "Project",
        &[(Icon::Gantt, "Timeline", Insert::Timeline), (Icon::Gantt, "Gantt", Insert::Gantt)],
    ),
];

enum Action {
    SaveSlide,
    ToggleDecimal,
    New,
    Sample,
    Open,
    Save,
    SaveAs,
    ExportPptx,
    OpenPowerPoint,
    ExportXlsx,
    OpenExcel,
    ExportPng,
    CopyPicture,
    CopyData,
    Insert(Insert),
    Delete,
    Duplicate,
    Undo,
    Redo,
    Align(AlignOp),
    PasteTable,
    ImportTable,
    SlideAdd,
    SlideDuplicate,
    SlideDelete,
    SlideMove(i32),
    SelectAll,
    Front,
    Back,
    /// Copy the selected Gantt bar, or the selected elements.
    Copy,
    Cut,
    /// Paste what was copied. `text` is the clipboard text from Cmd+V (None: use the last copy);
    /// `at` is a spot in a Gantt chart (chart id, activity, time) to paste a bar into.
    Paste { text: Option<String>, at: Option<(u64, usize, f32)> },
    /// Delete or duplicate the selected Gantt bar if there is one, otherwise the selected elements.
    ItemDelete,
    ItemDuplicate,
    /// Add a bar or milestone to a Gantt chart at (chart id, activity, time).
    GanttAdd { id: u64, row: usize, t: f32, milestone: bool },
    /// Show or hide a Gantt label column: 0 activity, 1 responsible, 2 remark.
    GanttColumn { id: u64, which: u8 },
    /// Fit a Gantt chart's timeline to its bars.
    GanttFit { id: u64 },
    /// Open the calendar window for a Gantt chart.
    GanttCalendar { id: u64 },
}

enum Drag {
    None,
    Move { start: Pos2, orig: Vec<(u64, Rect)> },
    Resize { id: u64, handle: usize, orig: Rect, start: Pos2 },
    Marquee { start: Pos2 },
    /// Dragging a Gantt bar (or one of its ends) inside the chart.
    GanttBar { id: u64, row: usize, bar: usize, part: BarPart, orig: (f32, f32), grab_t: f32 },
    /// Dragging the Gantt date line.
    GanttToday { id: u64 },
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum BarPart {
    Move,
    Start,
    End,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum GanttHit {
    /// (row, bar in that row, which part)
    Bar(usize, usize, BarPart),
    Today,
}

/// Every bar of a Gantt chart with its row and index, topmost (last drawn) first.
fn bars_topmost(g: &Gantt) -> Vec<(usize, usize, &Bar)> {
    let mut v: Vec<(usize, usize, &Bar)> = g.tasks.iter().enumerate().flat_map(|(r, t)| t.bars.iter().enumerate().map(move |(b, bar)| (r, b, bar))).collect();
    v.reverse();
    v
}

/// What part of a Gantt chart is under the pointer: a bar end, a bar, or the date line.
fn gantt_hit(el: &Element, xf: &Xf, p: Pos2) -> Option<GanttHit> {
    let Kind::Gantt(g) = &el.kind else { return None };
    let r = xf.el_rect(el);
    if !r.expand(8.0).contains(p) {
        return None;
    }
    let geo = draw::GanttGeo::new(r, xf.scale, g);
    let edge = 6.0;
    let bars = bars_topmost(g);
    // bar ends first, so short bars can still be stretched
    let bars: Vec<_> = bars.into_iter().filter(|(_, _, t)| geo.visible(t)).collect();
    for (row, b, t) in &bars {
        if t.milestone {
            continue;
        }
        let rect = geo.bar_rect(t, *row).expand2(Vec2::new(0.0, 3.0));
        if p.y >= rect.top() && p.y <= rect.bottom() {
            if (p.x - rect.left()).abs() <= edge {
                return Some(GanttHit::Bar(*row, *b, BarPart::Start));
            }
            if (p.x - rect.right()).abs() <= edge {
                return Some(GanttHit::Bar(*row, *b, BarPart::End));
            }
        }
    }
    // bars win where the date line crosses them; the line is grabbed anywhere else along it
    for (row, b, t) in &bars {
        if geo.bar_rect(t, *row).expand(3.0).contains(p) {
            return Some(GanttHit::Bar(*row, *b, BarPart::Move));
        }
    }
    if let Some((t, _)) = g.today_line() {
        let x = geo.x(t);
        if (p.x - x).abs() <= 5.0 && p.y >= geo.grid.top() - 6.0 && p.y <= geo.bottom + 30.0 * xf.scale {
            return Some(GanttHit::Today);
        }
    }
    None
}

enum ShotUse {
    Exit(PathBuf),
    Png(PathBuf, Rect),
    Clipboard(Rect),
}

pub struct Cli {
    pub screenshot: Option<PathBuf>,
    pub open: Option<PathBuf>,
    pub export_pptx: Option<PathBuf>,
    pub export_xlsx: Option<PathBuf>,
}

pub struct ThinkCraft {
    batch: Vec<(bool, PathBuf)>,
    deck: Deck,
    path: Option<PathBuf>,
    current: usize,
    selected: Vec<u64>,
    drag: Drag,
    guides: Vec<(bool, f32)>,
    marquee: Option<Rect>,
    undo: Vec<Deck>,
    redo: Vec<Deck>,
    committed: Deck,
    saved: Deck,
    zoom: f32,
    show_datasheet: bool,
    paste: Option<(String, String)>,
    status: String,
    screenshot: Option<PathBuf>,
    frame: u64,
    shot: Arc<Mutex<Option<Arc<ColorImage>>>>,
    shot_use: Option<ShotUse>,
    hide_overlay: bool,
    slide_rect: Rect,
    actions: Vec<Action>,
    about: bool,
    title_shown: String,
    elements_open: bool,
    elements_pos: Pos2,
    elements_opened_frame: u64,
    drag_label: Option<String>,
    /// The single Gantt bar or milestone picked inside the selected chart: (chart id, row, bar).
    gantt_sel: Option<(u64, usize, usize)>,
    /// The last thing copied inside ThinkCraft.
    clip: Option<Clip>,
    /// Empty Gantt spot under the last right-click: (chart id, activity, time).
    menu_cell: Option<(u64, usize, f32)>,
    radial: Option<Radial>,
    calendar_dlg: Option<CalendarDlg>,
}

impl ThinkCraft {
    pub fn new(cc: &eframe::CreationContext<'_>, cli: Cli) -> Self {
        let Cli { screenshot, open, export_pptx, export_xlsx } = cli;
        let batch: Vec<(bool, PathBuf)> = export_pptx.map(|p| (true, p)).into_iter().chain(export_xlsx.map(|p| (false, p))).collect();
        cc.egui_ctx.set_theme(egui::Theme::Light);
        cc.egui_ctx.style_mut_of(egui::Theme::Light, |s| {
            s.spacing.item_spacing = Vec2::new(6.0, 5.0);
            s.spacing.button_padding = Vec2::new(8.0, 3.0);
            s.visuals.selection.bg_fill = Color32::from_rgb(250, 214, 196);
            s.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
            s.visuals.hyperlink_color = ACCENT;
            s.visuals.panel_fill = PANEL_BG;
        });

        let mut deck = Deck::sample();
        let mut path = None;
        let mut status = "Welcome to ThinkCraft. Pick a chart in the ribbon to add it to the slide.".to_string();
        if screenshot.is_none() {
            if let Some(p) = open {
                match load_deck(&p) {
                    Ok(d) => {
                        deck = d;
                        path = Some(p);
                    }
                    Err(e) => status = e,
                }
            } else if let (Some(storage), true) = (cc.storage, batch.is_empty()) {
                if let Some(d) = eframe::get_value::<Deck>(storage, "deck") {
                    if !d.slides.is_empty() {
                        deck = d;
                    }
                }
                path = eframe::get_value::<Option<PathBuf>>(storage, "path").flatten();
            }
        }
        // Start with the first chart selected so the datasheet is visible.
        let current = std::env::var("THINKCRAFT_SLIDE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .map(|n| n.saturating_sub(1))
            .unwrap_or(0)
            .min(deck.slides.len() - 1);
        let first_chart = deck.slides[current].elements.iter().find(|e| matches!(e.kind, Kind::Chart(_) | Kind::Gantt(_))).map(|e| e.id);
        ThinkCraft {
            batch,
            committed: deck.clone(),
            saved: deck.clone(),
            deck,
            path,
            current,
            selected: first_chart.into_iter().collect(),
            drag: Drag::None,
            guides: vec![],
            marquee: None,
            undo: vec![],
            redo: vec![],
            zoom: 1.0,
            show_datasheet: true,
            paste: None,
            status,
            screenshot,
            frame: 0,
            shot: Arc::new(Mutex::new(None)),
            shot_use: None,
            hide_overlay: false,
            slide_rect: Rect::NOTHING,
            actions: vec![],
            about: false,
            title_shown: String::new(),
            elements_open: std::env::var("THINKCRAFT_ELEMENTS").is_ok(),
            elements_pos: Pos2::new(62.0, 106.0),
            elements_opened_frame: 0,
            drag_label: None,
            gantt_sel: None,
            clip: None,
            menu_cell: None,
            radial: None,
            calendar_dlg: None,
        }
    }

    /// The selected Gantt bar, if it still exists: (chart id, row, bar).
    fn gantt_item(&self) -> Option<(u64, usize, usize)> {
        let (id, row, bi) = self.gantt_sel?;
        if self.selected.as_slice() != [id] {
            return None;
        }
        let el = self.deck.slides[self.current].elements.iter().find(|e| e.id == id)?;
        let Kind::Gantt(g) = &el.kind else { return None };
        (row < g.tasks.len() && bi < g.tasks[row].bars.len()).then_some((id, row, bi))
    }

    fn gantt_mut(&mut self, id: u64) -> Option<&mut Gantt> {
        let cur = self.current;
        match &mut self.deck.slides[cur].elements.iter_mut().find(|e| e.id == id)?.kind {
            Kind::Gantt(g) => Some(g),
            _ => None,
        }
    }

    /// "Design: bar" or "Go-live: milestone", for status messages.
    fn item_name(&self, (id, row, bi): (u64, usize, usize)) -> String {
        let el = self.deck.slides[self.current].elements.iter().find(|e| e.id == id);
        match el.map(|e| &e.kind) {
            Some(Kind::Gantt(g)) => {
                let what = if g.tasks[row].bars[bi].milestone { "milestone" } else { "bar" };
                format!("the {what} in {}", g.tasks[row].label)
            }
            _ => "the item".into(),
        }
    }

    /// Copies the selected Gantt bar, or else the selected elements. Returns false if nothing was selected.
    fn copy_selection(&mut self, ctx: &egui::Context) -> bool {
        let clip = if let Some((id, row, bi)) = self.gantt_item() {
            let Some(g) = self.gantt_mut(id) else { return false };
            let bar = g.tasks[row].bars[bi].clone();
            self.status = keys(&format!("Copied {}. Press Cmd+V to paste it, then drag it where it should go.", self.item_name((id, row, bi))));
            Clip::GanttItem { row, bar }
        } else if !self.selected.is_empty() {
            let sel = self.selected.clone();
            let els: Vec<Element> = self.deck.slides[self.current].elements.iter().filter(|e| sel.contains(&e.id)).cloned().collect();
            self.status = keys(&format!("Copied {} element{}. Press Cmd+V to paste.", els.len(), if els.len() == 1 { "" } else { "s" }));
            Clip::Elements(els)
        } else {
            self.status = "Select something to copy: click a bar in a Gantt chart, or an element.".into();
            return false;
        };
        ctx.copy_text(clip.to_text());
        self.clip = Some(clip);
        true
    }

    fn paste_clip(&mut self, clip: Clip, at: Option<(u64, usize, f32)>) {
        match clip {
            Clip::GanttItem { row, bar } => {
                // into the spot right-clicked, else the selected bar's activity, else the copied bar's own activity
                let slide = &self.deck.slides[self.current];
                let chart = at.map(|a| a.0).or(self.gantt_item().map(|s| s.0)).or_else(|| {
                    let sel: Vec<u64> = slide.elements.iter().filter(|e| self.selected.contains(&e.id) && matches!(e.kind, Kind::Gantt(_))).map(|e| e.id).collect();
                    match sel.as_slice() {
                        [id] => Some(*id),
                        _ => {
                            let all: Vec<u64> = slide.elements.iter().filter(|e| matches!(e.kind, Kind::Gantt(_))).map(|e| e.id).collect();
                            (all.len() == 1).then(|| all[0])
                        }
                    }
                });
                let Some(id) = chart else {
                    self.status = "Click a Gantt chart first, then paste the bar into it.".into();
                    return;
                };
                let target_row = at.map(|a| a.1).or(self.gantt_item().map(|s| s.1)).unwrap_or(row);
                let Some(g) = self.gantt_mut(id) else { return };
                let (r, b) = paste_bar(g, target_row, bar, at.map(|a| a.2));
                self.selected = vec![id];
                self.gantt_sel = Some((id, r, b));
                let name = self.item_name((id, r, b));
                self.status = if at.is_some() {
                    format!("Pasted {name}. Drag it or its ends to adjust.")
                } else {
                    format!("Pasted {name}, on top of where it was. Drag it, or use the arrow keys, to move it.")
                };
            }
            Clip::Elements(els) => {
                let cur = self.current;
                let taken = |x: f32, y: f32, slide: &Slide| slide.elements.iter().any(|e| (e.x - x).abs() < 0.5 && (e.y - y).abs() < 0.5);
                let mut off = 0.0;
                while off < 400.0 && els.iter().any(|e| taken(e.x + off, e.y + off, &self.deck.slides[cur])) {
                    off += 16.0;
                }
                self.selected.clear();
                self.gantt_sel = None;
                let n = els.len();
                for mut e in els {
                    e.id = self.deck.new_id();
                    e.x += off;
                    e.y += off;
                    self.selected.push(e.id);
                    self.deck.slides[cur].elements.push(e);
                }
                self.status = format!("Pasted {n} element{}.", if n == 1 { "" } else { "s" });
            }
        }
    }

    fn style(&self) -> &'static Style {
        &STYLES[self.deck.style % STYLES.len()]
    }

    fn slide(&mut self) -> &mut Slide {
        self.current = self.current.min(self.deck.slides.len().saturating_sub(1));
        &mut self.deck.slides[self.current]
    }

    fn single(&self) -> Option<usize> {
        if self.selected.len() != 1 {
            return None;
        }
        let slide = self.deck.slides.get(self.current)?;
        slide.elements.iter().position(|e| e.id == self.selected[0])
    }

    fn request_shot(&mut self, ctx: &egui::Context, using: ShotUse) {
        self.hide_overlay = true;
        self.shot_use = Some(using);
        let slot = self.shot.clone();
        let c2 = ctx.clone();
        ctx.request_screenshot(move |img| {
            *slot.lock().unwrap() = Some(img);
            c2.request_repaint();
        });
        ctx.request_repaint();
    }

    fn handle_shot(&mut self, ctx: &egui::Context) {
        let img = self.shot.lock().unwrap().take();
        let Some(img) = img else { return };
        self.hide_overlay = false;
        let ppp = ctx.pixels_per_point();
        let crop = |r: Rect| -> image::RgbaImage {
            let x0 = ((r.left() * ppp).round().max(0.0) as usize).min(img.size[0]);
            let y0 = ((r.top() * ppp).round().max(0.0) as usize).min(img.size[1]);
            let x1 = ((r.right() * ppp).round() as usize).min(img.size[0]).max(x0 + 1);
            let y1 = ((r.bottom() * ppp).round() as usize).min(img.size[1]).max(y0 + 1);
            let mut out = image::RgbaImage::new((x1 - x0) as u32, (y1 - y0) as u32);
            for y in y0..y1 {
                for x in x0..x1 {
                    let c = img.pixels.get(y * img.size[0] + x).copied().unwrap_or(Color32::WHITE);
                    out.put_pixel((x - x0) as u32, (y - y0) as u32, image::Rgba([c.r(), c.g(), c.b(), 255]));
                }
            }
            out
        };
        match self.shot_use.take() {
            Some(ShotUse::Exit(path)) => {
                let full = crop(Rect::from_min_size(Pos2::ZERO, Vec2::new(img.size[0] as f32, img.size[1] as f32) / ppp));
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Err(e) = full.save(&path) {
                    eprintln!("Couldn't save the screenshot: {e}");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Some(ShotUse::Png(path, r)) => match crop(r).save(&path) {
                Ok(_) => self.status = format!("Saved the slide as a picture: {}", file_name(&path)),
                Err(e) => self.status = format!("Couldn't save the picture: {e}"),
            },
            Some(ShotUse::Clipboard(r)) => {
                let im = crop(r);
                let res = arboard::Clipboard::new().and_then(|mut cb| {
                    cb.set_image(arboard::ImageData {
                        width: im.width() as usize,
                        height: im.height() as usize,
                        bytes: std::borrow::Cow::Owned(im.into_raw()),
                    })
                });
                self.status = match res {
                    Ok(_) => "Slide copied as a picture. Paste it into PowerPoint, Excel or any other app.".into(),
                    Err(e) => format!("Couldn't copy to the clipboard: {e}"),
                };
            }
            None => {}
        }
    }

    // -----------------------------------------------------------------------
    // Actions

    fn run_actions(&mut self, ui: &mut egui::Ui) {
        let actions = std::mem::take(&mut self.actions);
        for a in actions {
            self.run(a, ui);
        }
    }

    fn run(&mut self, a: Action, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        match a {
            Action::New => {
                self.deck = Deck::blank();
                self.path = None;
                self.current = 0;
                self.selected.clear();
                self.saved = self.deck.clone();
                self.status = "New deck.".into();
            }
            Action::Sample => {
                self.deck = Deck::sample();
                self.path = None;
                self.current = 0;
                self.selected.clear();
                self.status = "Opened the sample deck.".into();
            }
            Action::Open => {
                if let Some(p) = rfd::FileDialog::new().add_filter("ThinkCraft deck", &["tcraft", "json"]).pick_file() {
                    match load_deck(&p) {
                        Ok(d) => {
                            self.deck = d;
                            self.saved = self.deck.clone();
                            self.status = format!("Opened {}", file_name(&p));
                            self.path = Some(p);
                            self.current = 0;
                            self.selected.clear();
                        }
                        Err(e) => self.status = e,
                    }
                }
            }
            Action::Save | Action::SaveAs => {
                let target = match (&self.path, matches!(a, Action::SaveAs)) {
                    (Some(p), false) => Some(p.clone()),
                    _ => rfd::FileDialog::new()
                        .add_filter("ThinkCraft deck", &["tcraft"])
                        .set_file_name(format!("{}.tcraft", safe_name(&self.deck.name)))
                        .save_file(),
                };
                if let Some(p) = target {
                    match serde_json::to_string_pretty(&self.deck).map_err(|e| e.to_string()).and_then(|s| std::fs::write(&p, s).map_err(|e| e.to_string())) {
                        Ok(_) => {
                            self.saved = self.deck.clone();
                            self.status = format!("Saved {}", file_name(&p));
                            self.path = Some(p);
                        }
                        Err(e) => self.status = format!("Couldn't save: {e}"),
                    }
                }
            }
            Action::ExportPptx => {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("PowerPoint presentation", &["pptx"])
                    .set_file_name(format!("{}.pptx", safe_name(&self.deck.name)))
                    .save_file()
                {
                    let all: Vec<usize> = (0..self.deck.slides.len()).collect();
                    self.status = match office::write_pptx(&p, &self.deck, &all, ui.painter()) {
                        Ok(_) => format!("Exported {} slides to PowerPoint: {}", all.len(), file_name(&p)),
                        Err(e) => e,
                    };
                }
            }
            Action::SaveSlide => {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("PowerPoint presentation", &["pptx"])
                    .set_file_name(format!("{} slide {}.pptx", safe_name(&self.deck.name), self.current + 1))
                    .save_file()
                {
                    self.status = match office::write_pptx(&p, &self.deck, &[self.current], ui.painter()) {
                        Ok(_) => format!("Saved this slide as PowerPoint: {}", file_name(&p)),
                        Err(e) => e,
                    };
                }
            }
            Action::ToggleDecimal => {
                self.deck.decimal_comma = !self.deck.decimal_comma;
                self.status = if self.deck.decimal_comma { "Numbers now use a decimal comma (1.234,5)." } else { "Numbers now use a decimal point (1,234.5)." }.into();
            }
            Action::OpenPowerPoint => {
                let p = temp_path(&self.deck.name, "pptx");
                let all: Vec<usize> = (0..self.deck.slides.len()).collect();
                self.status = match office::write_pptx(&p, &self.deck, &all, ui.painter()) {
                    Ok(_) => open_with(&p, "Microsoft PowerPoint", "PowerPoint"),
                    Err(e) => e,
                };
            }
            Action::ExportXlsx | Action::OpenExcel => {
                let slides: Vec<usize> = (0..self.deck.slides.len()).collect();
                let only = self.single().and_then(|i| {
                    let e = &self.deck.slides[self.current].elements[i];
                    matches!(e.kind, Kind::Chart(_) | Kind::Gantt(_) | Kind::Table(_)).then_some(e.id)
                });
                let slides = if only.is_some() { vec![self.current] } else { slides };
                let sheets = office::deck_sheets(&self.deck, &slides, only);
                let target = if matches!(a, Action::OpenExcel) {
                    Some(temp_path(&format!("{} data", self.deck.name), "xlsx"))
                } else {
                    rfd::FileDialog::new()
                        .add_filter("Excel workbook", &["xlsx"])
                        .set_file_name(format!("{} data.xlsx", safe_name(&self.deck.name)))
                        .save_file()
                };
                if let Some(p) = target {
                    self.status = match office::write_xlsx(&p, &sheets) {
                        Ok(_) if matches!(a, Action::OpenExcel) => open_with(&p, "Microsoft Excel", "Excel"),
                        Ok(_) => format!("Saved {} data sheet(s) for Excel: {}", sheets.len(), file_name(&p)),
                        Err(e) => e,
                    };
                }
            }
            Action::ExportPng => {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("PNG picture", &["png"])
                    .set_file_name(format!("{} slide {}.png", safe_name(&self.deck.name), self.current + 1))
                    .save_file()
                {
                    let r = self.slide_rect;
                    self.request_shot(&ctx, ShotUse::Png(p, r));
                }
            }
            Action::CopyPicture => {
                let r = self.slide_rect;
                self.request_shot(&ctx, ShotUse::Clipboard(r));
            }
            Action::CopyData => {
                if let Some(i) = self.single() {
                    if let Kind::Chart(c) = &self.deck.slides[self.current].elements[i].kind {
                        ctx.copy_text(office::chart_tsv(c));
                        self.status = "Datasheet copied. Paste it into Excel or any spreadsheet.".into();
                    }
                }
            }
            Action::Insert(what) => self.insert(what),
            Action::Delete => {
                let sel = self.selected.clone();
                self.slide().elements.retain(|e| !sel.contains(&e.id));
                self.selected.clear();
            }
            Action::Duplicate => {
                let sel = self.selected.clone();
                let copies: Vec<Element> = self.deck.slides[self.current].elements.iter().filter(|e| sel.contains(&e.id)).cloned().collect();
                self.selected.clear();
                for mut e in copies {
                    e.id = self.deck.new_id();
                    e.x += 16.0;
                    e.y += 16.0;
                    self.selected.push(e.id);
                    self.slide().elements.push(e);
                }
            }
            Action::Copy => {
                self.copy_selection(&ctx);
            }
            Action::Cut => {
                if self.copy_selection(&ctx) {
                    let status = self.status.replace("Copied", "Cut");
                    self.run(Action::ItemDelete, ui);
                    self.status = status;
                }
            }
            Action::Paste { text, at } => {
                let clip = match text {
                    Some(t) => match Clip::from_text(&t) {
                        Some(c) => c,
                        None => {
                            self.status = "The clipboard holds something from another app. ThinkCraft pastes bars and elements copied in ThinkCraft; use Paste from Excel… for data.".into();
                            return;
                        }
                    },
                    None => match self.clip.clone() {
                        Some(c) => c,
                        None => {
                            self.status = keys("Nothing copied yet. Click a bar or an element and press Cmd+C first.");
                            return;
                        }
                    },
                };
                self.clip = Some(clip.clone());
                self.paste_clip(clip, at);
            }
            Action::ItemDelete => match self.gantt_item() {
                Some((id, row, bi)) => {
                    let name = self.item_name((id, row, bi));
                    if let Some(g) = self.gantt_mut(id) {
                        g.tasks[row].bars.remove(bi);
                    }
                    self.gantt_sel = None;
                    self.status = format!("Deleted {name}.");
                }
                None => self.run(Action::Delete, ui),
            },
            Action::ItemDuplicate => match self.gantt_item() {
                Some((id, row, bi)) => {
                    if let Some(g) = self.gantt_mut(id) {
                        let bar = g.tasks[row].bars[bi].clone();
                        let (r, b) = paste_bar(g, row, bar, None);
                        self.gantt_sel = Some((id, r, b));
                        self.status = format!("Duplicated {}, on top of the original. Drag it, or use the arrow keys, to move it.", self.item_name((id, r, b)));
                    }
                }
                None => self.run(Action::Duplicate, ui),
            },
            Action::GanttAdd { id, row, t, milestone } => {
                if let Some(g) = self.gantt_mut(id) {
                    let len = if milestone { 0.0 } else { 1.0 };
                    let (r, b) = paste_bar(g, row, Bar { start: t, end: t + len, milestone, dashed: false, color: if milestone { 1 } else { 0 } }, Some(t));
                    self.selected = vec![id];
                    self.gantt_sel = Some((id, r, b));
                    self.status = format!("Added {}. Drag it or its ends to adjust.", self.item_name((id, r, b)));
                }
            }
            Action::GanttColumn { id, which } => {
                if let Some(g) = self.gantt_mut(id) {
                    let (flag, name) = match which {
                        0 => (&mut g.show_activity, "activity"),
                        1 => (&mut g.show_responsible, "responsible"),
                        _ => (&mut g.show_remark, "remark"),
                    };
                    *flag = !*flag;
                    let on = *flag;
                    self.selected = vec![id];
                    if on && which > 0 {
                        self.show_datasheet = true;
                    }
                    self.status = if on && which > 0 {
                        format!("Added the {name} label column. Fill it in the datasheet below.")
                    } else if on {
                        format!("Added the {name} label column.")
                    } else {
                        format!("Removed the {name} label column. Its text is kept and comes back when you add the column again.")
                    };
                }
            }
            Action::GanttFit { id } => {
                if let Some(g) = self.gantt_mut(id) {
                    let ok = g.fit_to_data();
                    let span = if g.start.is_some() { format!("{} weeks", g.weeks) } else { format!("{} periods", g.periods.len()) };
                    self.status = if ok { format!("The timeline now fits the bars: {span}.") } else { "There are no bars to fit the timeline to.".into() };
                }
            }
            Action::GanttCalendar { id } => {
                let start = self.gantt_mut(id).and_then(|g| g.start);
                match start {
                    Some([y, m, _]) => {
                        self.calendar_dlg = Some(CalendarDlg { id, year: y, month: m.clamp(1, 12) as u32, scroll: true, pending: None, chosen: None, hover: None, fresh: true })
                    }
                    None => self.status = "The calendar is for timelines in calendar weeks. Switch the chart to Calendar weeks first.".into(),
                }
            }
            Action::Undo => {
                if let Some(d) = self.undo.pop() {
                    self.redo.push(self.deck.clone());
                    self.deck = d;
                    self.committed = self.deck.clone();
                    self.status = "Undone.".into();
                }
            }
            Action::Redo => {
                if let Some(d) = self.redo.pop() {
                    self.undo.push(self.deck.clone());
                    self.deck = d;
                    self.committed = self.deck.clone();
                    self.status = "Redone.".into();
                }
            }
            Action::Align(op) => self.align(op),
            Action::PasteTable => {
                self.paste = Some((String::new(), String::new()));
            }
            Action::ImportTable => {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Spreadsheet or text", &["xlsx", "xlsm", "xls", "ods", "csv", "tsv", "txt"])
                    .pick_file()
                {
                    match office::read_table_file(&p) {
                        Ok(text) => self.apply_table(&text, &file_name(&p)),
                        Err(e) => self.status = e,
                    }
                }
            }
            Action::SlideAdd => {
                let at = self.current + 1;
                self.deck.slides.insert(at, Slide { title: "New slide".into(), footnote: String::new(), elements: vec![] });
                self.current = at;
                self.selected.clear();
            }
            Action::SlideDuplicate => {
                let mut s = self.deck.slides[self.current].clone();
                for e in &mut s.elements {
                    e.id = self.deck.new_id();
                }
                self.deck.slides.insert(self.current + 1, s);
                self.current += 1;
                self.selected.clear();
            }
            Action::SlideDelete => {
                if self.deck.slides.len() > 1 {
                    self.deck.slides.remove(self.current);
                    self.current = self.current.min(self.deck.slides.len() - 1);
                    self.selected.clear();
                }
            }
            Action::SlideMove(d) => {
                let to = self.current as i32 + d;
                if to >= 0 && (to as usize) < self.deck.slides.len() {
                    self.deck.slides.swap(self.current, to as usize);
                    self.current = to as usize;
                }
            }
            Action::SelectAll => {
                self.selected = self.deck.slides[self.current].elements.iter().map(|e| e.id).collect();
            }
            Action::Front | Action::Back => {
                let sel = self.selected.clone();
                let els = &mut self.slide().elements;
                let (mut picked, rest): (Vec<Element>, Vec<Element>) = els.drain(..).partition(|e| sel.contains(&e.id));
                if matches!(a, Action::Front) {
                    *els = rest;
                    els.append(&mut picked);
                } else {
                    *els = picked;
                    els.extend(rest);
                }
            }
        }
    }

    fn apply_table(&mut self, text: &str, from: &str) {
        let Some(i) = self.single() else {
            self.status = "Select a chart first.".into();
            return;
        };
        let el = &mut self.deck.slides[self.current].elements[i];
        match &mut el.kind {
            Kind::Chart(c) => match c.load_table(text) {
                Ok(_) => self.status = format!("Loaded {} series × {} categories from {from}.", c.series.len(), c.categories.len()),
                Err(e) => self.status = e,
            },
            Kind::Gantt(g) => {
                let rows: Vec<Vec<&str>> = text.lines().filter(|l| !l.trim().is_empty()).map(|l| l.split(['\t', ';', ',']).collect()).collect();
                let mut tasks: Vec<Task> = vec![];
                // optional label columns after the dates, found by their heading
                let find = |names: [&str; 2]| {
                    rows.first().and_then(|h| h.iter().skip(4).position(|c| names.iter().any(|n| c.trim().eq_ignore_ascii_case(n))).map(|i| i + 4))
                };
                let resp_col = find([g.responsible_header.as_str(), "Responsible"]);
                let remark_col = find([g.remark_header.as_str(), "Remark"]);
                for r in rows.iter().skip(1) {
                    let label = r.first().map(|s| s.trim().to_string()).unwrap_or_default();
                    let cell = |c: Option<usize>| c.and_then(|c| r.get(c)).map(|v| v.trim().to_string()).unwrap_or_default();
                    let (responsible, remark) = (cell(resp_col), cell(remark_col));
                    let has_dates = r.get(1).map(|v| !v.trim().is_empty()).unwrap_or(false);
                    if !has_dates {
                        // an activity without bars
                        let mut t = Task::new(&label, vec![]);
                        t.responsible = responsible;
                        t.remark = remark;
                        tasks.push(t);
                        continue;
                    }
                    let start = r.get(1).map(|v| parse_number(v) as f32).unwrap_or(0.0);
                    let end = r.get(2).map(|v| parse_number(v) as f32).unwrap_or(start);
                    let milestone = r.get(3).map(|v| !v.trim().is_empty()).unwrap_or(false) || (end - start).abs() < 1e-6;
                    let bar = Bar { start, end, milestone, color: 0, dashed: false };
                    // lines with the same activity name in a row share one activity
                    match tasks.last_mut() {
                        Some(t) if t.label == label => t.bars.push(bar),
                        _ => {
                            let mut t = Task::new(&label, vec![bar]);
                            t.responsible = responsible;
                            t.remark = remark;
                            tasks.push(t);
                        }
                    }
                }
                if tasks.is_empty() {
                    self.status = "Paste rows of: activity, start, end, milestone (and optionally Responsible and Remark columns).".into();
                } else {
                    self.status = format!("Loaded {} activities from {from}.", tasks.len());
                    g.show_responsible |= resp_col.is_some();
                    g.show_remark |= remark_col.is_some();
                    g.tasks = tasks;
                }
            }
            Kind::Table(t) => {
                let rows: Vec<Vec<String>> = text
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.split(if l.contains('\t') { '\t' } else if l.contains(';') { ';' } else { ',' }).map(|c| c.trim().to_string()).collect())
                    .collect();
                if rows.is_empty() {
                    self.status = "The pasted table was empty.".into();
                } else {
                    t.rows = rows;
                    t.normalize();
                    self.status = format!("Loaded a {} × {} table from {from}.", t.rows.len(), t.rows[0].len());
                }
            }
            _ => self.status = "Only charts, Gantt charts and tables take data.".into(),
        }
    }

    fn insert(&mut self, what: Insert) {
        let n = self.deck.slides[self.current].elements.len() as f32;
        let (w, h, kind) = match what {
            Insert::Chart(k) => (520.0, 340.0, Kind::Chart(Chart::new(k))),
            Insert::Gantt => (820.0, 320.0, Kind::Gantt(Gantt::default())),
            Insert::Text => (280.0, 90.0, Kind::Text(TextBox::default())),
            Insert::Harvey => (240.0, 34.0, Kind::Harvey(Harvey { quarters: 2, label: "Status".into() })),
            Insert::Check => (240.0, 34.0, Kind::Check(CheckMark { state: CheckState::Done, label: "Done".into() })),
            Insert::Process => (
                760.0,
                56.0,
                Kind::Process(Process { steps: vec!["Step 1".into(), "Step 2".into(), "Step 3".into(), "Step 4".into()], highlight: None }),
            ),
            Insert::Agenda => (
                420.0,
                240.0,
                Kind::Agenda(Agenda { items: vec!["Introduction".into(), "Analysis".into(), "Recommendation".into(), "Next steps".into()], current: 0 }),
            ),
            Insert::Shape(k) => (
                200.0,
                64.0,
                Kind::Shape(ShapeBox { shape: k, text: "Label".into(), fill: self.style().colors[1], text_color: [255, 255, 255], size: 15.0 }),
            ),
            Insert::Table => (560.0, 200.0, Kind::Table(Table::sample())),
            Insert::BuildDown => (520.0, 340.0, Kind::Chart(Chart::build_down())),
            Insert::Timeline => {
                let (y, m, d) = civil_from_days(today_days());
                (860.0, 360.0, Kind::Gantt(Gantt::calendar(y, m, d, 13)))
            }
        };
        let off = (n * 14.0) % 80.0;
        let x = ((SLIDE_W - w) / 2.0 + off).clamp(0.0, SLIDE_W - w.min(SLIDE_W));
        let y = ((SLIDE_H - h) / 2.0 + 20.0 + off).clamp(64.0, (SLIDE_H - h).max(64.0));
        let el = self.deck.element(x, y, w, h, kind);
        let label = el.kind.label();
        self.selected = vec![el.id];
        self.slide().elements.push(el);
        self.show_datasheet = true;
        self.status = format!("Added a {label}. Drag it to move, pull the corners to resize.");
    }

    fn align(&mut self, op: AlignOp) {
        let sel = self.selected.clone();
        let slide = &mut self.deck.slides[self.current];
        let mut els: Vec<&mut Element> = slide.elements.iter_mut().filter(|e| sel.contains(&e.id)).collect();
        if els.is_empty() {
            return;
        }
        // with one element, align to the content area of the slide
        let (l, t, r, b) = if els.len() == 1 {
            (40.0, 80.0, 920.0, 500.0)
        } else {
            (
                els.iter().map(|e| e.x).fold(f32::MAX, f32::min),
                els.iter().map(|e| e.y).fold(f32::MAX, f32::min),
                els.iter().map(|e| e.x + e.w).fold(f32::MIN, f32::max),
                els.iter().map(|e| e.y + e.h).fold(f32::MIN, f32::max),
            )
        };
        let first_w = els[0].w;
        let first_h = els[0].h;
        match op {
            AlignOp::Left => els.iter_mut().for_each(|e| e.x = l),
            AlignOp::Right => els.iter_mut().for_each(|e| e.x = r - e.w),
            AlignOp::Center => els.iter_mut().for_each(|e| e.x = (l + r) / 2.0 - e.w / 2.0),
            AlignOp::Top => els.iter_mut().for_each(|e| e.y = t),
            AlignOp::Bottom => els.iter_mut().for_each(|e| e.y = b - e.h),
            AlignOp::Middle => els.iter_mut().for_each(|e| e.y = (t + b) / 2.0 - e.h / 2.0),
            AlignOp::SameWidth => els.iter_mut().for_each(|e| e.w = first_w),
            AlignOp::SameHeight => els.iter_mut().for_each(|e| e.h = first_h),
            AlignOp::SpreadH => {
                els.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
                let total: f32 = els.iter().map(|e| e.w).sum();
                let gap = if els.len() > 1 { (r - l - total) / (els.len() - 1) as f32 } else { 0.0 };
                let mut x = l;
                for e in els.iter_mut() {
                    e.x = x;
                    x += e.w + gap;
                }
            }
            AlignOp::SpreadV => {
                els.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
                let total: f32 = els.iter().map(|e| e.h).sum();
                let gap = if els.len() > 1 { (b - t - total) / (els.len() - 1) as f32 } else { 0.0 };
                let mut y = t;
                for e in els.iter_mut() {
                    e.y = y;
                    y += e.h + gap;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // UI pieces

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New deck").clicked() {
                    self.actions.push(Action::New);
                }
                if ui.button("Open sample deck").clicked() {
                    self.actions.push(Action::Sample);
                }
                if ui.button("Open…").clicked() {
                    self.actions.push(Action::Open);
                }
                ui.separator();
                if ui.button(keys("Save  (Cmd+S)")).clicked() {
                    self.actions.push(Action::Save);
                }
                if ui.button("Save as…").clicked() {
                    self.actions.push(Action::SaveAs);
                }
                ui.separator();
                if ui.button("Export to PowerPoint (.pptx)…").clicked() {
                    self.actions.push(Action::ExportPptx);
                }
                if ui.button("Export data to Excel (.xlsx)…").clicked() {
                    self.actions.push(Action::ExportXlsx);
                }
                if ui.button("Export slide as picture (.png)…").clicked() {
                    self.actions.push(Action::ExportPng);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new(keys("Undo  (Cmd+Z)"))).clicked() {
                    self.actions.push(Action::Undo);
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new(keys("Redo  (Shift+Cmd+Z)"))).clicked() {
                    self.actions.push(Action::Redo);
                }
                ui.separator();
                if ui.button(keys("Duplicate  (Cmd+D)")).clicked() {
                    self.actions.push(Action::Duplicate);
                }
                if ui.button("Delete  (Backspace)").clicked() {
                    self.actions.push(Action::Delete);
                }
                if ui.button(keys("Select all  (Cmd+A)")).clicked() {
                    self.actions.push(Action::SelectAll);
                }
                ui.separator();
                if ui.button("Bring to front").clicked() {
                    self.actions.push(Action::Front);
                }
                if ui.button("Send to back").clicked() {
                    self.actions.push(Action::Back);
                }
            });
            ui.menu_button("Slide", |ui| {
                if ui.button("New slide").clicked() {
                    self.actions.push(Action::SlideAdd);
                }
                if ui.button("Duplicate slide").clicked() {
                    self.actions.push(Action::SlideDuplicate);
                }
                if ui.button("Delete slide").clicked() {
                    self.actions.push(Action::SlideDelete);
                }
                if ui.button("Save this slide as PowerPoint…").clicked() {
                    self.actions.push(Action::SaveSlide);
                }
                if ui.button("Copy slide as picture").clicked() {
                    self.actions.push(Action::CopyPicture);
                }
            });
            ui.menu_button("Tools", |ui| {
                let label = if self.deck.decimal_comma { "Switch to decimal point (1,234.5)" } else { "Switch to decimal comma (1.234,5)" };
                if ui.button(label).clicked() {
                    self.actions.push(Action::ToggleDecimal);
                }
                ui.separator();
                if ui.button("Open slides in PowerPoint").clicked() {
                    self.actions.push(Action::OpenPowerPoint);
                }
                if ui.button("Open data in Excel").clicked() {
                    self.actions.push(Action::OpenExcel);
                }
                if ui.button("Save this slide as PowerPoint…").clicked() {
                    self.actions.push(Action::SaveSlide);
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About ThinkCraft").clicked() {
                    self.about = true;
                }
            });
        });
    }

    fn ribbon_button(ui: &mut egui::Ui, icon: Icon, label: &str, tip: &str) -> egui::Response {
        Self::ribbon_button_w(ui, icon, label, tip, 52.0)
    }

    fn elements_popup(&mut self, ctx: &egui::Context) {
        if !self.elements_open {
            return;
        }
        let mut picked = None;
        let area = egui::Area::new(egui::Id::unique("elements_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(self.elements_pos + Vec2::new(0.0, 4.0))
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).fill(Color32::WHITE).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(2.0, 2.0);
                    for (title, items) in ELEMENT_SECTIONS {
                        ui.label(RichText::new(title).size(11.0).strong().color(Color32::from_rgb(110, 112, 120)));
                        for chunk in items.chunks(5) {
                            ui.horizontal(|ui| {
                                for (icon, label, ins) in chunk {
                                    if Self::ribbon_button_w(ui, *icon, label, &format!("Insert: {label}"), 66.0).clicked() {
                                        picked = Some(*ins);
                                    }
                                }
                            });
                        }
                        ui.add_space(6.0);
                    }
                });
            });
        if let Some(ins) = picked {
            self.actions.push(Action::Insert(ins));
            self.elements_open = false;
            return;
        }
        let outside_click = ctx.input(|i| {
            i.pointer.any_click() && i.pointer.interact_pos().map(|p| !area.response.rect.contains(p)).unwrap_or(false)
        });
        if (outside_click && self.frame > self.elements_opened_frame + 1) || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.elements_open = false;
        }
    }

    fn ribbon_button_w(ui: &mut egui::Ui, icon: Icon, label: &str, tip: &str, width: f32) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 56.0), Sense::click());
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect, 5.0, Color32::from_rgb(232, 233, 238));
        }
        let icon_rect = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 18.0), Vec2::new(30.0, 24.0));
        draw::draw_icon(&Canvas::paint(p.clone()), icon_rect, icon, Color32::from_rgb(52, 74, 120));
        p.text(Pos2::new(rect.center().x, rect.bottom() - 10.0), Align2::CENTER_CENTER, label, FontId::proportional(11.0), Color32::from_rgb(50, 52, 60));
        resp.on_hover_text(tip).on_hover_cursor(CursorIcon::PointingHand)
    }

    fn group(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let row = ui
                .horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.set_min_height(58.0);
                    add(ui);
                })
                .response
                .rect;
            let (r, _) = ui.allocate_exact_size(Vec2::new(row.width(), 14.0), Sense::hover());
            ui.painter().text(r.center(), Align2::CENTER_CENTER, title, FontId::proportional(10.5), Color32::from_rgb(120, 122, 130));
        });
        ui.separator();
    }

    fn align_button(ui: &mut egui::Ui, op: AlignOp, tip: &str) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(26.0, 24.0), Sense::click());
        let p = ui.painter();
        let bg = if resp.hovered() { Color32::from_rgb(226, 228, 234) } else { Color32::from_rgb(242, 243, 246) };
        p.rect_filled(rect, 4.0, bg);
        let b = rect.shrink(6.0);
        let ink = Color32::from_rgb(52, 74, 120);
        let light = Color32::from_rgb(140, 160, 200);
        let line = Stroke::new(1.4, Color32::from_rgb(196, 84, 36));
        let bar = |x0: f32, x1: f32, y0: f32, y1: f32, c: Color32| p.rect_filled(Rect::from_x_y_ranges(x0..=x1, y0..=y1), 0.0, c);
        let (l, r, t, bt, cx, cy) = (b.left(), b.right(), b.top(), b.bottom(), b.center().x, b.center().y);
        match op {
            AlignOp::Left => {
                bar(l + 1.0, r, t + 1.0, t + 5.0, ink);
                bar(l + 1.0, l + 8.0, bt - 5.0, bt - 1.0, light);
                p.line_segment([Pos2::new(l, t - 2.0), Pos2::new(l, bt + 2.0)], line);
            }
            AlignOp::Center => {
                bar(l, r, t + 1.0, t + 5.0, ink);
                bar(cx - 4.0, cx + 4.0, bt - 5.0, bt - 1.0, light);
                p.line_segment([Pos2::new(cx, t - 2.0), Pos2::new(cx, bt + 2.0)], line);
            }
            AlignOp::Right => {
                bar(l, r - 1.0, t + 1.0, t + 5.0, ink);
                bar(r - 8.0, r - 1.0, bt - 5.0, bt - 1.0, light);
                p.line_segment([Pos2::new(r, t - 2.0), Pos2::new(r, bt + 2.0)], line);
            }
            AlignOp::Top => {
                bar(l + 1.0, l + 5.0, t + 1.0, bt, ink);
                bar(r - 5.0, r - 1.0, t + 1.0, t + 8.0, light);
                p.line_segment([Pos2::new(l - 2.0, t), Pos2::new(r + 2.0, t)], line);
            }
            AlignOp::Middle => {
                bar(l + 1.0, l + 5.0, t, bt, ink);
                bar(r - 5.0, r - 1.0, cy - 4.0, cy + 4.0, light);
                p.line_segment([Pos2::new(l - 2.0, cy), Pos2::new(r + 2.0, cy)], line);
            }
            AlignOp::Bottom => {
                bar(l + 1.0, l + 5.0, t, bt - 1.0, ink);
                bar(r - 5.0, r - 1.0, bt - 8.0, bt - 1.0, light);
                p.line_segment([Pos2::new(l - 2.0, bt), Pos2::new(r + 2.0, bt)], line);
            }
            AlignOp::SameWidth => {
                bar(l, r, t, t + 4.0, ink);
                bar(l, r, bt - 4.0, bt, light);
                p.line_segment([Pos2::new(l, cy - 1.0), Pos2::new(r, cy - 1.0)], line);
            }
            AlignOp::SameHeight => {
                bar(l, l + 4.0, t, bt, ink);
                bar(r - 4.0, r, t, bt, light);
                p.line_segment([Pos2::new(cx, t), Pos2::new(cx, bt)], line);
            }
            AlignOp::SpreadH => {
                for k in 0..3 {
                    let x = l + (b.width() - 4.0) * k as f32 / 2.0;
                    bar(x, x + 4.0, t + 2.0, bt - 2.0, ink);
                }
            }
            AlignOp::SpreadV => {
                for k in 0..3 {
                    let y = t + (b.height() - 3.0) * k as f32 / 2.0;
                    bar(l + 2.0, r - 2.0, y, y + 3.0, ink);
                }
            }
        }
        resp.on_hover_text(tip).on_hover_cursor(CursorIcon::PointingHand)
    }

    fn ribbon(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // brand mark
            let (r, _) = ui.allocate_exact_size(Vec2::new(48.0, 72.0), Sense::hover());
            let c = Pos2::new(r.center().x, r.top() + 26.0);
            ui.painter().rect_filled(Rect::from_center_size(c, Vec2::splat(30.0)), 7.0, ACCENT);
            ui.painter().text(c, Align2::CENTER_CENTER, "TC", FontId::proportional(14.0), Color32::WHITE);
            ui.painter().text(Pos2::new(c.x, r.bottom() - 12.0), Align2::CENTER_CENTER, "ThinkCraft", FontId::proportional(9.5), ACCENT);
            ui.separator();

            let mut chosen: Option<Insert> = None;
            let mut toggle_elements = None;
            Self::group(ui, "Insert", |ui| {
                let resp = Self::ribbon_button(ui, Icon::Elements, "Elements", "All charts, shapes and layout elements");
                if self.elements_open {
                    ui.painter().rect_stroke(resp.rect, 5.0, Stroke::new(1.5, ACCENT), StrokeKind::Inside);
                }
                if resp.clicked() {
                    toggle_elements = Some(resp.rect.left_bottom());
                }
            });
            if let Some(pos) = toggle_elements {
                self.elements_open = !self.elements_open;
                self.elements_pos = pos;
                self.elements_opened_frame = self.frame;
            }
            Self::group(ui, "Charts", |ui| {
                let quick = [
                    (Icon::Chart(ChartKind::Stacked), "Stacked", Insert::Chart(ChartKind::Stacked)),
                    (Icon::Chart(ChartKind::Clustered), "Clustered", Insert::Chart(ChartKind::Clustered)),
                    (Icon::Chart(ChartKind::Percent), "100%", Insert::Chart(ChartKind::Percent)),
                    (Icon::Chart(ChartKind::Waterfall), "Waterfall", Insert::Chart(ChartKind::Waterfall)),
                    (Icon::Chart(ChartKind::Mekko), "Mekko", Insert::Chart(ChartKind::Mekko)),
                    (Icon::Chart(ChartKind::Line), "Line", Insert::Chart(ChartKind::Line)),
                    (Icon::Chart(ChartKind::Combination), "Combo", Insert::Chart(ChartKind::Combination)),
                    (Icon::Chart(ChartKind::Pie), "Pie", Insert::Chart(ChartKind::Pie)),
                ];
                for (icon, label, ins) in quick {
                    if Self::ribbon_button(ui, icon, label, &format!("Insert a {label} chart")).clicked() {
                        chosen = Some(ins);
                    }
                }
            });
            Self::group(ui, "Layout", |ui| {
                let items = [
                    (Icon::Gantt, "Timeline", "Insert a week-based project timeline", Insert::Timeline),
                    (Icon::Text, "Text", "Insert a text box", Insert::Text),
                    (Icon::Harvey, "Harvey", "Insert a Harvey ball status marker", Insert::Harvey),
                    (Icon::Check, "Check", "Insert a checkbox", Insert::Check),
                    (Icon::Agenda, "Agenda", "Insert an agenda", Insert::Agenda),
                ];
                for (icon, label, tip, ins) in items {
                    if Self::ribbon_button(ui, icon, label, tip).clicked() {
                        chosen = Some(ins);
                    }
                }
            });
            if let Some(c) = chosen {
                self.actions.push(Action::Insert(c));
            }

            let mut align = None;
            Self::group(ui, "Arrange", |ui| {
                ui.vertical(|ui| {
                    ui.add_space(3.0);
                    ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
                    ui.horizontal(|ui| {
                        for (op, tip) in [
                            (AlignOp::Left, "Align left edges"),
                            (AlignOp::Center, "Align centres"),
                            (AlignOp::Right, "Align right edges"),
                            (AlignOp::SameWidth, "Same width"),
                            (AlignOp::SpreadH, "Spread out evenly, left to right"),
                        ] {
                            if Self::align_button(ui, op, tip).clicked() {
                                align = Some(op);
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        for (op, tip) in [
                            (AlignOp::Top, "Align tops"),
                            (AlignOp::Middle, "Align middles"),
                            (AlignOp::Bottom, "Align bottoms"),
                            (AlignOp::SameHeight, "Same height"),
                            (AlignOp::SpreadV, "Spread out evenly, top to bottom"),
                        ] {
                            if Self::align_button(ui, op, tip).clicked() {
                                align = Some(op);
                            }
                        }
                    });
                });
            });
            if let Some(op) = align {
                self.actions.push(Action::Align(op));
            }

            let mut style = self.deck.style;
            Self::group(ui, "Style", |ui| {
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 6.0);
                    egui::ComboBox::from_id_salt("style").width(104.0).selected_text(STYLES[style % STYLES.len()].name).show_ui(ui, |ui| {
                        for (i, s) in STYLES.iter().enumerate() {
                            ui.selectable_value(&mut style, i, s.name);
                        }
                    });
                    let (r, _) = ui.allocate_exact_size(Vec2::new(110.0, 12.0), Sense::hover());
                    for (i, c) in STYLES[style % STYLES.len()].colors.iter().enumerate() {
                        let w = r.width() / 6.0;
                        ui.painter().rect_filled(
                            Rect::from_min_size(Pos2::new(r.left() + w * i as f32, r.top()), Vec2::new(w - 1.0, r.height())),
                            1.0,
                            draw::rgb(*c),
                        );
                    }
                });
            });
            self.deck.style = style;

            let mut act = None;
            Self::group(ui, "Office", |ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 2.0);
                    if ui.add_sized([136.0, 18.0], egui::Button::new("Open in PowerPoint")).on_hover_text("Build an editable PowerPoint file of every slide and open it").clicked() {
                        act = Some(Action::OpenPowerPoint);
                    }
                    if ui.add_sized([136.0, 18.0], egui::Button::new("Open data in Excel")).on_hover_text("Send the chart data (selected chart, or all charts) to an Excel workbook and open it").clicked() {
                        act = Some(Action::OpenExcel);
                    }
                    if ui.add_sized([136.0, 18.0], egui::Button::new("Copy slide as picture")).on_hover_text("Copy this slide as a picture to paste anywhere").clicked() {
                        act = Some(Action::CopyPicture);
                    }
                });
            });
            if let Some(a) = act {
                self.actions.push(a);
            }
        });
    }

    fn slides_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(RichText::new("Slides").strong());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if ui.small_button("+ New").on_hover_text("New slide").clicked() {
                self.actions.push(Action::SlideAdd);
            }
            if ui.small_button("Copy").on_hover_text("Duplicate slide").clicked() {
                self.actions.push(Action::SlideDuplicate);
            }
            if ui.small_button("Del").on_hover_text("Delete slide").clicked() {
                self.actions.push(Action::SlideDelete);
            }
            if ui.small_button("Up").on_hover_text("Move slide up").clicked() {
                self.actions.push(Action::SlideMove(-1));
            }
            if ui.small_button("Down").on_hover_text("Move slide down").clicked() {
                self.actions.push(Action::SlideMove(1));
            }
        });
        ui.add_space(4.0);
        let style = self.style();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut clicked = None;
            for (i, slide) in self.deck.slides.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}", i + 1)).size(11.0).color(Color32::GRAY));
                    let w = ui.available_width() - 4.0;
                    let h = w * SLIDE_H / SLIDE_W;
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
                    let p = ui.painter_at(r.expand(3.0));
                    let xf = Xf { origin: r.min, scale: w / SLIDE_W };
                    draw::draw_slide(&Canvas::paint(p.clone()), slide, &xf, style, i + 1);
                    let stroke = if i == self.current { Stroke::new(2.5, ACCENT) } else { Stroke::new(1.0, Color32::from_rgb(200, 202, 208)) };
                    p.rect_stroke(r, 1.0, stroke, StrokeKind::Outside);
                    if resp.clicked() {
                        clicked = Some(i);
                    }
                });
                ui.add_space(6.0);
            }
            if let Some(i) = clicked {
                if i != self.current {
                    self.current = i;
                    self.selected.clear();
                }
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let n = self.deck.slides.len();
            ui.label(RichText::new(format!("Slide {} of {}", self.current + 1, n)).size(11.5));
            ui.separator();
            let els = self.deck.slides[self.current].elements.len();
            ui.label(RichText::new(format!("{els} element{}", if els == 1 { "" } else { "s" })).size(11.5));
            ui.separator();
            ui.label(RichText::new(&self.status).size(11.5));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Fit").clicked() {
                    self.zoom = 1.0;
                }
                let pct = format!("{:.0}%", self.zoom * 100.0);
                ui.add(egui::Slider::new(&mut self.zoom, 0.4..=2.5).show_value(false).text(pct));
                ui.label(RichText::new("Zoom").size(11.5));
                ui.separator();
                let dirty = self.deck != self.saved;
                ui.label(RichText::new(if dirty { "Unsaved changes" } else { "Saved" }).size(11.5).color(if dirty { ACCENT } else { Color32::GRAY }));
            });
        });
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(avail, Sense::click_and_drag());
        let fit = ((avail.width() - 60.0) / SLIDE_W).min((avail.height() - 60.0) / SLIDE_H).max(0.1);
        let scale = fit * self.zoom;
        let size = Vec2::new(SLIDE_W, SLIDE_H) * scale;
        let origin = avail.center() - size / 2.0;
        let xf = Xf { origin, scale };
        let slide_rect = Rect::from_min_size(origin, size);
        self.slide_rect = slide_rect;
        let painter = ui.painter_at(avail);
        painter.rect_filled(avail, 0.0, CANVAS_BG);
        // shadow
        for k in 1..6 {
            painter.rect_filled(slide_rect.translate(Vec2::new(0.0, k as f32 * 0.8)).expand(k as f32), 2.0, Color32::from_black_alpha(10));
        }
        let style = self.style();
        let slide_painter = painter.with_clip_rect(slide_rect.intersect(avail));
        draw::draw_slide(&Canvas::paint(slide_painter), &self.deck.slides[self.current], &xf, style, self.current + 1);

        // ---- interaction
        let alt = ui.input(|i| i.modifiers.alt);
        let shift = ui.input(|i| i.modifiers.shift || i.modifiers.command);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let thresh = 6.0 / scale;
        let handle_at = |el: &Element, p: Pos2| -> Option<usize> {
            let r = xf.el_rect(el);
            handles(r).iter().position(|h| h.distance(p) < 7.0)
        };
        let hit = |slide: &Slide, p: Pos2| -> Option<u64> {
            let sp = xf.to_slide(p);
            slide.elements.iter().rev().find(|e| Rect::from_min_size(Pos2::new(e.x, e.y), Vec2::new(e.w, e.h)).expand(3.0).contains(sp)).map(|e| e.id)
        };

        let gantt_at = |slide: &Slide, p: Pos2| -> Option<(u64, GanttHit)> {
            slide.elements.iter().rev().find_map(|e| gantt_hit(e, &xf, p).map(|h| (e.id, h)))
        };

        // cursor feedback
        if let (Some(p), Drag::None) = (resp.hover_pos(), &self.drag) {
            let slide = &self.deck.slides[self.current];
            let on_handle = self.single().map(|i| handle_at(&slide.elements[i], p).is_some()).unwrap_or(false);
            if let (Some((_, h)), false) = (gantt_at(slide, p), on_handle) {
                ui.ctx().set_cursor_icon(match h {
                    GanttHit::Bar(_, _, BarPart::Move) => CursorIcon::Grab,
                    _ => CursorIcon::ResizeHorizontal,
                });
            } else if let Some(i) = self.single() {
                if let Some(h) = handle_at(&slide.elements[i], p) {
                    ui.ctx().set_cursor_icon(handle_cursor(h));
                } else if hit(slide, p).is_some() {
                    ui.ctx().set_cursor_icon(CursorIcon::Move);
                }
            } else if hit(slide, p).is_some() {
                ui.ctx().set_cursor_icon(CursorIcon::Move);
            }
        }

        if resp.drag_started() {
            let press = ui.input(|i| i.pointer.press_origin()).or(pointer).unwrap_or(avail.center());
            let slide = &self.deck.slides[self.current];
            let resize = self.single().and_then(|i| handle_at(&slide.elements[i], press).map(|h| (slide.elements[i].id, h, &slide.elements[i])));
            let gantt = gantt_at(slide, press);
            if let Some((id, h, el)) = resize {
                self.drag = Drag::Resize { id, handle: h, orig: Rect::from_min_size(Pos2::new(el.x, el.y), Vec2::new(el.w, el.h)), start: xf.to_slide(press) };
            } else if let Some((id, h)) = gantt {
                self.selected = vec![id];
                self.gantt_sel = match h {
                    GanttHit::Bar(row, bar, _) => Some((id, row, bar)),
                    GanttHit::Today => None,
                };
                let el = slide.elements.iter().find(|e| e.id == id).unwrap();
                self.drag = match (h, &el.kind) {
                    (GanttHit::Today, _) => Drag::GanttToday { id },
                    (GanttHit::Bar(row, bar, part), Kind::Gantt(g)) => {
                        let geo = draw::GanttGeo::new(xf.el_rect(el), xf.scale, g);
                        let t = &g.tasks[row].bars[bar];
                        Drag::GanttBar { id, row, bar, part, orig: (t.start, t.end), grab_t: geo.t(press.x) }
                    }
                    _ => Drag::None,
                };
            } else if let Some(id) = hit(slide, press) {
                self.gantt_sel = None;
                if !self.selected.contains(&id) {
                    if shift {
                        self.selected.push(id);
                    } else {
                        self.selected = vec![id];
                    }
                }
                let orig = slide
                    .elements
                    .iter()
                    .filter(|e| self.selected.contains(&e.id))
                    .map(|e| (e.id, Rect::from_min_size(Pos2::new(e.x, e.y), Vec2::new(e.w, e.h))))
                    .collect();
                self.drag = Drag::Move { start: xf.to_slide(press), orig };
            } else {
                self.gantt_sel = None;
                if !shift {
                    self.selected.clear();
                }
                self.drag = Drag::Marquee { start: xf.to_slide(press) };
            }
        }

        self.guides.clear();
        self.marquee = None;
        self.drag_label = None;

        // ---- Gantt bars and date line, dragged right on the slide
        let gantt_drag = match &self.drag {
            Drag::GanttBar { id, row, bar, part, orig, grab_t } => Some((*id, Some((*row, *bar, *part, *orig, *grab_t)))),
            Drag::GanttToday { id } => Some((*id, None)),
            _ => None,
        };
        if let (Some(p), Some((id, bar))) = (pointer, gantt_drag) {
            let cur = self.current;
            if let Some(el) = self.deck.slides[cur].elements.iter_mut().find(|e| e.id == id) {
                let rect = xf.el_rect(el);
                if let Kind::Gantt(g) = &mut el.kind {
                    let geo = draw::GanttGeo::new(rect, xf.scale, g);
                    let cols = g.columns() as f32;
                    let step = g.snap_step();
                    let snap = |v: f32| if alt { v } else { (v / step).round() * step };
                    match bar {
                        None => {
                            let t = snap(geo.t(p.x)).clamp(0.0, cols);
                            g.today = Some(t);
                            g.today_fixed = g.start.is_some();
                            self.drag_label = Some(format!("Date line: {}", g.date_label(t)));
                        }
                        Some((mut row, mut bi, part, (s0, e0), grab_t)) => {
                            if row < g.tasks.len() && bi < g.tasks[row].bars.len() {
                                let dt = geo.t(p.x) - grab_t;
                                let len = (e0 - s0).max(0.0);
                                {
                                    let t = &mut g.tasks[row].bars[bi];
                                    match part {
                                        BarPart::Move => {
                                            let s = snap(s0 + dt).clamp(0.0, (cols - len).max(0.0));
                                            t.start = s;
                                            t.end = if t.milestone { s } else { s + len };
                                        }
                                        BarPart::Start => t.start = snap(s0 + dt).clamp(0.0, (t.end - step).max(0.0)),
                                        BarPart::End => t.end = snap(e0 + dt).clamp(t.start + step, cols),
                                    }
                                }
                                // moving up or down puts the bar into another activity, next to what is there
                                if part == BarPart::Move {
                                    let target = geo.row_at(p.y, g.tasks.len());
                                    if target != row {
                                        let moved = g.tasks[row].bars.remove(bi);
                                        g.tasks[target].bars.push(moved);
                                        row = target;
                                        bi = g.tasks[target].bars.len() - 1;
                                    }
                                }
                                let label = &g.tasks[row].label;
                                let t = &g.tasks[row].bars[bi];
                                self.drag_label = Some(if t.milestone {
                                    format!("{label}: {}", g.date_label(t.start))
                                } else if g.start.is_some() {
                                    format!("{label}: {} – {}", g.date_label(t.start), g.date_label(t.end - 1.0 / 7.0))
                                } else {
                                    format!("{label}: {:.2} – {:.2}", t.start, t.end)
                                });
                                self.drag = Drag::GanttBar { id, row, bar: bi, part, orig: (s0, e0), grab_t };
                                self.gantt_sel = Some((id, row, bi));
                            }
                        }
                    }
                }
            }
        }

        if let Some(p) = pointer {
            let now = xf.to_slide(p);
            let others: Vec<Rect> = self.deck.slides[self.current]
                .elements
                .iter()
                .filter(|e| !self.selected.contains(&e.id))
                .map(|e| Rect::from_min_size(Pos2::new(e.x, e.y), Vec2::new(e.w, e.h)))
                .collect();
            let mut xs = vec![40.0, SLIDE_W / 2.0, 920.0];
            let mut ys = vec![80.0, SLIDE_H / 2.0, 500.0];
            for r in &others {
                xs.extend([r.left(), r.center().x, r.right()]);
                ys.extend([r.top(), r.center().y, r.bottom()]);
            }
            match &self.drag {
                Drag::Move { start, orig } => {
                    let mut d = now - *start;
                    let bb = orig.iter().fold(Rect::NOTHING, |a, (_, r)| a.union(*r)).translate(d);
                    if !alt {
                        if let Some((delta, at)) = snap(&[bb.left(), bb.center().x, bb.right()], &xs, thresh) {
                            d.x += delta;
                            self.guides.push((true, at));
                        }
                        if let Some((delta, at)) = snap(&[bb.top(), bb.center().y, bb.bottom()], &ys, thresh) {
                            d.y += delta;
                            self.guides.push((false, at));
                        }
                    }
                    let orig = orig.clone();
                    for e in self.slide().elements.iter_mut() {
                        if let Some((_, r)) = orig.iter().find(|(id, _)| *id == e.id) {
                            e.x = (r.left() + d.x).round();
                            e.y = (r.top() + d.y).round();
                        }
                    }
                }
                Drag::Resize { id, handle, orig, start } => {
                    let d = now - *start;
                    let (mut l, mut t, mut r, mut b) = (orig.left(), orig.top(), orig.right(), orig.bottom());
                    let h = *handle;
                    if matches!(h, 0 | 6 | 7) {
                        l += d.x;
                    }
                    if matches!(h, 2 | 3 | 4) {
                        r += d.x;
                    }
                    if matches!(h, 0 | 1 | 2) {
                        t += d.y;
                    }
                    if matches!(h, 4 | 5 | 6) {
                        b += d.y;
                    }
                    if !alt {
                        let sx = |v: &mut f32, list: &Vec<f32>, vertical: bool, guides: &mut Vec<(bool, f32)>| {
                            if let Some((delta, at)) = snap(&[*v], list, thresh) {
                                *v += delta;
                                guides.push((vertical, at));
                            }
                        };
                        if matches!(h, 0 | 6 | 7) {
                            sx(&mut l, &xs, true, &mut self.guides);
                        }
                        if matches!(h, 2 | 3 | 4) {
                            sx(&mut r, &xs, true, &mut self.guides);
                        }
                        if matches!(h, 0 | 1 | 2) {
                            sx(&mut t, &ys, false, &mut self.guides);
                        }
                        if matches!(h, 4 | 5 | 6) {
                            sx(&mut b, &ys, false, &mut self.guides);
                        }
                    }
                    let id = *id;
                    if let Some(e) = self.slide().elements.iter_mut().find(|e| e.id == id) {
                        let (l2, r2) = (l.min(r - 16.0), r.max(l + 16.0));
                        let (t2, b2) = (t.min(b - 16.0), b.max(t + 16.0));
                        e.x = l2.round();
                        e.y = t2.round();
                        e.w = (r2 - l2).round();
                        e.h = (b2 - t2).round();
                    }
                }
                Drag::Marquee { start } => {
                    let r = Rect::from_two_pos(*start, now);
                    self.marquee = Some(r);
                    let inside: Vec<u64> = self.deck.slides[self.current]
                        .elements
                        .iter()
                        .filter(|e| r.intersects(Rect::from_min_size(Pos2::new(e.x, e.y), Vec2::new(e.w, e.h))))
                        .map(|e| e.id)
                        .collect();
                    self.selected = inside;
                }
                _ => {}
            }
        }
        if resp.drag_stopped() || !ui.input(|i| i.pointer.primary_down()) {
            if !matches!(self.drag, Drag::None) {
                self.guides.clear();
                self.marquee = None;
            }
            self.drag = Drag::None;
        }

        if resp.clicked() {
            if let Some(p) = pointer {
                // a click on a Gantt bar or milestone picks that item inside the chart
                let picked = match gantt_at(&self.deck.slides[self.current], p) {
                    Some((id, GanttHit::Bar(row, bar, _))) if !shift => Some((id, row, bar)),
                    _ => None,
                };
                self.gantt_sel = picked;
                if let Some(item) = picked {
                    self.selected = vec![item.0];
                    self.status = keys(&format!(
                        "Picked {}. Cmd+C copies it, Cmd+V pastes, Delete removes it, arrows move it. Right-click for more.",
                        self.item_name(item)
                    ));
                } else {
                    match hit(&self.deck.slides[self.current], p) {
                        Some(id) if shift => {
                            if let Some(k) = self.selected.iter().position(|s| *s == id) {
                                self.selected.remove(k);
                            } else {
                                self.selected.push(id);
                            }
                        }
                        Some(id) => self.selected = vec![id],
                        None => {
                            if !shift {
                                self.selected.clear()
                            }
                        }
                    }
                }
            }
        }
        if resp.double_clicked() {
            self.show_datasheet = true;
            // double-click an empty spot in a Gantt row: add a bar there
            if let Some(p) = pointer {
                let cur = self.current;
                let mut added = None;
                for el in self.deck.slides[cur].elements.iter_mut().rev() {
                    if gantt_hit(el, &xf, p).is_some() {
                        break;
                    }
                    let rect = xf.el_rect(el);
                    if let Kind::Gantt(g) = &mut el.kind {
                        let geo = draw::GanttGeo::new(rect, xf.scale, g);
                        let inside = p.x >= geo.grid.left() && p.x <= geo.grid.right() && p.y >= geo.grid.top() && p.y <= geo.bottom;
                        if inside && !g.tasks.is_empty() {
                            let row = geo.row_at(p.y, g.tasks.len());
                            let cols = g.columns() as f32;
                            let len = if g.start.is_some() { 1.0 } else { 1.0f32.min(cols) };
                            let start = ((geo.t(p.x) / g.snap_step()).round() * g.snap_step()).clamp(0.0, (cols - len).max(0.0));
                            g.tasks[row].bars.push(Bar { start, end: start + len, milestone: false, dashed: false, color: 0 });
                            added = Some((el.id, g.tasks[row].label.clone()));
                            self.gantt_sel = Some((el.id, row, g.tasks[row].bars.len() - 1));
                        }
                        break;
                    }
                }
                if let Some((id, label)) = added {
                    self.selected = vec![id];
                    self.status = format!("Added a bar to {label}. Drag it or its ends to adjust.");
                }
            }
        }
        // right-click: pick what is under the pointer, once, when the menu opens
        if resp.secondary_clicked() {
            self.menu_cell = None;
            if let Some(p) = pointer {
                let slide = &self.deck.slides[self.current];
                match gantt_at(slide, p) {
                    Some((id, GanttHit::Bar(row, bar, _))) => {
                        self.selected = vec![id];
                        self.gantt_sel = Some((id, row, bar));
                    }
                    _ => {
                        self.gantt_sel = None;
                        if let Some(id) = hit(slide, p) {
                            if !self.selected.contains(&id) {
                                self.selected = vec![id];
                            }
                        }
                        // an empty spot inside a Gantt timeline: offer to paste or add there
                        for el in slide.elements.iter().rev() {
                            if !xf.el_rect(el).contains(p) {
                                continue;
                            }
                            if let Kind::Gantt(g) = &el.kind {
                                let geo = draw::GanttGeo::new(xf.el_rect(el), xf.scale, g);
                                if p.x >= geo.grid.left() && p.x <= geo.grid.right() && p.y >= geo.grid.top() && p.y <= geo.bottom && !g.tasks.is_empty() {
                                    self.menu_cell = Some((el.id, geo.row_at(p.y, g.tasks.len()), geo.t(p.x)));
                                }
                            }
                            break;
                        }
                        // a Gantt chart gets the round menu around its corner
                        let gantt_id = hit(slide, p).filter(|id| slide.elements.iter().any(|e| e.id == *id && matches!(e.kind, Kind::Gantt(_))));
                        if let Some(id) = gantt_id {
                            self.radial = Some(Radial { id, slide: self.current, cell: self.menu_cell, opened: self.frame });
                            egui::Popup::close_all(ui.ctx());
                        }
                    }
                }
            }
        }
        if self.radial.is_none() {
            self.list_menu(&resp);
        }
        self.radial_menu(ui.ctx(), &xf);

        // ---- overlays
        self.overlays(&painter, &xf, pointer, slide_rect);
    }

    /// The ordinary right-click menu (elements, and a picked Gantt bar).
    fn list_menu(&mut self, resp: &egui::Response) {
        resp.context_menu(|ui| {
            let has = !self.selected.is_empty();
            let can_paste = self.clip.is_some();
            if let Some(item) = self.gantt_item() {
                let name = self.item_name(item);
                ui.label(RichText::new(name[..1].to_uppercase() + &name[1..]).small().color(Color32::GRAY));
                let mut entry = |ui: &mut egui::Ui, label: &str, keys: &str, a: Action| {
                    if ui.add(egui::Button::new(label).shortcut_text(self::keys(keys))).clicked() {
                        self.actions.push(a);
                        ui.close();
                    }
                };
                entry(ui, "Copy", "⌘C", Action::Copy);
                entry(ui, "Cut", "⌘X", Action::Cut);
                if can_paste {
                    entry(ui, "Paste", "⌘V", Action::Paste { text: None, at: None });
                }
                entry(ui, "Duplicate", "⌘D", Action::ItemDuplicate);
                entry(ui, "Delete", "⌫", Action::ItemDelete);
                ui.separator();
                if ui.button("Copy slide as picture").clicked() {
                    self.actions.push(Action::CopyPicture);
                    ui.close();
                }
                return;
            }
            if ui.add_enabled(has, egui::Button::new("Copy").shortcut_text(keys("⌘C"))).clicked() {
                self.actions.push(Action::Copy);
                ui.close();
            }
            if ui.add_enabled(has, egui::Button::new("Cut").shortcut_text(keys("⌘X"))).clicked() {
                self.actions.push(Action::Cut);
                ui.close();
            }
            if ui.add_enabled(can_paste, egui::Button::new("Paste").shortcut_text(keys("⌘V"))).clicked() {
                self.actions.push(Action::Paste { text: None, at: None });
                ui.close();
            }
            if ui.add_enabled(has, egui::Button::new("Duplicate").shortcut_text(keys("⌘D"))).clicked() {
                self.actions.push(Action::Duplicate);
                ui.close();
            }
            if ui.add_enabled(has, egui::Button::new("Delete").shortcut_text("⌫")).clicked() {
                self.actions.push(Action::Delete);
                ui.close();
            }
            if ui.add_enabled(has, egui::Button::new("Bring to front")).clicked() {
                self.actions.push(Action::Front);
                ui.close();
            }
            if ui.add_enabled(has, egui::Button::new("Send to back")).clicked() {
                self.actions.push(Action::Back);
                ui.close();
            }
            ui.separator();
            if ui.button("Copy slide as picture").clicked() {
                self.actions.push(Action::CopyPicture);
                ui.close();
            }
        });
    }

    /// Selection outlines, the picked Gantt bar, snap guides, the marquee and the drag label.
    fn overlays(&mut self, painter: &egui::Painter, xf: &Xf, pointer: Option<Pos2>, slide_rect: Rect) {
        if let (Some(label), Some(p)) = (&self.drag_label, pointer) {
            let galley = painter.layout_no_wrap(label.clone(), FontId::proportional(12.0), Color32::WHITE);
            let r = Rect::from_min_size(p + Vec2::new(14.0, -30.0), galley.size() + Vec2::new(12.0, 6.0));
            painter.rect_filled(r, 4.0, Color32::from_rgb(40, 42, 52));
            painter.galley(r.min + Vec2::new(6.0, 3.0), galley, Color32::WHITE);
            self.status = label.clone();
        }
        if !self.hide_overlay {
            let single = self.selected.len() == 1;
            for e in self.deck.slides[self.current].elements.iter().filter(|e| self.selected.contains(&e.id)) {
                let r = xf.el_rect(e);
                painter.rect_stroke(r, 0.0, Stroke::new(1.2, ACCENT), StrokeKind::Outside);
                if single {
                    for h in handles(r) {
                        let hr = Rect::from_center_size(h, Vec2::splat(7.0));
                        painter.rect_filled(hr, 1.0, Color32::WHITE);
                        painter.rect_stroke(hr, 1.0, Stroke::new(1.2, ACCENT), StrokeKind::Inside);
                    }
                    painter.text(
                        r.left_top() + Vec2::new(0.0, -4.0),
                        Align2::LEFT_BOTTOM,
                        format!("{}  ·  {:.0} × {:.0}", e.kind.label(), e.w, e.h),
                        FontId::proportional(11.0),
                        ACCENT,
                    );
                }
            }
            // the picked Gantt bar or milestone
            if let Some((id, row, bi)) = self.gantt_item() {
                if let Some(el) = self.deck.slides[self.current].elements.iter().find(|e| e.id == id) {
                    if let Kind::Gantt(g) = &el.kind {
                        let geo = draw::GanttGeo::new(xf.el_rect(el), xf.scale, g);
                        let r = geo.bar_rect(&g.tasks[row].bars[bi], row).expand(3.0);
                        painter.rect_stroke(r, 2.0, Stroke::new(1.6, ACCENT), StrokeKind::Outside);
                        if !g.tasks[row].bars[bi].milestone {
                            for x in [r.left(), r.right()] {
                                let hr = Rect::from_center_size(Pos2::new(x, r.center().y), Vec2::new(5.0, 9.0));
                                painter.rect_filled(hr, 1.0, Color32::WHITE);
                                painter.rect_stroke(hr, 1.0, Stroke::new(1.2, ACCENT), StrokeKind::Inside);
                            }
                        }
                    }
                }
            }
            for (vertical, at) in &self.guides {
                let (a, b) = if *vertical { (xf.pos(*at, 0.0), xf.pos(*at, SLIDE_H)) } else { (xf.pos(0.0, *at), xf.pos(SLIDE_W, *at)) };
                painter.add(egui::Shape::dashed_line(&[a, b], Stroke::new(1.0, GUIDE), 4.0, 3.0));
            }
            if let Some(m) = self.marquee {
                let r = xf.rect(m.left(), m.top(), m.width(), m.height());
                painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(196, 84, 36, 24));
                painter.rect_stroke(r, 0.0, Stroke::new(1.0, ACCENT), StrokeKind::Inside);
            }
            if self.deck.slides[self.current].elements.is_empty() {
                painter.text(
                    slide_rect.center(),
                    Align2::CENTER_CENTER,
                    "Empty slide — pick a chart or element in the ribbon above",
                    FontId::proportional(16.0),
                    Color32::from_rgb(160, 162, 170),
                );
            }
        }
    }

    /// The round menu: buttons on a circle around the Gantt chart's top-right corner,
    /// each with a tip when hovered. Opened by right-clicking the chart.
    fn radial_menu(&mut self, ctx: &egui::Context, xf: &Xf) {
        let Some(rad) = &self.radial else { return };
        let (id, cell, opened) = (rad.id, rad.cell, rad.opened);
        let el = (rad.slide == self.current).then(|| self.deck.slides[self.current].elements.iter().find(|e| e.id == id)).flatten();
        let Some(Kind::Gantt(g)) = el.map(|e| &e.kind) else {
            self.radial = None;
            return;
        };
        let corner_raw = xf.el_rect(el.unwrap()).right_top();
        let (bubble, radius) = (16.0, 84.0);
        let screen = ctx.content_rect().shrink(radius + bubble + 4.0);
        let corner = Pos2::new(corner_raw.x.clamp(screen.left(), screen.right().max(screen.left())), corner_raw.y.clamp(screen.top(), screen.bottom().max(screen.top())));
        let several = self.selected.len() > 1;
        let bar_copied = matches!(self.clip, Some(Clip::GanttItem { .. }));
        let (calendar, act, resp, rem, datasheet) = (g.start.is_some(), g.show_activity, g.show_responsible, g.show_remark, self.show_datasheet);
        let info = |c: RadialCmd| -> (String, bool, bool) {
            // (tip, enabled, column shown)
            let col = |on: bool, name: &str| (format!("{} {name} label column", if on { "Remove" } else { "Add" }), true, on);
            match c {
                RadialCmd::Delete => ("Delete chart".into(), true, false),
                RadialCmd::SameWidth => (if several { "Same width".into() } else { "Same width (select several elements first)".into() }, several, false),
                RadialCmd::SameHeight => (if several { "Same height".into() } else { "Same height (select several elements first)".into() }, several, false),
                RadialCmd::Activity => col(act, "activity"),
                RadialCmd::Responsible => col(resp, "responsible"),
                RadialCmd::Remark => col(rem, "remark"),
                RadialCmd::Fit => ("Fit scale to data".into(), true, false),
                RadialCmd::Calendar => (if calendar { "Open calendar".into() } else { "Open calendar (calendar weeks only)".into() }, calendar, false),
                RadialCmd::AddBar => (if cell.is_some() { "Add bar here".into() } else { "Add bar here (right-click inside the timeline)".into() }, cell.is_some(), false),
                RadialCmd::AddMilestone => (if cell.is_some() { "Add milestone here".into() } else { "Add milestone here (right-click inside the timeline)".into() }, cell.is_some(), false),
                RadialCmd::PasteHere => (if bar_copied { "Paste bar here".into() } else { "Paste bar here (copy a bar first)".into() }, bar_copied, false),
                RadialCmd::Excel => ("Open data in Excel".into(), true, false),
                RadialCmd::Datasheet => (if datasheet { "Hide datasheet".into() } else { "Show datasheet".into() }, true, datasheet),
                RadialCmd::Duplicate => ("Duplicate chart".into(), true, false),
            }
        };
        let n = RADIAL.len() as f32;
        let centers: Vec<Pos2> = (0..RADIAL.len())
            .map(|i| {
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / n;
                corner + Vec2::new(a.cos(), a.sin()) * radius
            })
            .collect();
        let pointer = ctx.pointer_hover_pos();
        let hovered = pointer.and_then(|p| centers.iter().position(|c| c.distance(p) <= bubble + 1.0));
        let bounds = Rect::from_center_size(corner, Vec2::splat(2.0 * (radius + bubble + 2.0)));
        let mut picked = None;
        egui::Area::new(egui::Id::unique("gantt_radial")).order(egui::Order::Foreground).fixed_pos(bounds.min).show(ctx, |ui| {
            let (_, _) = ui.allocate_exact_size(bounds.size(), Sense::click());
            let painter = ui.painter();
            if let Some(h) = hovered {
                // a soft wedge from the corner to the hovered button
                let c = centers[h];
                let side = (c - corner).normalized().rot90() * bubble * 0.9;
                painter.add(egui::Shape::convex_polygon(vec![corner, c + side, c - side], Color32::from_black_alpha(28), Stroke::NONE));
            }
            for (i, cmd) in RADIAL.iter().enumerate() {
                let (_, enabled, on) = info(*cmd);
                let c = centers[i];
                let fill = if hovered == Some(i) && enabled { Color32::from_rgb(198, 199, 204) } else { Color32::from_rgb(236, 236, 239) };
                painter.circle_filled(c + Vec2::new(0.0, 1.0), bubble, Color32::from_black_alpha(30));
                painter.circle_filled(c, bubble, fill);
                radial_glyph(painter, c, *cmd, on, enabled);
            }
            if let Some(h) = hovered {
                let (tip, _, _) = info(RADIAL[h]);
                let galley = painter.layout_no_wrap(tip, FontId::proportional(13.0), Color32::WHITE);
                let c = centers[h];
                let out = (c - corner).normalized();
                let at = c + out * (bubble + 8.0) + Vec2::new(if out.x < -0.3 { -galley.size().x - 12.0 } else { 0.0 }, 6.0);
                let r = Rect::from_min_size(at, galley.size() + Vec2::new(12.0, 8.0));
                painter.rect_filled(r.translate(Vec2::new(0.0, 2.0)), 4.0, Color32::from_black_alpha(40));
                painter.rect_filled(r, 4.0, Color32::from_rgb(32, 33, 38));
                painter.galley(r.min + Vec2::new(6.0, 4.0), galley, Color32::WHITE);
            }
            if self.frame > opened + 1 {
                if let Some(h) = hovered.filter(|_| ui.input(|i| i.pointer.primary_clicked())) {
                    picked = Some(RADIAL[h]);
                }
            }
        });
        let outside_press = self.frame > opened + 1 && ctx.input(|i| i.pointer.any_pressed()) && hovered.is_none();
        if outside_press || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.radial = None;
        }
        let Some(cmd) = picked else { return };
        if !info(cmd).1 {
            return;
        }
        self.radial = None;
        self.gantt_sel = None;
        let action = match cmd {
            RadialCmd::Delete => Action::Delete,
            RadialCmd::SameWidth => Action::Align(AlignOp::SameWidth),
            RadialCmd::SameHeight => Action::Align(AlignOp::SameHeight),
            RadialCmd::Activity => Action::GanttColumn { id, which: 0 },
            RadialCmd::Responsible => Action::GanttColumn { id, which: 1 },
            RadialCmd::Remark => Action::GanttColumn { id, which: 2 },
            RadialCmd::Fit => Action::GanttFit { id },
            RadialCmd::Calendar => Action::GanttCalendar { id },
            RadialCmd::AddBar | RadialCmd::AddMilestone => {
                let (_, row, t) = cell.unwrap_or((id, 0, 0.0));
                Action::GanttAdd { id, row, t, milestone: cmd == RadialCmd::AddMilestone }
            }
            RadialCmd::PasteHere => {
                self.selected = vec![id];
                Action::Paste { text: None, at: cell }
            }
            RadialCmd::Excel => Action::OpenExcel,
            RadialCmd::Datasheet => {
                self.show_datasheet = !self.show_datasheet;
                return;
            }
            RadialCmd::Duplicate => Action::Duplicate,
        };
        self.actions.push(action);
    }

    /// The calendar window: a year planner with one line per month and the weekdays lined
    /// up. Click the first day and then the last day of the period the chart should show.
    fn calendar_window(&mut self, ctx: &egui::Context) {
        let Some(dlg) = &self.calendar_dlg else { return };
        let (id, mut year, mut month, mut scroll, pending, chosen, last_hover, fresh) = (dlg.id, dlg.year, dlg.month, dlg.scroll, dlg.pending, dlg.chosen, dlg.hover, dlg.fresh);
        let Some(g) = self.gantt_mut(id) else {
            self.calendar_dlg = None;
            return;
        };
        let Some([sy, sm, sd]) = g.start else {
            self.calendar_dlg = None;
            return;
        };
        let start = days_from_civil(sy, sm.max(1) as u32, sd.max(1) as u32);
        let weeks = g.weeks.max(1) as i64;
        // the period shown: the days picked if the chart still matches them, else its whole weeks
        let period = match chosen {
            Some((a, b)) if a - weekday(a) as i64 == start && (b - start) / 7 + 1 == weeks => (a, b),
            _ => (start, start + 7 * weeks - 1),
        };
        let today = today_days();
        let date = |d: i64| {
            let (y, m, dd) = civil_from_days(d);
            format!("{dd:02}.{m:02}.{y}")
        };
        let mut open = true;
        let mut clicked_day = None;
        let mut fit = false;
        let mut hover_day = None;
        // layout of the planner: month name, then week blocks of a week number and 7 days;
        // big like a wall planner, shrunk only when the window is too narrow for it
        let screen = ctx.content_rect();
        let base = 104.0 + 6.0 * 22.0 + 37.0 * 30.0;
        // fits small screens, and fills about 70% of a big one
        let k = ((screen.width() - 70.0) / base).min(1.0).max(screen.width() * 0.7 / base).clamp(0.6, 1.35);
        let (name_w, wk_w, day_w, row_h) = (104.0 * k, 22.0 * k, 30.0 * k, 25.0 * k);
        let total_w = name_w + 6.0 * wk_w + 37.0 * day_w;
        let slot_x = |k: usize| name_w + (k / 7 + 1) as f32 * wk_w + k as f32 * day_w;
        let (y0, y1) = (sy.min(year) - 1, (civil_from_days(period.1).0).max(year) + 1);
        let size = Vec2::new(total_w + 30.0, (screen.height() - 120.0).clamp(360.0, 1100.0));
        let mut window = egui::Window::new("Calendar").collapsible(false).resizable(true).default_size(size).open(&mut open);
        if fresh {
            window = window.current_pos(screen.center() - size / 2.0);
        }
        window
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let before = (year, month);
                    egui::ComboBox::from_id_salt("cal_month").width(110.0).selected_text(FULL_MONTHS[(month as usize + 11) % 12]).show_ui(ui, |ui| {
                        for (i, name) in FULL_MONTHS.iter().enumerate() {
                            ui.selectable_value(&mut month, i as u32 + 1, *name);
                        }
                    });
                    ui.add(egui::DragValue::new(&mut year).range(1990..=2100));
                    if (year, month) != before {
                        scroll = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let days = period.1 - period.0 + 1;
                        let (w, d) = (days / 7, days % 7);
                        let span = match (w, d) {
                            (0, d) => format!("{d} day{}", if d == 1 { "" } else { "s" }),
                            (w, 0) => format!("{w} week{}", if w == 1 { "" } else { "s" }),
                            (w, d) => format!("{w} week{} {d} day{}", if w == 1 { "" } else { "s" }, if d == 1 { "" } else { "s" }),
                        };
                        ui.label(RichText::new(format!("{} to {}, {span}", date(period.0), date(period.1))).size(17.0).strong());
                    });
                });
                let hint = match pending {
                    Some(p) => format!("First day: {}. Now click the last day of the period.", date(p)),
                    None => "Click the first day of the period, then the last day. The chart shows the whole calendar weeks they fall in; bars keep their dates.".into(),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(hint).color(if pending.is_some() { ACCENT } else { Color32::from_rgb(90, 92, 100) }));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Fit scale to data").on_hover_text("Show just the weeks that hold bars and milestones").clicked() {
                            fit = true;
                        }
                        if ui.button("Go to today").clicked() {
                            let (y, m, _) = civil_from_days(today);
                            (year, month, scroll) = (y, m, true);
                        }
                    });
                });
                ui.add_space(2.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let font = FontId::proportional(16.0 * k);
                    let small = FontId::proportional(11.0 * k);
                    for y in y0..=y1 {
                        // the year line with the weekday letters
                        let (hr, _) = ui.allocate_exact_size(Vec2::new(total_w, row_h + 2.0), Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(hr, 0.0, Color32::from_rgb(226, 228, 234));
                        p.text(hr.left_center() + Vec2::new(6.0, 0.0), Align2::LEFT_CENTER, y.to_string(), FontId::proportional(18.0 * k), Color32::from_rgb(30, 32, 38));
                        for k in 0..37 {
                            let c = Pos2::new(hr.left() + slot_x(k) + day_w / 2.0, hr.center().y);
                            let col = if k % 7 >= 5 { Color32::from_rgb(120, 122, 130) } else { Color32::from_rgb(40, 42, 48) };
                            p.text(c, Align2::CENTER_CENTER, ["M", "T", "W", "T", "F", "S", "S"][k % 7], font.clone(), col);
                        }
                        for m in 1..=12u32 {
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(total_w, row_h), Sense::click());
                            if scroll && (y, m) == (year, month) {
                                ui.scroll_to_rect(r, Some(egui::Align::TOP));
                                scroll = false;
                            }
                            let p = ui.painter();
                            // every other week block lightly shaded, so the weekdays line up visibly
                            for b in (1..6).step_by(2) {
                                let x0 = r.left() + name_w + b as f32 * (wk_w + 7.0 * day_w);
                                let x1 = (x0 + wk_w + 7.0 * day_w).min(r.right());
                                p.rect_filled(Rect::from_x_y_ranges(x0..=x1, r.y_range()), 0.0, Color32::from_rgb(244, 245, 248));
                            }
                            p.text(r.left_center() + Vec2::new(4.0, 0.0), Align2::LEFT_CENTER, FULL_MONTHS[m as usize - 1], font.clone(), Color32::from_rgb(30, 32, 38));
                            let first = days_from_civil(y, m, 1);
                            let next = if m == 12 { days_from_civil(y + 1, 1, 1) } else { days_from_civil(y, m + 1, 1) };
                            let wd = weekday(first) as usize;
                            let hover = resp.hover_pos();
                            let mut labelled = [false; 6];
                            for day in first..next {
                                let k = wd + (day - first) as usize;
                                let cell = Rect::from_min_size(Pos2::new(r.left() + slot_x(k), r.top()), Vec2::new(day_w, row_h));
                                if !labelled[k / 7] {
                                    labelled[k / 7] = true;
                                    let wx = r.left() + name_w + (k / 7) as f32 * (wk_w + 7.0 * day_w) + wk_w - 2.0;
                                    p.text(Pos2::new(wx, r.bottom() - 3.0), Align2::RIGHT_BOTTOM, format!("{:02}", iso_week(day).1), small.clone(), ACCENT);
                                }
                                let hovered = hover.map(|h| cell.contains(h)).unwrap_or(false);
                                if hovered {
                                    hover_day = Some(day);
                                    if resp.clicked() {
                                        clicked_day = Some(day);
                                    }
                                }
                            }
                            // while picking, preview the period up to the day under the pointer (last frame's,
                            // so the rows above it show the preview too)
                            let shown = match (pending, last_hover) {
                                (Some(a), Some(b)) => (a.min(b), a.max(b)),
                                (Some(a), None) => (a, a),
                                _ => period,
                            };
                            for day in first..next {
                                let k = wd + (day - first) as usize;
                                let cell = Rect::from_min_size(Pos2::new(r.left() + slot_x(k), r.top()), Vec2::new(day_w, row_h));
                                let inside = day >= shown.0 && day <= shown.1;
                                let hovered = hover.map(|h| cell.contains(h)).unwrap_or(false);
                                let fill = if inside && pending.is_some() {
                                    Some(Color32::from_rgb(226, 136, 98))
                                } else if inside {
                                    Some(ACCENT)
                                } else if day == today {
                                    Some(Color32::from_rgb(244, 170, 150))
                                } else if hovered {
                                    Some(Color32::from_rgb(214, 216, 224))
                                } else {
                                    None
                                };
                                if let Some(f) = fill {
                                    p.rect_filled(cell, 0.0, f);
                                }
                                if day == today {
                                    p.rect_stroke(cell.shrink(0.5), 0.0, Stroke::new(1.2, Color32::from_rgb(214, 36, 56)), StrokeKind::Inside);
                                }
                                if hovered && inside {
                                    p.rect_stroke(cell.shrink(0.5), 0.0, Stroke::new(1.2, Color32::WHITE), StrokeKind::Inside);
                                }
                                let weekend = k % 7 >= 5;
                                let col = if inside {
                                    Color32::WHITE
                                } else if weekend {
                                    Color32::from_rgb(120, 122, 130)
                                } else {
                                    Color32::from_rgb(30, 32, 38)
                                };
                                let (_, _, dd) = civil_from_days(day);
                                p.text(cell.center(), Align2::CENTER_CENTER, format!("{dd:02}"), font.clone(), col);
                            }
                            if resp.hovered() {
                                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                            }
                        }
                    }
                });
            });
        let mut chosen = chosen;
        let mut pending = pending;
        if let Some(day) = clicked_day {
            match pending.take() {
                None => pending = Some(day),
                Some(first) => {
                    let (a, b) = (first.min(day), first.max(day));
                    if let Some(g) = self.gantt_mut(id) {
                        g.set_period(a, b);
                        let w = g.weeks;
                        chosen = Some((a, b));
                        self.status = format!("The chart now shows {} to {} ({w} calendar week{}).", date(a), date(b), if w == 1 { "" } else { "s" });
                    }
                }
            }
        }
        if fit {
            if let Some(g) = self.gantt_mut(id) {
                g.fit_to_data();
                if let Some([y, m, _]) = g.start {
                    (year, month, scroll) = (y, m as u32, true);
                }
            }
            chosen = None;
            pending = None;
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if pending.is_some() {
                pending = None;
            } else {
                open = false;
            }
        }
        if hover_day != last_hover {
            ctx.request_repaint();
        }
        self.calendar_dlg = open.then_some(CalendarDlg { id, year, month, scroll, pending, chosen, hover: hover_day, fresh: false });
    }

    fn props_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(6.0);
            match self.single() {
                None if self.selected.len() > 1 => {
                    ui.heading(format!("{} elements", self.selected.len()));
                    ui.label("Use Arrange in the ribbon to line them up, give them the same size or spread them out evenly.");
                    ui.add_space(6.0);
                    if ui.button("Duplicate").clicked() {
                        self.actions.push(Action::Duplicate);
                    }
                    if ui.button("Delete").clicked() {
                        self.actions.push(Action::Delete);
                    }
                }
                None => self.slide_props(ui),
                Some(i) => self.element_props(ui, i),
            }
        });
    }

    fn slide_props(&mut self, ui: &mut egui::Ui) {
        ui.heading("Slide");
        ui.add_space(4.0);
        let cur = self.current;
        let slide = &mut self.deck.slides[cur];
        ui.label("Title");
        ui.add(egui::TextEdit::singleline(&mut slide.title).desired_width(f32::INFINITY));
        ui.label("Footnote");
        ui.add(egui::TextEdit::singleline(&mut slide.footnote).desired_width(f32::INFINITY));
        ui.add_space(10.0);
        ui.heading("Deck");
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut self.deck.name).desired_width(f32::INFINITY));
        ui.label("Colour style");
        egui::ComboBox::from_id_salt("style2").selected_text(STYLES[self.deck.style % STYLES.len()].name).show_ui(ui, |ui| {
            for (i, s) in STYLES.iter().enumerate() {
                ui.selectable_value(&mut self.deck.style, i, s.name);
            }
        });
        ui.add_space(12.0);
        ui.label(RichText::new("Tips").strong());
        ui.label(keys("• Click an element to select it, Shift-click to add more.\n• Drag on empty space to select several.\n• Elements snap to each other and to the slide margins; hold Option to move freely.\n• Double-click a chart to open its datasheet.\n• Arrow keys nudge, Shift + arrow nudges further."));
    }

    fn element_props(&mut self, ui: &mut egui::Ui, idx: usize) {
        let cur = self.current;
        let n_slides = self.deck.slides.len();
        let _ = n_slides;
        let el = &mut self.deck.slides[cur].elements[idx];
        ui.heading(el.kind.label());
        ui.add_space(2.0);
        egui::Grid::new("pos").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
            ui.label("X");
            ui.add(egui::DragValue::new(&mut el.x).speed(1.0));
            ui.label("Y");
            ui.add(egui::DragValue::new(&mut el.y).speed(1.0));
            ui.end_row();
            ui.label("W");
            ui.add(egui::DragValue::new(&mut el.w).speed(1.0).range(16.0..=2000.0));
            ui.label("H");
            ui.add(egui::DragValue::new(&mut el.h).speed(1.0).range(16.0..=2000.0));
            ui.end_row();
        });
        ui.separator();
        let mut push: Option<Action> = None;
        match &mut el.kind {
            Kind::Chart(c) => {
                ui.label("Chart type");
                let before = c.kind;
                egui::ComboBox::from_id_salt("ctype").selected_text(c.kind.label()).width(170.0).show_ui(ui, |ui| {
                    for k in ChartKind::ALL {
                        ui.selectable_value(&mut c.kind, k, k.label());
                    }
                });
                if before != c.kind && c.kind == ChartKind::Waterfall && c.series.len() > 1 {
                    c.series.truncate(1);
                }
                if before != c.kind && (before.is_xy() != c.kind.is_xy() || (c.kind == ChartKind::Bubble) != (before == ChartKind::Bubble) && c.kind.is_xy()) {
                    // scatter/bubble data has a different shape: start from sample data
                    let title = std::mem::take(&mut c.title);
                    *c = Chart::new(c.kind);
                    c.title = title;
                }
                ui.label("Title");
                ui.add(egui::TextEdit::singleline(&mut c.title).hint_text("No title").desired_width(f32::INFINITY));
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut c.labels, "Labels");
                    ui.checkbox(&mut c.totals, "Totals");
                    ui.checkbox(&mut c.legend, "Legend");
                    if c.kind.uses_value_axis() {
                        ui.checkbox(&mut c.axis, "Value axis");
                    }
                    if matches!(c.kind, ChartKind::Stacked | ChartKind::Waterfall) {
                        ui.checkbox(&mut c.connectors, "Connectors");
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Decimals");
                    ui.add(egui::DragValue::new(&mut c.decimals).range(0..=4));
                    ui.label("Unit");
                    ui.add(egui::TextEdit::singleline(&mut c.unit).desired_width(50.0).hint_text("e.g. m"));
                });
                if !matches!(c.kind, ChartKind::Pie | ChartKind::Doughnut) {
                    ui.horizontal(|ui| {
                        ui.label("Gap");
                        ui.add(egui::Slider::new(&mut c.gap, 0.05..=0.9).show_value(false));
                    });
                }
                if matches!(c.kind, ChartKind::Pie | ChartKind::Doughnut) && c.categories.len() > 1 {
                    ui.horizontal(|ui| {
                        ui.label("Show column");
                        let cats = c.categories.clone();
                        egui::ComboBox::from_id_salt("piecat").selected_text(cats.get(c.pie_category).cloned().unwrap_or_default()).show_ui(ui, |ui| {
                            for (i, n) in cats.iter().enumerate() {
                                ui.selectable_value(&mut c.pie_category, i, n);
                            }
                        });
                    });
                }
                if c.kind.supports_annotations() && c.categories.len() > 1 {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Annotations").strong());
                    let cats = c.categories.clone();
                    let last = cats.len() - 1;
                    annotation_row(ui, "Growth rate (CAGR)", "cagr", &mut c.cagr, &cats, last);
                    annotation_row(ui, "Difference arrow", "diff", &mut c.diff, &cats, last);
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Data").strong());
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Datasheet").clicked() {
                        self.show_datasheet = true;
                    }
                    if ui.button("Paste from Excel…").clicked() {
                        push = Some(Action::PasteTable);
                    }
                    if ui.button("Import file…").on_hover_text("Excel, OpenDocument or CSV").clicked() {
                        push = Some(Action::ImportTable);
                    }
                    if ui.button("Copy data").on_hover_text("Copy the datasheet to paste into Excel").clicked() {
                        push = Some(Action::CopyData);
                    }
                    if ui.button("Open in Excel").clicked() {
                        push = Some(Action::OpenExcel);
                    }
                });
                c.normalize();
            }
            Kind::Gantt(g) => {
                ui.label(RichText::new("Label columns").strong());
                egui::Grid::new("label_cols").num_columns(2).spacing([8.0, 3.0]).show(ui, |ui| {
                    ui.checkbox(&mut g.show_activity, "Activity").on_hover_text("Activity names, left of the timeline");
                    ui.add(egui::TextEdit::singleline(&mut g.header).desired_width(f32::INFINITY).hint_text("Heading"));
                    ui.end_row();
                    ui.checkbox(&mut g.show_responsible, "Responsible").on_hover_text("Who is responsible, next to the activity names");
                    ui.add(egui::TextEdit::singleline(&mut g.responsible_header).desired_width(f32::INFINITY).hint_text("Heading"));
                    ui.end_row();
                    ui.checkbox(&mut g.show_remark, "Remark").on_hover_text("A remark per activity, right of the timeline");
                    ui.add(egui::TextEdit::singleline(&mut g.remark_header).desired_width(f32::INFINITY).hint_text("Heading"));
                    ui.end_row();
                });
                ui.add_space(4.0);
                let mut calendar = g.start.is_some();
                ui.horizontal(|ui| {
                    ui.radio_value(&mut calendar, true, "Calendar weeks");
                    ui.radio_value(&mut calendar, false, "Own periods");
                });
                if calendar && g.start.is_none() {
                    let (y, m, d) = civil_from_days(today_days());
                    let cal = Gantt::calendar(y, m, d, g.periods.len().max(8) as u32);
                    g.start = cal.start;
                    g.weeks = cal.weeks;
                } else if !calendar && g.start.is_some() {
                    g.start = None;
                    if g.periods.is_empty() {
                        g.periods = (1..=g.weeks.max(1)).map(|i| format!("P{i}")).collect();
                    }
                }
                if let Some([y, m, d]) = g.start.as_mut() {
                    ui.horizontal(|ui| {
                        ui.label("Starts");
                        ui.add(egui::DragValue::new(d).range(1..=31).prefix("day "));
                        ui.add(egui::DragValue::new(m).range(1..=12).prefix("month "));
                        ui.add(egui::DragValue::new(y).range(1990..=2100));
                    });
                    // keep the start on a Monday
                    let days = days_from_civil(*y, (*m).clamp(1, 12) as u32, (*d).clamp(1, 31) as u32);
                    let monday = days - weekday(days) as i64;
                    let (yy, mm, dd) = civil_from_days(monday);
                    let (_, wk) = iso_week(monday);
                    *y = yy;
                    *m = mm as i32;
                    *d = dd as i32;
                    ui.horizontal(|ui| {
                        ui.label("Weeks");
                        ui.add(egui::DragValue::new(&mut g.weeks).range(1..=260));
                        ui.label(RichText::new(format!("(from week {wk})")).color(Color32::GRAY));
                    });
                    let mut has_today = g.today.is_some();
                    ui.add_space(4.0);
                    egui::Grid::new("scales").num_columns(3).spacing([14.0, 2.0]).show(ui, |ui| {
                        ui.label("");
                        ui.label(RichText::new("Scale").strong());
                        ui.label(RichText::new("Separator").strong());
                        ui.end_row();
                        for (k, name) in ["Years", "Quarters", "Months", "Weeks"].iter().enumerate() {
                            ui.label(*name);
                            ui.checkbox(&mut g.scales[k], "");
                            ui.checkbox(&mut g.separators[k], "");
                            ui.end_row();
                        }
                    });
                    ui.label(RichText::new("Weeks start on Monday (ISO week numbers).").color(Color32::GRAY));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut has_today, "Show date line");
                        if g.today_fixed && ui.button("Back to today").clicked() {
                            g.today_fixed = false;
                        }
                    });
                    // only change the line when the box is ticked or unticked (it used to be
                    // reset every frame, which pinned a dragged line to the first week)
                    if has_today != g.today.is_some() {
                        g.today = if has_today { Some(g.today_line().map(|(t, _)| t).unwrap_or(0.0)) } else { None };
                        if has_today {
                            g.today_fixed = false;
                        }
                    }
                } else {
                    ui.label("Time periods (comma separated)");
                    let mut s = g.periods.join(", ");
                    if ui.add(egui::TextEdit::multiline(&mut s).desired_rows(2).desired_width(f32::INFINITY)).changed() {
                        g.periods = s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
                    }
                    let mut has_today = g.today.is_some();
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut has_today, "Today line at");
                        let mut t = g.today.unwrap_or(0.0);
                        ui.add_enabled(has_today, egui::DragValue::new(&mut t).speed(0.05).range(0.0..=g.periods.len() as f32));
                        g.today = if has_today { Some(t) } else { None };
                    });
                }
                ui.label(RichText::new(keys("On the slide: drag a bar to move it in time, or up/down into another activity (an activity can hold several bars and milestones). Drag a bar's end to make it longer or shorter. Double-click an empty spot in a row to add a bar. Drag the date line left or right. Hold Option to move freely (no snapping to days). Click a bar or milestone to pick it: Cmd+C copies it and Cmd+V pastes the copy on top, ready to drag; arrow keys move it; Delete removes it. Right-click a bar for Copy, Cut, Paste and Delete. Right-click the chart for the round menu: label columns (activity on the left, responsible and remark on the right), fit scale to data, the calendar (click the first and the last day of the period to show), add or paste a bar there, and more.")).color(Color32::from_rgb(90, 92, 100)));
                ui.label("Names and exact numbers are in the datasheet below.");
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Datasheet").clicked() {
                        self.show_datasheet = true;
                    }
                    if ui.button("Paste from Excel…").clicked() {
                        push = Some(Action::PasteTable);
                    }
                    if ui.button("Import file…").clicked() {
                        push = Some(Action::ImportTable);
                    }
                });
            }
            Kind::Text(t) => {
                ui.label("Text");
                ui.add(egui::TextEdit::multiline(&mut t.text).desired_rows(5).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    ui.label("Size");
                    ui.add(egui::DragValue::new(&mut t.size).range(6.0..=96.0));
                    ui.label("Colour");
                    ui.color_edit_button_srgb(&mut t.color);
                });
                ui.horizontal(|ui| {
                    let mut filled = t.fill.is_some();
                    ui.checkbox(&mut filled, "Background");
                    let mut f = t.fill.unwrap_or([240, 243, 248]);
                    if filled {
                        ui.color_edit_button_srgb(&mut f);
                    }
                    t.fill = if filled { Some(f) } else { None };
                });
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut t.align, Align::Left, "Left");
                    ui.selectable_value(&mut t.align, Align::Center, "Centre");
                    ui.selectable_value(&mut t.align, Align::Right, "Right");
                });
            }
            Kind::Harvey(h) => {
                ui.label("Filled");
                ui.horizontal(|ui| {
                    for q in 0..=4u8 {
                        ui.selectable_value(&mut h.quarters, q, format!("{}%", q as u32 * 25));
                    }
                });
                ui.label("Label");
                ui.add(egui::TextEdit::singleline(&mut h.label).desired_width(f32::INFINITY));
            }
            Kind::Check(cm) => {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut cm.state, CheckState::Done, "Done");
                    ui.selectable_value(&mut cm.state, CheckState::Cross, "No");
                    ui.selectable_value(&mut cm.state, CheckState::Open, "Open");
                });
                ui.label("Label");
                ui.add(egui::TextEdit::singleline(&mut cm.label).desired_width(f32::INFINITY));
            }
            Kind::Process(pr) => {
                ui.label("Steps");
                list_editor(ui, &mut pr.steps, "Step");
                let mut hl = pr.highlight.map(|h| h + 1).unwrap_or(0);
                ui.horizontal(|ui| {
                    ui.label("Highlight step");
                    ui.add(egui::DragValue::new(&mut hl).range(0..=pr.steps.len()));
                    ui.label(if hl == 0 { "(none)" } else { "" });
                });
                pr.highlight = if hl == 0 { None } else { Some(hl - 1) };
            }
            Kind::Agenda(a) => {
                ui.label("Items");
                list_editor(ui, &mut a.items, "Item");
                ui.horizontal(|ui| {
                    ui.label("Current item");
                    let mut c = a.current + 1;
                    ui.add(egui::DragValue::new(&mut c).range(1..=a.items.len().max(1)));
                    a.current = c.saturating_sub(1);
                });
            }
            Kind::Shape(sh) => {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut sh.shape, ShapeKind::Rounded, "Rounded");
                    ui.selectable_value(&mut sh.shape, ShapeKind::Pentagon, "Pentagon");
                    ui.selectable_value(&mut sh.shape, ShapeKind::Chevron, "Chevron");
                });
                ui.label("Text");
                ui.add(egui::TextEdit::multiline(&mut sh.text).desired_rows(2).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    ui.label("Fill");
                    ui.color_edit_button_srgb(&mut sh.fill);
                    ui.label("Text");
                    ui.color_edit_button_srgb(&mut sh.text_color);
                    ui.label("Size");
                    ui.add(egui::DragValue::new(&mut sh.size).range(6.0..=72.0));
                });
            }
            Kind::Table(t) => {
                ui.checkbox(&mut t.header, "First row is a header");
                ui.label(format!("{} rows × {} columns. Edit the cells in the datasheet below.", t.rows.len(), t.rows.first().map(|r| r.len()).unwrap_or(0)));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Datasheet").clicked() {
                        self.show_datasheet = true;
                    }
                    if ui.button("Paste from Excel…").clicked() {
                        push = Some(Action::PasteTable);
                    }
                    if ui.button("Import file…").clicked() {
                        push = Some(Action::ImportTable);
                    }
                });
            }
        }
        ui.add_space(10.0);
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Duplicate").clicked() {
                push = Some(Action::Duplicate);
            }
            if ui.button("Delete").clicked() {
                push = Some(Action::Delete);
            }
            if ui.button("To front").clicked() {
                push = Some(Action::Front);
            }
        });
        if let Some(a) = push {
            self.actions.push(a);
        }
    }

    fn datasheet(&mut self, ui: &mut egui::Ui, idx: usize) {
        let cur = self.current;
        let style = self.style();
        let mut push = None;
        let mut close = false;
        let el = &mut self.deck.slides[cur].elements[idx];
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Datasheet — {}", el.kind.label())).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Close").on_hover_text("Close the datasheet").clicked() {
                    close = true;
                }
                if ui.small_button("Import file…").clicked() {
                    push = Some(Action::ImportTable);
                }
                if ui.small_button("Paste from Excel…").clicked() {
                    push = Some(Action::PasteTable);
                }
            });
        });
        ui.separator();
        match &mut el.kind {
            Kind::Chart(c) => {
                ui.horizontal(|ui| {
                    if ui.button("+ Series").clicked() {
                        c.add_series();
                    }
                    if ui.button("+ Column").clicked() {
                        c.add_category();
                    }
                    if ui.add_enabled(c.categories.len() > 1, egui::Button::new("− Last column")).clicked() {
                        c.categories.pop();
                        c.normalize();
                    }
                    if ui.button("Swap rows and columns").clicked() {
                        c.transpose();
                    }
                    if c.kind == ChartKind::Waterfall {
                        ui.label(RichText::new("Tick “=” to make a column a running total.").color(Color32::GRAY));
                    }
                });
                ui.add_space(2.0);
                let mut remove_series = None;
                let mut remove_cat = None;
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("sheet").striped(true).spacing([2.0, 2.0]).min_col_width(70.0).show(ui, |ui| {
                        ui.label("");
                        for (ci, cat) in c.categories.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add_sized([78.0, 20.0], egui::TextEdit::singleline(cat));
                                if ui.small_button("×").on_hover_text("Remove column").clicked() {
                                    remove_cat = Some(ci);
                                }
                            });
                        }
                        ui.end_row();
                        if c.kind == ChartKind::Waterfall {
                            ui.label(RichText::new("=  running total").color(Color32::GRAY));
                            for ci in 0..c.categories.len() {
                                ui.checkbox(&mut c.subtotal[ci], "=");
                            }
                            ui.end_row();
                        }
                        for (si, ser) in c.series.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                let mut col = ser.color.unwrap_or(style.colors[si % style.colors.len()]);
                                if ui.color_edit_button_srgb(&mut col).changed() {
                                    ser.color = Some(col);
                                }
                                ui.add_sized([120.0, 20.0], egui::TextEdit::singleline(&mut ser.name));
                                if ui.small_button("×").on_hover_text("Remove series").clicked() {
                                    remove_series = Some(si);
                                }
                            });
                            for (ci, v) in ser.values.iter_mut().enumerate() {
                                let is_sub = c.kind == ChartKind::Waterfall && c.subtotal.get(ci).copied().unwrap_or(false);
                                if is_sub {
                                    ui.label(RichText::new("=").color(Color32::GRAY));
                                } else {
                                    ui.add_sized([70.0, 20.0], egui::DragValue::new(v).speed(0.1).max_decimals(4));
                                }
                            }
                            ui.end_row();
                        }
                        if c.kind != ChartKind::Waterfall && c.kind != ChartKind::Pie && c.kind != ChartKind::Doughnut {
                            ui.label(RichText::new("Total").color(Color32::GRAY));
                            for ci in 0..c.categories.len() {
                                ui.label(RichText::new(draw::fmt_num(c.total(ci), c.decimals.max(1), "")).color(Color32::GRAY));
                            }
                            ui.end_row();
                        }
                    });
                });
                if let Some(si) = remove_series {
                    c.series.remove(si);
                }
                if let Some(ci) = remove_cat {
                    if c.categories.len() > 1 {
                        c.categories.remove(ci);
                        for s in &mut c.series {
                            if ci < s.values.len() {
                                s.values.remove(ci);
                            }
                        }
                        if ci < c.subtotal.len() {
                            c.subtotal.remove(ci);
                        }
                        c.cagr = None;
                        c.diff = None;
                    }
                }
                c.normalize();
            }
            Kind::Gantt(g) => {
                ui.horizontal(|ui| {
                    if ui.button("+ Activity").clicked() {
                        let start = g.tasks.iter().flat_map(|t| t.bars.iter().map(|b| b.end)).fold(0.0, f32::max).min(g.columns() as f32 - 1.0);
                        g.tasks.push(Task::one("New activity", start, start + 1.0, false, 0, false));
                    }
                    ui.label(RichText::new(if g.start.is_some() { "Start and end are in weeks from the first week (0 = its Monday). An activity can hold several bars and milestones." } else { "Start and end are in periods (0 = start of the first period). An activity can hold several bars and milestones." }).color(Color32::GRAY));
                });
                let mut remove_row = None;
                let mut remove_bar = None;
                let mut up = None;
                let mut add_bar: Option<(usize, bool)> = None;
                let maxp = g.columns() as f32;
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("gsheet").striped(true).spacing([6.0, 2.0]).show(ui, |ui| {
                        let (show_resp, show_remark) = (g.show_responsible, g.show_remark);
                        for h in ["Activity", "Start", "End", "Milestone", "Dashed", "Colour", ""] {
                            ui.label(RichText::new(h).strong());
                        }
                        if show_resp {
                            ui.label(RichText::new(&g.responsible_header).strong());
                        }
                        if show_remark {
                            ui.label(RichText::new(&g.remark_header).strong());
                        }
                        ui.end_row();
                        for (i, task) in g.tasks.iter_mut().enumerate() {
                            // the activity line: name and the activity's own buttons
                            ui.add_sized([180.0, 20.0], egui::TextEdit::singleline(&mut task.label));
                            ui.label("");
                            ui.label("");
                            ui.label("");
                            ui.label("");
                            ui.horizontal(|ui| {
                                if ui.small_button("+ Bar").on_hover_text("Add another bar to this activity").clicked() {
                                    add_bar = Some((i, false));
                                }
                                if ui.small_button("+ Milestone").on_hover_text("Add a milestone to this activity").clicked() {
                                    add_bar = Some((i, true));
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.small_button("Up").on_hover_text("Move the activity up").clicked() && i > 0 {
                                    up = Some(i);
                                }
                                if ui.small_button("×").on_hover_text("Delete the activity").clicked() {
                                    remove_row = Some(i);
                                }
                            });
                            if show_resp {
                                ui.add_sized([120.0, 20.0], egui::TextEdit::singleline(&mut task.responsible));
                            }
                            if show_remark {
                                ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(&mut task.remark));
                            }
                            ui.end_row();
                            // one line per bar or milestone
                            for (bi, t) in task.bars.iter_mut().enumerate() {
                                ui.label(RichText::new(if t.milestone { "      milestone" } else { "      bar" }).color(Color32::GRAY));
                                // bars outside the timeline keep their dates (they are hidden, not cut)
                                ui.add(egui::DragValue::new(&mut t.start).speed(0.05).range(0.0..=maxp).clamp_existing_to_range(false));
                                if t.milestone {
                                    ui.label("—");
                                    t.end = t.start;
                                } else {
                                    ui.add(egui::DragValue::new(&mut t.end).speed(0.05).range(t.start..=maxp).clamp_existing_to_range(false));
                                }
                                ui.checkbox(&mut t.milestone, "");
                                ui.checkbox(&mut t.dashed, "");
                                ui.horizontal(|ui| {
                                    for k in 0..4 {
                                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
                                        ui.painter().rect_filled(r.shrink(1.0), 2.0, draw::rgb(style.colors[k]));
                                        if t.color == k {
                                            ui.painter().rect_stroke(r, 2.0, Stroke::new(1.5, Color32::BLACK), StrokeKind::Inside);
                                        }
                                        if resp.clicked() {
                                            t.color = k;
                                        }
                                    }
                                });
                                if ui.small_button("×").on_hover_text("Delete this bar").clicked() {
                                    remove_bar = Some((i, bi));
                                }
                                ui.end_row();
                            }
                        }
                    });
                });
                if let Some((i, milestone)) = add_bar {
                    let after = g.tasks[i].bars.iter().map(|b| b.end).fold(0.0, f32::max);
                    let start = (after + if milestone { 0.5 } else { 0.0 }).min(maxp - 1.0).max(0.0);
                    let end = if milestone { start } else { (start + 1.0).min(maxp) };
                    g.tasks[i].bars.push(Bar { start, end, milestone, dashed: false, color: 0 });
                }
                if let Some((i, bi)) = remove_bar {
                    g.tasks[i].bars.remove(bi);
                }
                if let Some(i) = remove_row {
                    g.tasks.remove(i);
                }
                if let Some(i) = up {
                    g.tasks.swap(i, i - 1);
                }
            }
            Kind::Table(t) => {
                ui.horizontal(|ui| {
                    if ui.button("+ Row").clicked() {
                        let cols = t.rows.first().map(|r| r.len()).unwrap_or(1);
                        t.rows.push(vec![String::new(); cols]);
                    }
                    if ui.button("+ Column").clicked() {
                        for r in &mut t.rows {
                            r.push(String::new());
                        }
                    }
                    if ui.add_enabled(t.rows.len() > 1, egui::Button::new("- Last row")).clicked() {
                        t.rows.pop();
                    }
                    if ui.add_enabled(t.rows.first().map(|r| r.len() > 1).unwrap_or(false), egui::Button::new("- Last column")).clicked() {
                        for r in &mut t.rows {
                            r.pop();
                        }
                    }
                    ui.checkbox(&mut t.header, "First row is a header");
                });
                t.normalize();
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("tsheet").striped(true).spacing([2.0, 2.0]).show(ui, |ui| {
                        for row in t.rows.iter_mut() {
                            for (ci, cell) in row.iter_mut().enumerate() {
                                ui.add_sized([if ci == 0 { 160.0 } else { 110.0 }, 20.0], egui::TextEdit::singleline(cell));
                            }
                            ui.end_row();
                        }
                    });
                });
            }
            _ => {
                ui.label("This element has no data. Select a chart, a Gantt chart or a table.");
            }
        }
        if close {
            self.show_datasheet = false;
        }
        if let Some(a) = push {
            self.actions.push(a);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: Key| ctx.input(|i| i.key_pressed(k));
        if cmd && pressed(Key::Z) {
            self.actions.push(if shift { Action::Redo } else { Action::Undo });
        }
        if cmd && pressed(Key::Y) {
            self.actions.push(Action::Redo);
        }
        if cmd && pressed(Key::S) {
            self.actions.push(Action::Save);
        }
        if cmd && pressed(Key::D) {
            self.actions.push(Action::ItemDuplicate);
        }
        if cmd && pressed(Key::A) {
            self.actions.push(Action::SelectAll);
        }
        // Cmd+C, Cmd+X and Cmd+V arrive as clipboard events rather than key presses
        for ev in ctx.input(|i| i.events.clone()) {
            match ev {
                egui::Event::Copy => self.actions.push(Action::Copy),
                egui::Event::Cut => self.actions.push(Action::Cut),
                egui::Event::Paste(text) => self.actions.push(Action::Paste { text: Some(text), at: None }),
                egui::Event::PasteImage(_) => self.actions.push(Action::Paste { text: None, at: None }),
                _ => {}
            }
        }
        if pressed(Key::Delete) || pressed(Key::Backspace) {
            self.actions.push(Action::ItemDelete);
        }
        if pressed(Key::Escape) {
            if self.gantt_item().is_some() {
                self.gantt_sel = None;
            } else {
                self.selected.clear();
            }
        }
        // arrows move a picked Gantt bar: left/right by a day (Shift: a week), up/down to the next activity
        if let Some((id, row, bi)) = self.gantt_item() {
            let mut dt = 0.0;
            let mut drow = 0;
            if pressed(Key::ArrowLeft) {
                dt -= 1.0;
            }
            if pressed(Key::ArrowRight) {
                dt += 1.0;
            }
            if pressed(Key::ArrowUp) {
                drow -= 1;
            }
            if pressed(Key::ArrowDown) {
                drow += 1;
            }
            if dt != 0.0 || drow != 0 {
                if let Some(g) = self.gantt_mut(id) {
                    // Shift: a whole week (or period)
                    let step = if shift { 1.0 } else { g.snap_step() };
                    let (r, b) = nudge_bar(g, row, bi, dt * step, drow);
                    let status = format!("{}: {}", g.tasks[r].label, g.date_label(g.tasks[r].bars[b].start));
                    self.gantt_sel = Some((id, r, b));
                    self.status = status;
                }
            }
            return;
        }
        let step = if shift { 10.0 } else { 1.0 };
        let mut d = Vec2::ZERO;
        if pressed(Key::ArrowLeft) {
            d.x -= step;
        }
        if pressed(Key::ArrowRight) {
            d.x += step;
        }
        if pressed(Key::ArrowUp) {
            d.y -= step;
        }
        if pressed(Key::ArrowDown) {
            d.y += step;
        }
        if d != Vec2::ZERO {
            if self.selected.is_empty() {
                // arrows change slide when nothing is selected
                if d.y > 0.0 || d.x > 0.0 {
                    self.current = (self.current + 1).min(self.deck.slides.len() - 1);
                } else {
                    self.current = self.current.saturating_sub(1);
                }
            } else {
                let sel = self.selected.clone();
                for e in self.slide().elements.iter_mut().filter(|e| sel.contains(&e.id)) {
                    e.x += d.x;
                    e.y += d.y;
                }
            }
        }
    }

    fn paste_window(&mut self, ctx: &egui::Context) {
        let mut open = self.paste.is_some();
        let mut apply = None;
        if let Some((text, err)) = &mut self.paste {
            egui::Window::new("Paste data from Excel")
                .open(&mut open)
                .collapsible(false)
                .default_width(460.0)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(keys("Copy cells in Excel (or any spreadsheet) and paste them here with Cmd+V.\nFirst row: column names. First column: series names."));
                    ui.add(egui::TextEdit::multiline(text).desired_rows(10).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace).hint_text("\t2023\t2024\nEurope\t42\t48\nAsia\t14\t19"));
                    if !err.is_empty() {
                        ui.colored_label(Color32::from_rgb(190, 50, 40), err.as_str());
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Use this data").clicked() {
                            apply = Some(text.clone());
                        }
                    });
                });
        }
        if let Some(t) = apply {
            self.apply_table(&t, "the pasted table");
            open = false;
        }
        if !open {
            self.paste = None;
        }
    }
}

impl eframe::App for ThinkCraft {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frame += 1;
        draw::DECIMAL_COMMA.store(self.deck.decimal_comma, std::sync::atomic::Ordering::Relaxed);
        if let Some(path) = &self.screenshot {
            ctx.request_repaint();
            if self.frame == 120 && self.shot_use.is_none() {
                let p = path.clone();
                let slot = self.shot.clone();
                self.shot_use = Some(ShotUse::Exit(p));
                let c2 = ctx.clone();
                ctx.request_screenshot(move |img| {
                    *slot.lock().unwrap() = Some(img);
                    c2.request_repaint();
                });
            }
            if self.frame > 900 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        self.handle_shot(&ctx);
        if !self.batch.is_empty() {
            ctx.request_repaint();
            if self.frame == 3 {
                let all: Vec<usize> = match std::env::var("THINKCRAFT_SLIDE").ok().and_then(|s| s.parse::<usize>().ok()) {
                    Some(n) if n >= 1 && n <= self.deck.slides.len() => vec![n - 1],
                    _ => (0..self.deck.slides.len()).collect(),
                };
                for (pptx, path) in std::mem::take(&mut self.batch) {
                    let res = if pptx {
                        office::write_pptx(&path, &self.deck, &all, ui.painter())
                    } else {
                        office::write_xlsx(&path, &office::deck_sheets(&self.deck, &all, None))
                    };
                    match res {
                        Ok(_) => println!("Wrote {}", path.display()),
                        Err(e) => eprintln!("Couldn't write {}: {e}", path.display()),
                    }
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        self.shortcuts(&ctx);

        // window title
        let dirty = self.deck != self.saved;
        let title = format!("{}{} — ThinkCraft", if dirty { "• " } else { "" }, self.path.as_deref().map(file_name).unwrap_or_else(|| self.deck.name.clone()));
        if title != self.title_shown {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_shown = title;
        }

        egui::Panel::top("menu").frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(egui::Margin::symmetric(8, 2))).show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("ribbon")
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(egui::Margin::symmetric(8, 4)).stroke(Stroke::new(1.0, Color32::from_rgb(222, 224, 230))))
            .show(ui, |ui| self.ribbon(ui));
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(Color32::from_rgb(238, 239, 243)).inner_margin(egui::Margin::symmetric(10, 3)))
            .show(ui, |ui| self.status_bar(ui));

        let sheet_idx = self.single().filter(|i| {
            self.show_datasheet && matches!(self.deck.slides[self.current].elements[*i].kind, Kind::Chart(_) | Kind::Gantt(_) | Kind::Table(_))
        });
        egui::Panel::left("slides")
            .resizable(true)
            .default_size(176.0)
            .size_range(120.0..=320.0)
            .frame(egui::Frame::new().fill(PANEL_BG).inner_margin(egui::Margin::symmetric(8, 4)))
            .show(ui, |ui| self.slides_panel(ui));
        egui::Panel::right("props")
            .resizable(true)
            .default_size(300.0)
            .size_range(240.0..=460.0)
            .frame(egui::Frame::new().fill(PANEL_BG).inner_margin(egui::Margin::symmetric(10, 4)))
            .show(ui, |ui| self.props_panel(ui));
        if let Some(i) = sheet_idx {
            egui::Panel::bottom("datasheet")
                .resizable(true)
                .default_size(210.0)
                .size_range(120.0..=520.0)
                .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(egui::Margin::symmetric(10, 4)).stroke(Stroke::new(1.0, Color32::from_rgb(222, 224, 230))))
                .show(ui, |ui| self.datasheet(ui, i));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| self.canvas(ui));

        self.paste_window(&ctx);
        self.calendar_window(&ctx);
        self.elements_popup(&ctx);
        if self.about {
            egui::Window::new("About ThinkCraft").open(&mut self.about).collapsible(false).resizable(false).show(&ctx, |ui| {
                ui.heading("ThinkCraft");
                ui.label("Business charts and slide layouts: stacked, clustered, 100%, bar, line, area, waterfall, Mekko, pie and doughnut charts, Gantt timelines, Harvey balls, check marks, process flows and agendas.");
                ui.label("Send your work to PowerPoint and Excel, or to any app that opens .pptx, .xlsx or pictures.");
                ui.label(RichText::new("Version 0.1").color(Color32::GRAY));
            });
        }

        self.run_actions(ui);

        // undo history: record a step when the person lets go
        let idle = !ctx.input(|i| i.pointer.any_down()) && ctx.memory(|m| m.focused().is_none()) && matches!(self.drag, Drag::None);
        if idle && self.deck != self.committed {
            self.undo.push(std::mem::replace(&mut self.committed, self.deck.clone()));
            if self.undo.len() > 200 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        // keep the selection valid
        let ids: Vec<u64> = self.deck.slides[self.current.min(self.deck.slides.len() - 1)].elements.iter().map(|e| e.id).collect();
        self.selected.retain(|s| ids.contains(s));
        self.gantt_sel = self.gantt_item();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if self.screenshot.is_some() {
            return;
        }
        eframe::set_value(storage, "deck", &self.deck);
        eframe::set_value(storage, "path", &self.path);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        PANEL_BG.to_normalized_gamma_f32()
    }
}

// ---------------------------------------------------------------------------

const FULL_MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// The little picture on a round-menu button, centred on `c`. `on` marks a column (or the
/// datasheet) that is shown now, so the button removes it.
fn radial_glyph(p: &egui::Painter, c: Pos2, cmd: RadialCmd, on: bool, enabled: bool) {
    let fade = |col: Color32| if enabled { col } else { col.gamma_multiply(0.35) };
    let ink = fade(Color32::from_rgb(78, 80, 88));
    let light = fade(Color32::from_rgb(150, 152, 160));
    let red = fade(Color32::from_rgb(214, 36, 56));
    let green = fade(Color32::from_rgb(34, 132, 74));
    let blue = fade(Color32::from_rgb(32, 74, 140));
    let thin = Stroke::new(1.2, ink);
    let v = Vec2::new;
    let letter = |at: Pos2, s: &str, col: Color32, size: f32| {
        p.text(at, Align2::CENTER_CENTER, s, FontId::proportional(size), col);
    };
    // small round badge: × to remove, + to add
    let badge = |remove: bool| {
        let b = c + v(7.5, 7.5);
        p.circle_filled(b, 4.6, if remove { ink } else { green });
        let k = 2.2;
        let st = Stroke::new(1.3, Color32::WHITE);
        if remove {
            p.line_segment([b + v(-k, -k), b + v(k, k)], st);
            p.line_segment([b + v(-k, k), b + v(k, -k)], st);
        } else {
            p.line_segment([b + v(-k, 0.0), b + v(k, 0.0)], st);
            p.line_segment([b + v(0.0, -k), b + v(0.0, k)], st);
        }
    };
    let arrow2 = |a: Pos2, b: Pos2| {
        p.line_segment([a, b], thin);
        let d = (b - a).normalized() * 3.0;
        let n = d.rot90();
        p.line_segment([a, a + d + n], thin);
        p.line_segment([a, a + d - n], thin);
        p.line_segment([b, b - d + n], thin);
        p.line_segment([b, b - d - n], thin);
    };
    match cmd {
        RadialCmd::Delete => {
            let st = Stroke::new(3.0, red);
            p.line_segment([c + v(-6.0, -6.0), c + v(6.0, 6.0)], st);
            p.line_segment([c + v(-6.0, 6.0), c + v(6.0, -6.0)], st);
        }
        RadialCmd::Duplicate => {
            p.rect_stroke(Rect::from_min_size(c + v(-7.0, -7.0), v(9.0, 9.0)), 1.0, thin, StrokeKind::Middle);
            p.rect_filled(Rect::from_min_size(c + v(-2.0, -2.0), v(9.0, 9.0)), 1.0, Color32::from_rgb(236, 236, 239));
            p.rect_stroke(Rect::from_min_size(c + v(-2.0, -2.0), v(9.0, 9.0)), 1.0, thin, StrokeKind::Middle);
        }
        RadialCmd::SameWidth => {
            p.line_segment([c + v(-8.0, -7.0), c + v(-8.0, 7.0)], thin);
            p.line_segment([c + v(8.0, -7.0), c + v(8.0, 7.0)], thin);
            arrow2(c + v(-7.0, 0.0), c + v(7.0, 0.0));
        }
        RadialCmd::SameHeight => {
            p.line_segment([c + v(-7.0, -8.0), c + v(7.0, -8.0)], thin);
            p.line_segment([c + v(-7.0, 8.0), c + v(7.0, 8.0)], thin);
            arrow2(c + v(0.0, -7.0), c + v(0.0, 7.0));
        }
        RadialCmd::Activity | RadialCmd::Responsible | RadialCmd::Remark => {
            // a column divider with the column's letter on its side, and text lines below
            // activity: "A" left of the divider; responsible and remark: two "R" columns right
            // of it, the red one being the column this button adds or removes
            let x_line = if cmd == RadialCmd::Activity { 2.0 } else { -6.0 };
            p.line_segment([c + v(x_line, -9.0), c + v(x_line, 8.0)], thin);
            p.line_segment([c + v(-9.0, -1.0), c + v(9.0, -1.0)], thin);
            match cmd {
                RadialCmd::Activity => letter(c + v(-3.5, -5.5), "A", red, 10.5),
                RadialCmd::Responsible => {
                    letter(c + v(-1.0, -5.5), "R", red, 10.5);
                    letter(c + v(6.0, -5.5), "R", light, 10.5);
                }
                _ => {
                    letter(c + v(-1.0, -5.5), "R", light, 10.5);
                    letter(c + v(6.0, -5.5), "R", red, 10.5);
                }
            }
            for k in 0..3 {
                let y = 2.5 + k as f32 * 2.6;
                let (a, b) = if cmd == RadialCmd::Activity { (-8.0, x_line - 2.0) } else { (x_line + 2.0, 8.0) };
                p.line_segment([c + v(a, y), c + v(b, y)], Stroke::new(1.0, light));
            }
            badge(on);
        }
        RadialCmd::Fit => {
            for (k, w) in [(-3.5, 9.0), (0.0, 12.0), (3.5, 7.0)] {
                p.line_segment([c + v(-w / 2.0, k), c + v(w / 2.0, k)], Stroke::new(1.4, light));
            }
            p.add(egui::Shape::convex_polygon(vec![c + v(-10.5, -3.0), c + v(-7.0, 0.0), c + v(-10.5, 3.0)], red, Stroke::NONE));
            p.add(egui::Shape::convex_polygon(vec![c + v(10.5, -3.0), c + v(10.5, 3.0), c + v(7.0, 0.0)], red, Stroke::NONE));
        }
        RadialCmd::Calendar => {
            let r = Rect::from_center_size(c + v(0.0, 1.0), v(17.0, 15.0));
            p.rect_filled(r, 2.0, ink);
            p.rect_filled(Rect::from_min_size(r.min, v(17.0, 3.5)), 2.0, fade(Color32::from_rgb(50, 52, 58)));
            for x in [-4.5, 4.5] {
                p.line_segment([c + v(x, -9.0), c + v(x, -5.0)], Stroke::new(1.6, ink));
            }
            for row in 0..3 {
                for col in 0..4 {
                    let dot = c + v(-5.4 + col as f32 * 3.6, -0.5 + row as f32 * 3.4);
                    let col_ = if (row, col) == (1, 3) { red } else { Color32::WHITE };
                    p.rect_filled(Rect::from_center_size(dot, v(2.0, 2.0)), 0.0, col_);
                }
            }
        }
        RadialCmd::AddBar => {
            p.rect_filled(Rect::from_center_size(c + v(-1.5, -1.0), v(16.0, 6.0)), 1.0, blue);
            badge(false);
        }
        RadialCmd::AddMilestone => {
            let m = c + v(-1.5, -1.5);
            let d = 6.5;
            p.add(egui::Shape::convex_polygon(vec![m + v(0.0, -d), m + v(d, 0.0), m + v(0.0, d), m + v(-d, 0.0)], blue, Stroke::NONE));
            badge(false);
        }
        RadialCmd::PasteHere => {
            let r = Rect::from_center_size(c + v(0.0, 1.0), v(14.0, 17.0));
            p.rect_stroke(r, 2.0, thin, StrokeKind::Middle);
            p.rect_filled(Rect::from_center_size(Pos2::new(c.x, r.top()), v(7.0, 4.0)), 1.0, ink);
            p.rect_filled(Rect::from_center_size(c + v(0.0, 2.0), v(9.0, 3.5)), 1.0, blue);
        }
        RadialCmd::Excel => {
            let r = Rect::from_center_size(c, v(16.0, 16.0));
            p.rect_filled(r, 2.0, green);
            for k in 1..3 {
                let x = r.left() + k as f32 * 16.0 / 3.0;
                let y = r.top() + k as f32 * 16.0 / 3.0;
                p.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(1.0, Color32::WHITE));
                p.line_segment([Pos2::new(r.left(), y), Pos2::new(r.right(), y)], Stroke::new(1.0, Color32::WHITE));
            }
        }
        RadialCmd::Datasheet => {
            let r = Rect::from_center_size(c, v(18.0, 14.0));
            p.rect_stroke(r, 1.0, thin, StrokeKind::Middle);
            p.rect_filled(Rect::from_min_size(r.min, v(18.0, 4.0)), 1.0, light);
            for k in 1..3 {
                let x = r.left() + k as f32 * 6.0;
                p.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(1.0, light));
            }
            p.line_segment([Pos2::new(r.left(), r.center().y + 2.0), Pos2::new(r.right(), r.center().y + 2.0)], Stroke::new(1.0, light));
            if on {
                badge(true);
            }
        }
    }
}

fn annotation_row(ui: &mut egui::Ui, label: &str, id: &str, value: &mut Option<(usize, usize)>, cats: &[String], last: usize) {
    let mut on = value.is_some();
    ui.checkbox(&mut on, label);
    if on {
        let (mut a, mut b) = value.unwrap_or((0, last));
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.label("from");
            egui::ComboBox::from_id_salt((id, 0)).width(70.0).selected_text(cats[a.min(last)].clone()).show_ui(ui, |ui| {
                for (i, c) in cats.iter().enumerate() {
                    ui.selectable_value(&mut a, i, c);
                }
            });
            ui.label("to");
            egui::ComboBox::from_id_salt((id, 1)).width(70.0).selected_text(cats[b.min(last)].clone()).show_ui(ui, |ui| {
                for (i, c) in cats.iter().enumerate() {
                    ui.selectable_value(&mut b, i, c);
                }
            });
        });
        if a == b {
            b = if a == last { 0 } else { last };
        }
        *value = Some((a, b));
    } else {
        *value = None;
    }
}

fn list_editor(ui: &mut egui::Ui, items: &mut Vec<String>, noun: &str) {
    let mut remove = None;
    for (i, it) in items.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{}.", i + 1));
            ui.add(egui::TextEdit::singleline(it).desired_width(ui.available_width() - 30.0));
            if ui.small_button("×").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        items.remove(i);
    }
    if ui.button(format!("+ {noun}")).clicked() {
        items.push(format!("{noun} {}", items.len() + 1));
    }
}

fn handles(r: Rect) -> [Pos2; 8] {
    let (l, t, rr, b) = (r.left(), r.top(), r.right(), r.bottom());
    let (cx, cy) = (r.center().x, r.center().y);
    [
        Pos2::new(l, t),
        Pos2::new(cx, t),
        Pos2::new(rr, t),
        Pos2::new(rr, cy),
        Pos2::new(rr, b),
        Pos2::new(cx, b),
        Pos2::new(l, b),
        Pos2::new(l, cy),
    ]
}

fn handle_cursor(h: usize) -> CursorIcon {
    match h {
        0 | 4 => CursorIcon::ResizeNwSe,
        2 | 6 => CursorIcon::ResizeNeSw,
        1 | 5 => CursorIcon::ResizeVertical,
        _ => CursorIcon::ResizeHorizontal,
    }
}

/// Finds the closest guide to any of the candidate positions.
fn snap(candidates: &[f32], targets: &[f32], thresh: f32) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    for c in candidates {
        for t in targets {
            let d = t - c;
            if d.abs() <= thresh && best.map(|(b, _)| d.abs() < b.abs()).unwrap_or(true) {
                best = Some((d, *t));
            }
        }
    }
    best
}

fn load_deck(p: &Path) -> Result<Deck, String> {
    let s = std::fs::read_to_string(p).map_err(|e| format!("Couldn't open {}: {e}", file_name(p)))?;
    let mut d: Deck = serde_json::from_str(&s).map_err(|e| format!("{} isn't a ThinkCraft deck: {e}", file_name(p)))?;
    if d.slides.is_empty() {
        d.slides.push(Slide { title: String::new(), footnote: String::new(), elements: vec![] });
    }
    let max_id = d.slides.iter().flat_map(|s| s.elements.iter().map(|e| e.id)).max().unwrap_or(0);
    d.next_id = d.next_id.max(max_id);
    Ok(d)
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

fn safe_name(s: &str) -> String {
    let t: String = s.chars().map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c }).collect();
    if t.trim().is_empty() {
        "ThinkCraft".into()
    } else {
        t.trim().to_string()
    }
}

fn temp_path(name: &str, ext: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ThinkCraft");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{}.{ext}", safe_name(name)))
}

/// Opens a file in the named Office app, or in whatever app the system uses
/// for that kind of file.
fn open_with(p: &Path, mac_app: &str, short: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        let ok = std::process::Command::new("open").arg("-a").arg(mac_app).arg(p).status().map(|s| s.success()).unwrap_or(false);
        if ok {
            return format!("Opened in {short}.");
        }
        let ok = std::process::Command::new("open").arg(p).status().map(|s| s.success()).unwrap_or(false);
        if ok {
            return format!("{short} wasn't found, so the file opened in your default app.");
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let _ = mac_app;
        // `start "" "<path>"` opens the file in its default app (PowerPoint, Excel).
        // The path is always quoted so spaces and characters like & are safe,
        // and CREATE_NO_WINDOW keeps a console window from flashing up.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let ok = std::process::Command::new("cmd")
            .raw_arg(format!("/C start \"\" \"{}\"", p.display()))
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return format!("Opened in {short}.");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = mac_app;
        let ok = std::process::Command::new("xdg-open").arg(p).status().map(|s| s.success()).unwrap_or(false);
        if ok {
            return format!("Opened the file for {short}.");
        }
    }
    format!("Saved {} but couldn't open it automatically.", p.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gantt_hits_bars_ends_and_date_line() {
        let mut g = Gantt::calendar(2026, 11, 2, 13);
        g.today = Some(6.5);
        g.today_fixed = true;
        let el = Element { id: 1, x: 40.0, y: 92.0, w: 880.0, h: 390.0, kind: Kind::Gantt(g.clone()) };
        let xf = Xf { origin: Pos2::ZERO, scale: 1.0 };
        let geo = draw::GanttGeo::new(xf.el_rect(&el), 1.0, &g);
        // "Build" runs weeks 4..9 in row 3
        let b = geo.bar_rect(&g.tasks[3].bars[0], 3);
        assert_eq!(gantt_hit(&el, &xf, b.center()), Some(GanttHit::Bar(3, 0, BarPart::Move)));
        assert_eq!(gantt_hit(&el, &xf, Pos2::new(b.right() - 1.0, b.center().y)), Some(GanttHit::Bar(3, 0, BarPart::End)));
        assert_eq!(gantt_hit(&el, &xf, Pos2::new(b.left() + 1.0, b.center().y)), Some(GanttHit::Bar(3, 0, BarPart::Start)));
        // "Design" (row 2) holds a bar and a milestone; each can be grabbed on its own
        let m = geo.bar_rect(&g.tasks[2].bars[1], 2);
        assert_eq!(gantt_hit(&el, &xf, m.center()), Some(GanttHit::Bar(2, 1, BarPart::Move)));
        let d = geo.bar_rect(&g.tasks[2].bars[0], 2);
        assert_eq!(gantt_hit(&el, &xf, d.center()), Some(GanttHit::Bar(2, 0, BarPart::Move)));
        // the date line sits at week 6.5 and stays there once dragged
        let x = geo.x(6.5);
        assert_eq!(gantt_hit(&el, &xf, Pos2::new(x, geo.bottom + 10.0)), Some(GanttHit::Today));
        assert_eq!(g.today_line().map(|(t, _)| t), Some(6.5));
        // the activity names are not a bar
        assert_eq!(gantt_hit(&el, &xf, Pos2::new(60.0, geo.row_center(0))), None);
        assert_eq!(g.date_label(6.5), "18.12.2026");
    }

    #[test]
    fn older_files_with_one_bar_per_row_still_open() {
        let old = r#"{"label":"Build","start":4.0,"end":9.0,"milestone":false,"color":1}"#;
        let t: Task = serde_json::from_str(old).unwrap();
        assert_eq!(t.bars.len(), 1);
        assert_eq!(t.bars[0].end, 9.0);
        // and the new layout survives a save and load
        let mut t2 = t.clone();
        t2.bars.push(Bar { start: 10.0, end: 10.0, milestone: true, dashed: false, color: 0 });
        let back: Task = serde_json::from_str(&serde_json::to_string(&t2).unwrap()).unwrap();
        assert_eq!(back, t2);
    }

    #[test]
    fn copied_gantt_bar_pastes_on_top_and_can_be_moved() {
        let mut g = Gantt::calendar(2026, 11, 2, 13);
        // copy "Build" (row 3) through the clipboard text and paste it back
        let clip = Clip::GanttItem { row: 3, bar: g.tasks[3].bars[0].clone() };
        let Some(Clip::GanttItem { row, bar }) = Clip::from_text(&clip.to_text()) else { panic!("clipboard text didn't read back") };
        assert_eq!(Clip::from_text("some text from Excel"), None);
        let (r, b) = paste_bar(&mut g, row, bar, None);
        assert_eq!((r, b), (3, 1));
        assert_eq!(g.tasks[3].bars[1], g.tasks[3].bars[0]);
        // the copy is on top, so it is the one grabbed there
        let el = Element { id: 1, x: 40.0, y: 92.0, w: 880.0, h: 390.0, kind: Kind::Gantt(g.clone()) };
        let xf = Xf { origin: Pos2::ZERO, scale: 1.0 };
        let geo = draw::GanttGeo::new(xf.el_rect(&el), 1.0, &g);
        assert_eq!(gantt_hit(&el, &xf, geo.bar_rect(&g.tasks[3].bars[1], 3).center()), Some(GanttHit::Bar(3, 1, BarPart::Move)));
        // move it a week right and into "Test" below
        let (r, b) = nudge_bar(&mut g, 3, 1, 1.0, 1);
        assert_eq!((r, b), (4, 1));
        assert_eq!((g.tasks[4].bars[1].start, g.tasks[4].bars[1].end), (5.0, 10.0));
        assert_eq!(g.tasks[3].bars.len(), 1);
        // pasting at a spot keeps the length and stays inside the timeline
        let moved = g.tasks[4].bars[1].clone();
        let (r, b) = paste_bar(&mut g, 0, moved, Some(12.0));
        assert_eq!((g.tasks[r].bars[b].start, g.tasks[r].bars[b].end), (8.0, 13.0));
        // a milestone stays a milestone
        let kickoff = g.tasks[0].bars[0].clone();
        let (r, b) = paste_bar(&mut g, 99, kickoff, Some(2.3));
        assert_eq!(r, 5);
        let m = &g.tasks[r].bars[b];
        assert!(m.milestone && m.start == m.end && (m.start - 16.0 / 7.0).abs() < 1e-4);
    }

    #[test]
    fn label_columns_fit_scale_and_calendar() {
        let mut g = Gantt::calendar(2026, 11, 2, 13);
        let r = Rect::from_min_size(Pos2::ZERO, Vec2::new(880.0, 390.0));
        let plain = draw::GanttGeo::new(r, 1.0, &g);
        // responsible and remark columns both go right of the timeline
        g.show_responsible = true;
        g.show_remark = true;
        g.tasks[3].responsible = "Ana".into();
        g.tasks[3].remark = "On budget".into();
        let both = draw::GanttGeo::new(r, 1.0, &g);
        assert!(both.grid.left() == plain.grid.left() && both.grid.right() < plain.grid.right());
        assert_eq!(both.grid.right(), r.right() - both.responsible_w - both.remark_w);
        // they are saved, and older files without them still open
        let back: Gantt = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back, g);
        let mut v: serde_json::Value = serde_json::to_value(&g).unwrap();
        for k in ["show_activity", "show_responsible", "show_remark", "responsible_header", "remark_header"] {
            v.as_object_mut().unwrap().remove(k);
        }
        let old: Gantt = serde_json::from_value(v).unwrap();
        assert!(old.show_activity && !old.show_responsible && old.responsible_header == "Responsible");
        // fit scale to data: bars run from week 0 to 11.5, so 12 weeks from the same Monday
        assert!(g.fit_to_data());
        assert_eq!((g.weeks, g.start), (12, Some([2026, 11, 2])));
        // starting two weeks later keeps every date: "Build" (weeks 4..9) becomes 2..7
        g.set_start_keep_dates(days_from_civil(2026, 11, 18));
        assert_eq!(g.start, Some([2026, 11, 16]));
        assert_eq!((g.tasks[3].bars[0].start, g.tasks[3].bars[0].end), (2.0, 7.0));
        // the kick-off milestone now falls before the start: it keeps its date and is hidden
        assert_eq!(g.tasks[0].bars[0].start, -2.0);
        let geo = draw::GanttGeo::new(r, 1.0, &g);
        assert!(!geo.visible(&g.tasks[0].bars[0]) && geo.visible(&g.tasks[3].bars[0]));
        // picking Wednesday 4.2.2026 to Monday 15.2.2027 on the calendar shows weeks 06/2026 to 07/2027
        g.set_period(days_from_civil(2027, 2, 15), days_from_civil(2026, 2, 4));
        assert_eq!((g.start, g.weeks), (Some([2026, 2, 2]), 55));
        // and the kick-off (2.11.2026) is back in view, 39 weeks in
        assert_eq!(g.tasks[0].bars[0].start, 39.0);
    }
}

/// Shortcut names as this computer shows them: Cmd/⌘/Option on a Mac, Ctrl/Alt elsewhere.
/// Text is written with Mac names and passed through here before it is shown.
fn keys(text: &str) -> String {
    if cfg!(target_os = "macos") {
        return text.to_string();
    }
    text.replace("Shift+Cmd+", "Ctrl+Shift+")
        .replace("Cmd+", "Ctrl+")
        .replace('⌘', "Ctrl+")
        .replace("Option", "Alt")
}

#[cfg(test)]
mod key_label_tests {
    use super::keys;

    #[test]
    fn shortcut_labels_follow_the_platform() {
        let shown = keys("Undo  (Cmd+Z) · Redo  (Shift+Cmd+Z) · ⌘C · hold Option");
        if cfg!(target_os = "macos") {
            assert_eq!(shown, "Undo  (Cmd+Z) · Redo  (Shift+Cmd+Z) · ⌘C · hold Option");
        } else {
            assert_eq!(shown, "Undo  (Ctrl+Z) · Redo  (Ctrl+Shift+Z) · Ctrl+C · hold Alt");
        }
    }
}
