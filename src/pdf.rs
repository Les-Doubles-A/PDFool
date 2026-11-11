//! Opérations de structure sur les documents PDF (chargement, pages, fusion, sécurité, métadonnées).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use lopdf::{Bookmark, Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};

pub type Res<T> = Result<T, String>;

const PAGE_ATTRIBUTES: [&[u8]; 4] = [b"MediaBox", b"CropBox", b"Resources", b"Rotate"];

/// Charge un PDF, en le déchiffrant si un mot de passe est fourni.
pub fn load(path: &Path, password: &str) -> Res<Document> {
    let name = display_name(path);
    let doc = if password.is_empty() {
        Document::load(path)
    } else {
        Document::load_with_password(path, password)
    }
    .map_err(|e| {
        let msg = e.to_string();
        if msg.to_lowercase().contains("password") || msg.to_lowercase().contains("decrypt") {
            format!("{name} : document protégé, mot de passe manquant ou incorrect.")
        } else {
            format!("{name} : lecture impossible ({msg}).")
        }
    })?;
    if doc.is_encrypted() {
        return Err(format!(
            "{name} : document protégé, mot de passe manquant ou incorrect."
        ));
    }
    if doc.get_pages().is_empty() {
        return Err(format!("{name} : aucune page trouvée."));
    }
    Ok(doc)
}

/// Enregistre le document en compressant les flux et en ôtant les objets inutilisés.
pub fn save(doc: &mut Document, path: &Path) -> Res<()> {
    doc.change_producer("PDFool");
    doc.prune_objects();
    doc.compress();
    doc.save(path)
        .map_err(|e| format!("Écriture de {} impossible : {e}", display_name(path)))?;
    Ok(())
}

/// Enregistre avec flux d'objets et table de références compressée (fichier plus compact).
pub fn save_compact(doc: &mut Document, path: &Path) -> Res<()> {
    doc.change_producer("PDFool");
    doc.prune_objects();
    doc.delete_zero_length_streams();
    doc.compress();
    doc.renumber_objects();
    let mut file =
        std::fs::File::create(path).map_err(|e| format!("Création de {} impossible : {e}", display_name(path)))?;
    doc.save_modern(&mut file)
        .map_err(|e| format!("Écriture de {} impossible : {e}", display_name(path)))?;
    Ok(())
}

pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".into())
}

/// Chemin de sortie qui n'écrase jamais un fichier existant.
pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let p = Path::new(file_name);
    let base = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = p
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    (2..)
        .map(|i| dir.join(format!("{base}_{i}{ext}")))
        .find(|c| !c.exists())
        .expect("suite infinie")
}

pub fn page_count(doc: &Document) -> u32 {
    doc.get_pages().len() as u32
}

fn inherited(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut current = page_id;
    let mut seen = HashSet::new();
    while seen.insert(current) {
        let node = doc.get_dictionary(current).ok()?;
        if let Ok(value) = node.get(key) {
            match doc.dereference(value) {
                Ok((_, Object::Null)) | Err(_) => {}
                Ok(_) => return Some(value.clone()),
            }
        }
        current = node.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

/// Copie sur chaque page les attributs hérités de l'arbre des pages, pour pouvoir restructurer l'arbre.
pub fn materialize_pages(doc: &mut Document) {
    for page_id in doc.get_pages().into_values() {
        let values: Vec<(&[u8], Option<Object>)> = PAGE_ATTRIBUTES
            .iter()
            .map(|k| (*k, inherited(doc, page_id, k)))
            .collect();
        if let Ok(page) = doc.get_dictionary_mut(page_id) {
            for (key, value) in values {
                if let Some(v) = value {
                    page.set(key, v);
                }
            }
            if !page.has(b"MediaBox") {
                page.set("MediaBox", vec![0.into(), 0.into(), 595.into(), 842.into()]);
            }
            if !page.has(b"Resources") {
                page.set("Resources", Dictionary::new());
            }
        }
    }
}

fn pages_root(doc: &Document) -> Res<ObjectId> {
    doc.catalog()
        .and_then(|c| c.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(|_| "Arbre des pages introuvable.".to_string())
}

/// Reconstruit le document avec les pages données (numéros à partir de 1, ordre libre, doublons permis).
pub fn select_pages(doc: &mut Document, order: &[u32]) -> Res<()> {
    if order.is_empty() {
        return Err("Le document résultant ne contiendrait aucune page.".into());
    }
    materialize_pages(doc);
    let pages = doc.get_pages();
    let total = pages.len() as u32;
    let root = pages_root(doc)?;
    let mut used = HashSet::new();
    let mut kids = Vec::with_capacity(order.len());
    for &n in order {
        let id = *pages.get(&n).ok_or_else(|| format!("La page {n} n'existe pas."))?;
        let id = if used.insert(id) {
            id
        } else {
            let copy = doc.get_dictionary(id).map_err(|e| e.to_string())?.clone();
            doc.add_object(copy)
        };
        kids.push(id);
    }
    for id in &kids {
        doc.get_dictionary_mut(*id)
            .map_err(|e| e.to_string())?
            .set("Parent", root);
    }
    let root_dict = doc.get_dictionary_mut(root).map_err(|e| e.to_string())?;
    root_dict.set("Kids", kids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>());
    root_dict.set("Count", kids.len() as i64);
    root_dict.remove(b"Parent");

    let mut sorted = order.to_vec();
    sorted.sort_unstable();
    let is_permutation = sorted == (1..=total).collect::<Vec<_>>();
    if !is_permutation {
        // Signets et structure peuvent pointer vers des pages retirées.
        if let Ok(catalog) = doc.catalog_mut() {
            catalog.remove(b"Outlines");
            catalog.remove(b"StructTreeRoot");
            catalog.remove(b"MarkInfo");
            catalog.remove(b"PageLabels");
            if catalog
                .get(b"PageMode")
                .and_then(Object::as_name)
                .is_ok_and(|m| m == b"UseOutlines")
            {
                catalog.remove(b"PageMode");
            }
        }
    }
    doc.prune_objects();
    Ok(())
}

/// Fusionne plusieurs documents, avec un signet par document si demandé.
pub fn merge(docs: Vec<(String, Document)>, bookmarks: bool) -> Res<Document> {
    let mut out = Document::with_version("1.7");
    let mut next_id = 1;
    let mut kids = Vec::new();
    let mut firsts = Vec::new();
    for (name, mut doc) in docs {
        materialize_pages(&mut doc);
        doc.renumber_objects_with(next_id);
        next_id = doc.max_id + 1;
        let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
        if let Some(first) = pages.first() {
            firsts.push((name, *first));
        }
        kids.extend(pages);
        out.objects.extend(doc.objects);
    }
    out.max_id = out.objects.keys().map(|id| id.0).max().unwrap_or(0);
    let pages_id = out.new_object_id();
    for id in &kids {
        out.get_dictionary_mut(*id)
            .map_err(|e| e.to_string())?
            .set("Parent", pages_id);
    }
    out.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>(),
            "Count" => kids.len() as i64,
        }),
    );
    let catalog_id = out.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    out.trailer.set("Root", catalog_id);
    if bookmarks && !firsts.is_empty() {
        for (name, page) in firsts {
            out.add_bookmark(Bookmark::new(name, [0.0, 0.0, 0.0], 0, page), None);
        }
        if let Some(outline) = out.build_outline()
            && let Ok(catalog) = out.catalog_mut()
        {
            catalog.set("Outlines", outline);
            catalog.set("PageMode", "UseOutlines");
        }
    }
    out.prune_objects();
    Ok(out)
}

fn number(obj: &Object) -> f32 {
    obj.as_float().unwrap_or(0.0)
}

/// Rectangle `[x0, y0, x1, y1]` d'une page (CropBox sinon MediaBox), normalisé.
pub fn page_box(doc: &Document, page_id: ObjectId) -> [f32; 4] {
    let get = |key: &[u8]| -> Option<[f32; 4]> {
        let obj = inherited(doc, page_id, key)?;
        let (_, obj) = doc.dereference(&obj).ok()?;
        let arr = obj.as_array().ok()?;
        if arr.len() != 4 {
            return None;
        }
        let v: Vec<f32> = arr
            .iter()
            .map(|o| doc.dereference(o).map(|(_, o)| number(o)).unwrap_or(0.0))
            .collect();
        Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
    };
    get(b"CropBox")
        .or_else(|| get(b"MediaBox"))
        .unwrap_or([0.0, 0.0, 595.0, 842.0])
}

pub fn page_rotation(doc: &Document, page_id: ObjectId) -> i64 {
    inherited(doc, page_id, b"Rotate")
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360)
}

pub fn rotate(doc: &mut Document, pages: &[u32], angle: i64) -> Res<()> {
    materialize_pages(doc);
    let map = doc.get_pages();
    for n in pages {
        let id = map[n];
        let current = page_rotation(doc, id);
        let value = (current + angle).rem_euclid(360);
        doc.get_dictionary_mut(id)
            .map_err(|e| e.to_string())?
            .set("Rotate", value);
    }
    Ok(())
}

/// Rogne les marges (en points) en réduisant la CropBox.
pub fn crop(doc: &mut Document, pages: &[u32], top: f32, right: f32, bottom: f32, left: f32) -> Res<()> {
    materialize_pages(doc);
    let map = doc.get_pages();
    for n in pages {
        let id = map[n];
        let [x0, y0, x1, y1] = page_box(doc, id);
        // Les marges sont exprimées dans le sens d'affichage : on tient compte de la rotation.
        let (t, r, b, l) = match page_rotation(doc, id) {
            90 => (left, top, right, bottom),
            180 => (bottom, left, top, right),
            270 => (right, bottom, left, top),
            _ => (top, right, bottom, left),
        };
        let nb = [x0 + l, y0 + b, x1 - r, y1 - t];
        if nb[2] - nb[0] < 10.0 || nb[3] - nb[1] < 10.0 {
            return Err(format!("Les marges sont trop grandes pour la page {n}."));
        }
        let arr: Vec<Object> = nb.iter().map(|&v| Object::Real(v)).collect();
        let page = doc.get_dictionary_mut(id).map_err(|e| e.to_string())?;
        page.set("CropBox", arr.clone());
        page.set("TrimBox", arr);
    }
    Ok(())
}

/// Insère une page blanche (même format que la page de référence) après chaque page indiquée (0 = au début).
pub fn insert_blank(doc: &mut Document, after: &[u32]) -> Res<()> {
    materialize_pages(doc);
    let map = doc.get_pages();
    let total = map.len() as u32;
    let mut order: Vec<(u32, Option<ObjectId>)> = Vec::new();
    let reference = |n: u32| map[&n.clamp(1, total)];
    let blank_for = |doc: &mut Document, n: u32| -> Res<ObjectId> {
        let src = reference(n);
        let mut dict = Dictionary::new();
        dict.set("Type", "Page");
        for key in [b"MediaBox".as_slice(), b"CropBox", b"Rotate"] {
            if let Ok(v) = doc.get_dictionary(src).and_then(|d| d.get(key)) {
                dict.set(key, v.clone());
            }
        }
        dict.set("Resources", Dictionary::new());
        Ok(doc.add_object(dict))
    };
    if after.contains(&0) {
        let id = blank_for(doc, 1)?;
        order.push((0, Some(id)));
    }
    for n in 1..=total {
        order.push((n, None));
        if after.contains(&n) {
            let id = blank_for(doc, n)?;
            order.push((0, Some(id)));
        }
    }
    let root = pages_root(doc)?;
    let kids: Vec<ObjectId> = order.iter().map(|(n, blank)| blank.unwrap_or_else(|| map[n])).collect();
    for id in &kids {
        doc.get_dictionary_mut(*id)
            .map_err(|e| e.to_string())?
            .set("Parent", root);
    }
    let root_dict = doc.get_dictionary_mut(root).map_err(|e| e.to_string())?;
    root_dict.set("Kids", kids.iter().map(|&id| Object::Reference(id)).collect::<Vec<_>>());
    root_dict.set("Count", kids.len() as i64);
    root_dict.remove(b"Parent");
    doc.prune_objects();
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Métadonnées

pub const META_KEYS: [(&str, &str); 6] = [
    ("Title", "Titre"),
    ("Author", "Auteur"),
    ("Subject", "Sujet"),
    ("Keywords", "Mots-clés"),
    ("Creator", "Application d'origine"),
    ("Producer", "Producteur"),
];

pub fn read_metadata(doc: &Document) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(info) = doc
        .trailer
        .get(b"Info")
        .and_then(|o| doc.dereference(o))
        .and_then(|(_, o)| o.as_dict())
    else {
        return out;
    };
    for (key, value) in info.iter() {
        if let Ok((_, v)) = doc.dereference(value)
            && let Ok(text) = lopdf::decode_text_string(v)
            && !text.trim().is_empty()
        {
            out.insert(String::from_utf8_lossy(key).into_owned(), text);
        }
    }
    out
}

/// Remplace les métadonnées. Les champs vides sont supprimés. Le flux XMP est retiré pour rester cohérent.
pub fn write_metadata(doc: &mut Document, values: &[(String, String)]) {
    let mut info = doc
        .trailer
        .get(b"Info")
        .and_then(|o| doc.dereference(o))
        .and_then(|(_, o)| o.as_dict())
        .cloned()
        .unwrap_or_default();
    for (key, value) in values {
        if value.trim().is_empty() {
            info.remove(key.as_bytes());
        } else {
            info.set(key.as_bytes(), lopdf::text_string(value.trim()));
        }
    }
    info.set("ModDate", pdf_date_now());
    let id = doc.add_object(info);
    doc.trailer.set("Info", id);
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Metadata");
    }
}

pub fn strip_metadata(doc: &mut Document) {
    doc.trailer.remove(b"Info");
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Metadata");
    }
    let ids: Vec<ObjectId> = doc.get_pages().into_values().collect();
    for id in ids {
        if let Ok(page) = doc.get_dictionary_mut(id) {
            page.remove(b"Metadata");
            page.remove(b"PieceInfo");
        }
    }
}

fn pdf_date_now() -> Object {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let rem = secs % 86_400;
    let date = crate::text::today(); // JJ/MM/AAAA
    let parts: Vec<&str> = date.split('/').collect();
    let s = format!(
        "D:{}{}{}{:02}{:02}{:02}Z",
        parts[2],
        parts[1],
        parts[0],
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    );
    Object::String(s.into_bytes(), StringFormat::Literal)
}

// ---------------------------------------------------------------------------------------------
// Sécurité

pub struct ProtectOptions {
    pub user_password: String,
    pub owner_password: String,
    pub allow_print: bool,
    pub allow_copy: bool,
    pub allow_modify: bool,
    pub allow_annotate: bool,
}

/// Chiffre le document en AES-128 (PDF 1.6+).
pub fn protect(doc: &mut Document, opts: &ProtectOptions) -> Res<()> {
    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
    use lopdf::{EncryptionState, EncryptionVersion, Permissions};
    use std::sync::Arc;

    if opts.user_password.is_empty() && opts.owner_password.is_empty() {
        return Err("Indiquez au moins un mot de passe.".into());
    }
    let owner = if opts.owner_password.is_empty() {
        &opts.user_password
    } else {
        &opts.owner_password
    };
    let mut perms = Permissions::COPYABLE_FOR_ACCESSIBILITY;
    if opts.allow_print {
        perms |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
    }
    if opts.allow_copy {
        perms |= Permissions::COPYABLE;
    }
    if opts.allow_modify {
        perms |= Permissions::MODIFIABLE | Permissions::ASSEMBLABLE;
    }
    if opts.allow_annotate {
        perms |= Permissions::ANNOTABLE | Permissions::FILLABLE;
    }
    ensure_file_id(doc);
    if doc.version.as_str() < "1.6" {
        doc.version = "1.6".into();
    }
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: doc,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: owner,
        user_password: &opts.user_password,
        permissions: perms,
    };
    let state = EncryptionState::try_from(version).map_err(|e| format!("Chiffrement impossible : {e}"))?;
    doc.encrypt(&state)
        .map_err(|e| format!("Chiffrement impossible : {e}"))?;
    Ok(())
}

fn ensure_file_id(doc: &mut Document) {
    if doc
        .trailer
        .get(b"ID")
        .and_then(Object::as_array)
        .is_ok_and(|a| a.len() == 2)
    {
        return;
    }
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = Vec::with_capacity(16);
    for salt in 0..2u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        h.write_usize(doc.objects.len());
        bytes.extend_from_slice(&h.finish().to_be_bytes());
    }
    let id = Object::String(bytes, StringFormat::Hexadecimal);
    doc.trailer.set("ID", vec![id.clone(), id]);
}

// ---------------------------------------------------------------------------------------------
// Informations

pub fn describe(path: &Path, doc: &Document) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Fichier : {}", display_name(path)));
    if let Ok(meta) = std::fs::metadata(path) {
        lines.push(format!("Taille : {}", human_size(meta.len())));
    }
    lines.push(format!("Version PDF : {}", doc.version));
    let pages = doc.get_pages();
    lines.push(format!("Pages : {}", pages.len()));
    let mut formats: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for (n, id) in &pages {
        let [x0, y0, x1, y1] = page_box(doc, *id);
        let (mut w, mut h) = (x1 - x0, y1 - y0);
        if matches!(page_rotation(doc, *id), 90 | 270) {
            std::mem::swap(&mut w, &mut h);
        }
        formats.entry(paper_name(w, h)).or_default().push(*n);
    }
    for (format, list) in &formats {
        lines.push(format!("  {format} : {} page(s)", list.len()));
    }
    let catalog = doc.catalog().ok();
    let has = |k: &[u8]| catalog.is_some_and(|c| c.has(k));
    lines.push(format!("Signets : {}", if has(b"Outlines") { "oui" } else { "non" }));
    lines.push(format!("Formulaire : {}", if has(b"AcroForm") { "oui" } else { "non" }));
    lines.push(format!(
        "Balisé (accessibilité) : {}",
        if has(b"StructTreeRoot") { "oui" } else { "non" }
    ));
    let images: HashSet<ObjectId> = pages
        .values()
        .filter_map(|id| doc.get_page_images(*id).ok())
        .flatten()
        .map(|img| img.id)
        .collect();
    lines.push(format!("Images : {}", images.len()));
    let mut fonts = HashSet::new();
    for id in pages.values() {
        if let Ok(map) = doc.get_page_fonts(*id) {
            for dict in map.values() {
                if let Ok(name) = dict.get(b"BaseFont").and_then(Object::as_name) {
                    fonts.insert(String::from_utf8_lossy(name).into_owned());
                }
            }
        }
    }
    let mut fonts: Vec<String> = fonts.into_iter().collect();
    fonts.sort();
    lines.push(format!(
        "Polices : {}",
        if fonts.is_empty() {
            "aucune".into()
        } else {
            fonts.join(", ")
        }
    ));
    let meta = read_metadata(doc);
    if !meta.is_empty() {
        lines.push("Métadonnées :".into());
        for (k, v) in meta {
            let label = META_KEYS
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, l)| *l)
                .unwrap_or(k.as_str())
                .to_string();
            lines.push(format!("  {label} : {v}"));
        }
    }
    lines.join("\n")
}

pub fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b >= 1_048_576.0 {
        format!("{:.2} Mo", b / 1_048_576.0)
    } else if b >= 1024.0 {
        format!("{:.1} Ko", b / 1024.0)
    } else {
        format!("{bytes} octets")
    }
}

fn paper_name(w: f32, h: f32) -> String {
    let mm = |v: f32| (v * 25.4 / 72.0).round();
    let (wm, hm) = (mm(w), mm(h));
    const NAMED: [(&str, f32, f32); 6] = [
        ("A3", 297.0, 420.0),
        ("A4", 210.0, 297.0),
        ("A5", 148.0, 210.0),
        ("Lettre US", 216.0, 279.0),
        ("Légal US", 216.0, 356.0),
        ("A6", 105.0, 148.0),
    ];
    for (name, a, b) in NAMED {
        if (wm - a).abs() <= 2.0 && (hm - b).abs() <= 2.0 {
            return format!("{name} portrait ({wm} × {hm} mm)");
        }
        if (wm - b).abs() <= 2.0 && (hm - a).abs() <= 2.0 {
            return format!("{name} paysage ({wm} × {hm} mm)");
        }
    }
    format!("{wm} × {hm} mm")
}

// ---------------------------------------------------------------------------------------------
// Ajout de contenu par-dessus les pages

/// Ressources partagées par les surimpressions d'un document.
pub struct Overlay {
    q_id: ObjectId,
    big_q_id: ObjectId,
}

impl Overlay {
    pub fn new(doc: &mut Document) -> Self {
        materialize_pages(doc);
        let q_id = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
        let big_q_id = doc.add_object(Stream::new(Dictionary::new(), b"\nQ\n".to_vec()));
        Overlay { q_id, big_q_id }
    }

    /// Ajoute une ressource (police, état graphique, image) à la page.
    pub fn add_resource(
        &self,
        doc: &mut Document,
        page_id: ObjectId,
        category: &[u8],
        name: &str,
        value: Object,
    ) -> Res<()> {
        let res_obj = doc
            .get_dictionary(page_id)
            .ok()
            .and_then(|p| p.get(b"Resources").ok().cloned())
            .unwrap_or_else(|| Object::Dictionary(Dictionary::new()));
        let mut res = match res_obj {
            Object::Reference(r) => doc.get_dictionary(r).cloned().unwrap_or_default(),
            Object::Dictionary(d) => d,
            _ => Dictionary::new(),
        };
        let mut sub = match res.get(category) {
            Ok(Object::Reference(r)) => doc.get_dictionary(*r).cloned().unwrap_or_default(),
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => Dictionary::new(),
        };
        sub.set(name.as_bytes(), value);
        res.set(category, sub);
        doc.get_dictionary_mut(page_id)
            .map_err(|e| e.to_string())?
            .set("Resources", res);
        Ok(())
    }

    /// Ajoute un flux de contenu dessiné par-dessus la page, après isolement de l'état graphique existant.
    pub fn stamp(&self, doc: &mut Document, page_id: ObjectId, content: Vec<u8>) -> Res<()> {
        let stream_id = doc.add_object(Stream::new(Dictionary::new(), content));
        let page = doc.get_dictionary(page_id).map_err(|e| e.to_string())?;
        let mut existing: Vec<Object> = match page.get(b"Contents") {
            Ok(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Array(a)) => a.clone(),
                _ => vec![Object::Reference(*r)],
            },
            Ok(Object::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        let already_wrapped = existing.first() == Some(&Object::Reference(self.q_id));
        let mut contents = Vec::with_capacity(existing.len() + 3);
        if already_wrapped {
            contents.append(&mut existing);
        } else {
            contents.push(Object::Reference(self.q_id));
            contents.append(&mut existing);
            contents.push(Object::Reference(self.big_q_id));
        }
        contents.push(Object::Reference(stream_id));
        doc.get_dictionary_mut(page_id)
            .map_err(|e| e.to_string())?
            .set("Contents", contents);
        Ok(())
    }
}

/// Matrice qui place le repère « visuel » (origine en bas à gauche de la page affichée) dans l'espace utilisateur.
/// Renvoie (matrice, largeur visuelle, hauteur visuelle).
pub fn visual_frame(doc: &Document, page_id: ObjectId) -> ([f32; 6], f32, f32) {
    let [x0, y0, x1, y1] = page_box(doc, page_id);
    let (w, h) = (x1 - x0, y1 - y0);
    match page_rotation(doc, page_id) {
        90 => ([0.0, 1.0, -1.0, 0.0, x0 + w, y0], h, w),
        180 => ([-1.0, 0.0, 0.0, -1.0, x0 + w, y0 + h], w, h),
        270 => ([0.0, -1.0, 1.0, 0.0, x0, y0 + h], h, w),
        _ => ([1.0, 0.0, 0.0, 1.0, x0, y0], w, h),
    }
}
