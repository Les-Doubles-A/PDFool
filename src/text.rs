//! Encodage WinAnsi et métriques Helvetica pour écrire du texte avec les polices standard PDF.
//!
//! Les largeurs proviennent des fichiers AFM Adobe (Helvetica), indexées par code WinAnsi.

/// Largeurs Helvetica (unités de 1/1000 em) par code WinAnsi.
#[rustfmt::skip]
const HELVETICA_WIDTHS: [u16; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778,
    722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278,
    278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, 0, 556, 0, 222, 556,
    333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, 0, 222, 222, 333, 333, 350, 556, 1000,
    333, 1000, 500, 333, 944, 0, 500, 667, 0, 333, 556, 556, 556, 556, 260, 556, 333, 737, 370, 556,
    584, 0, 737, 333, 400, 584, 333, 333, 333, 556, 537, 278, 333, 333, 365, 556, 834, 834, 834, 611,
    667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278, 722, 722, 778, 778,
    778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611, 556, 556, 556, 556, 556, 556, 889, 500,
    556, 556, 556, 556, 278, 278, 278, 278, 556, 556, 556, 556, 556, 556, 556, 584, 611, 556, 556, 556,
    556, 500, 556, 500,
];

/// Hauteur des capitales Helvetica (AFM CapHeight).
pub const HELVETICA_CAP_HEIGHT: f32 = 0.718;
/// Largeur fixe de chaque glyphe Courier.
pub const COURIER_WIDTH: f32 = 0.6;

/// Convertit un caractère Unicode en code WinAnsi (cp1252). `None` si non représentable.
pub fn winansi_code(c: char) -> Option<u8> {
    let u = c as u32;
    match u {
        0x20..=0x7E => Some(u as u8),
        0xA0 => Some(b' '),
        0xAD => None,
        0xA1..=0xFF => Some(u as u8),
        _ => Some(match u {
            0x20AC => 0x80,
            0x201A => 0x82,
            0x0192 => 0x83,
            0x201E => 0x84,
            0x2026 => 0x85,
            0x2020 => 0x86,
            0x2021 => 0x87,
            0x02C6 => 0x88,
            0x2030 => 0x89,
            0x0160 => 0x8A,
            0x2039 => 0x8B,
            0x0152 => 0x8C,
            0x017D => 0x8E,
            0x2018 => 0x91,
            0x2019 => 0x92,
            0x201C => 0x93,
            0x201D => 0x94,
            0x2022 => 0x95,
            0x2013 => 0x96,
            0x2014 => 0x97,
            0x02DC => 0x98,
            0x2122 => 0x99,
            0x0161 => 0x9A,
            0x203A => 0x9B,
            0x0153 => 0x9C,
            0x017E => 0x9E,
            0x0178 => 0x9F,
            _ => return None,
        }),
    }
}

/// Encode une chaîne en WinAnsi ; les caractères non représentables deviennent `?`.
pub fn encode_winansi(text: &str) -> Vec<u8> {
    text.chars()
        .filter(|&c| c != '\u{AD}' && !c.is_control())
        .map(|c| winansi_code(c).unwrap_or(b'?'))
        .collect()
}

/// Largeur en points d'un texte encodé WinAnsi en Helvetica.
pub fn helvetica_width(encoded: &[u8], size: f32) -> f32 {
    let units: u32 = encoded.iter().map(|&b| HELVETICA_WIDTHS[b as usize] as u32).sum();
    units as f32 / 1000.0 * size
}

/// Chaîne hexadécimale PDF `<...>` pour un flux de contenu.
pub fn hex_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2 + 2);
    out.push('<');
    for b in bytes {
        out.push_str(&format!("{b:02X}"));
    }
    out.push('>');
    out
}

/// Formate un nombre pour un flux de contenu PDF (sans notation scientifique).
pub fn num(v: f32) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// Date du jour (UTC) au format JJ/MM/AAAA, sans dépendance externe.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{d:02}/{m:02}/{y}")
}

/// Algorithme de Howard Hinnant : jours depuis 1970-01-01 vers (année, mois, jour).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_french_text() {
        assert_eq!(encode_winansi("été"), vec![0xE9, b't', 0xE9]);
        assert_eq!(encode_winansi("€ œ"), vec![0x80, b' ', 0x9C]);
        assert_eq!(encode_winansi("日"), vec![b'?']);
    }

    #[test]
    fn widths() {
        assert_eq!(helvetica_width(b"10", 10.0), 11.12);
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_403), (2025, 11, 11));
    }

    #[test]
    fn numbers() {
        assert_eq!(num(1.5), "1.5");
        assert_eq!(num(2.0), "2");
        assert_eq!(num(-0.0001), "0");
    }
}
