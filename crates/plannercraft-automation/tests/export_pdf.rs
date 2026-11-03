//! `project.export_pdf` (J8, lot L7) : six modèles sur `tracking_j6`,
//! mise en page de la requête, refus (`param.invalid`,
//! `pdf.too_many_pages`), avertissement `pdf.glyph_missing`. Avec
//! `PLANNERCRAFT_PDF_OUT=<dossier>`, les PDF des six modèles y sont copiés
//! (artefact de CI, relu par `pdftoppm`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use plannercraft_automation::Workspace;
use plannercraft_automation::error::codes;
use plannercraft_automation::text::Language;
use serde_json::{Value, json};

/// Dossier temporaire propre à un test (retiré à la fin du test).
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("plannercraft-pdf-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().to_owned()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn now() -> plannercraft_core::Moment {
    "2026-11-20T09:00".parse().unwrap()
}

fn tracking() -> Workspace {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/mcp/expected/tracking_j6.xml");
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.call("project.open", json!({ "path": path.to_str().unwrap() }))
        .unwrap();
    ws
}

const TEMPLATES: [&str; 6] = [
    "classique",
    "sobre",
    "contraste_eleve",
    "presentation",
    "chantier",
    "impression_nb",
];

/// Les six modèles sur `tracking_j6` : un PDF par modèle, document ni
/// modifié ni marqué, mêmes octets à deux appels (H-R13).
#[test]
fn export_pdf_six_templates_tracking_j6() {
    let dir = TempDir::new("six");
    let mut ws = tracking();
    let generation = ws.session().unwrap().generation();
    let out = std::env::var_os("PLANNERCRAFT_PDF_OUT").map(PathBuf::from);
    for t in TEMPLATES {
        let path = dir.file(&format!("{t}.pdf"));
        let v = ws
            .call("project.export_pdf", json!({ "path": path, "style": t }))
            .unwrap();
        assert_eq!(v["path"], json!(path));
        assert!(v["pages"].as_u64().unwrap() >= 1, "{t} : {v}");
        assert!(v["shown"].as_u64().unwrap() > 10, "{t} : {v}");
        assert_eq!(v["diagnostics"], json!([]), "{t}");
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert_eq!(v["bytes"].as_u64().unwrap(), u64::try_from(bytes.len()).unwrap());
        let again = dir.file(&format!("{t}-2.pdf"));
        ws.call("project.export_pdf", json!({ "path": again, "style": t }))
            .unwrap();
        assert_eq!(bytes, std::fs::read(&again).unwrap(), "{t} : même octets");
        if let Some(out) = &out {
            std::fs::create_dir_all(out).unwrap();
            std::fs::write(out.join(format!("tracking_j6-{t}.pdf")), &bytes).unwrap();
        }
    }
    let s = ws.session().unwrap();
    assert_eq!(s.generation(), generation, "rien écrit dans le document");
    assert!(!s.is_dirty());
}

/// Papier, orientation et « pages en largeur » de la requête ; plage et
/// lignes ; refus `param.invalid` sans fichier écrit.
#[test]
fn export_pdf_page_overrides_and_refusals() {
    let dir = TempDir::new("over");
    let mut ws = tracking();
    let a3 = dir.file("a3.pdf");
    let v = ws
        .call(
            "project.export_pdf",
            json!({
                "path": a3, "style": "sobre",
                "page": { "paper": "a3", "landscape": true, "pages_wide": 2 },
                "range": { "from": "2026-11-02", "to": "2026-12-31" },
                "rows": { "critical_only": true, "outline_max": 2 }
            }),
        )
        .unwrap();
    assert!(v["hidden"].as_u64().unwrap() > 0, "{v}");
    let text = String::from_utf8_lossy(&std::fs::read(&a3).unwrap()).into_owned();
    assert!(text.contains("/MediaBox[0 0 1191 842]"), "A3 paysage");
    for (params, field) in [
        (
            json!({ "path": dir.file("x.pdf"), "page": { "pages_wide": 0 } }),
            "page.pages_wide",
        ),
        (
            json!({ "path": dir.file("x.pdf"), "page": { "pages_wide": 21 } }),
            "page.pages_wide",
        ),
        (
            json!({ "path": dir.file("x.pdf"), "range": { "from": "2026-12-01", "to": "2026-11-01" } }),
            "range.from",
        ),
        (
            json!({ "path": dir.file("x.pdf"), "rows": { "outline_max": 0 } }),
            "rows.outline_max",
        ),
    ] {
        let err = ws.call("project.export_pdf", params).unwrap_err();
        assert_eq!(err.code, codes::PARAM_INVALID);
        assert_eq!(err.field.as_deref(), Some(field));
    }
    assert!(!Path::new(&dir.file("x.pdf")).exists());
    let err = ws
        .call(
            "project.export_pdf",
            json!({ "path": dir.file("y.pdf"), "style": "inconnu" }),
        )
        .unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
}

fn chain(n: usize, names: &dyn Fn(usize) -> String) -> Workspace {
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Long", "start": "2026-11-02T08:00" }),
    )
    .unwrap();
    let mut ops: Vec<Value> = Vec::new();
    for i in 0..n {
        ops.push(json!({ "op": "task.add", "name": names(i), "duration": { "text": "3j" }, "as": format!("$t{i}") }));
        if i > 0 {
            ops.push(json!({ "op": "link.add", "pred": format!("$t{}", i - 1), "succ": format!("$t{i}") }));
        }
    }
    ws.call("batch.apply", json!({ "format_version": 1, "ops": ops }))
        .unwrap();
    ws
}

/// `pdf.too_many_pages` : 200 tâches enchaînées à l'échelle 400 % (style
/// du document) : plus de 1 000 pages, refus sans fichier écrit.
#[test]
fn export_pdf_too_many_pages() {
    let dir = TempDir::new("many");
    let mut ws = chain(200, &|i| format!("Tâche {i}"));
    let page = json!({
        "paper": "a4", "landscape": true, "margins_mm": [10, 10, 10, 10],
        "fit": { "scale": 400 }, "repeat_table": true,
        "header": ["{projet}", "", ""], "footer": ["", "", "{page}/{pages}"], "logo": null
    });
    ws.call(
        "style.apply",
        json!({ "template": "sobre", "patch": { "page": page } }),
    )
    .unwrap();
    let path = dir.file("many.pdf");
    let err = ws
        .call("project.export_pdf", json!({ "path": path }))
        .unwrap_err();
    assert_eq!(err.code, codes::PDF_TOO_MANY_PAGES, "{err:?}");
    assert!(!Path::new(&path).exists());
    // « 1 page en largeur » de la requête : tient.
    let v = ws
        .call(
            "project.export_pdf",
            json!({ "path": path, "page": { "pages_wide": 1 } }),
        )
        .unwrap();
    assert!(v["pages"].as_u64().unwrap() <= 1_000);
}

/// `pdf.glyph_missing` : noms hors d'Inter (synthétiques) : PDF écrit,
/// avertissement et caractères listés.
#[test]
fn export_pdf_glyph_missing_warning() {
    let dir = TempDir::new("cjk");
    let mut ws = chain(3, &|i| ["基礎", "東京", "Murs"][i].to_owned());
    let path = dir.file("cjk.pdf");
    let v = ws
        .call("project.export_pdf", json!({ "path": path, "style": "sobre" }))
        .unwrap();
    assert_eq!(v["diagnostics"], json!([codes::PDF_GLYPH_MISSING]));
    assert_eq!(v["missing_glyphs"], json!("京基東礎"));
    assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF-"));
}
