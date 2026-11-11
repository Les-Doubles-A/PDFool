//! Modifications visuelles : filigrane, numérotation / en-têtes, tampon image.

use std::path::Path;

use lopdf::{Document, Object, dictionary};

use crate::pdf::{Overlay, Res, visual_frame};
use crate::text::{HELVETICA_CAP_HEIGHT, encode_winansi, helvetica_width, hex_string, num, today};

const FONT_NAME: &str = "PDFoolF1";
const GS_NAME: &str = "PDFoolGS1";
const IMG_NAME: &str = "PDFoolIm1";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    Center,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Anchor {
    pub const ALL: [Anchor; 9] = [
        Anchor::TopLeft,
        Anchor::TopCenter,
        Anchor::TopRight,
        Anchor::MiddleLeft,
        Anchor::Center,
        Anchor::MiddleRight,
        Anchor::BottomLeft,
        Anchor::BottomCenter,
        Anchor::BottomRight,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Anchor::TopLeft => "Haut gauche",
            Anchor::TopCenter => "Haut centre",
            Anchor::TopRight => "Haut droite",
            Anchor::MiddleLeft => "Milieu gauche",
            Anchor::Center => "Centre",
            Anchor::MiddleRight => "Milieu droite",
            Anchor::BottomLeft => "Bas gauche",
            Anchor::BottomCenter => "Bas centre",
            Anchor::BottomRight => "Bas droite",
        }
    }

    /// Fractions horizontale et verticale (0 = gauche/bas, 1 = droite/haut).
    fn fractions(self) -> (f32, f32) {
        let col = match self {
            Anchor::TopLeft | Anchor::MiddleLeft | Anchor::BottomLeft => 0.0,
            Anchor::TopCenter | Anchor::Center | Anchor::BottomCenter => 0.5,
            _ => 1.0,
        };
        let row = match self {
            Anchor::TopLeft | Anchor::TopCenter | Anchor::TopRight => 1.0,
            Anchor::MiddleLeft | Anchor::Center | Anchor::MiddleRight => 0.5,
            _ => 0.0,
        };
        (col, row)
    }

    /// Coin inférieur gauche d'une boîte (w × h) placée dans une page (pw × ph) avec une marge.
    fn place(self, pw: f32, ph: f32, w: f32, h: f32, margin: f32) -> (f32, f32) {
        let (fx, fy) = self.fractions();
        let x = margin + (pw - 2.0 * margin - w) * fx;
        let y = margin + (ph - 2.0 * margin - h) * fy;
        (x, y)
    }
}

fn cm(m: [f32; 6]) -> String {
    format!(
        "{} {} {} {} {} {} cm\n",
        num(m[0]),
        num(m[1]),
        num(m[2]),
        num(m[3]),
        num(m[4]),
        num(m[5])
    )
}

fn rgb(c: [f32; 3]) -> String {
    format!("{} {} {} rg\n", num(c[0]), num(c[1]), num(c[2]))
}

fn add_font(doc: &mut Document, bold: bool) -> lopdf::ObjectId {
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => if bold { "Helvetica-Bold" } else { "Helvetica" },
        "Encoding" => "WinAnsiEncoding",
    })
}

fn add_gs(doc: &mut Document, opacity: f32) -> lopdf::ObjectId {
    doc.add_object(dictionary! {
        "Type" => "ExtGState",
        "ca" => Object::Real(opacity),
        "CA" => Object::Real(opacity),
    })
}

pub struct WatermarkOptions {
    pub text: String,
    pub size: f32,
    pub opacity: f32,
    pub angle: f32,
    pub color: [f32; 3],
    pub anchor: Anchor,
}

/// Filigrane texte en Helvetica, centré exactement grâce aux métriques AFM.
pub fn watermark(doc: &mut Document, pages: &[u32], o: &WatermarkOptions) -> Res<()> {
    if o.text.trim().is_empty() {
        return Err("Saisissez le texte du filigrane.".into());
    }
    let overlay = Overlay::new(doc);
    let font = add_font(doc, false);
    let gs = add_gs(doc, o.opacity);
    let encoded = encode_winansi(o.text.trim());
    let map = doc.get_pages();
    for n in pages {
        let id = map[n];
        let (m, pw, ph) = visual_frame(doc, id);
        let w = helvetica_width(&encoded, o.size);
        let h = o.size * HELVETICA_CAP_HEIGHT;
        let theta = o.angle.to_radians();
        let (s, c) = theta.sin_cos();
        // Encombrement du texte tourné, pour le positionner sans débordement.
        let bw = (w * c).abs() + (h * s).abs();
        let bh = (w * s).abs() + (h * c).abs();
        let (bx, by) = o.anchor.place(pw, ph, bw, bh, 20.0_f32.min(pw / 10.0));
        let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
        let mut content = String::from("q\n");
        content += &cm(m);
        content += &format!("/{GS_NAME} gs\n");
        content += &rgb(o.color);
        content += &format!("BT\n/{FONT_NAME} {} Tf\n", num(o.size));
        content += &format!(
            "{} {} {} {} {} {} Tm\n",
            num(c),
            num(s),
            num(-s),
            num(c),
            num(cx),
            num(cy)
        );
        content += &format!(
            "{} {} Td\n{} Tj\nET\nQ\n",
            num(-w / 2.0),
            num(-h / 2.0),
            hex_string(&encoded)
        );
        overlay.add_resource(doc, id, b"Font", FONT_NAME, Object::Reference(font))?;
        overlay.add_resource(doc, id, b"ExtGState", GS_NAME, Object::Reference(gs))?;
        overlay.stamp(doc, id, content.into_bytes())?;
    }
    Ok(())
}

pub struct StampTextOptions {
    pub template: String,
    pub anchor: Anchor,
    pub size: f32,
    pub margin_mm: f32,
    pub start: i64,
    pub color: [f32; 3],
}

/// Remplace les variables d'un modèle de texte.
pub fn expand_template(template: &str, page: i64, total: i64, file: &str) -> String {
    template
        .replace("{n}", &page.to_string())
        .replace("{total}", &total.to_string())
        .replace("{fichier}", file)
        .replace("{date}", &today())
}

/// Numérotation, en-tête ou pied de page texte.
pub fn stamp_text(doc: &mut Document, pages: &[u32], o: &StampTextOptions, source: &Path) -> Res<()> {
    if o.template.trim().is_empty() {
        return Err("Saisissez le texte à ajouter (par exemple : Page {n} / {total}).".into());
    }
    let overlay = Overlay::new(doc);
    let font = add_font(doc, false);
    let map = doc.get_pages();
    let file = crate::pdf::stem(source);
    let total = pages.len() as i64 + o.start - 1;
    let margin = o.margin_mm * 72.0 / 25.4;
    for (i, n) in pages.iter().enumerate() {
        let id = map[n];
        let (m, pw, ph) = visual_frame(doc, id);
        let text = expand_template(&o.template, o.start + i as i64, total, &file);
        let encoded = encode_winansi(&text);
        let w = helvetica_width(&encoded, o.size);
        let h = o.size * HELVETICA_CAP_HEIGHT;
        let (x, y) = o.anchor.place(pw, ph, w, h, margin);
        let mut content = String::from("q\n");
        content += &cm(m);
        content += &rgb(o.color);
        content += &format!(
            "BT\n/{FONT_NAME} {} Tf\n{} {} Td\n{} Tj\nET\nQ\n",
            num(o.size),
            num(x),
            num(y),
            hex_string(&encoded)
        );
        overlay.add_resource(doc, id, b"Font", FONT_NAME, Object::Reference(font))?;
        overlay.stamp(doc, id, content.into_bytes())?;
    }
    Ok(())
}

pub struct StampImageOptions {
    pub image: std::path::PathBuf,
    pub anchor: Anchor,
    pub width_mm: f32,
    pub margin_mm: f32,
    pub opacity: f32,
}

/// Appose une image (logo, signature, tampon) sur les pages choisies.
pub fn stamp_image(doc: &mut Document, pages: &[u32], o: &StampImageOptions) -> Res<()> {
    let overlay = Overlay::new(doc);
    let img = crate::images::embed_file(doc, &o.image)?;
    let gs = add_gs(doc, o.opacity);
    let map = doc.get_pages();
    let margin = o.margin_mm * 72.0 / 25.4;
    for n in pages {
        let id = map[n];
        let (m, pw, ph) = visual_frame(doc, id);
        let mut w = o.width_mm * 72.0 / 25.4;
        let mut h = w * img.height as f32 / img.width.max(1) as f32;
        let fit = ((pw - 2.0 * margin) / w).min((ph - 2.0 * margin) / h).min(1.0);
        w *= fit;
        h *= fit;
        let (x, y) = o.anchor.place(pw, ph, w, h, margin);
        let mut content = String::from("q\n");
        content += &cm(m);
        content += &format!("/{GS_NAME} gs\n");
        content += &cm([w, 0.0, 0.0, h, x, y]);
        content += &format!("/{IMG_NAME} Do\nQ\n");
        overlay.add_resource(doc, id, b"XObject", IMG_NAME, Object::Reference(img.id))?;
        overlay.add_resource(doc, id, b"ExtGState", GS_NAME, Object::Reference(gs))?;
        overlay.stamp(doc, id, content.into_bytes())?;
    }
    Ok(())
}
