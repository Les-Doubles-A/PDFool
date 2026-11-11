//! Catalogue des outils et exécution des traitements sur une liste de fichiers.

use std::path::{Path, PathBuf};

use crate::compress::{CompressOptions, compress};
use crate::convert::{self, ImagesToPdfOptions, PageFormat, RasterFormat, Renderer, TextToPdfOptions};
use crate::edit::{self, Anchor, StampImageOptions, StampTextOptions, WatermarkOptions};
use crate::pages::{parse_groups, parse_ordered, parse_set};
use crate::pdf::{self, ProtectOptions, Res, display_name, human_size, load, save, save_compact, stem, unique_path};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tool {
    Merge,
    Split,
    Extract,
    Delete,
    Reorder,
    Rotate,
    InsertBlank,
    Crop,
    Watermark,
    PageNumbers,
    StampImage,
    Metadata,
    Compress,
    Protect,
    Unlock,
    ImagesToPdf,
    TextToPdf,
    PdfToImages,
    PdfToText,
    ExtractImages,
    Flatten,
    Info,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Pdf,
    Images,
    Text,
}

pub const CATEGORIES: [(&str, &[Tool]); 5] = [
    (
        "Organiser",
        &[
            Tool::Merge,
            Tool::Split,
            Tool::Extract,
            Tool::Delete,
            Tool::Reorder,
            Tool::Rotate,
            Tool::InsertBlank,
        ],
    ),
    (
        "Modifier",
        &[
            Tool::Watermark,
            Tool::PageNumbers,
            Tool::StampImage,
            Tool::Crop,
            Tool::Metadata,
        ],
    ),
    (
        "Convertir",
        &[
            Tool::ImagesToPdf,
            Tool::TextToPdf,
            Tool::PdfToImages,
            Tool::PdfToText,
            Tool::ExtractImages,
        ],
    ),
    ("Optimiser", &[Tool::Compress, Tool::Flatten]),
    ("Sécurité et infos", &[Tool::Protect, Tool::Unlock, Tool::Info]),
];

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Merge => "Fusionner",
            Tool::Split => "Séparer",
            Tool::Extract => "Extraire des pages",
            Tool::Delete => "Supprimer des pages",
            Tool::Reorder => "Réorganiser",
            Tool::Rotate => "Pivoter",
            Tool::InsertBlank => "Insérer des pages blanches",
            Tool::Crop => "Rogner les marges",
            Tool::Watermark => "Filigrane",
            Tool::PageNumbers => "Numéros et en-têtes",
            Tool::StampImage => "Logo / signature",
            Tool::Metadata => "Métadonnées",
            Tool::Compress => "Compresser",
            Tool::Protect => "Protéger par mot de passe",
            Tool::Unlock => "Retirer le mot de passe",
            Tool::ImagesToPdf => "Images vers PDF",
            Tool::TextToPdf => "Texte vers PDF",
            Tool::PdfToImages => "PDF vers images",
            Tool::PdfToText => "PDF vers texte",
            Tool::ExtractImages => "Extraire les images",
            Tool::Flatten => "Aplatir (PDF image)",
            Tool::Info => "Informations",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Tool::Merge => "Réunit les documents dans l'ordre de la liste en un seul PDF.",
            Tool::Split => "Découpe chaque document : une page par fichier, par blocs de N pages ou selon des plages.",
            Tool::Extract => "Crée un nouveau PDF avec les pages choisies, dans l'ordre saisi.",
            Tool::Delete => "Retire les pages choisies et conserve le reste.",
            Tool::Reorder => {
                "Change l'ordre des pages : ordre personnalisé, inversion, ou recto/verso scanné séparément."
            }
            Tool::Rotate => "Fait pivoter tout ou partie des pages.",
            Tool::InsertBlank => "Ajoute une page blanche après les pages choisies (0 = au début).",
            Tool::Crop => "Réduit la zone visible de chaque page en rognant les marges.",
            Tool::Watermark => "Ajoute un texte en surimpression (CONFIDENTIEL, BROUILLON...).",
            Tool::PageNumbers => {
                "Ajoute numéros de page, en-têtes ou pieds de page. Variables : {n}, {total}, {fichier}, {date}."
            }
            Tool::StampImage => "Appose une image (logo, signature scannée, tampon) sur les pages.",
            Tool::Metadata => "Modifie ou efface le titre, l'auteur, le sujet et les mots-clés.",
            Tool::Compress => "Réduit la taille : nettoyage de la structure et recompression des images.",
            Tool::Protect => "Chiffre le document (AES 128 bits) et limite impression, copie et modification.",
            Tool::Unlock => "Enregistre une copie sans mot de passe (le mot de passe actuel est requis).",
            Tool::ImagesToPdf => "Convertit des images (JPG, PNG, BMP, GIF, TIFF, WebP) en PDF.",
            Tool::TextToPdf => "Met en page des fichiers texte (.txt) en PDF.",
            Tool::PdfToImages => "Exporte chaque page en image PNG ou JPEG.",
            Tool::PdfToText => "Extrait le texte des pages dans un fichier .txt.",
            Tool::ExtractImages => "Enregistre les images contenues dans le document.",
            Tool::Flatten => {
                "Transforme chaque page en image : fige formulaires et annotations, empêche la sélection du texte."
            }
            Tool::Info => "Affiche les caractéristiques du document : pages, formats, polices, métadonnées.",
        }
    }

    pub fn input(self) -> Input {
        match self {
            Tool::ImagesToPdf => Input::Images,
            Tool::TextToPdf => Input::Text,
            _ => Input::Pdf,
        }
    }

    pub fn writes_files(self) -> bool {
        self != Tool::Info
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SplitMode {
    EachPage,
    EveryN,
    Ranges,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReorderMode {
    Custom,
    Reverse,
    Interleave,
}

#[derive(Clone)]
pub struct Settings {
    pub password: String,
    pub pages: String,
    pub merge_bookmarks: bool,
    pub split_mode: SplitMode,
    pub split_n: u32,
    pub split_ranges: String,
    pub reorder_mode: ReorderMode,
    pub reorder_spec: String,
    pub rotate_angle: i64,
    pub blank_after: String,
    pub crop_mm: [f32; 4],
    pub wm_text: String,
    pub wm_size: f32,
    pub wm_opacity: f32,
    pub wm_angle: f32,
    pub wm_color: [f32; 3],
    pub wm_anchor: Anchor,
    pub num_template: String,
    pub num_anchor: Anchor,
    pub num_size: f32,
    pub num_margin: f32,
    pub num_start: i64,
    pub num_color: [f32; 3],
    pub stamp_image: Option<PathBuf>,
    pub stamp_anchor: Anchor,
    pub stamp_width: f32,
    pub stamp_margin: f32,
    pub stamp_opacity: f32,
    pub meta: Vec<(String, String)>,
    pub meta_strip: bool,
    pub cmp_images: bool,
    pub cmp_quality: u8,
    pub cmp_max_side: u32,
    pub cmp_strip: bool,
    pub prot_user: String,
    pub prot_owner: String,
    pub prot_print: bool,
    pub prot_copy: bool,
    pub prot_modify: bool,
    pub prot_annotate: bool,
    pub img_format: PageFormat,
    pub img_margin: f32,
    pub img_single: bool,
    pub txt_size: f32,
    pub txt_margin: f32,
    pub txt_format: PageFormat,
    pub raster_dpi: f32,
    pub raster_format: RasterFormat,
    pub raster_quality: u8,
    pub flatten_dpi: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            password: String::new(),
            pages: String::new(),
            merge_bookmarks: true,
            split_mode: SplitMode::EachPage,
            split_n: 2,
            split_ranges: "1-3, 4-".into(),
            reorder_mode: ReorderMode::Custom,
            reorder_spec: String::new(),
            rotate_angle: 90,
            blank_after: "fin".into(),
            crop_mm: [10.0; 4],
            wm_text: "CONFIDENTIEL".into(),
            wm_size: 60.0,
            wm_opacity: 0.2,
            wm_angle: 45.0,
            wm_color: [0.8, 0.1, 0.1],
            wm_anchor: Anchor::Center,
            num_template: "Page {n} / {total}".into(),
            num_anchor: Anchor::BottomCenter,
            num_size: 10.0,
            num_margin: 10.0,
            num_start: 1,
            num_color: [0.0, 0.0, 0.0],
            stamp_image: None,
            stamp_anchor: Anchor::BottomRight,
            stamp_width: 40.0,
            stamp_margin: 15.0,
            stamp_opacity: 1.0,
            meta: pdf::META_KEYS
                .iter()
                .take(4)
                .map(|(k, _)| (k.to_string(), String::new()))
                .collect(),
            meta_strip: false,
            cmp_images: true,
            cmp_quality: 70,
            cmp_max_side: 2000,
            cmp_strip: false,
            prot_user: String::new(),
            prot_owner: String::new(),
            prot_print: true,
            prot_copy: false,
            prot_modify: false,
            prot_annotate: false,
            img_format: PageFormat::A4,
            img_margin: 10.0,
            img_single: true,
            txt_size: 10.0,
            txt_margin: 20.0,
            txt_format: PageFormat::A4,
            raster_dpi: 150.0,
            raster_format: RasterFormat::Png,
            raster_quality: 85,
            flatten_dpi: 150.0,
        }
    }
}

pub struct Outcome {
    pub outputs: Vec<PathBuf>,
    pub report: String,
}

pub type Progress<'a> = &'a dyn Fn(f32, String);

fn out_dir(output: &Option<PathBuf>, source: &Path) -> PathBuf {
    output
        .clone()
        .or_else(|| source.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn check_inputs(tool: Tool, files: &[PathBuf]) -> Res<()> {
    if files.is_empty() {
        return Err("Ajoutez au moins un fichier.".into());
    }
    let bad: Vec<String> = files
        .iter()
        .filter(|f| {
            let ext = f
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            match tool.input() {
                Input::Pdf => ext != "pdf",
                Input::Images => !crate::images::is_image(f),
                Input::Text => !matches!(ext.as_str(), "txt" | "md" | "csv" | "log" | "ini" | "json" | "xml"),
            }
        })
        .map(|f| display_name(f))
        .collect();
    if !bad.is_empty() {
        let expected = match tool.input() {
            Input::Pdf => "des fichiers PDF",
            Input::Images => "des images",
            Input::Text => "des fichiers texte",
        };
        return Err(format!(
            "Cet outil attend {expected}. Fichiers non pris en charge : {}",
            bad.join(", ")
        ));
    }
    Ok(())
}

/// Exécute l'outil sur la liste de fichiers.
pub fn run(tool: Tool, files: &[PathBuf], s: &Settings, output: &Option<PathBuf>, progress: Progress) -> Res<Outcome> {
    check_inputs(tool, files)?;
    if let Some(dir) = output {
        std::fs::create_dir_all(dir).map_err(|e| format!("Dossier de sortie inaccessible : {e}"))?;
    }
    match tool {
        Tool::Merge => merge(files, s, output, progress),
        Tool::ImagesToPdf => images_to_pdf(files, s, output, progress),
        _ => {
            let mut outputs = Vec::new();
            let mut reports = Vec::new();
            let n = files.len() as f32;
            for (i, file) in files.iter().enumerate() {
                let sub = |f: f32, msg: String| progress((i as f32 + f.clamp(0.0, 1.0)) / n, msg);
                sub(0.0, format!("{} ({}/{})", display_name(file), i + 1, files.len()));
                let dir = out_dir(output, file);
                let (mut out, report) = run_one(tool, file, s, &dir, &sub).map_err(|e| {
                    if files.len() > 1 && !e.contains(&display_name(file)) {
                        format!("{} : {e}", display_name(file))
                    } else {
                        e
                    }
                })?;
                outputs.append(&mut out);
                if !report.is_empty() {
                    reports.push(report);
                }
            }
            progress(1.0, "Terminé".into());
            Ok(Outcome {
                outputs,
                report: reports.join("\n\n"),
            })
        }
    }
}

fn merge(files: &[PathBuf], s: &Settings, output: &Option<PathBuf>, progress: Progress) -> Res<Outcome> {
    if files.len() < 2 {
        return Err("Ajoutez au moins deux PDF à fusionner.".into());
    }
    let mut docs = Vec::new();
    for (i, f) in files.iter().enumerate() {
        progress(
            i as f32 / files.len() as f32 * 0.8,
            format!("Lecture {}/{}", i + 1, files.len()),
        );
        docs.push((stem(f), load(f, &s.password)?));
    }
    progress(0.85, "Assemblage".into());
    let mut doc = pdf::merge(docs, s.merge_bookmarks)?;
    let path = unique_path(&out_dir(output, &files[0]), "PDFool_fusion.pdf");
    save(&mut doc, &path)?;
    progress(1.0, "Terminé".into());
    let report = format!("{} documents fusionnés, {} pages.", files.len(), pdf::page_count(&doc));
    Ok(Outcome {
        outputs: vec![path],
        report,
    })
}

fn image_options(s: &Settings) -> ImagesToPdfOptions {
    ImagesToPdfOptions {
        format: s.img_format,
        margin_mm: if s.img_format == PageFormat::FitImage {
            0.0
        } else {
            s.img_margin
        },
        dpi: 96.0,
    }
}

fn images_to_pdf(files: &[PathBuf], s: &Settings, output: &Option<PathBuf>, progress: Progress) -> Res<Outcome> {
    let opts = image_options(s);
    let mut outputs = Vec::new();
    if s.img_single {
        let mut b = convert::Builder::new();
        for (i, f) in files.iter().enumerate() {
            progress(
                i as f32 / files.len() as f32,
                format!("Image {}/{}", i + 1, files.len()),
            );
            convert::image_to_page(&mut b, f, &opts)?;
        }
        let mut doc = b.finish();
        let name = if files.len() == 1 {
            format!("{}.pdf", stem(&files[0]))
        } else {
            "PDFool_images.pdf".into()
        };
        let path = unique_path(&out_dir(output, &files[0]), &name);
        save(&mut doc, &path)?;
        outputs.push(path);
    } else {
        for (i, f) in files.iter().enumerate() {
            progress(
                i as f32 / files.len() as f32,
                format!("Image {}/{}", i + 1, files.len()),
            );
            let mut b = convert::Builder::new();
            convert::image_to_page(&mut b, f, &opts)?;
            let mut doc = b.finish();
            let path = unique_path(&out_dir(output, f), &format!("{}.pdf", stem(f)));
            save(&mut doc, &path)?;
            outputs.push(path);
        }
    }
    progress(1.0, "Terminé".into());
    Ok(Outcome {
        report: format!("{} image(s) convertie(s).", files.len()),
        outputs,
    })
}

fn file_size(p: &Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn run_one(tool: Tool, file: &Path, s: &Settings, dir: &Path, progress: Progress) -> Res<(Vec<PathBuf>, String)> {
    let name = stem(file);
    let out = |suffix: &str, ext: &str| unique_path(dir, &format!("{name}_{suffix}.{ext}"));

    // Outils qui ne passent pas par lopdf.
    match tool {
        Tool::TextToPdf => {
            let text = convert::read_text_file(file)?;
            let mut doc = convert::text_to_pdf(
                &text,
                &TextToPdfOptions {
                    size: s.txt_size,
                    margin_mm: s.txt_margin,
                    format: s.txt_format,
                },
            );
            let path = unique_path(dir, &format!("{name}.pdf"));
            save(&mut doc, &path)?;
            let pages = pdf::page_count(&doc);
            return Ok((vec![path], format!("{} : {pages} page(s).", display_name(file))));
        }
        Tool::PdfToImages | Tool::Flatten => {
            let data = std::fs::read(file).map_err(|e| e.to_string())?;
            let renderer = Renderer::open(data, &s.password)?;
            let total = renderer.page_count() as u32;
            let pages = parse_set(&s.pages, total)?;
            if tool == Tool::Flatten {
                let mut doc = convert::rasterize_pdf(&renderer, &pages, s.flatten_dpi, 85, progress)?;
                let path = out("image", "pdf");
                save(&mut doc, &path)?;
                return Ok((
                    vec![path.clone()],
                    format!(
                        "{} : {} → {}",
                        display_name(file),
                        human_size(file_size(file)),
                        human_size(file_size(&path))
                    ),
                ));
            }
            let ext = if s.raster_format == RasterFormat::Png {
                "png"
            } else {
                "jpg"
            };
            let mut outputs = Vec::new();
            for (i, n) in pages.iter().enumerate() {
                progress(i as f32 / pages.len() as f32, format!("Page {n}/{total}"));
                let img = renderer.render(*n as usize - 1, s.raster_dpi)?;
                let bytes = convert::encode_raster(img, s.raster_format, s.raster_quality)?;
                let path = out(&format!("page_{n:0width$}", width = total.to_string().len()), ext);
                std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
                outputs.push(path);
            }
            let report = format!("{} : {} image(s) créée(s).", display_name(file), outputs.len());
            return Ok((outputs, report));
        }
        _ => {}
    }

    let mut doc = load(file, &s.password)?;
    let total = pdf::page_count(&doc);
    let selected = || parse_set(&s.pages, total);
    let mut report = String::new();
    let outputs = match tool {
        Tool::Split => {
            let groups: Vec<Vec<u32>> = match s.split_mode {
                SplitMode::EachPage => (1..=total).map(|p| vec![p]).collect(),
                SplitMode::EveryN => {
                    let n = s.split_n.max(1);
                    (1..=total)
                        .collect::<Vec<_>>()
                        .chunks(n as usize)
                        .map(|c| c.to_vec())
                        .collect()
                }
                SplitMode::Ranges => parse_groups(&s.split_ranges, total)?,
            };
            let width = total.to_string().len();
            let mut outputs = Vec::new();
            for (i, group) in groups.iter().enumerate() {
                progress(
                    i as f32 / groups.len() as f32,
                    format!("Partie {}/{}", i + 1, groups.len()),
                );
                let mut part = doc.clone();
                pdf::select_pages(&mut part, group)?;
                let first = group[0];
                let last = *group.last().unwrap();
                let suffix = if group.len() == 1 {
                    format!("page_{first:0width$}")
                } else {
                    format!("pages_{first:0width$}-{last:0width$}")
                };
                let path = out(&suffix, "pdf");
                save(&mut part, &path)?;
                outputs.push(path);
            }
            report = format!("{} : {} fichier(s) créé(s).", display_name(file), outputs.len());
            outputs
        }
        Tool::Extract => {
            if s.pages.trim().is_empty() {
                return Err("Indiquez les pages à extraire, par exemple : 1-3, 7.".into());
            }
            let order = parse_ordered(&s.pages, total)?;
            pdf::select_pages(&mut doc, &order)?;
            vec![save_to(&mut doc, out("extrait", "pdf"))?]
        }
        Tool::Delete => {
            if s.pages.trim().is_empty() {
                return Err("Indiquez les pages à supprimer, par exemple : 2, 5-6.".into());
            }
            let remove = parse_set(&s.pages, total)?;
            let keep: Vec<u32> = (1..=total).filter(|p| !remove.contains(p)).collect();
            if keep.is_empty() {
                return Err("Impossible de supprimer toutes les pages.".into());
            }
            pdf::select_pages(&mut doc, &keep)?;
            report = format!(
                "{} : {} page(s) supprimée(s), {} restante(s).",
                display_name(file),
                remove.len(),
                keep.len()
            );
            vec![save_to(&mut doc, out("pages_supprimees", "pdf"))?]
        }
        Tool::Reorder => {
            let order: Vec<u32> = match s.reorder_mode {
                ReorderMode::Reverse => (1..=total).rev().collect(),
                ReorderMode::Custom => {
                    let mut order = parse_ordered(&s.reorder_spec, total)?;
                    // Les pages non citées sont ajoutées à la fin dans leur ordre d'origine.
                    let rest: Vec<u32> = (1..=total).filter(|p| !order.contains(p)).collect();
                    order.extend(rest);
                    order
                }
                ReorderMode::Interleave => {
                    // Rectos (1re moitié) puis versos dans l'ordre inverse (2e moitié), scannés séparément.
                    let half = total.div_ceil(2);
                    let mut order = Vec::new();
                    for i in 0..half {
                        order.push(i + 1);
                        let back = total - i;
                        if back > half {
                            order.push(back);
                        }
                    }
                    order
                }
            };
            pdf::select_pages(&mut doc, &order)?;
            vec![save_to(&mut doc, out("reorganise", "pdf"))?]
        }
        Tool::Rotate => {
            pdf::rotate(&mut doc, &selected()?, s.rotate_angle)?;
            vec![save_to(&mut doc, out("pivote", "pdf"))?]
        }
        Tool::InsertBlank => {
            let after: Vec<u32> = if s.blank_after.trim() == "0" {
                vec![0]
            } else {
                let mut v = Vec::new();
                for part in s.blank_after.split(',') {
                    if part.trim() == "0" {
                        v.push(0);
                    } else if !part.trim().is_empty() {
                        v.extend(parse_set(part, total)?);
                    }
                }
                v
            };
            if after.is_empty() {
                return Err("Indiquez après quelles pages insérer une page blanche.".into());
            }
            pdf::insert_blank(&mut doc, &after)?;
            vec![save_to(&mut doc, out("pages_blanches", "pdf"))?]
        }
        Tool::Crop => {
            let [t, r, b, l] = s.crop_mm.map(|v| v * 72.0 / 25.4);
            pdf::crop(&mut doc, &selected()?, t, r, b, l)?;
            vec![save_to(&mut doc, out("rogne", "pdf"))?]
        }
        Tool::Watermark => {
            let o = WatermarkOptions {
                text: s.wm_text.clone(),
                size: s.wm_size,
                opacity: s.wm_opacity,
                angle: s.wm_angle,
                color: s.wm_color,
                anchor: s.wm_anchor,
            };
            edit::watermark(&mut doc, &selected()?, &o)?;
            vec![save_to(&mut doc, out("filigrane", "pdf"))?]
        }
        Tool::PageNumbers => {
            let o = StampTextOptions {
                template: s.num_template.clone(),
                anchor: s.num_anchor,
                size: s.num_size,
                margin_mm: s.num_margin,
                start: s.num_start,
                color: s.num_color,
            };
            edit::stamp_text(&mut doc, &selected()?, &o, file)?;
            vec![save_to(&mut doc, out("numerote", "pdf"))?]
        }
        Tool::StampImage => {
            let image = s.stamp_image.clone().ok_or("Choisissez l'image à apposer.")?;
            let o = StampImageOptions {
                image,
                anchor: s.stamp_anchor,
                width_mm: s.stamp_width,
                margin_mm: s.stamp_margin,
                opacity: s.stamp_opacity,
            };
            edit::stamp_image(&mut doc, &selected()?, &o)?;
            vec![save_to(&mut doc, out("tampon", "pdf"))?]
        }
        Tool::Metadata => {
            if s.meta_strip {
                pdf::strip_metadata(&mut doc);
            } else {
                pdf::write_metadata(&mut doc, &s.meta);
            }
            vec![save_to(&mut doc, out("metadonnees", "pdf"))?]
        }
        Tool::Compress => {
            let o = CompressOptions {
                images: s.cmp_images,
                quality: s.cmp_quality,
                max_side: s.cmp_max_side,
                strip_metadata: s.cmp_strip,
            };
            let r = compress(&mut doc, &o, progress);
            let path = out("compresse", "pdf");
            save_compact(&mut doc, &path)?;
            let (before, after) = (file_size(file), file_size(&path));
            let gain = if before > 0 {
                100.0 - after as f64 * 100.0 / before as f64
            } else {
                0.0
            };
            report = format!(
                "{} : {} → {} ({}{:.0} %), {} image(s) recompressée(s) sur {}.",
                display_name(file),
                human_size(before),
                human_size(after),
                if gain >= 0.0 { "-" } else { "+" },
                gain.abs(),
                r.images_recompressed,
                r.images_seen
            );
            vec![path]
        }
        Tool::Protect => {
            let o = ProtectOptions {
                user_password: s.prot_user.clone(),
                owner_password: s.prot_owner.clone(),
                allow_print: s.prot_print,
                allow_copy: s.prot_copy,
                allow_modify: s.prot_modify,
                allow_annotate: s.prot_annotate,
            };
            doc.change_producer("PDFool");
            doc.prune_objects();
            doc.compress();
            pdf::protect(&mut doc, &o)?;
            let path = out("protege", "pdf");
            doc.save(&path).map_err(|e| format!("Écriture impossible : {e}"))?;
            vec![path]
        }
        Tool::Unlock => {
            if s.password.is_empty() {
                return Err("Saisissez le mot de passe actuel du document dans « Mot de passe d'ouverture ».".into());
            }
            vec![save_to(&mut doc, out("deverrouille", "pdf"))?]
        }
        Tool::PdfToText => {
            let text = convert::pdf_to_text(&doc, &selected()?);
            let path = out("texte", "txt");
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(text.replace('\n', "\r\n").as_bytes());
            std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
            vec![path]
        }
        Tool::ExtractImages => {
            let (images, skipped) = convert::extract_images(&doc, &selected()?);
            let mut outputs = Vec::new();
            for (i, (page, img)) in images.iter().enumerate() {
                let path = out(&format!("p{page}_img{}", i + 1), img.ext);
                std::fs::write(&path, &img.bytes).map_err(|e| e.to_string())?;
                outputs.push(path);
            }
            report = format!("{} : {} image(s) extraite(s)", display_name(file), outputs.len());
            if skipped > 0 {
                report += &format!(", {skipped} ignorée(s) (format non pris en charge)");
            }
            report.push('.');
            outputs
        }
        Tool::Info => {
            report = pdf::describe(file, &doc);
            Vec::new()
        }
        Tool::Merge | Tool::ImagesToPdf | Tool::TextToPdf | Tool::PdfToImages | Tool::Flatten => {
            unreachable!()
        }
    };
    progress(1.0, "Terminé".into());
    Ok((outputs, report))
}

fn save_to(doc: &mut lopdf::Document, path: PathBuf) -> Res<PathBuf> {
    save(doc, &path)?;
    Ok(path)
}
