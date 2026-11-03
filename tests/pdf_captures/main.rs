//! Captures PDF des modèles de mise en page (J8, lot L16, vérification 4 de
//! `docs/plans/j8.md`) : `project.export_pdf` (`plannercraft_automation::pdf`,
//! même chemin que la CLI et le MCP) sur le plan de suivi de J6
//! (`tests/mcp/expected/tracking_j6.xml`) pour chacun des six modèles, et
//! sur le réseau généré de 5 000 tâches en Sobre, A3 paysage.
//!
//! - `pdf_captures_six_templates` (toujours exécuté) : un PDF par modèle,
//!   au moins une page, aucun glyphe manquant, aucune ligne masquée ;
//!   deux exports égaux octet pour octet (H-R13).
//! - `pdf_captures_timeline` (toujours exécuté, ADR-0017 lot L1, H-UI-82) :
//!   style du document avec chronologie (récapitulative, tâches, deux
//!   jalons) et case « Chronologie » de la mise en page ; PDF déterministe,
//!   différent de l'export sans frise, aucun glyphe manquant.
//! - `pdf_captures_network_5000` (ignoré, release) : 5 000 tâches, Sobre,
//!   A3 paysage, 1 page en largeur ; au plus `MAX_PAGES` pages.
//!
//! Avec `PLANNERCRAFT_PDF_CAPTURES=<dossier>`, chaque PDF est écrit dans ce
//! dossier (`tracking_j6_<modèle>.pdf`, `network_5000_sobre_a3.pdf`) : le
//! job CI `pdf-captures` les rend en PNG par `pdftoppm` et publie le tout en
//! artefact, pour la relecture visuelle (aucune référence d'image versionnée).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stderr,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use std::path::PathBuf;

use plannercraft_automation::command::params::ExportPdfParams;
use plannercraft_automation::pdf::{PdfExport, export};
use plannercraft_automation::session::DocOrigin;
use plannercraft_automation::text::Language;
use plannercraft_automation::{CallEnv, Session, call_with};
use plannercraft_tests::generate::ui_network;
use plannercraft_tests::{moment, tests_path};
use serde_json::json;

/// Clés des six modèles intégrés (ordre de la galerie).
const TEMPLATES: [&str; 6] = [
    "classique",
    "sobre",
    "contraste_eleve",
    "presentation",
    "chantier",
    "impression_nb",
];

/// Dossier de sortie demandé, s'il y en a un.
fn out_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("PLANNERCRAFT_PDF_CAPTURES")?);
    std::fs::create_dir_all(&dir).unwrap();
    Some(dir)
}

/// Exporte `s` avec les paramètres `params` (JSON du registre).
fn pdf_of(s: &Session, params: &serde_json::Value) -> PdfExport {
    let p: ExportPdfParams = serde_json::from_value(params.clone()).unwrap();
    export(s, &p, Language::Fr).unwrap_or_else(|e| panic!("{params} : {e:?}"))
}

/// Écrit `bytes` sous `name` dans le dossier demandé.
fn keep(name: &str, bytes: &[u8]) {
    if let Some(dir) = out_dir() {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        eprintln!("PDF écrit : {}", path.display());
    }
}

#[test]
fn pdf_captures_six_templates() {
    let xml = std::fs::read(tests_path("mcp/expected/tracking_j6.xml")).unwrap();
    let origin = DocOrigin {
        name: "tracking_j6.xml".to_owned(),
        path: None,
    };
    let (session, _) = Session::open_bytes(&xml, origin, moment("2026-10-26T08:00"), Language::Fr).unwrap();
    let tasks = u32::try_from(session.project().tasks.len()).unwrap();
    for key in TEMPLATES {
        let params = json!({ "path": format!("{key}.pdf"), "style": key });
        let first = pdf_of(&session, &params);
        assert!(first.bytes.starts_with(b"%PDF-"), "{key}");
        assert_ne!(first.report.pages, 0, "{key}");
        assert_eq!(first.report.missing_glyphs, "", "{key} : glyphes manquants");
        assert_eq!(first.report.hidden, 0, "{key} : lignes masquées");
        assert_eq!(first.report.shown, tasks, "{key}");
        let again = pdf_of(&session, &params);
        assert_eq!(first.bytes, again.bytes, "{key} : PDF non déterministe (H-R13)");
        eprintln!("{key} : {} page(s)", first.report.pages);
        keep(&format!("tracking_j6_{key}.pdf"), &first.bytes);
    }
}

/// H-UI-82 : `project.export_pdf` imprime la chronologie du style du
/// document quand sa mise en page la demande (même chemin que la CLI et le
/// MCP) ; PDF gardé sous `tracking_j6_timeline.pdf` pour la relecture.
#[test]
fn pdf_captures_timeline() {
    let xml = std::fs::read(tests_path("mcp/expected/tracking_j6.xml")).unwrap();
    let origin = DocOrigin {
        name: "tracking_j6.xml".to_owned(),
        path: None,
    };
    let now = moment("2026-10-26T08:00");
    let (mut session, _) = Session::open_bytes(&xml, origin, now, Language::Fr).unwrap();
    let page = json!({
        "paper": "a4", "landscape": true, "margins_mm": [10, 10, 10, 10],
        "fit": { "scale": 100 }, "repeat_table": true,
        "header": ["{projet}", "", ""], "footer": ["", "", "{page}/{pages}"], "logo": null,
        "timeline": true
    });
    let patch = json!({
        "template": "sobre",
        "patch": { "page": page, "timeline": { "tasks": [1, 3, 6, 8, 11, 16, 20] } }
    });
    call_with(
        &mut session,
        &CallEnv::new(now, Language::Fr),
        "style.apply",
        patch,
    )
    .unwrap();
    let with = pdf_of(&session, &json!({ "path": "timeline.pdf" }));
    assert!(with.bytes.starts_with(b"%PDF-"));
    assert_ne!(with.report.pages, 0);
    assert_eq!(with.report.missing_glyphs, "", "glyphes manquants");
    let again = pdf_of(&session, &json!({ "path": "timeline.pdf" }));
    assert_eq!(with.bytes, again.bytes, "PDF non déterministe (H-R13)");
    let without = pdf_of(&session, &json!({ "path": "plain.pdf", "style": "sobre" }));
    assert_ne!(with.bytes, without.bytes);
    eprintln!("chronologie : {} page(s)", with.report.pages);
    keep("tracking_j6_timeline.pdf", &with.bytes);
}

#[test]
#[ignore = "réseau de 5 000 tâches : à lancer en --release (job CI pdf-captures)"]
fn pdf_captures_network_5000() {
    let project = ui_network(250, 19, 10_000);
    let origin = DocOrigin {
        name: "reseau_5000.xml".to_owned(),
        path: None,
    };
    let session = Session::from_project(project, origin, moment("2026-01-05T08:00"), Language::Fr);
    let params = json!({
        "path": "network_5000_sobre_a3.pdf",
        "style": "sobre",
        "page": { "paper": "a3", "landscape": true, "pages_wide": 1 }
    });
    let out = pdf_of(&session, &params);
    assert!(out.bytes.starts_with(b"%PDF-"));
    assert_eq!(out.report.shown, 5_000);
    assert!(
        out.report.pages > 1 && out.report.pages <= 1_000,
        "{} pages",
        out.report.pages
    );
    assert_eq!(out.report.missing_glyphs, "");
    eprintln!("réseau 5 000 : {} pages", out.report.pages);
    keep("network_5000_sobre_a3.pdf", &out.bytes);
}
