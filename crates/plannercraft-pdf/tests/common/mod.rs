//! Outils des tests du PDF : projets construits à la main et lecture
//! simple des PDF écrits par krilla (objets, flux `FlateDecode`, tables
//! `ToUnicode`, opérateurs de texte `Tf`, `Tj`, `TJ`).

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::string_slice,
    clippy::many_single_char_names,
    reason = "tests : un échec doit s'arrêter net et lisiblement ; chaque fichier n'utilise qu'une partie des outils"
)]

use std::collections::BTreeMap;

use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::{
    Duration, DurationUnit, Lag, Link, LinkType, Minutes, Moment, OutlineLevel, Project, ProjectInit,
    ScheduleAnchor, Task, TaskUid, TimeBasis,
};
use plannercraft_render::Lang;
use plannercraft_render::geometry::{NoTexts, Prim, Scene, Viewport, layout};
use plannercraft_render::paginate::{Pages, fit_zoom, paginate};
use plannercraft_render::style::{TemplateId, Variant, template};
use plannercraft_render::{RenderLimits, Zoom};

/// Instant ISO.
pub fn at(s: &str) -> Moment {
    s.parse().unwrap()
}

/// Projet vide démarrant à `start`.
pub fn project(name: &str, start: &str) -> Project {
    Project::new(ProjectInit {
        name: name.to_owned(),
        anchor: ScheduleAnchor::FromStart(at(start)),
        current_date: at(start),
        calendar_names: BuiltinCalendarNames::default(),
    })
}

/// Ajoute une tâche de `days` jours ouvrés au niveau `level`.
pub fn add(p: &mut Project, name: &str, days: i64, level: u16) -> TaskUid {
    let uid = TaskUid::new(u32::try_from(p.tasks.len() + 1).unwrap());
    let options = p.options;
    let mut t = Task::new(uid, name.to_owned(), &options);
    t.outline_level = OutlineLevel::new(level).unwrap();
    t.duration = Duration::new(
        Minutes::new(days * 480),
        DurationUnit::Day,
        TimeBasis::Working,
        false,
    )
    .unwrap();
    p.tasks.push(t).unwrap();
    uid
}

/// Lien FS `pred` → `succ`.
pub fn link(p: &mut Project, pred: TaskUid, succ: TaskUid) {
    p.tasks.get_mut(succ).unwrap().predecessors.push(Link {
        predecessor: pred,
        kind: LinkType::FinishToStart,
        lag: Lag::ZERO,
    });
}

/// Projet « P » du repère (`docs/plans/j8.md` § 2).
pub fn project_p() -> Project {
    let mut p = project("P", "2026-10-19T08:00");
    add(&mut p, "Gros œuvre", 0, 1);
    let t2 = add(&mut p, "Fondations", 5, 2);
    let t3 = add(&mut p, "Murs", 10, 2);
    let t4 = add(&mut p, "Livraison", 0, 2);
    add(&mut p, "Second œuvre", 0, 1);
    let t6 = add(&mut p, "Plâtrerie", 4, 2);
    let t7 = add(&mut p, "Peinture", 3, 2);
    link(&mut p, t2, t3);
    link(&mut p, t3, t4);
    link(&mut p, t3, t6);
    link(&mut p, t6, t7);
    p
}

/// Scène d'un projet dans un modèle (variante d'impression), zoom ajusté
/// à la mise en page du modèle.
pub fn scene_of(p: &Project, id: TemplateId) -> Scene {
    let st = template(id);
    let s = plannercraft_core::schedule(p);
    let vp = Viewport {
        zoom: fit_zoom(&st.page).unwrap_or(Zoom::Week),
        limits: RenderLimits {
            max_rows: 10_000,
            ..RenderLimits::default()
        },
        ..Viewport::new(Variant::Print, Lang::Fr)
    };
    layout(p, &s, &st, &NoTexts, &vp).0
}

/// Pages d'un projet dans un modèle.
pub fn pages_of(p: &Project, id: TemplateId) -> Pages {
    paginate(&scene_of(p, id), &template(id).page, &NoTexts).unwrap()
}

/// Textes de la scène.
pub fn scene_texts(scene: &Scene) -> Vec<String> {
    scene
        .items
        .iter()
        .filter_map(|it| match &it.prim {
            Prim::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Objet PDF : dictionnaire et flux décodé.
#[derive(Debug, Default)]
struct Obj {
    dict: String,
    stream: Vec<u8>,
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Objets du document, par numéro.
fn objects(pdf: &[u8]) -> BTreeMap<u32, Obj> {
    let mut out = BTreeMap::new();
    let mut pos = 0;
    while let Some(i) = find(pdf, b" 0 obj\n", pos) {
        let mut s = i;
        while s > 0 && pdf[s - 1].is_ascii_digit() {
            s -= 1;
        }
        let num: u32 = std::str::from_utf8(&pdf[s..i]).unwrap().parse().unwrap();
        let body_start = i + b" 0 obj\n".len();
        let stream_at = find(pdf, b"stream\n", body_start);
        let end_at = find(pdf, b"endobj", body_start).unwrap();
        let mut obj = Obj::default();
        match stream_at {
            Some(st) if st < end_at => {
                let dict = String::from_utf8_lossy(&pdf[body_start..st]).into_owned();
                let len: usize = dict
                    .split("/Length ")
                    .nth(1)
                    .unwrap()
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap();
                let raw = &pdf[st + 7..st + 7 + len];
                obj.stream = if dict.contains("/FlateDecode") {
                    miniz_oxide::inflate::decompress_to_vec_zlib(raw).unwrap()
                } else {
                    raw.to_vec()
                };
                obj.dict = dict;
                pos = st + 7 + len;
            }
            _ => {
                obj.dict = String::from_utf8_lossy(&pdf[body_start..end_at]).into_owned();
                pos = end_at;
            }
        }
        out.insert(num, obj);
    }
    out
}

/// Référence `/Key N 0 R` d'un dictionnaire.
fn reference(dict: &str, key: &str) -> Option<u32> {
    let rest = dict.split(&format!("/{key} ")).nth(1)?;
    rest.split(' ').next()?.parse().ok()
}

/// Table `ToUnicode` : code → texte.
fn cmap(stream: &[u8]) -> BTreeMap<u16, String> {
    let text = String::from_utf8_lossy(stream);
    let mut out = BTreeMap::new();
    let hex = |s: &str| -> Vec<u16> {
        let s = s.trim().trim_start_matches('<').trim_end_matches('>');
        (0..s.len() / 4)
            .map(|k| u16::from_str_radix(&s[k * 4..k * 4 + 4], 16).unwrap())
            .collect()
    };
    let mut mode = "";
    for line in text.lines() {
        if line.ends_with("beginbfchar") {
            mode = "char";
            continue;
        }
        if line.ends_with("beginbfrange") {
            mode = "range";
            continue;
        }
        if line.starts_with("end") {
            mode = "";
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        match (mode, parts.as_slice()) {
            ("char", [code, uni]) => {
                out.insert(hex(code)[0], String::from_utf16_lossy(&hex(uni)));
            }
            ("range", [a, b, uni]) => {
                let (a, b) = (hex(a)[0], hex(b)[0]);
                let base = hex(uni);
                for (k, code) in (a..=b).enumerate() {
                    let mut u = base.clone();
                    if let Some(last) = u.last_mut() {
                        *last += u16::try_from(k).unwrap();
                    }
                    out.insert(code, String::from_utf16_lossy(&u));
                }
            }
            _ => {}
        }
    }
    out
}

/// Chaîne littérale PDF `( … )` décodée (échappements), à partir de `i`
/// (après la parenthèse ouvrante) ; rend les octets et la position après la
/// parenthèse fermante.
fn literal(s: &[u8], mut i: usize) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let mut depth = 1;
    while i < s.len() {
        let c = s[i];
        match c {
            b'\\' => {
                i += 1;
                let e = s[i];
                match e {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'0'..=b'7' => {
                        let mut v = 0_u32;
                        let mut n = 0;
                        while n < 3 && i < s.len() && (b'0'..=b'7').contains(&s[i]) {
                            v = v * 8 + u32::from(s[i] - b'0');
                            i += 1;
                            n += 1;
                        }
                        out.push(u8::try_from(v & 0xFF).unwrap());
                        continue;
                    }
                    other => out.push(other),
                }
            }
            b'(' => {
                depth += 1;
                out.push(c);
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return (out, i + 1);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
        i += 1;
    }
    (out, i)
}

/// Textes de chaque page, un par bloc `BT … ET` (une suite de glyphes).
pub fn page_texts(pdf: &[u8]) -> Vec<Vec<String>> {
    let objs = objects(pdf);
    // Police de chaque objet `Type0` : sa table `ToUnicode`.
    let maps: BTreeMap<u32, BTreeMap<u16, String>> = objs
        .iter()
        .filter(|(_, o)| o.dict.contains("/Subtype/Type0"))
        .filter_map(|(n, o)| Some((*n, cmap(&objs.get(&reference(&o.dict, "ToUnicode")?)?.stream))))
        .collect();
    let mut pages = Vec::new();
    // Pages dans l'ordre de `Kids`.
    let root = objs.values().find(|o| o.dict.contains("/Type/Pages")).unwrap();
    let kids = root
        .dict
        .split("/Kids[")
        .nth(1)
        .unwrap()
        .split(']')
        .next()
        .unwrap();
    let ids: Vec<u32> = kids.split(" 0 R").filter_map(|s| s.trim().parse().ok()).collect();
    for id in ids {
        let page = &objs[&id];
        let res = &objs[&reference(&page.dict, "Resources").unwrap()];
        let fonts_dict = res
            .dict
            .split("/Font<<")
            .nth(1)
            .unwrap_or("")
            .split(">>")
            .next()
            .unwrap_or("");
        let mut fonts: BTreeMap<String, u32> = BTreeMap::new();
        for entry in fonts_dict.split('/').filter(|s| !s.is_empty()) {
            let mut it = entry.split_whitespace();
            if let (Some(name), Some(n)) = (it.next(), it.next()) {
                fonts.insert(name.to_owned(), n.parse().unwrap());
            }
        }
        let content = &objs[&reference(&page.dict, "Contents").unwrap()].stream;
        pages.push(decode_content(content, &fonts, &maps));
    }
    pages
}

fn decode_content(
    content: &[u8],
    fonts: &BTreeMap<String, u32>,
    maps: &BTreeMap<u32, BTreeMap<u16, String>>,
) -> Vec<String> {
    let mut out = Vec::new();
    let mut current: Option<&BTreeMap<u16, String>> = None;
    let mut block: Option<String> = None;
    let mut i = 0;
    while i < content.len() {
        let c = content[i];
        if c == b'(' {
            let (bytes, next) = literal(content, i + 1);
            if let (Some(b), Some(map)) = (block.as_mut(), current) {
                for pair in bytes.chunks(2) {
                    let code = u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]);
                    b.push_str(map.get(&code).map_or("\u{FFFD}", String::as_str));
                }
            }
            i = next;
            continue;
        }
        if content[i..].starts_with(b"BT") && (i == 0 || content[i - 1].is_ascii_whitespace()) {
            block = Some(String::new());
            i += 2;
            continue;
        }
        if content[i..].starts_with(b"ET") && content[i - 1].is_ascii_whitespace() {
            if let Some(b) = block.take() {
                out.push(b);
            }
            i += 2;
            continue;
        }
        if c == b'/' {
            let name: String = content[i + 1..]
                .iter()
                .take_while(|b| b.is_ascii_alphanumeric())
                .map(|b| char::from(*b))
                .collect();
            if let Some(n) = fonts.get(&name) {
                current = maps.get(n);
            }
            i += 1 + name.len();
            continue;
        }
        i += 1;
    }
    out
}
