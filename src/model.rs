//! The document: a deck of slides, each with charts and layout elements.
//! Slide coordinates are in "slide units": a slide is 960 x 540.

use serde::{Deserialize, Serialize};

pub const SLIDE_W: f32 = 960.0;
pub const SLIDE_H: f32 = 540.0;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Deck {
    pub name: String,
    pub style: usize,
    #[serde(default)]
    pub decimal_comma: bool,
    pub slides: Vec<Slide>,
    #[serde(default)]
    pub next_id: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Slide {
    pub title: String,
    #[serde(default)]
    pub footnote: String,
    pub elements: Vec<Element>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Element {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub kind: Kind,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub enum Kind {
    Chart(Chart),
    Gantt(Gantt),
    Text(TextBox),
    Harvey(Harvey),
    Check(CheckMark),
    Process(Process),
    Agenda(Agenda),
    Shape(ShapeBox),
    Table(Table),
}

impl Kind {
    pub fn label(&self) -> &'static str {
        match self {
            Kind::Chart(c) => c.kind.label(),
            Kind::Gantt(_) => "Gantt chart",
            Kind::Text(_) => "Text box",
            Kind::Harvey(_) => "Harvey ball",
            Kind::Check(_) => "Check mark",
            Kind::Process(_) => "Process flow",
            Kind::Agenda(_) => "Agenda",
            Kind::Shape(s) => match s.shape {
                ShapeKind::Rounded => "Rounded rectangle",
                ShapeKind::Pentagon => "Pentagon",
                ShapeKind::Chevron => "Chevron",
            },
            Kind::Table(_) => "Table",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChartKind {
    Stacked,
    Clustered,
    Percent,
    Bar,
    Line,
    Area,
    Waterfall,
    Mekko,
    Pie,
    Doughnut,
    Combination,
    AreaPercent,
    Scatter,
    Bubble,
}

impl ChartKind {
    pub const ALL: [ChartKind; 14] = [
        ChartKind::Stacked,
        ChartKind::Clustered,
        ChartKind::Percent,
        ChartKind::Bar,
        ChartKind::Line,
        ChartKind::Area,
        ChartKind::Waterfall,
        ChartKind::Mekko,
        ChartKind::Pie,
        ChartKind::Doughnut,
        ChartKind::Combination,
        ChartKind::AreaPercent,
        ChartKind::Scatter,
        ChartKind::Bubble,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            ChartKind::Stacked => "Stacked",
            ChartKind::Clustered => "Clustered",
            ChartKind::Percent => "100%",
            ChartKind::Bar => "Bar",
            ChartKind::Line => "Line",
            ChartKind::Area => "Area",
            ChartKind::Waterfall => "Waterfall",
            ChartKind::Mekko => "Mekko",
            ChartKind::Pie => "Pie",
            ChartKind::Doughnut => "Doughnut",
            ChartKind::Combination => "Combination",
            ChartKind::AreaPercent => "100% Area",
            ChartKind::Scatter => "Scatter",
            ChartKind::Bubble => "Bubble",
        }
    }
    pub fn uses_value_axis(&self) -> bool {
        matches!(
            self,
            ChartKind::Stacked
                | ChartKind::Clustered
                | ChartKind::Line
                | ChartKind::Area
                | ChartKind::Waterfall
                | ChartKind::Combination
        )
    }
    pub fn is_xy(&self) -> bool {
        matches!(self, ChartKind::Scatter | ChartKind::Bubble)
    }
    pub fn supports_annotations(&self) -> bool {
        matches!(
            self,
            ChartKind::Stacked | ChartKind::Clustered | ChartKind::Line | ChartKind::Area
        )
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Chart {
    pub kind: ChartKind,
    pub title: String,
    pub categories: Vec<String>,
    pub series: Vec<Series>,
    /// Waterfall: which categories are computed subtotals ("=" columns).
    #[serde(default)]
    pub subtotal: Vec<bool>,
    pub labels: bool,
    pub totals: bool,
    pub legend: bool,
    pub axis: bool,
    #[serde(default)]
    pub connectors: bool,
    pub decimals: usize,
    #[serde(default)]
    pub unit: String,
    pub gap: f32,
    /// Growth-rate bubble between two categories.
    #[serde(default)]
    pub cagr: Option<(usize, usize)>,
    /// Difference arrow between two category totals.
    #[serde(default)]
    pub diff: Option<(usize, usize)>,
    /// Pie / doughnut: which category column is shown.
    #[serde(default)]
    pub pie_category: usize,
}

impl Chart {
    pub fn new(kind: ChartKind) -> Self {
        let mut c = Chart {
            kind,
            title: String::new(),
            categories: vec!["2022".into(), "2023".into(), "2024".into(), "2025".into()],
            series: vec![
                Series { name: "Product A".into(), values: vec![12.0, 14.0, 17.0, 19.0], color: None },
                Series { name: "Product B".into(), values: vec![8.0, 9.0, 11.0, 14.0], color: None },
                Series { name: "Product C".into(), values: vec![5.0, 6.0, 6.0, 8.0], color: None },
            ],
            subtotal: vec![false; 4],
            labels: true,
            totals: true,
            legend: true,
            axis: false,
            connectors: false,
            decimals: 0,
            unit: String::new(),
            gap: 0.45,
            cagr: None,
            diff: None,
            pie_category: 0,
        };
        match kind {
            ChartKind::Waterfall => {
                c.categories = vec![
                    "Revenue 2024".into(),
                    "Price".into(),
                    "Volume".into(),
                    "Mix".into(),
                    "Costs".into(),
                    "Revenue 2025".into(),
                ];
                c.series = vec![Series {
                    name: "Change".into(),
                    values: vec![100.0, 12.0, 18.0, -6.0, -9.0, 0.0],
                    color: None,
                }];
                c.subtotal = vec![false, false, false, false, false, true];
                c.legend = false;
                c.totals = false;
                c.connectors = true;
            }
            ChartKind::Mekko => {
                c.categories = vec!["North".into(), "South".into(), "East".into(), "West".into()];
                c.series = vec![
                    Series { name: "Us".into(), values: vec![40.0, 20.0, 15.0, 10.0], color: None },
                    Series { name: "Rival 1".into(), values: vec![30.0, 25.0, 10.0, 12.0], color: None },
                    Series { name: "Rival 2".into(), values: vec![20.0, 10.0, 12.0, 6.0], color: None },
                    Series { name: "Others".into(), values: vec![10.0, 15.0, 8.0, 7.0], color: None },
                ];
            }
            ChartKind::Pie | ChartKind::Doughnut => {
                c.categories = vec!["Share".into()];
                c.series = vec![
                    Series { name: "Online".into(), values: vec![46.0], color: None },
                    Series { name: "Retail".into(), values: vec![31.0], color: None },
                    Series { name: "Partners".into(), values: vec![15.0], color: None },
                    Series { name: "Other".into(), values: vec![8.0], color: None },
                ];
                c.totals = false;
            }
            ChartKind::Line => {
                c.totals = false;
                c.axis = true;
            }
            ChartKind::Clustered => {
                c.totals = false;
            }
            ChartKind::Combination => {
                c.series.push(Series { name: "Margin".into(), values: vec![22.0, 25.0, 27.0, 31.0], color: None });
                c.totals = false;
                c.axis = true;
            }
            ChartKind::AreaPercent => {
                c.totals = false;
            }
            ChartKind::Scatter | ChartKind::Bubble => {
                c.categories = if kind == ChartKind::Bubble {
                    vec!["X".into(), "Y".into(), "Size".into()]
                } else {
                    vec!["X".into(), "Y".into()]
                };
                c.series = [("Alpha", 2.0, 30.0, 12.0), ("Beta", 4.5, 55.0, 30.0), ("Gamma", 6.0, 42.0, 8.0), ("Delta", 8.0, 70.0, 20.0), ("Epsilon", 3.5, 64.0, 15.0)]
                    .iter()
                    .map(|(n, x, y, z)| Series {
                        name: n.to_string(),
                        values: if kind == ChartKind::Bubble { vec![*x, *y, *z] } else { vec![*x, *y] },
                        color: None,
                    })
                    .collect();
                c.legend = false;
                c.totals = false;
                c.axis = true;
            }
            _ => {}
        }
        c.subtotal.resize(c.categories.len(), false);
        c
    }

    pub fn build_down() -> Self {
        let mut c = Chart::new(ChartKind::Waterfall);
        c.categories = vec!["Revenue".into(), "Materials".into(), "Staff".into(), "Overheads".into(), "Tax".into(), "Profit".into()];
        c.series[0].values = vec![120.0, -38.0, -31.0, -14.0, -9.0, 0.0];
        c.subtotal = vec![false, false, false, false, false, true];
        c
    }

    /// Makes every series as long as the category list.
    pub fn normalize(&mut self) {
        let n = self.categories.len();
        for s in &mut self.series {
            s.values.resize(n, 0.0);
        }
        self.subtotal.resize(n, false);
        if let Some((a, b)) = self.cagr {
            if a >= n || b >= n || a == b {
                self.cagr = None;
            }
        }
        if let Some((a, b)) = self.diff {
            if a >= n || b >= n || a == b {
                self.diff = None;
            }
        }
        if self.pie_category >= n.max(1) {
            self.pie_category = 0;
        }
    }

    pub fn total(&self, cat: usize) -> f64 {
        self.series.iter().map(|s| s.values.get(cat).copied().unwrap_or(0.0)).sum()
    }

    pub fn add_category(&mut self) {
        let name = match self.categories.last().and_then(|c| c.parse::<i64>().ok()) {
            Some(y) => (y + 1).to_string(),
            None => format!("Cat {}", self.categories.len() + 1),
        };
        self.categories.push(name);
        self.normalize();
    }

    pub fn add_series(&mut self) {
        let n = self.categories.len();
        self.series.push(Series {
            name: format!("Series {}", self.series.len() + 1),
            values: vec![1.0; n],
            color: None,
        });
    }

    pub fn transpose(&mut self) {
        let old_cats = self.categories.clone();
        let old_series = self.series.clone();
        self.categories = old_series.iter().map(|s| s.name.clone()).collect();
        self.series = old_cats
            .iter()
            .enumerate()
            .map(|(ci, name)| Series {
                name: name.clone(),
                values: old_series.iter().map(|s| s.values.get(ci).copied().unwrap_or(0.0)).collect(),
                color: None,
            })
            .collect();
        self.cagr = None;
        self.diff = None;
        self.normalize();
    }

    /// Reads a pasted or imported table. First row: categories (first cell
    /// ignored). Each following row: series name, then values.
    pub fn load_table(&mut self, text: &str) -> Result<(), String> {
        let sep = if text.contains('\t') {
            '\t'
        } else if text.contains(';') {
            ';'
        } else {
            ','
        };
        let rows: Vec<Vec<String>> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split(sep).map(|c| c.trim().trim_matches('"').to_string()).collect())
            .collect();
        if rows.len() < 2 || rows[0].len() < 2 {
            return Err("Paste at least a header row and one data row, with names in the first column.".into());
        }
        let cats: Vec<String> = rows[0][1..].to_vec();
        let mut series = Vec::new();
        for r in &rows[1..] {
            let name = r.first().cloned().unwrap_or_default();
            let values = (0..cats.len())
                .map(|i| r.get(i + 1).map(|v| parse_number(v)).unwrap_or(0.0))
                .collect();
            series.push(Series { name, values, color: None });
        }
        self.categories = cats;
        self.series = series;
        self.cagr = None;
        self.diff = None;
        self.normalize();
        Ok(())
    }
}

pub fn parse_number(v: &str) -> f64 {
    let cleaned: String = v
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == ',')
        .collect();
    // "1,5" (decimal comma) vs "1,500" (thousands)
    let cleaned = if cleaned.contains('.') {
        cleaned.replace(',', "")
    } else if cleaned.matches(',').count() == 1 && cleaned.split(',').nth(1).map(|d| d.len() != 3).unwrap_or(false) {
        cleaned.replace(',', ".")
    } else {
        cleaned.replace(',', "")
    };
    cleaned.parse().unwrap_or(0.0)
}

/// One bar or milestone on a Gantt row.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Bar {
    pub start: f32,
    pub end: f32,
    pub milestone: bool,
    /// Drawn as a dashed outline (planned or optional work).
    #[serde(default)]
    pub dashed: bool,
    #[serde(default)]
    pub color: usize,
}

/// A Gantt row (activity). It can hold any number of bars and milestones.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(from = "TaskRaw")]
pub struct Task {
    pub label: String,
    pub bars: Vec<Bar>,
    /// Who is responsible (shown when the chart has a responsible column).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub responsible: String,
    /// A remark (shown when the chart has a remark column, right of the timeline).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remark: String,
}

/// Reads both the current layout and older files where a row had exactly one bar.
#[derive(Deserialize)]
struct TaskRaw {
    label: String,
    #[serde(default)]
    bars: Option<Vec<Bar>>,
    #[serde(default)]
    start: f32,
    #[serde(default)]
    end: f32,
    #[serde(default)]
    milestone: bool,
    #[serde(default)]
    dashed: bool,
    #[serde(default)]
    color: usize,
    #[serde(default)]
    responsible: String,
    #[serde(default)]
    remark: String,
}

impl From<TaskRaw> for Task {
    fn from(r: TaskRaw) -> Self {
        let bars = r.bars.unwrap_or_else(|| {
            vec![Bar { start: r.start, end: r.end, milestone: r.milestone, dashed: r.dashed, color: r.color }]
        });
        Task { label: r.label, bars, responsible: r.responsible, remark: r.remark }
    }
}

impl Task {
    /// A row with a single bar (or milestone when `milestone` is true).
    pub fn one(label: &str, start: f32, end: f32, milestone: bool, color: usize, dashed: bool) -> Self {
        Task::new(label, vec![Bar { start, end, milestone, dashed, color }])
    }

    /// A row with the given bars and no responsible or remark.
    pub fn new(label: &str, bars: Vec<Bar>) -> Self {
        Task { label: label.to_string(), bars, responsible: String::new(), remark: String::new() }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Gantt {
    pub periods: Vec<String>,
    pub tasks: Vec<Task>,
    pub today: Option<f32>,
    /// Calendar mode: the Monday the timeline starts on; columns are weeks.
    #[serde(default)]
    pub start: Option<[i32; 3]>,
    #[serde(default)]
    pub weeks: u32,
    #[serde(default = "activity_header")]
    pub header: String,
    /// Calendar header rows shown: years, quarters, months, weeks.
    #[serde(default = "default_scales")]
    pub scales: [bool; 4],
    /// Full-height separator lines for: years, quarters, months, weeks.
    #[serde(default = "default_separators")]
    pub separators: [bool; 4],
    /// Calendar mode: the date line was dragged by hand (uses `today`, in weeks)
    /// instead of following the real date.
    #[serde(default)]
    pub today_fixed: bool,
    /// Label columns: activity names left of the timeline, who is responsible next to them,
    /// and remarks right of the timeline.
    #[serde(default = "yes")]
    pub show_activity: bool,
    #[serde(default)]
    pub show_responsible: bool,
    #[serde(default)]
    pub show_remark: bool,
    #[serde(default = "responsible_header")]
    pub responsible_header: String,
    #[serde(default = "remark_header")]
    pub remark_header: String,
}

impl Gantt {
    /// Position (in columns) and label of the date line, if shown.
    pub fn today_line(&self) -> Option<(f32, String)> {
        let t = self.today?;
        match self.start {
            Some([y, m, d]) => {
                let start = days_from_civil(y, m.max(1) as u32, d.max(1) as u32);
                let pos = if self.today_fixed { t } else { (today_days() - start) as f32 / 7.0 };
                let day = start + (pos * 7.0).round() as i64;
                let (yy, mm, dd) = civil_from_days(day);
                Some((pos, format!("{dd:02}.{mm:02}.{yy}")))
            }
            None => Some((t, "Today".to_string())),
        }
    }

    /// The date a calendar position stands for (for labels while dragging).
    pub fn date_label(&self, pos: f32) -> String {
        match self.start {
            Some([y, m, d]) => {
                let start = days_from_civil(y, m.max(1) as u32, d.max(1) as u32);
                let (yy, mm, dd) = civil_from_days(start + (pos * 7.0).round() as i64);
                format!("{dd:02}.{mm:02}.{yy}")
            }
            None => format!("{pos:.2}"),
        }
    }

    /// Shrinks or grows the timeline to just hold every bar and milestone: in calendar
    /// mode from the week of the earliest to the week of the latest, with own periods by
    /// dropping empty periods at both ends. Bars keep their dates. False if there are no bars.
    pub fn fit_to_data(&mut self) -> bool {
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for b in self.tasks.iter().flat_map(|t| t.bars.iter()) {
            lo = lo.min(b.start);
            hi = hi.max(b.end.max(b.start));
        }
        if lo > hi {
            return false;
        }
        let shift = lo.floor().max(if self.start.is_some() { f32::MIN } else { 0.0 });
        let span = ((hi - shift).ceil().max(1.0)) as usize;
        match self.start {
            Some([y, m, d]) => {
                let start = days_from_civil(y, m.max(1) as u32, d.max(1) as u32) + shift as i64 * 7;
                let (yy, mm, dd) = civil_from_days(start);
                self.start = Some([yy, mm as i32, dd as i32]);
                self.weeks = span as u32;
            }
            None => {
                let first = (shift as usize).min(self.periods.len());
                let last = (first + span).min(self.periods.len()).max(first);
                if last > first {
                    self.periods = self.periods[first..last].to_vec();
                }
            }
        }
        self.shift_bars(-shift);
        true
    }

    /// Calendar mode: starts the timeline on the Monday of `days` (days since 1970) while
    /// every bar keeps its date; bars that fall outside are cut to the timeline.
    pub fn set_start_keep_dates(&mut self, days: i64) {
        let Some([y, m, d]) = self.start else { return };
        let old = days_from_civil(y, m.max(1) as u32, d.max(1) as u32);
        let monday = days - weekday(days) as i64;
        let (yy, mm, dd) = civil_from_days(monday);
        self.start = Some([yy, mm as i32, dd as i32]);
        self.shift_bars((old - monday) as f32 / 7.0);
        let cols = self.columns() as f32;
        for b in self.tasks.iter_mut().flat_map(|t| t.bars.iter_mut()) {
            b.start = b.start.clamp(0.0, cols);
            b.end = if b.milestone { b.start } else { b.end.clamp(b.start, cols) };
        }
    }

    fn shift_bars(&mut self, by: f32) {
        for b in self.tasks.iter_mut().flat_map(|t| t.bars.iter_mut()) {
            b.start += by;
            b.end += by;
        }
        if self.start.is_none() || self.today_fixed {
            if let Some(t) = self.today.as_mut() {
                *t += by;
            }
        }
    }

    /// Smallest step for dragging: one day in calendar mode, a quarter period otherwise.
    pub fn snap_step(&self) -> f32 {
        if self.start.is_some() {
            1.0 / 7.0
        } else {
            0.25
        }
    }
}

fn default_scales() -> [bool; 4] {
    [true, false, true, true]
}

fn default_separators() -> [bool; 4] {
    [true, false, true, true]
}

fn activity_header() -> String {
    "Activity".into()
}

fn yes() -> bool {
    true
}

fn responsible_header() -> String {
    "Responsible".into()
}

fn remark_header() -> String {
    "Remark".into()
}

impl Gantt {
    pub fn columns(&self) -> usize {
        if self.start.is_some() {
            self.weeks.max(1) as usize
        } else {
            self.periods.len().max(1)
        }
    }

    /// A week-based timeline starting on the Monday of the given date's week.
    pub fn calendar(y: i32, m: u32, d: u32, weeks: u32) -> Self {
        let days = days_from_civil(y, m, d);
        let monday = days - weekday(days) as i64;
        let (yy, mm, dd) = civil_from_days(monday);
        Gantt {
            periods: vec![],
            tasks: vec![
                Task::one("Kick-off", 0.0, 0.0, true, 0, false),
                Task::one("Requirements", 0.0, 2.0, false, 0, false),
                Task::new(
                    "Design",
                    vec![
                        Bar { start: 1.5, end: 4.0, milestone: false, dashed: false, color: 0 },
                        Bar { start: 4.5, end: 4.5, milestone: true, dashed: false, color: 1 },
                    ],
                ),
                Task::one("Build", 4.0, 9.0, false, 1, false),
                Task::one("Test", 8.0, 11.0, false, 2, true),
                Task::one("Go-live", 11.5, 11.5, true, 0, false),
            ],
            today: Some(0.0),
            start: Some([yy, mm as i32, dd as i32]),
            weeks,
            header: activity_header(),
            scales: default_scales(),
            separators: default_separators(),
            today_fixed: false,
            show_activity: true,
            show_responsible: false,
            show_remark: false,
            responsible_header: responsible_header(),
            remark_header: remark_header(),
        }
    }
}

// ---- dates (proleptic Gregorian, days since 1970-01-01)

pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// 0 = Monday ... 6 = Sunday
pub fn weekday(days: i64) -> u32 {
    ((days + 3).rem_euclid(7)) as u32
}

/// ISO week number and its year.
pub fn iso_week(days: i64) -> (i32, u32) {
    let thursday = days - weekday(days) as i64 + 3;
    let (y, _, _) = civil_from_days(thursday);
    let jan1 = days_from_civil(y, 1, 1);
    (y, ((thursday - jan1) / 7 + 1) as u32)
}

pub fn today_days() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86400) as i64)
        .unwrap_or(0)
}

pub const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

impl Default for Gantt {
    fn default() -> Self {
        Gantt {
            periods: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            tasks: vec![
                Task::one("Discovery", 0.0, 2.0, false, 0, false),
                Task::one("Design", 1.5, 4.5, false, 0, false),
                Task::one("Design sign-off", 4.5, 4.5, true, 0, false),
                Task::one("Build", 4.5, 9.0, false, 1, false),
                Task::one("Pilot", 8.0, 10.5, false, 2, false),
                Task::one("Launch", 11.0, 11.0, true, 0, false),
            ],
            today: Some(5.3),
            start: None,
            weeks: 0,
            header: activity_header(),
            scales: default_scales(),
            separators: default_separators(),
            today_fixed: false,
            show_activity: true,
            show_responsible: false,
            show_remark: false,
            responsible_header: responsible_header(),
            remark_header: remark_header(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TextBox {
    pub text: String,
    pub size: f32,
    pub color: [u8; 3],
    pub fill: Option<[u8; 3]>,
    pub align: Align,
}

impl Default for TextBox {
    fn default() -> Self {
        TextBox { text: "Text".into(), size: 16.0, color: [40, 40, 48], fill: None, align: Align::Left }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapeKind {
    Rounded,
    Pentagon,
    Chevron,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct ShapeBox {
    pub shape: ShapeKind,
    pub text: String,
    pub fill: [u8; 3],
    pub text_color: [u8; 3],
    pub size: f32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Table {
    pub rows: Vec<Vec<String>>,
    pub header: bool,
}

impl Table {
    pub fn sample() -> Self {
        Table {
            rows: vec![
                vec!["Option".into(), "Cost".into(), "Time".into(), "Risk".into()],
                vec!["Build in-house".into(), "High".into(), "12 months".into(), "Medium".into()],
                vec!["Partner".into(), "Medium".into(), "6 months".into(), "Low".into()],
                vec!["Buy".into(), "Low".into(), "3 months".into(), "High".into()],
            ],
            header: true,
        }
    }
    pub fn normalize(&mut self) {
        let cols = self.rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
        if self.rows.is_empty() {
            self.rows.push(vec![String::new(); cols]);
        }
        for r in &mut self.rows {
            r.resize(cols, String::new());
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Harvey {
    /// 0..=4 quarters filled.
    pub quarters: u8,
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CheckState {
    Done,
    Cross,
    Open,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct CheckMark {
    pub state: CheckState,
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Process {
    pub steps: Vec<String>,
    pub highlight: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Agenda {
    pub items: Vec<String>,
    pub current: usize,
}

// ---------------------------------------------------------------------------
// Colour styles

pub struct Style {
    pub name: &'static str,
    pub colors: [[u8; 3]; 6],
    pub accent: [u8; 3],
}

pub const STYLES: [Style; 5] = [
    Style {
        name: "Ocean",
        colors: [[22, 58, 110], [48, 112, 186], [116, 166, 224], [182, 208, 238], [90, 96, 110], [196, 200, 208]],
        accent: [22, 58, 110],
    },
    Style {
        name: "Forest",
        colors: [[22, 88, 72], [44, 140, 112], [118, 190, 160], [190, 226, 208], [92, 100, 96], [200, 206, 202]],
        accent: [22, 88, 72],
    },
    Style {
        name: "Sunset",
        colors: [[150, 46, 40], [214, 96, 58], [240, 160, 92], [248, 210, 150], [96, 90, 96], [206, 200, 204]],
        accent: [150, 46, 40],
    },
    Style {
        name: "Graphite",
        colors: [[40, 44, 52], [86, 92, 104], [136, 142, 154], [188, 192, 200], [224, 120, 40], [220, 222, 226]],
        accent: [224, 120, 40],
    },
    Style {
        name: "Vivid",
        colors: [[88, 56, 200], [0, 158, 180], [246, 170, 30], [232, 78, 108], [74, 170, 90], [160, 160, 170]],
        accent: [88, 56, 200],
    },
];

// ---------------------------------------------------------------------------
// The sample deck shown on first start.

impl Deck {
    pub fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn element(&mut self, x: f32, y: f32, w: f32, h: f32, kind: Kind) -> Element {
        Element { id: self.new_id(), x, y, w, h, kind }
    }

    pub fn blank() -> Deck {
        Deck {
            name: "Untitled".into(),
            style: 0,
            decimal_comma: false,
            slides: vec![Slide { title: "Click to add a title".into(), footnote: String::new(), elements: vec![] }],
            next_id: 0,
        }
    }

    pub fn sample() -> Deck {
        let mut d = Deck { name: "Quarterly review".into(), style: 0, decimal_comma: false, slides: vec![], next_id: 0 };

        // 1: stacked chart with growth annotation
        let mut stacked = Chart::new(ChartKind::Stacked);
        stacked.categories = vec!["2021".into(), "2022".into(), "2023".into(), "2024".into(), "2025".into()];
        stacked.series = vec![
            Series { name: "Europe".into(), values: vec![42.0, 48.0, 53.0, 61.0, 68.0], color: None },
            Series { name: "Americas".into(), values: vec![30.0, 33.0, 39.0, 44.0, 52.0], color: None },
            Series { name: "Asia".into(), values: vec![14.0, 19.0, 24.0, 31.0, 37.0], color: None },
        ];
        stacked.cagr = Some((0, 4));
        stacked.normalize();
        let e1 = d.element(40.0, 92.0, 560.0, 400.0, Kind::Chart(stacked));
        let note = d.element(
            630.0,
            110.0,
            290.0,
            150.0,
            Kind::Text(TextBox {
                text: "Revenue nearly doubled in four years.\n\nAsia is the fastest-growing region and now makes up a fifth of sales."
                    .into(),
                size: 15.0,
                color: [40, 40, 48],
                fill: Some([240, 243, 248]),
                align: Align::Left,
            }),
        );
        let h1 = d.element(630.0, 290.0, 290.0, 34.0, Kind::Harvey(Harvey { quarters: 4, label: "Europe on target".into() }));
        let h2 = d.element(630.0, 334.0, 290.0, 34.0, Kind::Harvey(Harvey { quarters: 3, label: "Americas on track".into() }));
        let h3 = d.element(630.0, 378.0, 290.0, 34.0, Kind::Harvey(Harvey { quarters: 2, label: "Asia needs partners".into() }));
        d.slides.push(Slide {
            title: "Revenue by region, in millions".into(),
            footnote: "Source: sample data".into(),
            elements: vec![e1, note, h1, h2, h3],
        });

        // 2: waterfall
        let wf = Chart::new(ChartKind::Waterfall);
        let e = d.element(40.0, 92.0, 880.0, 400.0, Kind::Chart(wf));
        d.slides.push(Slide {
            title: "What moved revenue from 2024 to 2025".into(),
            footnote: "Source: sample data".into(),
            elements: vec![e],
        });

        // 3: Gantt
        let g = d.element(40.0, 92.0, 880.0, 390.0, Kind::Gantt(Gantt::calendar(2026, 11, 2, 13)));
        d.slides.push(Slide { title: "Project plan".into(), footnote: String::new(), elements: vec![g] });

        // 4: Mekko + doughnut
        let mut m = Chart::new(ChartKind::Mekko);
        m.title = "Market share by region".into();
        let e = d.element(40.0, 92.0, 520.0, 400.0, Kind::Chart(m));
        let mut p = Chart::new(ChartKind::Doughnut);
        p.title = "Sales channels".into();
        let e2 = d.element(590.0, 92.0, 330.0, 400.0, Kind::Chart(p));
        d.slides.push(Slide { title: "Market overview".into(), footnote: "Source: sample data".into(), elements: vec![e, e2] });

        // 5: agenda + process
        let a = d.element(
            40.0,
            100.0,
            420.0,
            300.0,
            Kind::Agenda(Agenda {
                items: vec![
                    "Where we stand".into(),
                    "What moved revenue".into(),
                    "Project plan".into(),
                    "Market overview".into(),
                    "Next steps".into(),
                ],
                current: 1,
            }),
        );
        let pr = d.element(
            40.0,
            430.0,
            880.0,
            60.0,
            Kind::Process(Process {
                steps: vec!["Analyse".into(), "Decide".into(), "Build".into(), "Launch".into()],
                highlight: Some(1),
            }),
        );
        let c1 = d.element(500.0, 110.0, 400.0, 34.0, Kind::Check(CheckMark { state: CheckState::Done, label: "Budget approved".into() }));
        let c2 = d.element(500.0, 154.0, 400.0, 34.0, Kind::Check(CheckMark { state: CheckState::Done, label: "Team in place".into() }));
        let c3 = d.element(500.0, 198.0, 400.0, 34.0, Kind::Check(CheckMark { state: CheckState::Open, label: "Pilot customers signed".into() }));
        let c4 = d.element(500.0, 242.0, 400.0, 34.0, Kind::Check(CheckMark { state: CheckState::Cross, label: "Partner in Asia".into() }));
        d.slides.push(Slide { title: "Agenda".into(), footnote: String::new(), elements: vec![a, pr, c1, c2, c3, c4] });

        // 6: more chart types and a table
        let mut combo = Chart::new(ChartKind::Combination);
        combo.title = "Sales and margin".into();
        let e1 = d.element(40.0, 84.0, 430.0, 270.0, Kind::Chart(combo));
        let mut bub = Chart::new(ChartKind::Bubble);
        bub.title = "Markets: growth vs. share".into();
        let e2 = d.element(500.0, 84.0, 420.0, 270.0, Kind::Chart(bub));
        let t = d.element(40.0, 372.0, 880.0, 128.0, Kind::Table(Table::sample()));
        d.slides.push(Slide { title: "Options compared".into(), footnote: "Source: sample data".into(), elements: vec![e1, e2, t] });
        d
    }
}
