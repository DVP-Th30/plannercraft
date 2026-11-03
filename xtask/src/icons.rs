//! `cargo xtask icons [--check]` : icônes du ruban (ADR-0017 D6, H-UI-77).
//!
//! Lit les SVG Lucide vendorisés de `assets/icons/lucide/` (copiés tels quels
//! du paquet `lucide-static@1.55.0`, licences dans le même dossier) avec
//! `roxmltree` et les aplatit en polylignes avec `kurbo` (tolérance
//! [`TOLERANCE`] unité de grille). Écrit
//! `crates/plannercraft-ui-egui/src/ribbon/icon_data.rs` : `enum Icon` (un variant
//! par fichier, nom en CamelCase, ordre des noms de fichiers) et, pour chaque
//! icône, ses polylignes en 1/64 d'unité de la grille 24 (drapeau « fermée »)
//! et ses points isolés (sous-chemins de longueur < [`DOT_LENGTH`], comme le
//! `h.01` de Lucide), dessinés en disques. Aucune lecture de SVG à
//! l'exécution : l'interface trace ces polylignes avec epaint.
//!
//! Sous-ensemble accepté (tout le reste est refusé explicitement, avec le
//! nom du fichier) : racine `svg` de `viewBox="0 0 24 24"`, `fill="none"`,
//! `stroke="currentColor"`, `stroke-width="2"`, bouts et jonctions ronds ;
//! éléments `path`, `line`, `polyline`, `polygon`, `rect` (coins arrondis
//! compris, `rx` = `ry`), `circle`, `ellipse`, sans `transform` ; `fill`
//! d'un élément : `none` ou `currentColor` seulement ; points dans la
//! grille (à une unité près).
//!
//! `--check` échoue si le fichier écrit n'est pas celui que donnent les SVG
//! (fraîcheur ; même contrôle par le test `h_ui_77_icon_data_fresh`).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kurbo::{BezPath, PathEl, Point, Shape as _};

/// Dossier des SVG, relatif à la racine du dépôt.
const SOURCE: &str = "assets/icons/lucide";

/// Fichier écrit, relatif à la racine du dépôt.
const OUTPUT: &str = "crates/plannercraft-ui-egui/src/ribbon/icon_data.rs";

/// Version du paquet Lucide vendorisé (rappel écrit dans le fichier).
const LUCIDE_VERSION: &str = "1.55.0";

/// Tolérance d'aplatissement, en unités de grille.
const TOLERANCE: f64 = 0.02;

/// Sous-chemin plus court que ceci (unités de grille) : point isolé.
const DOT_LENGTH: f64 = 0.1;

/// Sous-unités par unité de grille dans le fichier écrit.
const SUBUNITS: f64 = 64.0;

/// Côté de la grille Lucide.
const GRID: f64 = 24.0;

/// Points par ligne dans le fichier écrit (lisibilité des diffs).
const PER_LINE: usize = 10;

/// Attributs acceptés sur la racine `svg`, avec leur valeur imposée
/// (`None` : toute valeur).
const ROOT_ATTRS: [(&str, Option<&str>); 10] = [
    ("class", None),
    ("width", Some("24")),
    ("height", Some("24")),
    ("viewBox", Some("0 0 24 24")),
    ("fill", Some("none")),
    ("stroke", Some("currentColor")),
    ("stroke-width", Some("2")),
    ("stroke-linecap", Some("round")),
    ("stroke-linejoin", Some("round")),
    ("xmlns", None),
];

/// Polyligne d'une icône, en 1/64 d'unité de grille.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    closed: bool,
    points: Vec<(i16, i16)>,
}

/// Formes d'une icône.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct IconShapes {
    lines: Vec<Line>,
    dots: Vec<(i16, i16)>,
}

pub fn run(args: &[String]) -> ExitCode {
    let check = match args {
        [] => false,
        [flag] if flag == "--check" => true,
        _ => {
            eprintln!("usage : cargo xtask icons [--check]");
            return ExitCode::FAILURE;
        }
    };
    let root = repo_root();
    let generated = match generate(&root) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("xtask icons : {error}");
            return ExitCode::FAILURE;
        }
    };
    let path = root.join(OUTPUT);
    if check {
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current == generated {
            println!("xtask icons : OK ({OUTPUT} à jour)");
            return ExitCode::SUCCESS;
        }
        eprintln!("xtask icons : {OUTPUT} périmé, relancer `cargo xtask icons`");
        return ExitCode::FAILURE;
    }
    match std::fs::write(&path, generated) {
        Ok(()) => {
            println!("xtask icons : {OUTPUT} écrit");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("xtask icons : écriture de {OUTPUT} impossible : {error}");
            ExitCode::FAILURE
        }
    }
}

/// Noms des SVG du dossier source (sans extension), triés.
fn icon_names(root: &Path) -> Result<Vec<String>, String> {
    let dir = root.join(SOURCE);
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{SOURCE} illisible : {e}"))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{SOURCE} illisible : {e}"))?;
        let file = entry.file_name().to_string_lossy().into_owned();
        if let Some(name) = file.strip_suffix(".svg") {
            names.push(name.to_owned());
        }
    }
    names.sort();
    if names.is_empty() {
        return Err(format!("aucun SVG dans {SOURCE}"));
    }
    Ok(names)
}

/// Nom de variant : `chevrons-up-down` ⇒ `ChevronsUpDown`, `columns-3` ⇒
/// `Columns3`.
fn variant_name(name: &str) -> String {
    name.split('-')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect()
}

/// Nom de constante : `chevrons-up-down` ⇒ `CHEVRONS_UP_DOWN`.
fn const_name(name: &str) -> String {
    name.replace('-', "_").to_uppercase()
}

/// Nombre d'un attribut.
fn number(node: roxmltree::Node<'_, '_>, attr: &str, file: &str) -> Result<f64, String> {
    let text = node
        .attribute(attr)
        .ok_or_else(|| format!("{file} : <{}> sans `{attr}`", node.tag_name().name()))?;
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{file} : `{attr}` illisible : {text}"))
}

/// Nombre facultatif (absent ⇒ `None`).
fn opt_number(node: roxmltree::Node<'_, '_>, attr: &str, file: &str) -> Result<Option<f64>, String> {
    if node.attribute(attr).is_some() {
        number(node, attr, file).map(Some)
    } else {
        Ok(None)
    }
}

/// Liste de points `x,y x,y …` de `polyline` et `polygon`.
fn points(node: roxmltree::Node<'_, '_>, file: &str) -> Result<Vec<Point>, String> {
    let text = node
        .attribute("points")
        .ok_or_else(|| format!("{file} : <{}> sans `points`", node.tag_name().name()))?;
    let values: Vec<f64> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<f64>().ok().filter(|v| v.is_finite()))
        .collect::<Option<_>>()
        .ok_or_else(|| format!("{file} : `points` illisible : {text}"))?;
    if !values.len().is_multiple_of(2) || values.len() < 4 {
        return Err(format!("{file} : `points` incomplet : {text}"));
    }
    Ok(values
        .chunks(2)
        .filter_map(|c| match c {
            [x, y] => Some(Point::new(*x, *y)),
            _ => None,
        })
        .collect())
}

/// Attributs géométriques acceptés par élément.
fn geometry_attrs(tag: &str) -> Option<&'static [&'static str]> {
    Some(match tag {
        "path" => &["d"],
        "line" => &["x1", "y1", "x2", "y2"],
        "polyline" | "polygon" => &["points"],
        "rect" => &["x", "y", "width", "height", "rx", "ry"],
        "circle" => &["cx", "cy", "r"],
        "ellipse" => &["cx", "cy", "rx", "ry"],
        _ => return None,
    })
}

/// Chemin d'un élément de forme.
fn element_path(node: roxmltree::Node<'_, '_>, file: &str) -> Result<BezPath, String> {
    let tag = node.tag_name().name();
    let allowed =
        geometry_attrs(tag).ok_or_else(|| format!("{file} : élément <{tag}> non pris en charge"))?;
    for attr in node.attributes() {
        let name = attr.name();
        if name == "fill" {
            if !matches!(attr.value(), "none" | "currentColor") {
                return Err(format!(
                    "{file} : <{tag}> fill=\"{}\" non pris en charge",
                    attr.value()
                ));
            }
        } else if !allowed.contains(&name) {
            return Err(format!(
                "{file} : attribut `{name}` de <{tag}> non pris en charge"
            ));
        }
    }
    let path = match tag {
        "path" => {
            let d = node
                .attribute("d")
                .ok_or_else(|| format!("{file} : <path> sans `d`"))?;
            BezPath::from_svg(d).map_err(|e| format!("{file} : chemin illisible ({e}) : {d}"))?
        }
        "line" => {
            let mut p = BezPath::new();
            p.move_to((number(node, "x1", file)?, number(node, "y1", file)?));
            p.line_to((number(node, "x2", file)?, number(node, "y2", file)?));
            p
        }
        "polyline" | "polygon" => {
            let pts = points(node, file)?;
            let mut p = BezPath::new();
            for (i, pt) in pts.into_iter().enumerate() {
                if i == 0 {
                    p.move_to(pt);
                } else {
                    p.line_to(pt);
                }
            }
            if tag == "polygon" {
                p.close_path();
            }
            p
        }
        "rect" => {
            let left = number(node, "x", file).unwrap_or(0.0);
            let top = number(node, "y", file).unwrap_or(0.0);
            let width = number(node, "width", file)?;
            let height = number(node, "height", file)?;
            let rx = opt_number(node, "rx", file)?;
            let ry = opt_number(node, "ry", file)?;
            let radius = match (rx, ry) {
                (None, None) => 0.0,
                (Some(one), None) | (None, Some(one)) => one,
                (Some(rx), Some(ry)) if (rx - ry).abs() < 1e-9 => rx,
                (Some(_), Some(_)) => return Err(format!("{file} : <rect> avec rx ≠ ry non pris en charge")),
            };
            if width <= 0.0 || height <= 0.0 || radius < 0.0 {
                return Err(format!("{file} : <rect> de taille nulle ou négative"));
            }
            kurbo::RoundedRect::new(left, top, left + width, top + height, radius).to_path(TOLERANCE / 10.0)
        }
        "circle" => {
            let c = Point::new(number(node, "cx", file)?, number(node, "cy", file)?);
            let r = number(node, "r", file)?;
            if r <= 0.0 {
                return Err(format!("{file} : <circle> de rayon nul ou négatif"));
            }
            kurbo::Circle::new(c, r).to_path(TOLERANCE / 10.0)
        }
        // `ellipse` : seul élément restant (`geometry_attrs`).
        _ => {
            let c = Point::new(number(node, "cx", file)?, number(node, "cy", file)?);
            let rx = number(node, "rx", file)?;
            let ry = number(node, "ry", file)?;
            if rx <= 0.0 || ry <= 0.0 {
                return Err(format!("{file} : <ellipse> de rayon nul ou négatif"));
            }
            kurbo::Ellipse::new(c, (rx, ry), 0.0).to_path(TOLERANCE / 10.0)
        }
    };
    Ok(path)
}

/// Coordonnée de grille ⇒ 1/64 d'unité, bornée à la grille élargie d'une
/// unité (refus au-delà).
fn subunit(v: f64, file: &str) -> Result<i16, String> {
    if !(-1.0..=GRID + 1.0).contains(&v) {
        return Err(format!("{file} : point hors de la grille : {v}"));
    }
    let scaled = (v * SUBUNITS).round();
    // Borné ci-dessus : |scaled| ≤ 25 × 64.
    #[allow(clippy::cast_possible_truncation, reason = "valeur bornée à ±1 600")]
    Ok(scaled as i16)
}

/// Aplatit `path` et range ses sous-chemins en polylignes ou en points.
fn flatten_into(path: &BezPath, shapes: &mut IconShapes, file: &str) -> Result<(), String> {
    // Sous-chemins aplatis : (points en unités de grille, fermé).
    let mut subpaths: Vec<(Vec<Point>, bool)> = Vec::new();
    kurbo::flatten(path.iter(), TOLERANCE, |el| match el {
        PathEl::MoveTo(p) => subpaths.push((vec![p], false)),
        PathEl::LineTo(p) => {
            if let Some((pts, _)) = subpaths.last_mut() {
                pts.push(p);
            }
        }
        PathEl::ClosePath => {
            if let Some((_, closed)) = subpaths.last_mut() {
                *closed = true;
            }
        }
        // `flatten` ne rend que des segments.
        PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
    });
    for (pts, closed) in subpaths {
        let length: f64 = pts
            .windows(2)
            .map(|w| match w {
                [a, b] => a.distance(*b),
                _ => 0.0,
            })
            .sum();
        let Some(first) = pts.first() else { continue };
        if length < DOT_LENGTH {
            shapes
                .dots
                .push((subunit(first.x, file)?, subunit(first.y, file)?));
            continue;
        }
        let mut line: Vec<(i16, i16)> = Vec::with_capacity(pts.len());
        for p in &pts {
            let q = (subunit(p.x, file)?, subunit(p.y, file)?);
            if line.last() != Some(&q) {
                line.push(q);
            }
        }
        if closed && line.len() > 1 && line.first() == line.last() {
            line.pop();
        }
        if line.len() < 2 {
            shapes
                .dots
                .push((subunit(first.x, file)?, subunit(first.y, file)?));
            continue;
        }
        shapes.lines.push(Line { closed, points: line });
    }
    Ok(())
}

/// Formes d'un SVG.
fn read_icon(text: &str, file: &str) -> Result<IconShapes, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("{file} : XML illisible : {e}"))?;
    let svg = doc.root_element();
    if svg.tag_name().name() != "svg" {
        return Err(format!(
            "{file} : racine <{}> au lieu de <svg>",
            svg.tag_name().name()
        ));
    }
    for attr in svg.attributes() {
        let Some((_, expected)) = ROOT_ATTRS.iter().find(|(name, _)| *name == attr.name()) else {
            return Err(format!(
                "{file} : attribut `{}` de <svg> non pris en charge",
                attr.name()
            ));
        };
        if let Some(expected) = expected
            && attr.value() != *expected
        {
            return Err(format!(
                "{file} : <svg> {}=\"{}\" au lieu de \"{expected}\"",
                attr.name(),
                attr.value()
            ));
        }
    }
    let mut shapes = IconShapes::default();
    for child in svg.children() {
        if child.is_comment() || (child.is_text() && child.text().is_none_or(|t| t.trim().is_empty())) {
            continue;
        }
        if !child.is_element() {
            return Err(format!("{file} : contenu non pris en charge dans <svg>"));
        }
        if child.has_children() {
            return Err(format!("{file} : <{}> a des enfants", child.tag_name().name()));
        }
        let path = element_path(child, file)?;
        flatten_into(&path, &mut shapes, file)?;
    }
    if shapes.lines.is_empty() && shapes.dots.is_empty() {
        return Err(format!("{file} : aucune forme"));
    }
    Ok(shapes)
}

/// Écrit une liste de points sur des lignes de [`PER_LINE`].
fn write_points(out: &mut String, points: &[(i16, i16)], indent: &str) {
    for chunk in points.chunks(PER_LINE) {
        out.push_str(indent);
        for (x, y) in chunk {
            let _ = write!(out, "({x}, {y}), ");
        }
        // Espace final retiré.
        if out.ends_with(' ') {
            out.pop();
        }
        out.push('\n');
    }
}

/// Texte de `icon_data.rs` pour les SVG du dépôt.
fn generate(root: &Path) -> Result<String, String> {
    let names = icon_names(root)?;
    let mut icons = Vec::with_capacity(names.len());
    for name in &names {
        let file = format!("{SOURCE}/{name}.svg");
        let text =
            std::fs::read_to_string(root.join(&file)).map_err(|e| format!("{file} illisible : {e}"))?;
        icons.push((name, read_icon(&text, &file)?));
    }
    let mut out = String::new();
    let _ = write!(
        out,
        "//! Icônes du ruban (H-UI-77, ADR-0017 D6) : Lucide {LUCIDE_VERSION} (ISC ; dérivées de\n\
         //! Feather : MIT), `assets/icons/lucide/` (licences : `LICENSE` du même dossier).\n\
         //!\n\
         //! Fichier généré par `cargo xtask icons` : ne pas modifier à la main\n\
         //! (fraîcheur vérifiée par `h_ui_77_icon_data_fresh`). Coordonnées en\n\
         //! 1/64 d'unité de la grille 24 de Lucide ; trait de 2 unités, bouts ronds.\n\n"
    );
    let _ = writeln!(out, "/// Polyligne d'une icône (1/64 d'unité de grille).");
    out.push_str(
        "#[derive(Debug)]\npub struct IconLine {\n    /// Fermée (dernier point relié au premier).\n    \
         pub closed: bool,\n    /// Points, au moins deux.\n    pub points: &'static [(i16, i16)],\n}\n\n",
    );
    let _ = writeln!(
        out,
        "/// Formes d'une icône : polylignes et points isolés (disques)."
    );
    out.push_str(
        "#[derive(Debug)]\npub struct IconData {\n    /// Polylignes.\n    pub lines: &'static [IconLine],\n    \
         /// Points isolés.\n    pub dots: &'static [(i16, i16)],\n}\n\n",
    );
    let _ = writeln!(
        out,
        "/// Icône du ruban : un variant par fichier de `assets/icons/lucide/`."
    );
    out.push_str("#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]\npub enum Icon {\n");
    for (name, _) in &icons {
        let _ = writeln!(out, "    /// `{name}.svg`.\n    {},", variant_name(name));
    }
    out.push_str("}\n\nimpl Icon {\n    /// Toutes les icônes, dans l'ordre des fichiers.\n");
    let _ = writeln!(out, "    pub const ALL: [Self; {}] = [", icons.len());
    for (name, _) in &icons {
        let _ = writeln!(out, "        Self::{},", variant_name(name));
    }
    out.push_str("    ];\n\n    /// Nom du fichier Lucide (sans `.svg`).\n    #[must_use]\n    pub const fn name(self) -> &'static str {\n        match self {\n");
    for (name, _) in &icons {
        let _ = writeln!(out, "            Self::{} => \"{name}\",", variant_name(name));
    }
    out.push_str("        }\n    }\n\n    /// Formes de l'icône.\n    #[must_use]\n    pub const fn data(self) -> &'static IconData {\n        match self {\n");
    for (name, _) in &icons {
        let _ = writeln!(
            out,
            "            Self::{} => &{},",
            variant_name(name),
            const_name(name)
        );
    }
    out.push_str("        }\n    }\n}\n");
    for (name, shapes) in &icons {
        let _ = write!(
            out,
            "\n/// `{name}.svg`.\nstatic {}: IconData = IconData {{\n    lines: &[\n",
            const_name(name)
        );
        for line in &shapes.lines {
            let _ = writeln!(out, "        IconLine {{ closed: {}, points: &[", line.closed);
            write_points(&mut out, &line.points, "            ");
            out.push_str("        ] },\n");
        }
        out.push_str("    ],\n    dots: &[");
        if !shapes.dots.is_empty() {
            out.push('\n');
            write_points(&mut out, &shapes.dots, "        ");
            out.push_str("    ");
        }
        out.push_str("],\n};\n");
    }
    Ok(out)
}

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]

    use super::*;

    /// H-UI-77 : le fichier du dépôt est celui que donnent les SVG du dépôt.
    #[test]
    fn h_ui_77_icon_data_fresh() {
        let root = repo_root();
        let generated = generate(&root).unwrap();
        let current = std::fs::read_to_string(root.join(OUTPUT)).unwrap();
        assert!(
            current == generated,
            "{OUTPUT} périmé : relancer `cargo xtask icons`"
        );
    }

    /// Deux générations : mêmes octets.
    #[test]
    fn icons_generation_deterministic() {
        let root = repo_root();
        assert_eq!(generate(&root).unwrap(), generate(&root).unwrap());
    }

    /// Noms de variants.
    #[test]
    fn icons_variant_names() {
        assert_eq!(variant_name("chevrons-up-down"), "ChevronsUpDown");
        assert_eq!(variant_name("columns-3"), "Columns3");
        assert_eq!(variant_name("undo-2"), "Undo2");
        assert_eq!(const_name("undo-2"), "UNDO_2");
    }

    const HEAD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"#;

    fn icon(body: &str) -> Result<IconShapes, String> {
        read_icon(&format!("{HEAD}{body}</svg>"), "test.svg")
    }

    /// `h.01` ⇒ point isolé ; ligne ⇒ polyligne ouverte ; cercle ⇒ fermée.
    #[test]
    fn icons_dots_lines_and_closed_shapes() {
        let s = icon(
            r#"<path d="M4 12h.01"/><line x1="2" y1="3" x2="10" y2="3"/><circle cx="12" cy="12" r="3"/>"#,
        )
        .unwrap();
        assert_eq!(s.dots, vec![(256, 768)]);
        assert_eq!(s.lines.len(), 2);
        assert_eq!(
            s.lines[0],
            Line {
                closed: false,
                points: vec![(128, 192), (640, 192)]
            }
        );
        assert!(s.lines[1].closed && s.lines[1].points.len() > 8);
        // Arcs compacts (« 00-2-2 ») : lus.
        icon(r#"<path d="M21 10.5V5a2 2 0 00-2-2H5"/>"#).unwrap();
        // Rectangle arrondi.
        let r = icon(r#"<rect width="18" height="18" x="3" y="3" rx="2"/>"#).unwrap();
        assert!(r.lines[0].closed);
    }

    /// Refus explicites : élément, `transform`, `fill` coloré, racine
    /// différente, point hors grille.
    #[test]
    fn icons_reject_unsupported() {
        for body in [
            r#"<text x="1" y="1">a</text>"#,
            r#"<path d="M1 1h4" transform="rotate(45)"/>"#,
            r##"<path d="M1 1h4" fill="#ff0000"/>"##,
            r#"<g><path d="M1 1h4"/></g>"#,
            r#"<path d="M1 1h40"/>"#,
            r#"<rect width="4" height="4" rx="1" ry="2"/>"#,
            "",
        ] {
            assert!(icon(body).is_err(), "{body}");
        }
        assert!(icon(r#"<path d="M1 1h4" fill="currentColor"/>"#).is_ok());
        let wrong = r#"<svg viewBox="0 0 32 32" fill="none"><path d="M1 1h4"/></svg>"#;
        assert!(read_icon(wrong, "w.svg").is_err());
    }
}
