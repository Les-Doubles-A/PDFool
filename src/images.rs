//! Intégration d'images dans un PDF et lecture des images qu'il contient.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageReader, metadata::Orientation};
use lopdf::{Document, Object, ObjectId, Stream, dictionary};

use crate::pdf::{Res, display_name};

pub const IMAGE_EXTENSIONS: [&str; 9] = ["jpg", "jpeg", "png", "bmp", "gif", "tif", "tiff", "webp", "jfif"];

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_string_lossy().to_lowercase().as_str()))
        .unwrap_or(false)
}

pub struct Embedded {
    pub id: ObjectId,
    pub width: u32,
    pub height: u32,
}

/// Informations d'en-tête JPEG utiles pour l'intégrer tel quel (sans recompression).
struct JpegInfo {
    width: u32,
    height: u32,
    components: u8,
    precision: u8,
}

fn jpeg_info(data: &[u8]) -> Option<JpegInfo> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        let is_sof = (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_sof {
            let seg = data.get(i + 4..i + 2 + len)?;
            if seg.len() < 6 {
                return None;
            }
            return Some(JpegInfo {
                precision: seg[0],
                height: u16::from_be_bytes([seg[1], seg[2]]) as u32,
                width: u16::from_be_bytes([seg[3], seg[4]]) as u32,
                components: seg[5],
            });
        }
        if marker == 0xDA {
            return None;
        }
        i += 2 + len;
    }
    None
}

fn jpeg_orientation(data: &[u8]) -> Orientation {
    image::codecs::jpeg::JpegDecoder::new(Cursor::new(data))
        .and_then(|mut d| d.orientation())
        .unwrap_or(Orientation::NoTransforms)
}

/// Décode une image en appliquant l'orientation EXIF.
pub fn decode_file(path: &Path) -> Res<DynamicImage> {
    let name = display_name(path);
    let reader = ImageReader::open(path)
        .map_err(|e| format!("{name} : {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("{name} : {e}"))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| format!("{name} : format non pris en charge ({e})"))?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| format!("{name} : image illisible ({e})"))?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// Intègre un fichier image. Les JPEG RVB ou niveaux de gris sont copiés sans perte de qualité.
pub fn embed_file(doc: &mut Document, path: &Path) -> Res<Embedded> {
    let data = std::fs::read(path).map_err(|e| format!("{} : {e}", display_name(path)))?;
    if let Some(info) = jpeg_info(&data)
        && info.precision == 8
        && matches!(info.components, 1 | 3)
        && jpeg_orientation(&data) == Orientation::NoTransforms
    {
        let dict = dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => info.width as i64,
            "Height" => info.height as i64,
            "ColorSpace" => if info.components == 1 { "DeviceGray" } else { "DeviceRGB" },
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        };
        let id = doc.add_object(Stream::new(dict, data).with_compression(false));
        return Ok(Embedded {
            id,
            width: info.width,
            height: info.height,
        });
    }
    let img = decode_file(path)?;
    Ok(embed_dynamic(doc, &img))
}

/// Intègre une image décodée (flux compressé sans perte, avec masque de transparence si besoin).
pub fn embed_dynamic(doc: &mut Document, img: &DynamicImage) -> Embedded {
    let (width, height) = (img.width(), img.height());
    let has_alpha = img.color().has_alpha();
    let is_gray = !img.color().has_color();
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => width as i64,
        "Height" => height as i64,
        "BitsPerComponent" => 8,
    };
    let pixels = if is_gray {
        dict.set("ColorSpace", "DeviceGray");
        img.to_luma8().into_raw()
    } else {
        dict.set("ColorSpace", "DeviceRGB");
        img.to_rgb8().into_raw()
    };
    if has_alpha {
        let alpha: Vec<u8> = img.to_rgba8().pixels().map(|p| p.0[3]).collect();
        if alpha.iter().any(|&a| a != 255) {
            let smask = dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => width as i64,
                "Height" => height as i64,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            };
            let mut stream = Stream::new(smask, alpha);
            let _ = stream.compress();
            let smask_id = doc.add_object(stream);
            dict.set("SMask", smask_id);
        }
    }
    let mut stream = Stream::new(dict, pixels);
    let _ = stream.compress();
    let id = doc.add_object(stream);
    Embedded { id, width, height }
}

/// Image extraite d'un PDF : extension et contenu prêt à écrire.
pub struct Extracted {
    pub ext: &'static str,
    pub bytes: Vec<u8>,
}

/// Convertit un flux image PDF en fichier (JPEG/JPEG 2000 tels quels, sinon PNG). `None` si non pris en charge.
pub fn extract_stream(doc: &Document, stream: &Stream) -> Option<Extracted> {
    let filters: Vec<Vec<u8>> = stream
        .filters()
        .map(|f| f.iter().map(|n| n.to_vec()).collect())
        .unwrap_or_default();
    match filters.last().map(|f| f.as_slice()) {
        Some(b"DCTDecode") if filters.len() == 1 => {
            return Some(Extracted {
                ext: "jpg",
                bytes: stream.content.clone(),
            });
        }
        Some(b"JPXDecode") if filters.len() == 1 => {
            return Some(Extracted {
                ext: "jp2",
                bytes: stream.content.clone(),
            });
        }
        _ => {}
    }
    let image = decode_raw(doc, stream)?;
    let mut out = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(Extracted { ext: "png", bytes: out })
}

/// Décode un flux image non JPEG (8 bits, RVB / gris, ou couleurs indexées RVB).
pub fn decode_raw(doc: &Document, stream: &Stream) -> Option<DynamicImage> {
    let dict = &stream.dict;
    let filters = stream.filters().unwrap_or_default();
    if filters.iter().any(|f| {
        !matches!(
            *f,
            b"FlateDecode" | b"LZWDecode" | b"ASCIIHexDecode" | b"ASCII85Decode" | b"RunLengthDecode"
        )
    }) {
        return None;
    }
    let width = dict.get(b"Width").ok()?.as_i64().ok()? as u32;
    let height = dict.get(b"Height").ok()?.as_i64().ok()? as u32;
    let bpc = dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(8);
    if dict.get(b"ImageMask").and_then(Object::as_bool).unwrap_or(false) {
        return None;
    }
    let data = if filters.is_empty() {
        stream.content.clone()
    } else {
        stream.decompressed_content().ok()?
    };
    let cs = dict
        .get(b"ColorSpace")
        .ok()
        .and_then(|o| doc.dereference(o).ok())
        .map(|(_, o)| o.clone());
    let (kind, palette) = match &cs {
        Some(Object::Name(n)) => (n.clone(), None),
        Some(Object::Array(a)) => {
            let first = a.first().and_then(|o| o.as_name().ok()).unwrap_or(b"").to_vec();
            if first == b"ICCBased" {
                let n = a
                    .get(1)
                    .and_then(|o| doc.dereference(o).ok())
                    .and_then(|(_, o)| o.as_stream().ok())
                    .and_then(|s| s.dict.get(b"N").ok().and_then(|n| n.as_i64().ok()))
                    .unwrap_or(3);
                (
                    if n == 1 {
                        b"DeviceGray".to_vec()
                    } else if n == 3 {
                        b"DeviceRGB".to_vec()
                    } else {
                        first
                    },
                    None,
                )
            } else if first == b"Indexed" {
                let base_rgb = a
                    .get(1)
                    .and_then(|o| doc.dereference(o).ok())
                    .and_then(|(_, o)| o.as_name().ok())
                    == Some(b"DeviceRGB");
                let lookup = a
                    .get(3)
                    .and_then(|o| doc.dereference(o).ok())
                    .and_then(|(_, o)| match o {
                        Object::String(s, _) => Some(s.clone()),
                        Object::Stream(s) => s.decompressed_content().ok().or_else(|| Some(s.content.clone())),
                        _ => None,
                    });
                if !base_rgb {
                    return None;
                }
                (first, lookup)
            } else {
                (first, None)
            }
        }
        _ => return None,
    };
    let pixels = (width as usize).checked_mul(height as usize)?;
    match (kind.as_slice(), bpc) {
        (b"DeviceRGB", 8) if data.len() >= pixels * 3 => {
            image::RgbImage::from_raw(width, height, data[..pixels * 3].to_vec()).map(DynamicImage::ImageRgb8)
        }
        (b"DeviceGray", 8) if data.len() >= pixels => {
            image::GrayImage::from_raw(width, height, data[..pixels].to_vec()).map(DynamicImage::ImageLuma8)
        }
        (b"DeviceCMYK", 8) if data.len() >= pixels * 4 => {
            let rgb: Vec<u8> = data[..pixels * 4]
                .chunks_exact(4)
                .flat_map(|p| {
                    let k = 255 - p[3] as u32;
                    [p[0], p[1], p[2]].map(|c| ((255 - c as u32) * k / 255) as u8)
                })
                .collect();
            image::RgbImage::from_raw(width, height, rgb).map(DynamicImage::ImageRgb8)
        }
        (b"Indexed", 8) if data.len() >= pixels => {
            let lut = palette?;
            let rgb: Vec<u8> = data[..pixels]
                .iter()
                .flat_map(|&i| {
                    let o = i as usize * 3;
                    [
                        lut.get(o).copied().unwrap_or(0),
                        lut.get(o + 1).copied().unwrap_or(0),
                        lut.get(o + 2).copied().unwrap_or(0),
                    ]
                })
                .collect();
            image::RgbImage::from_raw(width, height, rgb).map(DynamicImage::ImageRgb8)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_jpeg_header() {
        let img = DynamicImage::new_rgb8(31, 17);
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Jpeg)
            .unwrap();
        let info = jpeg_info(&buf).unwrap();
        assert_eq!(
            (info.width, info.height, info.components, info.precision),
            (31, 17, 3, 8)
        );
    }
}
