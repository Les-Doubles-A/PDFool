#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! PDFool : outils PDF locaux, hors ligne.

mod app;
mod compress;
mod convert;
mod edit;
mod images;
mod jobs;
mod pages;
mod pdf;
#[cfg(test)]
mod tests;
mod text;

fn icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../PDFool.png"))
        .ok()?
        .into_rgba8();
    let img = image::imageops::resize(&img, 256, 256, image::imageops::FilterType::Triangle);
    Some(egui::IconData {
        width: img.width(),
        height: img.height(),
        rgba: img.into_raw(),
    })
}

use eframe::egui;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("PDFool")
        .with_inner_size([1180.0, 820.0])
        .with_min_inner_size([900.0, 580.0])
        .with_drag_and_drop(true);
    if let Some(icon) = icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let files: Vec<std::path::PathBuf> = std::env::args_os()
        .skip(1)
        .map(Into::into)
        .filter(|p: &std::path::PathBuf| p.is_file())
        .collect();
    eframe::run_native(
        "PDFool",
        options,
        Box::new(move |cc| Ok(Box::new(app::PdfApp::new(cc, files)))),
    )
}
