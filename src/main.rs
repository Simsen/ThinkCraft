#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod canvas;
mod draw;
mod model;
mod office;

use eframe::egui;

/// The ThinkCraft icon: an orange rounded square with white chart bars and a growth line.
fn icon_rgba(size: u32) -> Vec<u8> {
    let n = size as f32;
    let mut px = vec![0u8; (size * size * 4) as usize];
    let inside_round = |x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32| -> f32 {
        // signed coverage (0..1) of a rounded rectangle, with one pixel of smoothing
        let cx = x.clamp(x0 + r, x1 - r);
        let cy = y.clamp(y0 + r, y1 - r);
        let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r;
        (0.5 - d).clamp(0.0, 1.0)
    };
    let seg_dist = |x: f32, y: f32, a: (f32, f32), b: (f32, f32)| -> f32 {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
        ((x - a.0 - t * dx).powi(2) + (y - a.1 - t * dy).powi(2)).sqrt()
    };
    let m = n * 0.09; // macOS icons leave a margin around the tile
    let bars = [(0.25, 0.62), (0.43, 0.48), (0.61, 0.34)];
    let line = [(n * 0.24, n * 0.50), (n * 0.46, n * 0.36), (n * 0.76, n * 0.22)];
    for yy in 0..size {
        for xx in 0..size {
            let (x, y) = (xx as f32 + 0.5, yy as f32 + 0.5);
            let tile = inside_round(x, y, m, m, n - m, n - m, n * 0.2);
            if tile <= 0.0 {
                continue;
            }
            // warm gradient from top to bottom
            let g = y / n;
            let mut c = [(214.0 - 30.0 * g), (102.0 - 30.0 * g), (48.0 - 16.0 * g)];
            let mut white = 0.0f32;
            for (bx, top) in bars {
                white = white.max(inside_round(x, y, n * bx, n * top, n * (bx + 0.13), n * 0.78, n * 0.015));
            }
            for w in line.windows(2) {
                white = white.max((n * 0.022 - seg_dist(x, y, w[0], w[1]) + 0.5).clamp(0.0, 1.0));
            }
            let dot = n * 0.045 - ((x - line[2].0).powi(2) + (y - line[2].1).powi(2)).sqrt();
            white = white.max((dot + 0.5).clamp(0.0, 1.0));
            for ch in &mut c {
                *ch = *ch * (1.0 - white) + 255.0 * white;
            }
            let i = ((yy * size + xx) * 4) as usize;
            px[i] = c[0] as u8;
            px[i + 1] = c[1] as u8;
            px[i + 2] = c[2] as u8;
            px[i + 3] = (tile * 255.0) as u8;
        }
    }
    px
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    // used when packaging the Mac app: writes the icon picture and exits
    if let Some(i) = args.iter().position(|a| a == "--make-icon") {
        let path = args.get(i + 1).cloned().unwrap_or_else(|| "icon.png".into());
        let size = 1024;
        let img = image::RgbaImage::from_raw(size, size, icon_rgba(size)).expect("icon size");
        if let Err(e) = img.save(&path) {
            eprintln!("Couldn't write the icon: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let screenshot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from);
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    let export_pptx = flag("--export-pptx");
    let export_xlsx = flag("--export-xlsx");
    let values: Vec<&std::path::PathBuf> = [&screenshot, &export_pptx, &export_xlsx].into_iter().flatten().collect();
    let open_file = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("--") && !values.iter().any(|v| v.to_str() == Some(a.as_str())))
        .map(std::path::PathBuf::from);
    let background = screenshot.is_some() || export_pptx.is_some() || export_xlsx.is_some();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ThinkCraft")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([960.0, 600.0])
        .with_icon(egui::IconData { rgba: icon_rgba(256), width: 256, height: 256 });

    let mut options = eframe::NativeOptions::default();
    if background {
        viewport = viewport.with_active(false);
        #[cfg(target_os = "windows")]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
        #[cfg(target_os = "macos")]
        {
            options.event_loop_builder = Some(Box::new(|builder| {
                use winit::platform::macos::EventLoopBuilderExtMacOS;
                builder.with_activate_ignoring_other_apps(false);
            }));
        }
        options.persist_window = false;
    }
    options.viewport = viewport;

    eframe::run_native(
        "ThinkCraft",
        options,
        Box::new(move |cc| Ok(Box::new(app::ThinkCraft::new(cc, app::Cli { screenshot, open: open_file, export_pptx, export_xlsx })))),
    )
}
