//! Optimisation de la taille des PDF : nettoyage de structure et recompression des images.

use image::DynamicImage;
use image::imageops::FilterType;
use lopdf::{Document, Object, ObjectId};

use crate::images::decode_raw;

pub struct CompressOptions {
    /// Recompresser les images en JPEG.
    pub images: bool,
    pub quality: u8,
    /// Plus grand côté autorisé (en pixels) ; 0 = pas de réduction.
    pub max_side: u32,
    pub strip_metadata: bool,
}

pub struct CompressReport {
    pub images_seen: usize,
    pub images_recompressed: usize,
}

pub fn compress(doc: &mut Document, o: &CompressOptions, progress: &dyn Fn(f32, String)) -> CompressReport {
    let mut report = CompressReport {
        images_seen: 0,
        images_recompressed: 0,
    };
    if o.strip_metadata {
        crate::pdf::strip_metadata(doc);
    }
    // Les miniatures intégrées sont superflues : les lecteurs les régénèrent.
    let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for id in pages {
        if let Ok(p) = doc.get_dictionary_mut(id) {
            p.remove(b"Thumb");
        }
    }
    if o.images {
        let ids: Vec<ObjectId> = doc
            .objects
            .iter()
            .filter(|(_, obj)| {
                obj.as_stream().is_ok_and(|s| {
                    s.dict
                        .get(b"Subtype")
                        .and_then(Object::as_name)
                        .is_ok_and(|n| n == b"Image")
                })
            })
            .map(|(id, _)| *id)
            .collect();
        // Les masques de transparence doivent rester sans perte.
        let masks: std::collections::HashSet<ObjectId> = doc
            .objects
            .values()
            .filter_map(|o| o.as_stream().ok())
            .filter_map(|s| s.dict.get(b"SMask").and_then(Object::as_reference).ok())
            .collect();
        let total = ids.len().max(1);
        for (i, id) in ids.into_iter().enumerate() {
            if masks.contains(&id) {
                continue;
            }
            report.images_seen += 1;
            progress(i as f32 / total as f32, format!("Image {}/{}", i + 1, total));
            if recompress_image(doc, id, o) {
                report.images_recompressed += 1;
            }
        }
    }
    report
}

fn recompress_image(doc: &mut Document, id: ObjectId, o: &CompressOptions) -> bool {
    let Ok(stream) = doc.get_object(id).and_then(Object::as_stream) else {
        return false;
    };
    let dict = &stream.dict;
    if dict.get(b"ImageMask").and_then(Object::as_bool).unwrap_or(false) || dict.has(b"Decode") || dict.has(b"Mask") {
        return false;
    }
    let filters: Vec<Vec<u8>> = stream
        .filters()
        .map(|f| f.iter().map(|n| n.to_vec()).collect())
        .unwrap_or_default();
    let is_dct = filters.len() == 1 && filters[0] == b"DCTDecode";
    let cs_name = dict
        .get(b"ColorSpace")
        .and_then(Object::as_name)
        .ok()
        .map(|n| n.to_vec());
    let img: DynamicImage = if is_dct {
        if !matches!(cs_name.as_deref(), Some(b"DeviceRGB") | Some(b"DeviceGray")) {
            return false;
        }
        match image::load_from_memory_with_format(&stream.content, image::ImageFormat::Jpeg) {
            Ok(i) => i,
            Err(_) => return false,
        }
    } else {
        match decode_raw(doc, stream) {
            Some(i) => i,
            None => return false,
        }
    };
    let old_len = stream.content.len();
    let (w, h) = (img.width(), img.height());
    if w < 16 || h < 16 {
        return false;
    }
    let img = if o.max_side > 0 && w.max(h) > o.max_side {
        img.resize(o.max_side, o.max_side, FilterType::Triangle)
    } else {
        img
    };
    let gray = !img.color().has_color();
    let mut out = Vec::new();
    let encoded = if gray {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, o.quality).encode_image(&img.to_luma8())
    } else {
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, o.quality).encode_image(&img.to_rgb8())
    };
    if encoded.is_err() || out.len() as f32 > old_len as f32 * 0.9 {
        return false;
    }
    let (nw, nh) = (img.width(), img.height());
    let Ok(stream) = doc.get_object_mut(id).and_then(Object::as_stream_mut) else {
        return false;
    };
    stream.dict.set("Filter", "DCTDecode");
    stream.dict.remove(b"DecodeParms");
    stream.dict.set("Width", nw as i64);
    stream.dict.set("Height", nh as i64);
    stream.dict.set("BitsPerComponent", 8);
    stream
        .dict
        .set("ColorSpace", if gray { "DeviceGray" } else { "DeviceRGB" });
    stream.set_content(out);
    stream.allows_compression = false;
    true
}
