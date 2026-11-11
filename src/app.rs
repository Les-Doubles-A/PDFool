//! Interface graphique (egui).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{
    self, Align, Color32, ComboBox, CornerRadius, DragValue, Frame, Layout, Margin, ProgressBar, RichText, ScrollArea,
    Slider, Stroke, TextEdit, TextureHandle, TextureOptions, Vec2,
};

use crate::convert::{PageFormat, RasterFormat, Renderer};
use crate::edit::Anchor;
use crate::jobs::{self, CATEGORIES, Input, Outcome, ReorderMode, Settings, SplitMode, Tool};
use crate::pdf::{self, display_name, human_size};

const ACCENT: Color32 = Color32::from_rgb(0xD9, 0x48, 0x2B);
const ACCENT_DARK: Color32 = Color32::from_rgb(0xF0, 0x6A, 0x4A);

enum Msg {
    Progress(f32, String),
    Done(Result<Outcome, String>),
}

struct FileEntry {
    path: PathBuf,
    detail: String,
}

struct Preview {
    path: PathBuf,
    page: usize,
    total: usize,
    texture: Option<TextureHandle>,
    error: Option<String>,
    rx: Option<Receiver<Result<(egui::ColorImage, usize), String>>>,
    requested: Option<(PathBuf, usize, String)>,
}

pub struct PdfApp {
    tool: Tool,
    files: Vec<FileEntry>,
    selected: Option<usize>,
    settings: Settings,
    same_dir: bool,
    output_dir: Option<PathBuf>,
    job: Option<Receiver<Msg>>,
    progress: (f32, String),
    result: Option<Result<Outcome, String>>,
    preview: Option<Preview>,
}

impl PdfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, files: Vec<PathBuf>) -> Self {
        install_fonts(&cc.egui_ctx);
        style(&cc.egui_ctx);
        let mut app = PdfApp {
            tool: Tool::Merge,
            files: Vec::new(),
            selected: None,
            settings: Settings::default(),
            same_dir: true,
            output_dir: None,
            job: None,
            progress: (0.0, "Prêt".into()),
            result: None,
            preview: None,
        };
        app.add_files(files);
        app
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            if path.is_dir() || self.files.iter().any(|f| f.path == path) {
                continue;
            }
            let detail = describe_file(&path);
            self.files.push(FileEntry { path, detail });
        }
        if self.selected.is_none() && !self.files.is_empty() {
            self.selected = Some(0);
        }
    }

    fn busy(&self) -> bool {
        self.job.is_some()
    }

    fn start(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let tool = self.tool;
        let files: Vec<PathBuf> = self.files.iter().map(|f| f.path.clone()).collect();
        let settings = self.settings.clone();
        let output = if self.same_dir { None } else { self.output_dir.clone() };
        if !self.same_dir && output.is_none() {
            self.result = Some(Err("Choisissez le dossier de sortie.".into()));
            return;
        }
        let ctx = ctx.clone();
        self.result = None;
        self.progress = (0.0, "Démarrage".into());
        self.job = Some(rx);
        std::thread::spawn(move || {
            let report = |f: f32, m: String| {
                let _ = tx.send(Msg::Progress(f, m));
                ctx.request_repaint();
            };
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                jobs::run(tool, &files, &settings, &output, &report)
            }))
            .unwrap_or_else(|_| Err("Erreur interne pendant le traitement (document probablement mal formé).".into()));
            let _ = tx.send(Msg::Done(outcome));
            ctx.request_repaint();
        });
    }

    fn poll_job(&mut self) {
        let Some(rx) = &self.job else { return };
        let mut finished = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Progress(f, m) => self.progress = (f, m),
                Msg::Done(r) => finished = Some(r),
            }
        }
        if let Some(r) = finished {
            self.progress = match &r {
                Ok(_) => (1.0, "Terminé".into()),
                Err(_) => (0.0, "Erreur".into()),
            };
            self.result = Some(r);
            self.job = None;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Aperçu

    fn request_preview(&mut self, ctx: &egui::Context) {
        let Some(path) = self.selected.and_then(|i| self.files.get(i)).map(|f| f.path.clone()) else {
            self.preview = None;
            return;
        };
        let is_pdf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
        let is_img = crate::images::is_image(&path);
        if !is_pdf && !is_img {
            self.preview = None;
            return;
        }
        let preview = self.preview.get_or_insert_with(|| Preview {
            path: path.clone(),
            page: 0,
            total: 0,
            texture: None,
            error: None,
            rx: None,
            requested: None,
        });
        if preview.path != path {
            *preview = Preview {
                path: path.clone(),
                page: 0,
                total: 0,
                texture: None,
                error: None,
                rx: None,
                requested: None,
            };
        }
        let key = (path.clone(), preview.page, self.settings.password.clone());
        if preview.requested.as_ref() == Some(&key) {
            return;
        }
        preview.requested = Some(key);
        let (tx, rx) = channel();
        preview.rx = Some(rx);
        let page = preview.page;
        let password = self.settings.password.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(|| render_preview(&path, page, &password, is_pdf))
                .unwrap_or_else(|_| Err("Aperçu impossible pour ce document.".into()));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    fn poll_preview(&mut self, ctx: &egui::Context) {
        let Some(p) = &mut self.preview else { return };
        let Some(rx) = &p.rx else { return };
        if let Ok(result) = rx.try_recv() {
            p.rx = None;
            match result {
                Ok((img, total)) => {
                    p.total = total;
                    p.error = None;
                    p.texture = Some(ctx.load_texture("apercu", img, TextureOptions::LINEAR));
                }
                Err(e) => {
                    p.error = Some(e);
                    p.texture = None;
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Panneaux

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ScrollArea::vertical().show(ui, |ui| {
            ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                ui.spacing_mut().button_padding = Vec2::new(10.0, 4.0);
                ui.add_space(4.0);
                for (category, tools) in CATEGORIES {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(category.to_uppercase())
                            .small()
                            .strong()
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(2.0);
                    for tool in tools {
                        let selected = self.tool == *tool;
                        let text = RichText::new(tool.label()).size(14.5);
                        let resp = ui.add(egui::Button::selectable(selected, text));
                        if resp.clicked() && !self.busy() {
                            if self.tool != *tool {
                                self.result = None;
                                self.progress = (0.0, "Prêt".into());
                            }
                            self.tool = *tool;
                        }
                    }
                }
                ui.add_space(12.0);
            });
        });
    }

    fn preview_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading("Aperçu");
        ui.add_space(4.0);
        let Some(p) = &mut self.preview else {
            ui.weak("Sélectionnez un PDF ou une image dans la liste pour l'afficher ici.");
            return;
        };
        ui.label(RichText::new(display_name(&p.path)).strong());
        if p.total > 1 {
            ui.horizontal(|ui| {
                if ui.add_enabled(p.page > 0, egui::Button::new("◀")).clicked() {
                    p.page -= 1;
                }
                ui.label(format!("Page {} / {}", p.page + 1, p.total));
                if ui.add_enabled(p.page + 1 < p.total, egui::Button::new("▶")).clicked() {
                    p.page += 1;
                }
            });
        }
        ui.add_space(6.0);
        if let Some(err) = &p.error {
            ui.colored_label(ui.visuals().error_fg_color, err);
        } else if let Some(tex) = &p.texture {
            let avail = ui.available_size() - Vec2::new(4.0, 8.0);
            let size = tex.size_vec2();
            let scale = (avail.x / size.x).min(avail.y / size.y).min(1.5);
            let draw = size * scale;
            ui.vertical_centered(|ui| {
                Frame::new()
                    .stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
                    .show(ui, |ui| {
                        ui.add(egui::Image::new(tex).fit_to_exact_size(draw));
                    });
            });
        }
        if p.rx.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.weak("Rendu en cours...");
            });
        }
    }

    fn files_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Fichiers").strong().size(16.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            !self.files.is_empty() && !self.busy(),
                            egui::Button::new("Tout retirer"),
                        )
                        .clicked()
                    {
                        self.files.clear();
                        self.selected = None;
                    }
                    if ui
                        .add_enabled(self.files.len() > 1 && !self.busy(), egui::Button::new("Trier A-Z"))
                        .clicked()
                    {
                        let current = self.selected.map(|i| self.files[i].path.clone());
                        self.files.sort_by_key(|f| display_name(&f.path).to_lowercase());
                        self.selected = current.and_then(|c| self.files.iter().position(|f| f.path == c));
                    }
                    let add = ui.add_enabled(
                        !self.busy(),
                        egui::Button::new(RichText::new("Ajouter des fichiers").strong().color(Color32::WHITE))
                            .fill(ACCENT),
                    );
                    if add.clicked() {
                        let dialog = match self.tool.input() {
                            Input::Pdf => rfd::FileDialog::new().add_filter("Documents PDF", &["pdf", "PDF"]),
                            Input::Images => {
                                rfd::FileDialog::new().add_filter("Images", &crate::images::IMAGE_EXTENSIONS)
                            }
                            Input::Text => {
                                rfd::FileDialog::new().add_filter("Fichiers texte", &["txt", "md", "csv", "log"])
                            }
                        };
                        if let Some(paths) = dialog.set_title("Ajouter des fichiers").pick_files() {
                            self.add_files(paths);
                        }
                    }
                });
            });
            ui.add_space(4.0);
            if self.files.is_empty() {
                let hint = match self.tool.input() {
                    Input::Pdf => "Glissez-déposez des PDF ici ou cliquez sur « Ajouter des fichiers ».",
                    Input::Images => {
                        "Glissez-déposez des images (JPG, PNG, ...) ici ou cliquez sur « Ajouter des fichiers »."
                    }
                    Input::Text => "Glissez-déposez des fichiers .txt ici ou cliquez sur « Ajouter des fichiers ».",
                };
                Frame::new()
                    .stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(18))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical_centered(|ui| ui.weak(hint));
                    });
                return;
            }
            let mut action: Option<(usize, i32)> = None;
            let busy = self.busy();
            let count = self.files.len();
            ScrollArea::vertical()
                .max_height(220.0)
                .id_salt("files")
                .show(ui, |ui| {
                    for (i, f) in self.files.iter().enumerate() {
                        let selected = self.selected == Some(i);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{:>2}.", i + 1)).monospace().weak());
                            let resp = ui.add(egui::Button::selectable(selected, RichText::new(display_name(&f.path))));
                            if resp.clicked() {
                                self.selected = Some(i);
                            }
                            resp.on_hover_text(f.path.display().to_string());
                            ui.weak(&f.detail);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.add_enabled_ui(!busy, |ui| {
                                    if ui.small_button("✕").on_hover_text("Retirer").clicked() {
                                        action = Some((i, 0));
                                    }
                                    if ui
                                        .add_enabled(i + 1 < count, egui::Button::new("▼").small())
                                        .on_hover_text("Descendre")
                                        .clicked()
                                    {
                                        action = Some((i, 1));
                                    }
                                    if ui
                                        .add_enabled(i > 0, egui::Button::new("▲").small())
                                        .on_hover_text("Monter")
                                        .clicked()
                                    {
                                        action = Some((i, -1));
                                    }
                                });
                            });
                        });
                    }
                });
            if let Some((i, dir)) = action {
                match dir {
                    0 => {
                        self.files.remove(i);
                        self.selected = match self.selected {
                            Some(s) if s == i => (!self.files.is_empty()).then(|| i.min(self.files.len() - 1)),
                            Some(s) if s > i => Some(s - 1),
                            s => s,
                        };
                    }
                    d => {
                        let j = (i as i32 + d) as usize;
                        self.files.swap(i, j);
                        if self.selected == Some(i) {
                            self.selected = Some(j);
                        } else if self.selected == Some(j) {
                            self.selected = Some(i);
                        }
                    }
                }
            }
            if self.tool.input() == Input::Pdf {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Mot de passe d'ouverture :");
                    ui.add(
                        TextEdit::singleline(&mut self.settings.password)
                            .password(true)
                            .hint_text("si le PDF est protégé")
                            .desired_width(200.0),
                    );
                });
            }
        });
    }

    fn options_card(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.settings;
        let tool = self.tool;
        let uses_pages = matches!(
            tool,
            Tool::Rotate
                | Tool::Crop
                | Tool::Watermark
                | Tool::PageNumbers
                | Tool::StampImage
                | Tool::PdfToImages
                | Tool::PdfToText
                | Tool::ExtractImages
                | Tool::Flatten
        );
        let has_options = uses_pages || !matches!(tool, Tool::Unlock | Tool::Info);
        if !has_options {
            return;
        }
        card(ui, |ui| {
            ui.label(RichText::new("Options").strong().size(16.0));
            ui.add_space(4.0);
            egui::Grid::new("options").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                match tool {
                    Tool::Merge => {
                        ui.label("Signets");
                        ui.checkbox(&mut s.merge_bookmarks, "Ajouter un signet par document");
                        ui.end_row();
                        ui.label("");
                        ui.weak("L'ordre de la liste est l'ordre de fusion (utilisez ▲ ▼).");
                        ui.end_row();
                    }
                    Tool::Split => {
                        ui.label("Mode");
                        ui.vertical(|ui| {
                            ui.radio_value(&mut s.split_mode, SplitMode::EachPage, "Une page par fichier");
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut s.split_mode, SplitMode::EveryN, "Blocs de");
                                ui.add(DragValue::new(&mut s.split_n).range(1..=10_000));
                                ui.label("pages");
                            });
                            ui.radio_value(&mut s.split_mode, SplitMode::Ranges, "Selon des plages (un fichier par plage)");
                        });
                        ui.end_row();
                        if s.split_mode == SplitMode::Ranges {
                            ui.label("Plages");
                            ui.add(TextEdit::singleline(&mut s.split_ranges).hint_text("1-3, 4-8, 9-").desired_width(260.0));
                            ui.end_row();
                        }
                    }
                    Tool::Extract | Tool::Delete => {
                        ui.label(if tool == Tool::Extract { "Pages à extraire" } else { "Pages à supprimer" });
                        ui.add(TextEdit::singleline(&mut s.pages).hint_text("1-3, 7, 10-").desired_width(260.0));
                        ui.end_row();
                        pages_help(ui);
                    }
                    Tool::Reorder => {
                        ui.label("Mode");
                        ui.vertical(|ui| {
                            ui.radio_value(&mut s.reorder_mode, ReorderMode::Custom, "Ordre personnalisé");
                            ui.radio_value(&mut s.reorder_mode, ReorderMode::Reverse, "Inverser l'ordre des pages");
                            ui.radio_value(&mut s.reorder_mode, ReorderMode::Interleave, "Entrelacer recto / verso (versos scannés à l'envers)");
                        });
                        ui.end_row();
                        if s.reorder_mode == ReorderMode::Custom {
                            ui.label("Nouvel ordre");
                            ui.add(TextEdit::singleline(&mut s.reorder_spec).hint_text("3, 1, 2, 4-").desired_width(260.0));
                            ui.end_row();
                            ui.label("");
                            ui.weak("Les pages non citées sont ajoutées à la fin. Une page citée deux fois est dupliquée.");
                            ui.end_row();
                        }
                    }
                    Tool::Rotate => {
                        ui.label("Angle");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut s.rotate_angle, 90, "90° horaire");
                            ui.radio_value(&mut s.rotate_angle, 180, "180°");
                            ui.radio_value(&mut s.rotate_angle, 270, "90° anti-horaire");
                        });
                        ui.end_row();
                    }
                    Tool::InsertBlank => {
                        ui.label("Après les pages");
                        ui.add(TextEdit::singleline(&mut s.blank_after).hint_text("0, 2, fin").desired_width(260.0));
                        ui.end_row();
                        ui.label("");
                        ui.weak("0 = au début, « fin » = à la fin, « tout » = après chaque page (impression recto seul).");
                        ui.end_row();
                    }
                    Tool::Crop => {
                        for (i, label) in ["Haut", "Droite", "Bas", "Gauche"].iter().enumerate() {
                            ui.label(*label);
                            ui.add(DragValue::new(&mut s.crop_mm[i]).range(0.0..=300.0).speed(0.5).suffix(" mm"));
                            ui.end_row();
                        }
                    }
                    Tool::Watermark => {
                        ui.label("Texte");
                        ui.add(TextEdit::singleline(&mut s.wm_text).desired_width(260.0));
                        ui.end_row();
                        ui.label("Taille");
                        ui.add(Slider::new(&mut s.wm_size, 8.0..=200.0).suffix(" pt"));
                        ui.end_row();
                        ui.label("Opacité");
                        percent_slider(ui, &mut s.wm_opacity);
                        ui.end_row();
                        ui.label("Angle");
                        ui.add(Slider::new(&mut s.wm_angle, -90.0..=90.0).suffix("°"));
                        ui.end_row();
                        ui.label("Couleur");
                        ui.color_edit_button_rgb(&mut s.wm_color);
                        ui.end_row();
                        ui.label("Position");
                        anchor_combo(ui, "wm_anchor", &mut s.wm_anchor);
                        ui.end_row();
                    }
                    Tool::PageNumbers => {
                        ui.label("Texte");
                        ui.add(TextEdit::singleline(&mut s.num_template).desired_width(260.0));
                        ui.end_row();
                        ui.label("Modèles");
                        ui.horizontal_wrapped(|ui| {
                            for t in ["{n}", "Page {n} / {total}", "{n} / {total}", "{fichier} - {date}", "Confidentiel - page {n}"] {
                                if ui.small_button(t).clicked() {
                                    s.num_template = t.into();
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("Position");
                        anchor_combo(ui, "num_anchor", &mut s.num_anchor);
                        ui.end_row();
                        ui.label("Taille");
                        ui.add(Slider::new(&mut s.num_size, 6.0..=36.0).suffix(" pt"));
                        ui.end_row();
                        ui.label("Marge");
                        ui.add(DragValue::new(&mut s.num_margin).range(0.0..=100.0).speed(0.5).suffix(" mm"));
                        ui.end_row();
                        ui.label("Premier numéro");
                        ui.add(DragValue::new(&mut s.num_start).range(-9999..=99_999));
                        ui.end_row();
                        ui.label("Couleur");
                        ui.color_edit_button_rgb(&mut s.num_color);
                        ui.end_row();
                    }
                    Tool::StampImage => {
                        ui.label("Image");
                        ui.horizontal(|ui| {
                            if ui.button("Choisir...").clicked()
                                && let Some(p) = rfd::FileDialog::new().add_filter("Images", &crate::images::IMAGE_EXTENSIONS).pick_file()
                            {
                                s.stamp_image = Some(p);
                            }
                            match &s.stamp_image {
                                Some(p) => ui.label(display_name(p)),
                                None => ui.weak("aucune (PNG transparent conseillé)"),
                            };
                        });
                        ui.end_row();
                        ui.label("Position");
                        anchor_combo(ui, "stamp_anchor", &mut s.stamp_anchor);
                        ui.end_row();
                        ui.label("Largeur");
                        ui.add(Slider::new(&mut s.stamp_width, 5.0..=200.0).suffix(" mm"));
                        ui.end_row();
                        ui.label("Marge");
                        ui.add(DragValue::new(&mut s.stamp_margin).range(0.0..=100.0).speed(0.5).suffix(" mm"));
                        ui.end_row();
                        ui.label("Opacité");
                        percent_slider(ui, &mut s.stamp_opacity);
                        ui.end_row();
                    }
                    Tool::Metadata => {
                        ui.label("Action");
                        ui.vertical(|ui| {
                            ui.radio_value(&mut s.meta_strip, false, "Modifier les métadonnées");
                            ui.radio_value(&mut s.meta_strip, true, "Tout effacer (anonymiser)");
                        });
                        ui.end_row();
                        if !s.meta_strip {
                            for (key, value) in s.meta.iter_mut() {
                                let label = pdf::META_KEYS.iter().find(|(k, _)| k == key).map(|(_, l)| *l).unwrap_or(key);
                                ui.label(label);
                                ui.add(TextEdit::singleline(value).desired_width(300.0));
                                ui.end_row();
                            }
                            ui.label("");
                            let file = self.selected.and_then(|i| self.files.get(i)).map(|f| f.path.clone());
                            if ui.add_enabled(file.is_some(), egui::Button::new("Lire depuis le fichier sélectionné")).clicked()
                                && let Some(path) = file
                                && let Ok(doc) = pdf::load(&path, &s.password)
                            {
                                let meta = pdf::read_metadata(&doc);
                                for (key, value) in s.meta.iter_mut() {
                                    *value = meta.get(key).cloned().unwrap_or_default();
                                }
                            }
                            ui.end_row();
                        }
                    }
                    Tool::Compress => {
                        ui.label("Images");
                        ui.checkbox(&mut s.cmp_images, "Recompresser les images en JPEG");
                        ui.end_row();
                        if s.cmp_images {
                            ui.label("Niveau");
                            ui.horizontal(|ui| {
                                for (label, q, side) in [("Léger", 85u8, 3000u32), ("Moyen", 70, 2000), ("Fort", 50, 1400), ("Extrême", 35, 1000)] {
                                    if ui.selectable_label(s.cmp_quality == q && s.cmp_max_side == side, label).clicked() {
                                        s.cmp_quality = q;
                                        s.cmp_max_side = side;
                                    }
                                }
                            });
                            ui.end_row();
                            ui.label("Qualité JPEG");
                            ui.add(Slider::new(&mut s.cmp_quality, 10..=95));
                            ui.end_row();
                            ui.label("Côté max.");
                            ui.add(DragValue::new(&mut s.cmp_max_side).range(0..=20_000).speed(10).suffix(" px"));
                            ui.end_row();
                        }
                        ui.label("Métadonnées");
                        ui.checkbox(&mut s.cmp_strip, "Supprimer les métadonnées");
                        ui.end_row();
                    }
                    Tool::Protect => {
                        ui.label("Mot de passe d'ouverture");
                        ui.add(TextEdit::singleline(&mut s.prot_user).password(true).hint_text("demandé pour lire").desired_width(220.0));
                        ui.end_row();
                        ui.label("Mot de passe propriétaire");
                        ui.add(TextEdit::singleline(&mut s.prot_owner).password(true).hint_text("pour lever les restrictions").desired_width(220.0));
                        ui.end_row();
                        ui.label("Autoriser");
                        ui.vertical(|ui| {
                            ui.checkbox(&mut s.prot_print, "Impression");
                            ui.checkbox(&mut s.prot_copy, "Copie du texte et des images");
                            ui.checkbox(&mut s.prot_modify, "Modification et assemblage");
                            ui.checkbox(&mut s.prot_annotate, "Annotations et formulaires");
                        });
                        ui.end_row();
                    }
                    Tool::ImagesToPdf => {
                        ui.label("Format de page");
                        format_combo(ui, "img_format", &mut s.img_format, true);
                        ui.end_row();
                        if s.img_format != PageFormat::FitImage {
                            ui.label("Marge");
                            ui.add(DragValue::new(&mut s.img_margin).range(0.0..=100.0).speed(0.5).suffix(" mm"));
                            ui.end_row();
                        }
                        ui.label("Sortie");
                        ui.vertical(|ui| {
                            ui.radio_value(&mut s.img_single, true, "Un seul PDF (ordre de la liste)");
                            ui.radio_value(&mut s.img_single, false, "Un PDF par image");
                        });
                        ui.end_row();
                    }
                    Tool::TextToPdf => {
                        ui.label("Format de page");
                        format_combo(ui, "txt_format", &mut s.txt_format, false);
                        ui.end_row();
                        ui.label("Taille du texte");
                        ui.add(Slider::new(&mut s.txt_size, 6.0..=24.0).suffix(" pt"));
                        ui.end_row();
                        ui.label("Marge");
                        ui.add(DragValue::new(&mut s.txt_margin).range(5.0..=80.0).speed(0.5).suffix(" mm"));
                        ui.end_row();
                    }
                    Tool::PdfToImages => {
                        ui.label("Format");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut s.raster_format, RasterFormat::Png, "PNG");
                            ui.radio_value(&mut s.raster_format, RasterFormat::Jpeg, "JPEG");
                        });
                        ui.end_row();
                        ui.label("Résolution");
                        ui.add(Slider::new(&mut s.raster_dpi, 36.0..=600.0).suffix(" ppp"));
                        ui.end_row();
                        if s.raster_format == RasterFormat::Jpeg {
                            ui.label("Qualité");
                            ui.add(Slider::new(&mut s.raster_quality, 10..=100));
                            ui.end_row();
                        }
                    }
                    Tool::Flatten => {
                        ui.label("Résolution");
                        ui.add(Slider::new(&mut s.flatten_dpi, 72.0..=300.0).suffix(" ppp"));
                        ui.end_row();
                    }
                    _ => {}
                }
                if uses_pages {
                    ui.label("Pages");
                    ui.add(TextEdit::singleline(&mut s.pages).hint_text("vide = toutes (ex. 1-3, 7, paires)").desired_width(260.0));
                    ui.end_row();
                }
            });
        });
    }

    fn result_card(&mut self, ui: &mut egui::Ui) {
        let Some(result) = &self.result else { return };
        card(ui, |ui| match result {
            Err(e) => {
                ui.label(
                    RichText::new("Le traitement n'a pas abouti")
                        .strong()
                        .color(ui.visuals().error_fg_color),
                );
                ui.label(e);
            }
            Ok(outcome) => {
                if self.tool.writes_files() {
                    ui.label(
                        RichText::new(format!("Terminé : {} fichier(s) créé(s)", outcome.outputs.len()))
                            .strong()
                            .color(Color32::from_rgb(0x2F, 0x9E, 0x44)),
                    );
                }
                if !outcome.report.is_empty() {
                    ui.add_space(4.0);
                    let mut text = outcome.report.as_str();
                    ui.add(
                        TextEdit::multiline(&mut text)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY),
                    );
                }
                if !outcome.outputs.is_empty() {
                    ui.add_space(4.0);
                    ScrollArea::vertical()
                        .max_height(160.0)
                        .id_salt("outputs")
                        .show(ui, |ui| {
                            for path in outcome.outputs.iter().take(200) {
                                ui.horizontal(|ui| {
                                    if ui.link(display_name(path)).on_hover_text("Ouvrir").clicked() {
                                        open_path(path);
                                    }
                                    ui.weak(human_size(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)));
                                });
                            }
                            if outcome.outputs.len() > 200 {
                                ui.weak(format!("... et {} autre(s)", outcome.outputs.len() - 200));
                            }
                        });
                    if ui.button("Afficher dans l'explorateur").clicked() {
                        reveal(&outcome.outputs[0]);
                    }
                }
            }
        });
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if self.tool.writes_files() {
                ui.label("Enregistrer dans :");
                ui.radio_value(&mut self.same_dir, true, "le dossier d'origine");
                ui.radio_value(&mut self.same_dir, false, "un autre dossier :");
                let label = self
                    .output_dir
                    .as_ref()
                    .map(|d| d.display().to_string())
                    .unwrap_or_else(|| "choisir un dossier...".into());
                if ui.button(label).clicked()
                    && let Some(dir) = rfd::FileDialog::new().set_title("Dossier de sortie").pick_folder()
                {
                    self.output_dir = Some(dir);
                    self.same_dir = false;
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let text = if self.busy() {
                    "Traitement..."
                } else if self.tool == Tool::Info {
                    "Analyser"
                } else {
                    "Lancer"
                };
                let button = egui::Button::new(RichText::new(text).strong().size(16.0).color(Color32::WHITE))
                    .fill(ACCENT)
                    .min_size(Vec2::new(140.0, 36.0));
                if ui.add_enabled(!self.busy() && !self.files.is_empty(), button).clicked() {
                    let ctx = ui.ctx().clone();
                    self.start(&ctx);
                }
                ui.add_space(10.0);
                ui.add(
                    ProgressBar::new(self.progress.0)
                        .text(self.progress.1.clone())
                        .desired_width(ui.available_width())
                        .animate(self.busy()),
                );
            });
        });
        ui.add_space(8.0);
    }
}

impl eframe::App for PdfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_job();
        self.poll_preview(&ctx);

        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() && !self.busy() {
            self.add_files(dropped);
        }
        self.request_preview(&ctx);

        egui::Panel::top("entete")
            .frame(
                Frame::new()
                    .fill(header_fill(ui))
                    .inner_margin(Margin::symmetric(18, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("PDFool").size(26.0).strong().color(accent(ui)));
                    ui.add_space(8.0);
                    ui.weak("Les outils essentiels pour vos PDF, hors ligne et sans transfert.");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        egui::widgets::global_theme_preference_buttons(ui);
                        ui.weak(format!("v{}", env!("CARGO_PKG_VERSION")));
                    });
                });
            });
        egui::Panel::left("outils")
            .resizable(false)
            .exact_size(210.0)
            .show(ui, |ui| self.sidebar(ui));
        egui::Panel::right("apercu")
            .resizable(true)
            .default_size(330.0)
            .min_size(220.0)
            .show(ui, |ui| self.preview_panel(ui));
        egui::Panel::bottom("actions").show(ui, |ui| self.bottom_bar(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new(self.tool.label()).size(22.0).strong());
                ui.label(self.tool.description());
                ui.add_space(8.0);
                self.files_card(ui);
                ui.add_space(8.0);
                self.options_card(ui);
                ui.add_space(8.0);
                self.result_card(ui);
            });
        });

        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if hovering {
            let screen = ctx.content_rect();
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("depot")));
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(140));
            painter.text(
                screen.center(),
                egui::Align2::CENTER_CENTER,
                "Déposez les fichiers pour les ajouter",
                egui::FontId::proportional(24.0),
                Color32::WHITE,
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Utilitaires d'interface

fn header_fill(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x1E, 0x1D, 0x20)
    } else {
        Color32::from_rgb(0xF5, 0xF1, 0xED)
    }
}

fn accent(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode { ACCENT_DARK } else { ACCENT }
}

fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) {
    Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn pages_help(ui: &mut egui::Ui) {
    ui.label("");
    ui.weak("Exemples : 1-3, 7 · 10- (jusqu'à la fin) · fin · paires · impaires · 5-1 (ordre inverse)");
    ui.end_row();
}

fn percent_slider(ui: &mut egui::Ui, value: &mut f32) {
    let mut pct = (*value * 100.0).round();
    if ui.add(Slider::new(&mut pct, 5.0..=100.0).suffix(" %")).changed() {
        *value = pct / 100.0;
    }
}

fn anchor_combo(ui: &mut egui::Ui, id: &str, value: &mut Anchor) {
    ComboBox::from_id_salt(id)
        .selected_text(value.label())
        .show_ui(ui, |ui| {
            for a in Anchor::ALL {
                ui.selectable_value(value, a, a.label());
            }
        });
}

fn format_combo(ui: &mut egui::Ui, id: &str, value: &mut PageFormat, allow_fit: bool) {
    ComboBox::from_id_salt(id)
        .selected_text(value.label())
        .show_ui(ui, |ui| {
            for f in PageFormat::ALL {
                if f == PageFormat::FitImage && !allow_fit {
                    continue;
                }
                ui.selectable_value(value, f, f.label());
            }
        });
}

fn describe_file(path: &Path) -> String {
    let size = std::fs::metadata(path).map(|m| human_size(m.len())).unwrap_or_default();
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        match lopdf::Document::load_metadata(path) {
            Ok(m) if m.encrypted && m.page_count == 0 => format!("protégé · {size}"),
            Ok(m) => format!(
                "{} p. · {size}{}",
                m.page_count,
                if m.encrypted { " · protégé" } else { "" }
            ),
            Err(_) => format!("{size} · illisible ?"),
        }
    } else {
        size
    }
}

fn render_preview(path: &Path, page: usize, password: &str, is_pdf: bool) -> Result<(egui::ColorImage, usize), String> {
    let (img, total) = if is_pdf {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let r = Renderer::open(data, password)?;
        let total = r.page_count();
        (r.render_fit(page.min(total.saturating_sub(1)), 900.0)?, total)
    } else {
        let img = crate::images::decode_file(path)?;
        let img = if img.width().max(img.height()) > 1200 {
            img.thumbnail(1200, 1200)
        } else {
            img
        };
        (img.to_rgba8(), 1)
    };
    let size = [img.width() as usize, img.height() as usize];
    Ok((egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), total))
}

fn open_path(path: &Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

fn reveal(path: &Path) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn();
    #[cfg(not(windows))]
    let _ = path
        .parent()
        .map(|p| std::process::Command::new("xdg-open").arg(p).spawn());
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Windows"))
        .join("Fonts");
    let mut primary = Vec::new();
    for (name, file) in [("segoe", "segoeui.ttf"), ("segoe_symbol", "seguisym.ttf")] {
        if let Ok(bytes) = std::fs::read(dir.join(file)) {
            fonts
                .font_data
                .insert(name.into(), Arc::new(egui::FontData::from_owned(bytes)));
            primary.push(name.to_string());
        }
    }
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        // Segoe UI en premier, symboles Segoe en secours, puis les polices egui.
        for (i, name) in primary.iter().enumerate() {
            list.insert(i, name.clone());
        }
    }
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Monospace)
        && primary.iter().any(|n| n == "segoe_symbol")
    {
        list.push("segoe_symbol".into());
    }
    ctx.set_fonts(fonts);
}

fn style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.5));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.5));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(11.5));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(18.0));
        let accent = if style.visuals.dark_mode { ACCENT_DARK } else { ACCENT };
        style.visuals.selection.bg_fill = accent.linear_multiply(0.35);
        style.visuals.selection.stroke = Stroke::new(1.0, accent);
        style.visuals.hyperlink_color = accent;
        style.visuals.widgets.noninteractive.corner_radius = CornerRadius::same(6);
        style.visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
        style.visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
        style.visuals.widgets.active.corner_radius = CornerRadius::same(6);
    });
}
