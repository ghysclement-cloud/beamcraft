//! Lecture des noms de corps dans un fichier STEP.
//!
//! OpenCASCADE (via `cadrum`) nous rend les solides, mais pas leurs noms.
//! Les noms sont pourtant dans le fichier : SolidWorks les écrit dans les entités
//! `MANIFOLD_SOLID_BREP('Cornière - ailes égales 40X40X4(1)[2]', …)`.
//! On relit donc le texte du fichier pour les récupérer, dans le même ordre
//! que celui où OpenCASCADE transfère les solides (l'ordre de la liste
//! d'éléments de chaque `…_SHAPE_REPRESENTATION`).

use std::collections::HashMap;

/// Renvoie les noms des solides, dans l'ordre de transfert.
pub fn solid_names(step_text: &str) -> Vec<String> {
    let entities = parse_entities(step_text);
    let mut names = Vec::new();
    let mut ids: Vec<&u64> = entities.keys().collect();
    ids.sort();
    for id in ids {
        let (ty, args) = &entities[id];
        if !ty.ends_with("SHAPE_REPRESENTATION") {
            continue;
        }
        let parts = split_top_level(args);
        let Some(items) = parts.get(1) else { continue };
        for item in refs(items) {
            if let Some((ity, iargs)) = entities.get(&item) {
                if ity == "MANIFOLD_SOLID_BREP" || ity == "BREP_WITH_VOIDS" {
                    let name = split_top_level(iargs)
                        .first()
                        .map(|s| decode_string(s))
                        .unwrap_or_default();
                    names.push(name);
                }
            }
        }
    }
    names
}

/// Nettoie un nom pour l'affichage (« NONE » ou vide → None).
pub fn clean_name(raw: &str) -> Option<String> {
    let n = raw.trim();
    if n.is_empty() || n.eq_ignore_ascii_case("NONE") {
        None
    } else {
        Some(n.to_string())
    }
}

/// `#id = TYPE ( args ) ;` → { id: (TYPE, args) }. Les entités complexes sont ignorées.
fn parse_entities(text: &str) -> HashMap<u64, (String, String)> {
    let mut map = HashMap::new();
    let data = match text.find("DATA;") {
        Some(i) => &text[i + 5..],
        None => text,
    };
    let mut stmt = String::new();
    let mut in_str = false;
    for ch in data.chars() {
        if ch == '\'' {
            in_str = !in_str; // '' (apostrophe doublée) = deux bascules, sans effet
        }
        if ch == ';' && !in_str {
            if let Some((id, ty, args)) = parse_statement(&stmt) {
                map.insert(id, (ty, args));
            }
            stmt.clear();
        } else {
            stmt.push(ch);
        }
    }
    map
}

fn parse_statement(stmt: &str) -> Option<(u64, String, String)> {
    let s = stmt.trim();
    let rest = s.strip_prefix('#')?;
    let eq = rest.find('=')?;
    let id: u64 = rest[..eq].trim().parse().ok()?;
    let body = rest[eq + 1..].trim();
    let open = body.find('(')?;
    let ty = body[..open].trim();
    if ty.is_empty() {
        return None; // entité complexe « ( A() B() ) »
    }
    let close = body.rfind(')')?;
    Some((id, ty.to_string(), body[open + 1..close].to_string()))
}

/// Découpe « a, (b, c), 'd,e' » au niveau 0 → ["a", "(b, c)", "'d,e'"].
fn split_top_level(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    for ch in args.chars() {
        match ch {
            '\'' => in_str = !in_str,
            '(' if !in_str => depth += 1,
            ')' if !in_str => depth -= 1,
            ',' if !in_str && depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Toutes les références « #123 » d'un morceau de texte.
fn refs(s: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'#' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if let Ok(n) = s[start..j].parse() {
                out.push(n);
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Décode une chaîne STEP : 'Corni\X2\00E8\X0\re' → « Cornière ».
fn decode_string(raw: &str) -> String {
    let s = raw
        .trim()
        .trim_start_matches('\'')
        .trim_end_matches('\'')
        .replace("''", "'");
    let mut out = String::new();
    let mut rest = s.as_str();
    while let Some(pos) = rest.find('\\') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        if let Some(t) = tail.strip_prefix("\\X2\\") {
            // UTF-16 en hexadécimal, 4 chiffres par caractère, jusqu'à \X0\
            let end = t.find("\\X0\\").unwrap_or(t.len());
            let units: Vec<u16> = t[..end]
                .as_bytes()
                .chunks(4)
                .filter_map(|c| u16::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
                .collect();
            out.push_str(&String::from_utf16_lossy(&units));
            rest = t.get(end + 4..).unwrap_or("");
        } else if let Some(t) = tail.strip_prefix("\\X\\") {
            // Un octet ISO 8859-1 : \X\E9 → é
            if let Some(hex) = t.get(..2) {
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v as char);
                }
            }
            rest = t.get(2..).unwrap_or("");
        } else if let Some(t) = tail.strip_prefix("\\\\") {
            out.push('\\');
            rest = t;
        } else {
            out.push('\\');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodage_des_accents() {
        assert_eq!(
            decode_string("'Corni\\X2\\00e8\\X0\\re 40X40X4(1)[2]'"),
            "Cornière 40X40X4(1)[2]"
        );
        assert_eq!(decode_string("'Plat \\X\\E9paisseur'"), "Plat épaisseur");
        assert_eq!(decode_string("'l''aile'"), "l'aile");
    }

    #[test]
    fn noms_dans_l_ordre_de_la_representation() {
        let step = "DATA;\n#1 = MANIFOLD_SOLID_BREP ( 'B', #9 ) ;\n#2 = MANIFOLD_SOLID_BREP ( 'A', #9 ) ;\n\
                    #3 = ADVANCED_BREP_SHAPE_REPRESENTATION ( 'x', ( #2, #7, #1 ), #8 ) ;\nENDSEC;";
        assert_eq!(solid_names(step), vec!["A", "B"]);
    }
}
