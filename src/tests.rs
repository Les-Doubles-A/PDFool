//! Tests de bout en bout : chaque outil est exécuté sur des fichiers générés, puis le résultat est relu et rendu.

use std::path::{Path, PathBuf};

use lopdf::{Document, Object};

use crate::convert::{PageFormat, Renderer, TextToPdfOptions, text_to_pdf};
use crate::jobs::{ReorderMode, Settings, SplitMode, Tool, run};
use crate::pdf;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pdfool_test_{name}_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// PDF de `n` pages ; la page k contient « PAGE k ».
fn sample(dir: &Path, name: &str, n: u32) -> PathBuf {
    let text: Vec<String> = (1..=n).map(|k| format!("PAGE {k}\n{}", "ligne\n".repeat(80))).collect();
    let o = TextToPdfOptions {
        size: 10.0,
        margin_mm: 20.0,
        format: PageFormat::A4,
    };
    // Chaque bloc fait plus d'une page : on construit page par page pour maîtriser le compte.
    let docs: Vec<(String, Document)> = text
        .iter()
        .map(|t| {
            let first = t.lines().take(40).collect::<Vec<_>>().join("\n");
            (name.to_string(), text_to_pdf(&first, &o))
        })
        .collect();
    let mut doc = pdf::merge(docs, false).unwrap();
    let path = dir.join(format!("{name}.pdf"));
    pdf::save(&mut doc, &path).unwrap();
    assert_eq!(pdf::page_count(&Document::load(&path).unwrap()), n);
    path
}

fn sample_images(dir: &Path) -> (PathBuf, PathBuf) {
    let png = dir.join("logo.png");
    let mut img = image::RgbaImage::new(64, 32);
    for (x, _, p) in img.enumerate_pixels_mut() {
        *p = image::Rgba([200, 30, 30, if x < 32 { 255 } else { 80 }]);
    }
    img.save(&png).unwrap();
    let jpg = dir.join("photo.jpg");
    let rgb = image::RgbImage::from_fn(120, 80, |x, y| image::Rgb([(x * 2) as u8, (y * 3) as u8, 128]));
    rgb.save(&jpg).unwrap();
    (png, jpg)
}

fn exec(tool: Tool, files: &[PathBuf], s: &Settings) -> crate::jobs::Outcome {
    run(tool, files, s, &None, &|_, _| {}).unwrap_or_else(|e| panic!("{tool:?} : {e}"))
}

fn pages_of(path: &Path) -> u32 {
    pdf::page_count(&Document::load(path).unwrap())
}

fn render_ok(path: &Path) -> image::RgbaImage {
    let r = Renderer::open(std::fs::read(path).unwrap(), "").unwrap();
    r.render_fit(0, 300.0).unwrap()
}

fn first_text(path: &Path, page: u32) -> String {
    Document::load(path).unwrap().extract_text(&[page]).unwrap()
}

#[test]
fn organise_tools() {
    let t = TempDir::new("organise");
    let a = sample(&t.0, "a", 3);
    let b = sample(&t.0, "b", 2);
    let mut s = Settings::default();

    let out = exec(Tool::Merge, &[a.clone(), b.clone()], &s);
    assert_eq!(pages_of(&out.outputs[0]), 5);
    let merged = Document::load(&out.outputs[0]).unwrap();
    assert!(merged.catalog().unwrap().has(b"Outlines"));
    render_ok(&out.outputs[0]);

    let out = exec(Tool::Split, &[a.clone()], &s);
    assert_eq!(out.outputs.len(), 3);
    assert!(first_text(&out.outputs[2], 1).contains("PAGE 3"));
    s.split_mode = SplitMode::EveryN;
    s.split_n = 2;
    assert_eq!(exec(Tool::Split, &[a.clone()], &s).outputs.len(), 2);
    s.split_mode = SplitMode::Ranges;
    s.split_ranges = "1, 2-".into();
    let out = exec(Tool::Split, &[a.clone()], &s);
    assert_eq!(out.outputs.iter().map(|p| pages_of(p)).collect::<Vec<_>>(), vec![1, 2]);

    s.pages = "3, 1".into();
    let out = exec(Tool::Extract, &[a.clone()], &s);
    assert_eq!(pages_of(&out.outputs[0]), 2);
    assert!(first_text(&out.outputs[0], 1).contains("PAGE 3"));
    assert!(first_text(&out.outputs[0], 2).contains("PAGE 1"));

    s.pages = "2".into();
    let out = exec(Tool::Delete, &[a.clone()], &s);
    assert_eq!(pages_of(&out.outputs[0]), 2);
    assert!(first_text(&out.outputs[0], 2).contains("PAGE 3"));

    s.reorder_mode = ReorderMode::Reverse;
    let out = exec(Tool::Reorder, &[a.clone()], &s);
    assert!(first_text(&out.outputs[0], 1).contains("PAGE 3"));
    let five = exec(
        Tool::Merge,
        &[a.clone(), b.clone()],
        &Settings {
            merge_bookmarks: false,
            ..Settings::default()
        },
    )
    .outputs[0]
        .clone();
    s.reorder_mode = ReorderMode::Interleave;
    let out = exec(Tool::Reorder, &[five], &s);
    // a = PAGE 1..3, b = PAGE 1..2 → ordre 1,5,2,4,3 = a1, b2, a2, b1, a3
    let texts: Vec<String> = (1..=5).map(|p| first_text(&out.outputs[0], p)).collect();
    assert!(texts[1].contains("PAGE 2") && texts[3].contains("PAGE 1") && texts[4].contains("PAGE 3"));
    s.reorder_mode = ReorderMode::Custom;
    s.reorder_spec = "2, 2".into();
    let out = exec(Tool::Reorder, &[a.clone()], &s);
    assert_eq!(pages_of(&out.outputs[0]), 4, "page dupliquée + pages restantes");

    s.pages = "1".into();
    let out = exec(Tool::Rotate, &[a.clone()], &s);
    let doc = Document::load(&out.outputs[0]).unwrap();
    let pages = doc.get_pages();
    assert_eq!(pdf::page_rotation(&doc, pages[&1]), 90);
    assert_eq!(pdf::page_rotation(&doc, pages[&2]), 0);
    let img = render_ok(&out.outputs[0]);
    assert!(img.width() > img.height(), "page pivotée affichée en paysage");

    s.blank_after = "0, fin".into();
    let out = exec(Tool::InsertBlank, &[a.clone()], &s);
    assert_eq!(pages_of(&out.outputs[0]), 5);
    assert!(first_text(&out.outputs[0], 2).contains("PAGE 1"));
}

fn diff(a: &image::RgbaImage, b: &image::RgbaImage) -> u64 {
    a.pixels()
        .zip(b.pixels())
        .flat_map(|(p, q)| (0..3).map(move |c| (p.0[c] as i32 - q.0[c] as i32).unsigned_abs() as u64))
        .sum()
}

#[test]
fn edit_tools() {
    let t = TempDir::new("edit");
    let a = sample(&t.0, "a", 2);
    let (png, _) = sample_images(&t.0);
    let base = render_ok(&a);
    let mut s = Settings::default();

    let out = exec(Tool::Watermark, &[a.clone()], &s);
    let img = render_ok(&out.outputs[0]);
    if let Ok(dir) = std::env::var("PDFOOL_DUMP") {
        img.save(Path::new(&dir).join("filigrane.png")).unwrap();
    }
    assert!(diff(&base, &img) > 10_000, "le filigrane doit être visible");
    assert!(first_text(&out.outputs[0], 2).contains("CONFIDENTIEL"));

    s.num_template = "Page {n} / {total}".into();
    let out = exec(Tool::PageNumbers, &[a.clone()], &s);
    assert!(first_text(&out.outputs[0], 2).contains("Page 2 / 2"));
    assert!(diff(&base, &render_ok(&out.outputs[0])) > 100);

    s.stamp_image = Some(png);
    let out = exec(Tool::StampImage, &[a.clone()], &s);
    assert!(diff(&base, &render_ok(&out.outputs[0])) > 1_000);

    let out = exec(Tool::Crop, &[a.clone()], &s);
    let doc = Document::load(&out.outputs[0]).unwrap();
    let [x0, _, x1, _] = pdf::page_box(&doc, doc.get_pages()[&1]);
    assert!((x1 - x0 - (595.28 - 2.0 * 28.35)).abs() < 1.0);

    s.meta = vec![
        ("Title".into(), "Rapport été".into()),
        ("Author".into(), "Équipe".into()),
    ];
    let out = exec(Tool::Metadata, &[a.clone()], &s);
    let meta = pdf::read_metadata(&Document::load(&out.outputs[0]).unwrap());
    assert_eq!(meta.get("Title").map(String::as_str), Some("Rapport été"));
    s.meta_strip = true;
    let out = exec(Tool::Metadata, &[out.outputs[0].clone()], &s);
    assert!(
        pdf::read_metadata(&Document::load(&out.outputs[0]).unwrap())
            .get("Title")
            .is_none()
    );
}

#[test]
fn security_and_optimise() {
    let t = TempDir::new("secu");
    let a = sample(&t.0, "a", 2);
    let mut s = Settings::default();
    s.prot_user = "secret".into();
    let out = exec(Tool::Protect, &[a.clone()], &s);
    let locked = out.outputs[0].clone();
    assert!(pdf::load(&locked, "").is_err());
    assert!(pdf::load(&locked, "faux").is_err());
    assert_eq!(pdf::page_count(&pdf::load(&locked, "secret").unwrap()), 2);
    assert!(Renderer::open(std::fs::read(&locked).unwrap(), "secret").is_ok());

    s.password = "secret".into();
    let out = exec(Tool::Unlock, &[locked], &s);
    let doc = Document::load(&out.outputs[0]).unwrap();
    assert!(!doc.is_encrypted());
    assert!(first_text(&out.outputs[0], 1).contains("PAGE 1"));
    s.password.clear();

    let (png, jpg) = sample_images(&t.0);
    let out = exec(Tool::ImagesToPdf, &[png, jpg.clone()], &s);
    let with_images = out.outputs[0].clone();
    assert_eq!(pages_of(&with_images), 2);
    render_ok(&with_images);
    let out = exec(Tool::ExtractImages, &[with_images.clone()], &s);
    assert_eq!(out.outputs.len(), 2, "{}", out.report);
    assert!(out.outputs.iter().any(|p| p.extension().unwrap() == "jpg"));
    assert_eq!(
        std::fs::read(out.outputs.iter().find(|p| p.extension().unwrap() == "jpg").unwrap()).unwrap(),
        std::fs::read(&jpg).unwrap()
    );

    s.cmp_quality = 30;
    s.cmp_max_side = 50;
    let out = exec(Tool::Compress, &[with_images.clone()], &s);
    assert_eq!(pages_of(&out.outputs[0]), 2);
    render_ok(&out.outputs[0]);
    let out = exec(Tool::Compress, &[a.clone()], &s);
    assert!(first_text(&out.outputs[0], 1).contains("PAGE 1"));

    let out = exec(Tool::Flatten, &[a.clone()], &s);
    assert_eq!(pages_of(&out.outputs[0]), 2);
    let doc = Document::load(&out.outputs[0]).unwrap();
    assert!(doc.extract_text(&[1]).unwrap_or_default().trim().is_empty());
}

#[test]
fn convert_tools() {
    let t = TempDir::new("convert");
    let a = sample(&t.0, "a", 3);
    let mut s = Settings::default();
    s.raster_dpi = 50.0;
    let out = exec(Tool::PdfToImages, &[a.clone()], &s);
    assert_eq!(out.outputs.len(), 3);
    let img = image::open(&out.outputs[0]).unwrap();
    assert_eq!(img.width(), (595.28_f32 * 50.0 / 72.0) as u32);

    s.pages = "2-3".into();
    let out = exec(Tool::PdfToText, &[a.clone()], &s);
    let text = std::fs::read_to_string(&out.outputs[0]).unwrap();
    assert!(text.contains("PAGE 2") && text.contains("PAGE 3") && !text.contains("PAGE 1\r"));

    let txt = t.0.join("notes.txt");
    std::fs::write(&txt, "Bonjour l'été — à bientôt €\n".repeat(200)).unwrap();
    let out = exec(Tool::TextToPdf, &[txt], &s);
    assert!(pages_of(&out.outputs[0]) >= 2);
    assert!(first_text(&out.outputs[0], 1).contains("Bonjour l'été"));

    let out = exec(Tool::Info, &[a.clone()], &s);
    assert!(out.report.contains("Pages : 3"), "{}", out.report);
    assert!(out.outputs.is_empty());
}

#[test]
fn rejects_bad_input() {
    let t = TempDir::new("bad");
    let a = sample(&t.0, "a", 2);
    let s = Settings {
        pages: "5".into(),
        ..Settings::default()
    };
    assert!(run(Tool::Extract, &[a.clone()], &s, &None, &|_, _| {}).is_err());
    assert!(run(Tool::Merge, &[a.clone()], &s, &None, &|_, _| {}).is_err());
    assert!(run(Tool::ImagesToPdf, &[a.clone()], &s, &None, &|_, _| {}).is_err());
    let s = Settings {
        pages: "1-2".into(),
        ..Settings::default()
    };
    assert!(run(Tool::Delete, &[a], &s, &None, &|_, _| {}).is_err());
    let _ = Object::Null;
}
