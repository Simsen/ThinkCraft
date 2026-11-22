//! Writing PowerPoint (.pptx) and Excel (.xlsx) files, and reading tables
//! from Excel, CSV or OpenDocument files.

use crate::canvas::{Canvas, Item};
use crate::draw::{self, Xf};
use crate::model::*;
use eframe::egui::{self, epaint::ColorMode, Color32, Painter, Pos2, Rect, Shape};
use std::io::Write;
use std::path::Path;

const EMU: f32 = 12700.0; // one slide unit is one point

fn emu(v: f32) -> i64 {
    (v * EMU).round() as i64
}

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => o.push(c),
        }
    }
    o
}

fn hex(c: Color32) -> Option<String> {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    if a < 8 {
        None
    } else {
        Some(format!("{r:02X}{g:02X}{b:02X}"))
    }
}

fn fill_xml(c: Color32) -> String {
    match hex(c) {
        Some(h) => format!("<a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill>"),
        None => "<a:noFill/>".into(),
    }
}

fn line_xml(width: f32, c: Color32) -> String {
    match hex(c) {
        Some(h) if width > 0.0 => format!(
            "<a:ln w=\"{}\"><a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill></a:ln>",
            emu(width).max(3175)
        ),
        _ => "<a:ln><a:noFill/></a:ln>".into(),
    }
}

struct SlideXml {
    next_id: u32,
    body: String,
}

impl SlideXml {
    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }
}

fn xfrm(r: Rect) -> String {
    format!(
        "<a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
        emu(r.left()),
        emu(r.top()),
        emu(r.width()).max(1),
        emu(r.height()).max(1)
    )
}

fn sp(sx: &mut SlideXml, name: &str, r: Rect, geom: &str, fill: &str, line: &str) -> String {
    let id = sx.id();
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{} {id}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}{geom}{fill}{line}</p:spPr></p:sp>",
        esc(name),
        xfrm(r)
    )
}

/// A straight line as PowerPoint stores it (a connector with flips), which
/// every slide app understands.
fn line_shape(sx: &mut SlideXml, a: Pos2, b: Pos2, width: f32, color: Color32, dashed: bool, arrow: bool) -> (String, Rect) {
    let bb = Rect::from_two_pos(a, b);
    let Some(h) = hex(color) else { return (String::new(), bb) };
    let id = sx.id();
    let mut flips = String::new();
    if b.x < a.x {
        flips.push_str(" flipH=\"1\"");
    }
    if b.y < a.y {
        flips.push_str(" flipV=\"1\"");
    }
    let dash = if dashed { "<a:prstDash val=\"dash\"/>" } else { "" };
    let tail = if arrow { "<a:tailEnd type=\"triangle\" w=\"med\" len=\"med\"/>" } else { "" };
    let xml = format!(
        "<p:cxnSp><p:nvCxnSpPr><p:cNvPr id=\"{id}\" name=\"{} {id}\"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr><p:spPr><a:xfrm{flips}><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom><a:ln w=\"{}\"><a:solidFill><a:srgbClr val=\"{h}\"/></a:solidFill>{dash}{tail}</a:ln></p:spPr></p:cxnSp>",
        if arrow { "Arrow" } else { "Line" },
        emu(bb.left()),
        emu(bb.top()),
        emu(bb.width()),
        emu(bb.height()),
        emu(width).max(3175)
    );
    (xml, bb)
}

fn path_geom(points: &[Pos2], closed: bool) -> (Rect, String) {
    let bb = Rect::from_points(points);
    let w = emu(bb.width()).max(1);
    let h = emu(bb.height()).max(1);
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        let x = emu(p.x - bb.left());
        let y = emu(p.y - bb.top());
        if i == 0 {
            d.push_str(&format!("<a:moveTo><a:pt x=\"{x}\" y=\"{y}\"/></a:moveTo>"));
        } else {
            d.push_str(&format!("<a:lnTo><a:pt x=\"{x}\" y=\"{y}\"/></a:lnTo>"));
        }
    }
    if closed {
        d.push_str("<a:close/>");
    }
    let geom = format!(
        "<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l=\"0\" t=\"0\" r=\"r\" b=\"b\"/><a:pathLst><a:path w=\"{w}\" h=\"{h}\"{}>{d}</a:path></a:pathLst></a:custGeom>",
        if closed { "" } else { " fill=\"none\"" }
    );
    (bb, geom)
}

fn shape_xml(sx: &mut SlideXml, shape: &Shape, out: &mut String, bounds: &mut Rect) {
    match shape {
        Shape::Vec(v) => {
            for s in v {
                shape_xml(sx, s, out, bounds);
            }
        }
        Shape::Rect(r) => {
            if r.rect.width() <= 0.0 && r.rect.height() <= 0.0 {
                return;
            }
            let cr = r.corner_radius.nw as f32;
            let geom = if cr > 0.5 {
                let adj = ((cr / r.rect.width().min(r.rect.height()).max(0.01)) * 100000.0).min(50000.0) as i64;
                format!("<a:prstGeom prst=\"roundRect\"><a:avLst><a:gd name=\"adj\" fmla=\"val {adj}\"/></a:avLst></a:prstGeom>")
            } else {
                "<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>".to_string()
            };
            // thin white separators between stacked segments look noisy in PowerPoint
            let line = if r.fill == Color32::TRANSPARENT || r.stroke.color != Color32::WHITE {
                line_xml(r.stroke.width, r.stroke.color)
            } else {
                line_xml(0.0, Color32::TRANSPARENT)
            };
            if r.fill == Color32::TRANSPARENT && r.stroke.color == Color32::WHITE {
                return;
            }
            out.push_str(&sp(sx, "Rectangle", r.rect, &geom, &fill_xml(r.fill), &line));
            *bounds = bounds.union(r.rect);
        }
        Shape::Circle(c) => {
            let rect = Rect::from_center_size(c.center, egui::Vec2::splat(c.radius * 2.0));
            out.push_str(&sp(
                sx,
                "Oval",
                rect,
                "<a:prstGeom prst=\"ellipse\"><a:avLst/></a:prstGeom>",
                &fill_xml(c.fill),
                &line_xml(c.stroke.width, c.stroke.color),
            ));
            *bounds = bounds.union(rect);
        }
        Shape::LineSegment { points, stroke } => {
            let (xml, bb) = line_shape(sx, points[0], points[1], stroke.width, stroke.color, false, false);
            out.push_str(&xml);
            *bounds = bounds.union(bb);
        }
        Shape::Path(ps) => {
            if ps.points.len() < 2 {
                return;
            }
            let (bb, geom) = path_geom(&ps.points, ps.closed);
            let stroke_col = match &ps.stroke.color {
                ColorMode::Solid(c) => *c,
                _ => Color32::GRAY,
            };
            if !ps.closed {
                // open polylines become plain line segments
                for w in ps.points.windows(2) {
                    let (xml, bb) = line_shape(sx, w[0], w[1], ps.stroke.width, stroke_col, false, false);
                    out.push_str(&xml);
                    *bounds = bounds.union(bb);
                }
                return;
            }
            let fill = fill_xml(ps.fill);
            out.push_str(&sp(sx, "Freeform", bb, &geom, &fill, &line_xml(ps.stroke.width, stroke_col)));
            *bounds = bounds.union(bb);
        }
        _ => {}
    }
}

fn text_xml(sx: &mut SlideXml, rect: Rect, text: &str, size: f32, color: Color32, align: Align, wrap: bool) -> (String, Rect) {
    let id = sx.id();
    // PowerPoint's fonts are a little wider than ours; give single-line labels room.
    let rect = if wrap {
        rect
    } else {
        let extra = rect.width() * 0.25 + 4.0;
        match align {
            Align::Left => Rect::from_min_max(rect.min, Pos2::new(rect.right() + extra, rect.bottom())),
            Align::Center => rect.expand2(egui::vec2(extra / 2.0, 0.0)),
            Align::Right => Rect::from_min_max(Pos2::new(rect.left() - extra, rect.top()), rect.max),
        }
    };
    let algn = match align {
        Align::Left => "l",
        Align::Center => "ctr",
        Align::Right => "r",
    };
    let sz = ((size * 100.0).round() as i64).clamp(100, 400000);
    let col = hex(color).unwrap_or_else(|| "000000".into());
    let mut paras = String::new();
    for line in text.split('\n') {
        if line.is_empty() {
            paras.push_str(&format!("<a:p><a:pPr algn=\"{algn}\"/><a:endParaRPr lang=\"en-US\" sz=\"{sz}\"/></a:p>"));
        } else {
            paras.push_str(&format!(
                "<a:p><a:pPr algn=\"{algn}\"/><a:r><a:rPr lang=\"en-US\" sz=\"{sz}\" dirty=\"0\"><a:solidFill><a:srgbClr val=\"{col}\"/></a:solidFill><a:latin typeface=\"Calibri\"/></a:rPr><a:t>{}</a:t></a:r></a:p>",
                esc(line)
            ));
        }
    }
    let xml = format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Text {id}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap=\"{}\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\" anchor=\"{}\" rtlCol=\"0\"><a:noAutofit/></a:bodyPr><a:lstStyle/>{paras}</p:txBody></p:sp>",
        xfrm(rect),
        if wrap { "square" } else { "none" },
        if wrap { "t" } else { "ctr" }
    );
    (xml, rect)
}

/// Turns the recorded drawing into slide XML, grouping each element.
fn items_to_xml(items: Vec<Item>) -> String {
    let mut sx = SlideXml { next_id: 1, body: String::new() };
    // stack of (name, xml, bounds)
    let mut stack: Vec<(String, String, Rect)> = vec![(String::new(), String::new(), Rect::NOTHING)];
    for it in items {
        match it {
            Item::Begin(name) => stack.push((name, String::new(), Rect::NOTHING)),
            Item::End => {
                if stack.len() > 1 {
                    let (name, xml, bb) = stack.pop().unwrap();
                    let id = sx.id();
                    let parent = stack.last_mut().unwrap();
                    if !bb.is_positive() {
                        parent.1.push_str(&xml);
                        continue;
                    }
                    let off = format!(
                        "<a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/><a:chOff x=\"{}\" y=\"{}\"/><a:chExt cx=\"{}\" cy=\"{}\"/>",
                        emu(bb.left()), emu(bb.top()), emu(bb.width()).max(1), emu(bb.height()).max(1),
                        emu(bb.left()), emu(bb.top()), emu(bb.width()).max(1), emu(bb.height()).max(1)
                    );
                    parent.1.push_str(&format!(
                        "<p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"{id}\" name=\"{} {id}\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm>{off}</a:xfrm></p:grpSpPr>{xml}</p:grpSp>",
                        esc(&name)
                    ));
                    parent.2 = parent.2.union(bb);
                }
            }
            Item::Shape(s) => {
                let top = stack.last_mut().unwrap();
                let mut out = String::new();
                let mut bb = top.2;
                shape_xml(&mut sx, &s, &mut out, &mut bb);
                let top = stack.last_mut().unwrap();
                top.1.push_str(&out);
                top.2 = bb;
            }
            Item::Sector { center, r_in, r_out, a0, a1, color } => {
                let rect = Rect::from_center_size(center, egui::Vec2::splat(r_out * 2.0));
                let ang = |a: f32| -> i64 {
                    let deg = a.to_degrees().rem_euclid(360.0);
                    ((deg * 60000.0).round() as i64) % 21600000
                };
                let full = (a1 - a0).abs() >= std::f32::consts::TAU - 1e-3;
                let geom = if r_in <= 0.0 {
                    if full {
                        "<a:prstGeom prst=\"ellipse\"><a:avLst/></a:prstGeom>".to_string()
                    } else {
                        format!(
                            "<a:prstGeom prst=\"pie\"><a:avLst><a:gd name=\"adj1\" fmla=\"val {}\"/><a:gd name=\"adj2\" fmla=\"val {}\"/></a:avLst></a:prstGeom>",
                            ang(a0),
                            ang(a1)
                        )
                    }
                } else {
                    let thick = (((r_out - r_in) / (2.0 * r_out)) * 100000.0).round() as i64;
                    if full {
                        format!("<a:prstGeom prst=\"donut\"><a:avLst><a:gd name=\"adj\" fmla=\"val {}\"/></a:avLst></a:prstGeom>", thick.min(50000))
                    } else {
                        format!(
                            "<a:prstGeom prst=\"blockArc\"><a:avLst><a:gd name=\"adj1\" fmla=\"val {}\"/><a:gd name=\"adj2\" fmla=\"val {}\"/><a:gd name=\"adj3\" fmla=\"val {}\"/></a:avLst></a:prstGeom>",
                            ang(a0),
                            ang(a1),
                            thick.min(50000)
                        )
                    }
                };
                let xml = sp(&mut sx, "Slice", rect, &geom, &fill_xml(color), "<a:ln><a:noFill/></a:ln>");
                let top = stack.last_mut().unwrap();
                top.1.push_str(&xml);
                top.2 = top.2.union(rect);
            }
            Item::Line { from, to, stroke, dashed, arrow } => {
                let (xml, bb) = line_shape(&mut sx, from, to, stroke.width, stroke.color, dashed, arrow);
                let top = stack.last_mut().unwrap();
                top.1.push_str(&xml);
                top.2 = top.2.union(bb);
            }
            Item::Preset { rect, prst, adj, color } => {
                let av = match adj {
                    Some(v) => format!("<a:avLst><a:gd name=\"adj\" fmla=\"val {v}\"/></a:avLst>"),
                    None => "<a:avLst/>".into(),
                };
                let geom = format!("<a:prstGeom prst=\"{prst}\">{av}</a:prstGeom>");
                let xml = sp(&mut sx, prst, rect, &geom, &fill_xml(color), "<a:ln><a:noFill/></a:ln>");
                let top = stack.last_mut().unwrap();
                top.1.push_str(&xml);
                top.2 = top.2.union(rect);
            }
            Item::Text { rect, text, size, color, align, wrap } => {
                let (xml, r) = text_xml(&mut sx, rect, &text, size, color, align, wrap);
                let top = stack.last_mut().unwrap();
                top.1.push_str(&xml);
                top.2 = top.2.union(r);
            }
        }
    }
    while stack.len() > 1 {
        let (_, xml, _) = stack.pop().unwrap();
        stack.last_mut().unwrap().1.push_str(&xml);
    }
    sx.body = stack.pop().unwrap().1;
    sx.body
}

const NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
const HDR: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const EMPTY_TREE: &str = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";

fn theme_xml(style: &Style) -> String {
    let c = |i: usize| format!("{:02X}{:02X}{:02X}", style.colors[i][0], style.colors[i][1], style.colors[i][2]);
    format!(
        r#"{HDR}<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="ThinkCraft"><a:themeElements><a:clrScheme name="ThinkCraft"><a:dk1><a:srgbClr val="26283A"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="{}"/></a:accent1><a:accent2><a:srgbClr val="{}"/></a:accent2><a:accent3><a:srgbClr val="{}"/></a:accent3><a:accent4><a:srgbClr val="{}"/></a:accent4><a:accent5><a:srgbClr val="{}"/></a:accent5><a:accent6><a:srgbClr val="{}"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="ThinkCraft"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="ThinkCraft"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#,
        c(0), c(1), c(2), c(3), c(4), c(5)
    )
}

fn zip_files(path: &Path, files: Vec<(String, String)>) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| format!("Couldn't create the file: {e}"))?;
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in files {
        z.start_file(name, opts).map_err(|e| e.to_string())?;
        z.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
    }
    z.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn core_props(title: &str) -> String {
    format!(
        r#"{HDR}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:creator>ThinkCraft</dc:creator></cp:coreProperties>"#,
        esc(title)
    )
}

fn app_props() -> String {
    format!(r#"{HDR}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>ThinkCraft</Application></Properties>"#)
}

/// Writes the slides as an editable PowerPoint file: every chart and element
/// becomes a group of native shapes and text boxes.
pub fn write_pptx(path: &Path, deck: &Deck, slides: &[usize], painter: &Painter) -> Result<(), String> {
    let style = &STYLES[deck.style % STYLES.len()];
    let xf = Xf { origin: Pos2::ZERO, scale: 1.0 };
    let mut files: Vec<(String, String)> = Vec::new();
    let n = slides.len();

    let mut ct = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/><Override PartName="/ppt/presProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presProps+xml"/><Override PartName="/ppt/viewProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml"/><Override PartName="/ppt/tableStyles.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>"#,
    );
    for i in 1..=n {
        ct.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
        ));
    }
    ct.push_str("</Types>");
    files.push(("[Content_Types].xml".into(), ct));

    files.push((
        "_rels/.rels".into(),
        format!(
            r#"{HDR}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="ppt/presentation.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="{REL}/extended-properties" Target="docProps/app.xml"/></Relationships>"#
        ),
    ));
    files.push(("docProps/core.xml".into(), core_props(&deck.name)));
    files.push(("docProps/app.xml".into(), app_props()));

    // presentation
    let mut ids = String::new();
    let mut rels = format!(
        r#"{HDR}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/slideMaster" Target="slideMasters/slideMaster1.xml"/>"#
    );
    for i in 1..=n {
        ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 255 + i, i + 1));
        rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"{REL}/slide\" Target=\"slides/slide{i}.xml\"/>", i + 1));
    }
    rels.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"{REL}/theme\" Target=\"theme/theme1.xml\"/><Relationship Id=\"rId{}\" Type=\"{REL}/presProps\" Target=\"presProps.xml\"/><Relationship Id=\"rId{}\" Type=\"{REL}/viewProps\" Target=\"viewProps.xml\"/><Relationship Id=\"rId{}\" Type=\"{REL}/tableStyles\" Target=\"tableStyles.xml\"/></Relationships>",
        n + 2, n + 3, n + 4, n + 5
    ));
    files.push((
        "ppt/presentation.xml".into(),
        format!(
            "{HDR}<p:presentation {NS} saveSubsetFonts=\"1\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"12192000\" cy=\"6858000\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>"
        ),
    ));
    files.push(("ppt/_rels/presentation.xml.rels".into(), rels));
    files.push(("ppt/presProps.xml".into(), format!("{HDR}<p:presentationPr {NS}/>")));
    files.push((
        "ppt/viewProps.xml".into(),
        format!("{HDR}<p:viewPr {NS}><p:normalViewPr/><p:gridSpacing cx=\"76200\" cy=\"76200\"/></p:viewPr>"),
    ));
    files.push((
        "ppt/tableStyles.xml".into(),
        format!("{HDR}<a:tblStyleLst xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" def=\"{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}\"/>"),
    ));
    files.push(("ppt/theme/theme1.xml".into(), theme_xml(style)));
    files.push((
        "ppt/slideMasters/slideMaster1.xml".into(),
        format!(
            "{HDR}<p:sldMaster {NS}><p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg><p:spTree>{EMPTY_TREE}</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz=\"3200\"/></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr><a:defRPr sz=\"1800\"/></a:lvl1pPr></p:bodyStyle><p:otherStyle><a:lvl1pPr><a:defRPr sz=\"1800\"/></a:lvl1pPr></p:otherStyle></p:txStyles></p:sldMaster>"
        ),
    ));
    files.push((
        "ppt/slideMasters/_rels/slideMaster1.xml.rels".into(),
        format!(
            r#"{HDR}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="{REL}/theme" Target="../theme/theme1.xml"/></Relationships>"#
        ),
    ));
    files.push((
        "ppt/slideLayouts/slideLayout1.xml".into(),
        format!(
            "{HDR}<p:sldLayout {NS} type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\"><p:spTree>{EMPTY_TREE}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"
        ),
    ));
    files.push((
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(),
        format!(
            r#"{HDR}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#
        ),
    ));

    for (k, &si) in slides.iter().enumerate() {
        let slide = &deck.slides[si];
        let canvas = Canvas::record(painter.clone());
        draw::draw_slide(&canvas, slide, &xf, style, si + 1);
        let body = items_to_xml(canvas.take());
        files.push((
            format!("ppt/slides/slide{}.xml", k + 1),
            format!("{HDR}<p:sld {NS}><p:cSld><p:spTree>{EMPTY_TREE}{body}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"),
        ));
        files.push((
            format!("ppt/slides/_rels/slide{}.xml.rels", k + 1),
            format!(
                r#"{HDR}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#
            ),
        ));
    }
    zip_files(path, files)
}

// ---------------------------------------------------------------------------
// Excel

fn col_name(mut i: usize) -> String {
    let mut s = String::new();
    i += 1;
    while i > 0 {
        let r = (i - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        i = (i - 1) / 26;
    }
    s
}

pub enum Cell {
    S(String),
    N(f64),
    Empty,
}

pub struct Sheet {
    pub name: String,
    pub rows: Vec<Vec<Cell>>,
}

fn sheet_name(base: &str, used: &mut Vec<String>) -> String {
    let mut clean: String = base.chars().filter(|c| !"[]:*?/\\".contains(*c)).collect();
    if clean.trim().is_empty() {
        clean = "Data".into();
    }
    let mut name: String = clean.chars().take(28).collect();
    let mut k = 2;
    while used.iter().any(|u| u.eq_ignore_ascii_case(&name)) {
        name = format!("{} {k}", clean.chars().take(25).collect::<String>());
        k += 1;
    }
    used.push(name.clone());
    name
}

/// One worksheet per chart or Gantt chart in the given slides.
pub fn deck_sheets(deck: &Deck, slides: &[usize], only: Option<u64>) -> Vec<Sheet> {
    let mut used = Vec::new();
    let mut sheets = Vec::new();
    for &si in slides {
        let slide = &deck.slides[si];
        for e in &slide.elements {
            if only.is_some() && only != Some(e.id) {
                continue;
            }
            match &e.kind {
                Kind::Chart(c) => {
                    let title = if c.title.is_empty() { format!("{} {}", si + 1, c.kind.label()) } else { c.title.clone() };
                    let mut rows = vec![];
                    let mut head = vec![Cell::Empty];
                    head.extend(c.categories.iter().map(|x| Cell::S(x.clone())));
                    rows.push(head);
                    for s in &c.series {
                        let mut r = vec![Cell::S(s.name.clone())];
                        r.extend(s.values.iter().map(|v| Cell::N(*v)));
                        rows.push(r);
                    }
                    if c.kind == ChartKind::Waterfall {
                        let mut r = vec![Cell::S("Subtotal (=)".into())];
                        r.extend(c.subtotal.iter().map(|b| if *b { Cell::S("=".into()) } else { Cell::Empty }));
                        rows.push(r);
                    }
                    sheets.push(Sheet { name: sheet_name(&title, &mut used), rows });
                }
                Kind::Gantt(g) => {
                    let mut rows = vec![vec![
                        Cell::S("Activity".into()),
                        Cell::S("Start".into()),
                        Cell::S("End".into()),
                        Cell::S("Milestone".into()),
                    ]];
                    // one line per bar; an activity with several bars repeats its name
                    for row in &g.tasks {
                        if row.bars.is_empty() {
                            rows.push(vec![Cell::S(row.label.clone())]);
                        }
                        for t in &row.bars {
                            rows.push(vec![
                                Cell::S(row.label.clone()),
                                Cell::N(t.start as f64),
                                Cell::N(t.end as f64),
                                Cell::S(if t.milestone { "yes".into() } else { String::new() }),
                            ]);
                        }
                    }
                    sheets.push(Sheet { name: sheet_name("Project plan", &mut used), rows });
                }
                Kind::Table(t) => {
                    let rows = t
                        .rows
                        .iter()
                        .map(|r| {
                            r.iter()
                                .map(|c| match c.trim().parse::<f64>() {
                                    Ok(v) => Cell::N(v),
                                    Err(_) if c.is_empty() => Cell::Empty,
                                    Err(_) => Cell::S(c.clone()),
                                })
                                .collect()
                        })
                        .collect();
                    sheets.push(Sheet { name: sheet_name("Table", &mut used), rows });
                }
                _ => {}
            }
        }
    }
    sheets
}

pub fn write_xlsx(path: &Path, sheets: &[Sheet]) -> Result<(), String> {
    if sheets.is_empty() {
        return Err("There is no chart data to send to Excel.".into());
    }
    let main = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    let mut files = Vec::new();
    let mut ct = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>"#,
    );
    let mut shared: Vec<String> = Vec::new();
    let mut shared_count = 0usize;
    let mut wb_sheets = String::new();
    let mut wb_rels = format!(r#"{HDR}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (i, sh) in sheets.iter().enumerate() {
        let k = i + 1;
        ct.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{k}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        ));
        wb_sheets.push_str(&format!("<sheet name=\"{}\" sheetId=\"{k}\" r:id=\"rId{k}\"/>", esc(&sh.name)));
        wb_rels.push_str(&format!("<Relationship Id=\"rId{k}\" Type=\"{REL}/worksheet\" Target=\"worksheets/sheet{k}.xml\"/>"));
        let mut data = String::new();
        for (ri, row) in sh.rows.iter().enumerate() {
            data.push_str(&format!("<row r=\"{}\">", ri + 1));
            for (ci, cell) in row.iter().enumerate() {
                let r = format!("{}{}", col_name(ci), ri + 1);
                let style = if ri == 0 || ci == 0 { " s=\"1\"" } else { "" };
                match cell {
                    Cell::S(s) => {
                        let idx = match shared.iter().position(|x| x == s) {
                            Some(i) => i,
                            None => {
                                shared.push(s.clone());
                                shared.len() - 1
                            }
                        };
                        shared_count += 1;
                        data.push_str(&format!("<c r=\"{r}\" t=\"s\"{style}><v>{idx}</v></c>"));
                    }
                    Cell::N(v) => data.push_str(&format!("<c r=\"{r}\"><v>{v}</v></c>")),
                    Cell::Empty => {}
                }
            }
            data.push_str("</row>");
        }
        files.push((
            format!("xl/worksheets/sheet{k}.xml"),
            {
                let rows = sh.rows.len().max(1);
                let cols = sh.rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
                format!(
                    "{HDR}<worksheet xmlns=\"{main}\" xmlns:r=\"{REL}\"><dimension ref=\"A1:{}{rows}\"/><sheetViews><sheetView workbookViewId=\"0\"{}/></sheetViews><sheetFormatPr defaultRowHeight=\"15\"/><cols><col min=\"1\" max=\"1\" width=\"22\" customWidth=\"1\"/></cols><sheetData>{data}</sheetData><pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/></worksheet>",
                    col_name(cols - 1),
                    if k == 1 { " tabSelected=\"1\"" } else { "" }
                )
            },
        ));
    }
    let n = sheets.len();
    wb_rels.push_str(&format!(
        "<Relationship Id=\"rId{}\" Type=\"{REL}/styles\" Target=\"styles.xml\"/><Relationship Id=\"rId{}\" Type=\"{REL}/sharedStrings\" Target=\"sharedStrings.xml\"/></Relationships>",
        n + 1,
        n + 2
    ));
    ct.push_str("<Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/></Types>");
    let sst: String = shared.iter().map(|s| format!("<si><t xml:space=\"preserve\">{}</t></si>", esc(s))).collect();
    files.push((
        "xl/sharedStrings.xml".into(),
        format!("{HDR}<sst xmlns=\"{main}\" count=\"{shared_count}\" uniqueCount=\"{}\">{sst}</sst>", shared.len()),
    ));
    files.push(("[Content_Types].xml".into(), ct));
    files.push((
        "_rels/.rels".into(),
        format!(
            r#"{HDR}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="xl/workbook.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="{REL}/extended-properties" Target="docProps/app.xml"/></Relationships>"#
        ),
    ));
    files.push(("docProps/core.xml".into(), core_props("ThinkCraft data")));
    files.push(("docProps/app.xml".into(), app_props()));
    files.push((
        "xl/workbook.xml".into(),
        format!("{HDR}<workbook xmlns=\"{main}\" xmlns:r=\"{REL}\"><bookViews><workbookView xWindow=\"0\" yWindow=\"0\" windowWidth=\"28800\" windowHeight=\"17000\"/></bookViews><sheets>{wb_sheets}</sheets></workbook>"),
    ));
    files.push(("xl/_rels/workbook.xml.rels".into(), wb_rels));
    files.push((
        "xl/styles.xml".into(),
        format!(
            "{HDR}<styleSheet xmlns=\"{main}\"><fonts count=\"2\"><font><sz val=\"11\"/><name val=\"Calibri\"/></font><font><b/><sz val=\"11\"/><name val=\"Calibri\"/></font></fonts><fills count=\"2\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill></fills><borders count=\"1\"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs><cellXfs count=\"2\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/><xf numFmtId=\"0\" fontId=\"1\" fillId=\"0\" borderId=\"0\" xfId=\"0\" applyFont=\"1\"/></cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles></styleSheet>"
        ),
    ));
    zip_files(path, files)
}

/// Tab-separated text of a chart's datasheet, ready to paste into Excel.
pub fn chart_tsv(c: &Chart) -> String {
    let mut s = String::new();
    s.push('\t');
    s.push_str(&c.categories.join("\t"));
    s.push('\n');
    for ser in &c.series {
        s.push_str(&ser.name);
        for v in &ser.values {
            s.push('\t');
            s.push_str(&v.to_string());
        }
        s.push('\n');
    }
    s
}

/// Reads the first sheet of an Excel / OpenDocument file, or a CSV/TSV text
/// file, as tab-separated text.
pub fn read_table_file(path: &Path) -> Result<String, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if matches!(ext.as_str(), "csv" | "tsv" | "txt") {
        return std::fs::read_to_string(path).map_err(|e| format!("Couldn't read the file: {e}"));
    }
    use calamine::{open_workbook_auto, Data, Reader};
    let mut wb = open_workbook_auto(path).map_err(|e| format!("Couldn't open the workbook: {e}"))?;
    let first = wb.sheet_names().first().cloned().ok_or("The workbook has no sheets.")?;
    let range = wb.worksheet_range(&first).map_err(|e| format!("Couldn't read the sheet: {e}"))?;
    let mut out = String::new();
    for row in range.rows() {
        let cells: Vec<String> = row
            .iter()
            .map(|c| match c {
                Data::Empty => String::new(),
                Data::String(s) => s.replace('\t', " "),
                Data::Float(f) => f.to_string(),
                Data::Int(i) => i.to_string(),
                Data::Bool(b) => b.to_string(),
                other => other.to_string(),
            })
            .collect();
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excel_round_trip() {
        let deck = Deck::sample();
        let sheets = deck_sheets(&deck, &[0], None);
        let path = std::env::temp_dir().join("thinkcraft-roundtrip.xlsx");
        write_xlsx(&path, &sheets).unwrap();
        let text = read_table_file(&path).unwrap();
        let mut c = Chart::new(ChartKind::Stacked);
        c.load_table(&text).unwrap();
        assert_eq!(c.categories, vec!["2021", "2022", "2023", "2024", "2025"]);
        assert_eq!(c.series.len(), 3);
        assert_eq!(c.series[0].name, "Europe");
        assert_eq!(c.series[2].values[4], 37.0);
    }

    #[test]
    fn pasted_table() {
        let mut c = Chart::new(ChartKind::Stacked);
        c.load_table("\t2023\t2024\nNorth\t1,5\t2\nSouth\t3\t4\n").unwrap();
        assert_eq!(c.series[0].values, vec![1.5, 2.0]);
        assert_eq!(c.series[1].name, "South");
    }
}
