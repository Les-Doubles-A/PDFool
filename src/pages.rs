//! Analyse des sélections de pages saisies par l'utilisateur.
//!
//! Syntaxe : `1-3, 7, 10-` (jusqu'à la fin), `-4` (depuis le début), `8-5` (ordre décroissant),
//! `fin` ou `n` (dernière page), `paires`, `impaires`, `tout`.

pub type Groups = Vec<Vec<u32>>;

/// Analyse une sélection et renvoie un groupe de pages par élément séparé par une virgule.
pub fn parse_groups(spec: &str, total: u32) -> Result<Groups, String> {
    if total == 0 {
        return Err("Le document ne contient aucune page.".into());
    }
    let mut groups = Vec::new();
    for raw in spec.split([',', ';', '\n']) {
        let part: String = raw
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_lowercase();
        if part.is_empty() {
            continue;
        }
        let group: Vec<u32> = match part.as_str() {
            "tout" | "toutes" | "all" | "*" => (1..=total).collect(),
            "paires" | "paire" | "pair" | "even" => (1..=total).filter(|p| p % 2 == 0).collect(),
            "impaires" | "impaire" | "impair" | "odd" => (1..=total).filter(|p| p % 2 == 1).collect(),
            _ => {
                let (a, b) = match part.split_once('-') {
                    Some((a, b)) => (
                        if a.is_empty() { 1 } else { number(a, total)? },
                        if b.is_empty() { total } else { number(b, total)? },
                    ),
                    None => {
                        let n = number(&part, total)?;
                        (n, n)
                    }
                };
                if a <= b {
                    (a..=b).collect()
                } else {
                    (b..=a).rev().collect()
                }
            }
        };
        if group.is_empty() {
            return Err(format!("La sélection « {} » ne contient aucune page.", raw.trim()));
        }
        groups.push(group);
    }
    if groups.is_empty() {
        return Err("Indiquez au moins une page, par exemple : 1-3, 7.".into());
    }
    Ok(groups)
}

/// Sélection à plat, dans l'ordre saisi (doublons conservés).
pub fn parse_ordered(spec: &str, total: u32) -> Result<Vec<u32>, String> {
    Ok(parse_groups(spec, total)?.into_iter().flatten().collect())
}

/// Ensemble de pages trié et sans doublon. Une sélection vide signifie toutes les pages.
pub fn parse_set(spec: &str, total: u32) -> Result<Vec<u32>, String> {
    if spec.trim().is_empty() {
        return Ok((1..=total).collect());
    }
    let mut pages = parse_ordered(spec, total)?;
    pages.sort_unstable();
    pages.dedup();
    Ok(pages)
}

fn number(s: &str, total: u32) -> Result<u32, String> {
    if s == "fin" || s == "n" || s == "end" || s == "z" {
        return Ok(total);
    }
    let n: u32 = s
        .parse()
        .map_err(|_| format!("« {s} » n'est pas un numéro de page valide."))?;
    if n == 0 {
        return Err("Les pages sont numérotées à partir de 1.".into());
    }
    if n > total {
        return Err(format!(
            "La page {n} n'existe pas (le document compte {total} page(s))."
        ));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(parse_ordered("1-3, 7", 10).unwrap(), vec![1, 2, 3, 7]);
        assert_eq!(parse_ordered("8-", 10).unwrap(), vec![8, 9, 10]);
        assert_eq!(parse_ordered("-2", 10).unwrap(), vec![1, 2]);
        assert_eq!(parse_ordered("3-1", 10).unwrap(), vec![3, 2, 1]);
        assert_eq!(parse_ordered("fin", 4).unwrap(), vec![4]);
        assert_eq!(parse_ordered("paires", 5).unwrap(), vec![2, 4]);
        assert_eq!(parse_set("3,1,3", 5).unwrap(), vec![1, 3]);
        assert_eq!(parse_set("", 3).unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_groups("1-2; 3", 3).unwrap(), vec![vec![1, 2], vec![3]]);
    }

    #[test]
    fn errors() {
        assert!(parse_ordered("0", 3).is_err());
        assert!(parse_ordered("4", 3).is_err());
        assert!(parse_ordered("a", 3).is_err());
        assert!(parse_ordered("", 3).is_err());
    }
}
