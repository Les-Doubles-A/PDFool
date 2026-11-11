//! Conversions : images ↔ PDF, texte ↔ PDF, rendu des pages.

use std::collections::HashSet;
use std::path::Path;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};
use image::{DynamicImage, RgbaImage};
use lopdf::{Document, Object, Stream, dictionary};

use crate::images::{Embedded, embed_file, extract_stream};
use crate::pdf::{Res, display_name};
use crate::text::{COURIER_WIDTH, encode_winansi, hex_string, num};

pub const MM: f32 = 72.0 / 25.4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PageFormat {
    FitImage,
    A4,
    A3,
    A5,
    Letter,
}

impl PageFormat {
    pub const ALL: [PageFormat; 5] = [
        PageFormat::FitImage,
        PageFormat::A4,
        PageFormat::A3,
        PageFormat::A5,
        PageFormat::Letter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PageFormat::FitImage => "Taille de l'image",
            PageFormat::A4 => "A4",
            PageFormat::A3 => "A3",
            PageFormat::A5 => "A5",
            PageFormat::Letter => "Lettre US",
        }
    }

    /// Dimensions portrait en points.
    pub fn size(self) -> Option<(f32, f32)> {
        match self {
            PageFormat::FitImage => None,
            PageFormat::A4 => Some((595.28, 841.89)),
            PageFormat::A3 => Some((841.89, 1190.55)),
            PageFormat::A5 => Some((419.53, 595.28)),
            PageFormat::Letter => Some((612.0, 792.0)),
        }
    }
}

/// Construit un document vide prêt à recevoir des pages.
pub struct Builder {
    pub doc: Document,
    pages_id: lopdf::ObjectId,
    kids: Vec<Object>,
}

impl Builder {
    pub fn new() -> Self {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        Builder {
            doc,
            pages_id,
            kids: Vec::new(),
        }
    }

    pub fn add_page(&mut self, width: f32, height: f32, content: Vec<u8>, resources: lopdf::Dictionary) {
        let content_id = self.doc.add_object(Stream::new(dictionary! {}, content));
        let page = self.doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => self.pages_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(width), Object::Real(height)],
            "Contents" => content_id,
            "Resources" => resources,
        });
        self.kids.push(page.into());
    }

    pub fn finish(mut self) -> Document {
        let count = self.kids.len() as i64;
        self.doc.objects.insert(
            self.pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => self.kids, "Count" => count }),
        );
        let catalog = self
            .doc
            .add_object(dictionary! { "Type" => "Catalog", "Pages" => self.pages_id });
        self.doc.trailer.set("Root", catalog);
        self.doc
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ImagesToPdfOptions {
    pub format: PageFormat,
    pub margin_mm: f32,
    /// Résolution supposée pour « Taille de l'image ».
    pub dpi: f32,
}

/// Ajoute une page contenant l'image donnée.
pub fn add_image_page(b: &mut Builder, img: &Embedded, o: &ImagesToPdfOptions) {
    let margin = o.margin_mm * MM;
    let (iw, ih) = (img.width as f32, img.height as f32);
    let (pw, ph) = match o.format.size() {
        None => (iw * 72.0 / o.dpi + 2.0 * margin, ih * 72.0 / o.dpi + 2.0 * margin),
        Some((w, h)) => {
            // Orientation automatique selon l'image.
            if iw > ih { (h, w) } else { (w, h) }
        }
    };
    let scale = ((pw - 2.0 * margin) / iw).min((ph - 2.0 * margin) / ih);
    let (w, h) = (iw * scale, ih * scale);
    let (x, y) = ((pw - w) / 2.0, (ph - h) / 2.0);
    let content = format!("q\n{} 0 0 {} {} {} cm\n/Im0 Do\nQ\n", num(w), num(h), num(x), num(y));
    let resources = dictionary! { "XObject" => dictionary! { "Im0" => img.id } };
    b.add_page(pw, ph, content.into_bytes(), resources);
}

pub fn image_to_page(b: &mut Builder, path: &Path, o: &ImagesToPdfOptions) -> Res<()> {
    let img = embed_file(&mut b.doc, path)?;
    add_image_page(b, &img, o);
    Ok(())
}

pub struct TextToPdfOptions {
    pub size: f32,
    pub margin_mm: f32,
    pub format: PageFormat,
}

/// Met en page un texte brut en Courier (police à chasse fixe, coupure exacte des lignes).
pub fn text_to_pdf(text: &str, o: &TextToPdfOptions) -> Document {
    let (pw, ph) = o.format.size().unwrap_or((595.28, 841.89));
    let margin = o.margin_mm * MM;
    let leading = o.size * 1.25;
    let cols = (((pw - 2.0 * margin) / (o.size * COURIER_WIDTH)).floor() as usize).max(10);
    let rows = (((ph - 2.0 * margin) / leading).floor() as usize).max(5);
    let lines = wrap_text(text, cols);
    let mut b = Builder::new();
    let font = b.doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier", "Encoding" => "WinAnsiEncoding",
    });
    let chunks: Vec<&[String]> = if lines.is_empty() {
        vec![&[][..]]
    } else {
        lines.chunks(rows).collect()
    };
    for chunk in chunks {
        let mut content = format!(
            "BT\n/F1 {} Tf\n{} TL\n{} {} Td\n",
            num(o.size),
            num(leading),
            num(margin),
            num(ph - margin - o.size)
        );
        for line in chunk {
            content += &format!("{} Tj T*\n", hex_string(&encode_winansi(line)));
        }
        content += "ET\n";
        b.add_page(
            pw,
            ph,
            content.into_bytes(),
            dictionary! { "Font" => dictionary! { "F1" => font } },
        );
    }
    b.finish()
}

/// Coupe le texte en lignes de `cols` caractères au plus, en respectant les mots.
pub fn wrap_text(text: &str, cols: usize) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        let line = raw.replace('\t', "    ");
        if line.chars().count() <= cols {
            out.push(line.trim_end().to_string());
            continue;
        }
        let mut current = String::new();
        for word in line.split(' ') {
            let mut word = word.to_string();
            loop {
                let cur_len = current.chars().count();
                let word_len = word.chars().count();
                let needed = if current.is_empty() {
                    word_len
                } else {
                    cur_len + 1 + word_len
                };
                if needed <= cols {
                    if !current.is_empty() {
                        current.push(' ');
                    }
                    current.push_str(&word);
                    break;
                }
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                    continue;
                }
                // Mot plus long qu'une ligne : coupure franche.
                let head: String = word.chars().take(cols).collect();
                word = word.chars().skip(cols).collect();
                out.push(head);
                if word.is_empty() {
                    break;
                }
            }
        }
        out.push(current);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

/// Lit un fichier texte (UTF-8, sinon Windows-1252).
pub fn read_text_file(path: &Path) -> Res<String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{} : {e}", display_name(path)))?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes).to_vec();
    match String::from_utf8(bytes) {
        Ok(s) => Ok(s),
        Err(e) => Ok(e.into_bytes().iter().map(|&b| cp1252_char(b)).collect()),
    }
}

fn cp1252_char(b: u8) -> char {
    if !(0x80..0xA0).contains(&b) {
        return b as char;
    }
    (0u32..=0xFFFF)
        .filter_map(char::from_u32)
        .find(|&c| crate::text::winansi_code(c) == Some(b) && c as u32 >= 0x80)
        .unwrap_or('?')
}

/// Extrait le texte des pages données.
pub fn pdf_to_text(doc: &Document, pages: &[u32]) -> String {
    let mut out = String::new();
    for (i, n) in pages.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&format!("===== Page {n} =====\n"));
        match doc.extract_text(&[*n]) {
            Ok(t) => out.push_str(t.trim_end()),
            Err(_) => out.push_str("[texte non extractible]"),
        }
    }
    out.push('\n');
    out
}

/// Images intégrées d'un document. Renvoie (n° de page, image) en ignorant les doublons.
pub fn extract_images(doc: &Document, pages: &[u32]) -> (Vec<(u32, crate::images::Extracted)>, usize) {
    let map = doc.get_pages();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut skipped = 0;
    for n in pages {
        let Ok(images) = doc.get_page_images(map[n]) else {
            continue;
        };
        for img in images {
            if !seen.insert(img.id) {
                continue;
            }
            match doc.get_object(img.id).and_then(Object::as_stream) {
                Ok(stream) => match extract_stream(doc, stream) {
                    Some(e) => out.push((*n, e)),
                    None => skipped += 1,
                },
                Err(_) => skipped += 1,
            }
        }
    }
    (out, skipped)
}

// ---------------------------------------------------------------------------------------------
// Rendu des pages (hayro)

/// Document prêt pour le rendu.
pub struct Renderer {
    pdf: Pdf,
}

impl Renderer {
    pub fn open(data: Vec<u8>, password: &str) -> Res<Self> {
        let pdf = Pdf::new_with_password(data, password).map_err(|e| match e {
            hayro::hayro_syntax::LoadPdfError::Decryption(_) => {
                "Document protégé : mot de passe manquant ou incorrect.".to_string()
            }
            _ => "Document illisible.".to_string(),
        })?;
        Ok(Renderer { pdf })
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Rend une page pour qu'elle tienne dans un carré de `max_px` pixels.
    pub fn render_fit(&self, index: usize, max_px: f32) -> Res<RgbaImage> {
        let pages = self.pdf.pages();
        let page = pages.get(index).ok_or("Page introuvable.")?;
        let (w, h) = page.render_dimensions();
        self.render(index, 72.0 * max_px / w.max(h).max(1.0))
    }

    /// Rend une page (index à partir de 0) à la résolution donnée, sur fond blanc.
    pub fn render(&self, index: usize, dpi: f32) -> Res<RgbaImage> {
        let pages = self.pdf.pages();
        let page = pages.get(index).ok_or("Page introuvable.")?;
        let (w, h) = page.render_dimensions();
        // Limite de taille du tampon de rendu (u16) et de la mémoire.
        let max_side = 12_000.0;
        let scale = (dpi / 72.0).min(max_side / w.max(1.0)).min(max_side / h.max(1.0));
        let settings = InterpreterSettings::default();
        let cache = RenderCache::new();
        let pixmap = hayro::render(
            page,
            &cache,
            &settings,
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        );
        let (pw, ph) = (pixmap.width() as u32, pixmap.height() as u32);
        let data = pixmap.data_as_u8_slice().to_vec();
        RgbaImage::from_raw(pw, ph, data).ok_or_else(|| "Rendu impossible.".to_string())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RasterFormat {
    Png,
    Jpeg,
}

pub fn encode_raster(img: RgbaImage, format: RasterFormat, quality: u8) -> Res<Vec<u8>> {
    let mut out = Vec::new();
    match format {
        RasterFormat::Png => {
            DynamicImage::ImageRgba8(img)
                .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
                .map_err(|e| e.to_string())?;
        }
        RasterFormat::Jpeg => {
            let rgb = DynamicImage::ImageRgba8(img).to_rgb8();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
                .encode_image(&rgb)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

/// Recrée un PDF dont chaque page est une image (aplatit formulaires, annotations, calques).
pub fn rasterize_pdf(
    r: &Renderer,
    pages: &[u32],
    dpi: f32,
    quality: u8,
    progress: &dyn Fn(f32, String),
) -> Res<Document> {
    let mut b = Builder::new();
    let total = pages.len();
    for (i, n) in pages.iter().enumerate() {
        progress(i as f32 / total as f32, format!("Page {n}"));
        let img = r.render(*n as usize - 1, dpi)?;
        let (w, h) = (img.width(), img.height());
        let jpeg = encode_raster(img, RasterFormat::Jpeg, quality)?;
        let id = b.doc.add_object(
            Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
                    "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Filter" => "DCTDecode",
                },
                jpeg,
            )
            .with_compression(false),
        );
        let scale = 72.0 / dpi;
        let (pw, ph) = (w as f32 * scale, h as f32 * scale);
        let content = format!("q\n{} 0 0 {} 0 0 cm\n/Im0 Do\nQ\n", num(pw), num(ph));
        b.add_page(
            pw,
            ph,
            content.into_bytes(),
            dictionary! { "XObject" => dictionary! { "Im0" => id } },
        );
    }
    Ok(b.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_words() {
        assert_eq!(wrap_text("un deux trois", 7), vec!["un deux", "trois"]);
        assert_eq!(wrap_text("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap_text("a\n\nb\n\n", 10), vec!["a", "", "b"]);
    }

    #[test]
    fn cp1252() {
        assert_eq!(cp1252_char(0x80), '€');
        assert_eq!(cp1252_char(0xE9), 'é');
    }
}
