//! Style dans le document (lot L6 de J8, ADR-0016 D7) : `style.apply`,
//! `style.get`, `style.list`, `style.set_bar_role`,
//! `style.reset_to_template`, `style.save_as_template` ; annulation,
//! `dry_run`, écriture et lecture de `<?plannercraft-style?>` (H-R12), modèle par
//! défaut (H-UI-43), aller-retour (P56).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use plannercraft_automation::command::style::{self, HEADLESS_DEFAULT};
use plannercraft_automation::error::codes;
use plannercraft_automation::session::DocOrigin;
use plannercraft_automation::text::Language;
use plannercraft_automation::{AnchorSpec, ApplyMode, Batch, CallEnv, NewProject, Session, call_with};
use plannercraft_mspdi::write::WriteOptions;
use plannercraft_mspdi::{ImportCode, style_pi};
use plannercraft_render::style::{BarRole, Rgb, TemplateId, Variant, template};
use proptest::prelude::*;
use serde_json::{Value, json};

fn now() -> plannercraft_core::Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn env() -> CallEnv {
    CallEnv::new(now(), Language::Fr)
}

fn session() -> Session {
    let init = NewProject {
        name: "P".to_owned(),
        anchor: AnchorSpec::Start(now()),
        lang: None,
    };
    let mut s = Session::new_project(init, now()).unwrap();
    let batch = Batch::from_json(json!({
        "format_version": 1,
        "ops": [
            { "op": "task.add", "name": "Fondations", "duration": { "text": "5j" } },
            { "op": "task.add", "name": "Murs", "duration": { "text": "10j" } }
        ]
    }))
    .unwrap();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    s
}

fn call(s: &mut Session, id: &str, params: Value) -> Value {
    call_with(s, &env(), id, params).unwrap_or_else(|e| panic!("{id} : {e:?}"))
}

fn payload(s: &Session) -> Option<String> {
    style_pi::entry(&s.project().extras).map(|e| e.payload.clone())
}

fn xml(s: &Session) -> String {
    String::from_utf8(s.save_bytes(&WriteOptions::default()).xml).unwrap()
}

fn reopen(bytes: &[u8]) -> (Session, Vec<plannercraft_mspdi::ImportDiagnostic>) {
    let origin = DocOrigin {
        name: "p.xml".to_owned(),
        path: None,
    };
    Session::open_bytes(bytes, origin, now(), Language::Fr).unwrap()
}

#[test]
fn style_apply_undo_redo() {
    let mut s = session();
    let before = s.project().clone();
    let bytes_before = xml(&s);
    assert!(!bytes_before.contains("plannercraft-style"));
    s.mark_saved();
    let out = call(&mut s, "style.apply", json!({ "template": "presentation" }));
    assert_eq!(out["committed"], true);
    assert!(s.is_dirty(), "document marqué modifié");
    assert_eq!(payload(&s).as_deref(), Some(r#"{"base":"presentation","v":1}"#));
    assert!(xml(&s).contains("<?plannercraft-style {\"base\":\"presentation\",\"v\":1}?>"));
    // Une entrée d'annulation par appel.
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.project(), &before);
    assert_eq!(xml(&s), bytes_before);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(payload(&s).as_deref(), Some(r#"{"base":"presentation","v":1}"#));
    // Surcharge seule : fusionnée avec le style du document.
    call(
        &mut s,
        "style.apply",
        json!({ "patch": { "density": "compact" } }),
    );
    let doc = style::doc_style(s.project()).unwrap();
    assert_eq!(doc.base, TemplateId::Presentation);
    assert_eq!(
        doc.over.density,
        Some(plannercraft_render::style::Density::Compact)
    );
    // Retour au défaut : plus rien dans le fichier.
    call(&mut s, "style.apply", json!({ "reset": true }));
    assert!(payload(&s).is_none());
    assert_eq!(xml(&s), bytes_before);
    call(&mut s, "history.undo", json!({}));
    assert!(style::doc_style(s.project()).unwrap().over.density.is_some());
}

#[test]
fn style_apply_dry_run() {
    let mut s = session();
    let before = s.project().clone();
    let generation = s.generation();
    let out = call(
        &mut s,
        "style.apply",
        json!({ "template": "sobre", "dry_run": true }),
    );
    assert_eq!(out["committed"], false);
    assert_eq!(s.project(), &before);
    assert_eq!(s.generation(), generation);
    let out = call(
        &mut s,
        "style.set_bar_role",
        json!({ "role": "critical", "color": "#AA0000", "dry_run": true }),
    );
    assert_eq!(out["committed"], false);
    assert_eq!(s.project(), &before);
}

#[test]
fn style_apply_refusals_leave_session_intact() {
    let mut s = session();
    let before = s.project().clone();
    for (params, code) in [
        (json!({}), codes::PARAM_INVALID),
        (
            json!({ "reset": true, "template": "sobre" }),
            codes::PARAM_INVALID,
        ),
        (json!({ "patch": { "unknown": 1 } }), codes::STYLE_INVALID),
        (
            json!({ "patch": { "font": { "family": "inter", "size16": 999 } } }),
            codes::STYLE_INVALID,
        ),
        (
            json!({ "patch": { "name": "x".repeat(65) } }),
            codes::STYLE_INVALID,
        ),
        (json!({ "template": "nope" }), codes::PARAM_INVALID),
    ] {
        let err = call_with(&mut s, &env(), "style.apply", params.clone()).unwrap_err();
        assert_eq!(err.code, code, "{params}");
    }
    assert_eq!(s.project(), &before);
}

/// H-UI-59, H-UI-60 : un geste sur un type de barre = une entrée
/// d'annulation ; les autres rôles et variantes ne bougent pas.
#[test]
fn r_style_set_bar_role_undo() {
    let mut s = session();
    call(&mut s, "style.apply", json!({ "template": "sobre" }));
    let sobre = template(TemplateId::Sobre);
    let history = s.history().entries.len();
    call(
        &mut s,
        "style.set_bar_role",
        json!({ "role": "critical", "color": "#AA0000", "thickness": 60, "ends": "square",
                "text_left": "duration", "text_right": null }),
    );
    assert_eq!(s.history().entries.len(), history + 1);
    let st = style::effective_style(s.project(), HEADLESS_DEFAULT);
    let paint = st.tokens(Variant::Light).bars[&BarRole::Critical];
    assert_eq!(paint.fill, Rgb(0xAA, 0, 0));
    // Contour gardé (celui du modèle), sombre et rôle voisin inchangés.
    assert_eq!(
        paint.outline,
        sobre.tokens(Variant::Light).bars[&BarRole::Critical].outline
    );
    assert_eq!(
        st.tokens(Variant::Dark).bars[&BarRole::Critical],
        sobre.tokens(Variant::Dark).bars[&BarRole::Critical]
    );
    assert_eq!(st.bars[&BarRole::Task], sobre.bars[&BarRole::Task]);
    let shape = st.bars[&BarRole::Critical];
    assert_eq!((shape.height_pct, shape.radius16), (60, 0));
    assert_eq!(shape.pattern, sobre.bars[&BarRole::Critical].pattern);
    let texts = st.texts[&BarRole::Critical];
    assert_eq!(texts.left, Some(plannercraft_render::style::BarField::Duration));
    assert_eq!(texts.right, None);
    // Second geste sur un autre rôle : le premier reste.
    call(
        &mut s,
        "style.set_bar_role",
        json!({ "role": "task", "color": "#004488", "variant": "dark" }),
    );
    let st = style::effective_style(s.project(), HEADLESS_DEFAULT);
    assert_eq!(
        st.tokens(Variant::Dark).bars[&BarRole::Task].fill,
        Rgb(0, 0x44, 0x88)
    );
    assert_eq!(
        st.tokens(Variant::Light).bars[&BarRole::Critical].fill,
        Rgb(0xAA, 0, 0)
    );
    // Ctrl+Z : couleur d'avant (H-UI-60).
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(style::effective_style(s.project(), HEADLESS_DEFAULT), sobre);
    // Sans style : part du modèle `base` (défaut Classique).
    let mut fresh = session();
    call(
        &mut fresh,
        "style.set_bar_role",
        json!({ "role": "task", "color": "#123456", "base": "chantier" }),
    );
    assert_eq!(
        style::doc_style(fresh.project()).unwrap().base,
        TemplateId::Chantier
    );
    // Refus : aucun réglage, couleur mal formée, épaisseur hors bornes.
    for params in [
        json!({ "role": "task" }),
        json!({ "role": "task", "color": "red" }),
        json!({ "role": "task", "thickness": 5 }),
        json!({ "role": "task", "variant": "dark" }),
    ] {
        let err = call_with(&mut s, &env(), "style.set_bar_role", params.clone()).unwrap_err();
        assert_eq!(err.code, codes::PARAM_INVALID, "{params}");
    }
}

/// H-UI-61 « Revenir au modèle » : toutes les surcharges, ou celles d'un
/// type de barre ; le modèle de base reste, une entrée d'annulation.
#[test]
fn r_style_reset_to_template() {
    let mut s = session();
    call(
        &mut s,
        "style.apply",
        json!({ "template": "sobre", "patch": { "density": "airy" } }),
    );
    call(
        &mut s,
        "style.set_bar_role",
        json!({ "role": "critical", "color": "#AA0000" }),
    );
    call(
        &mut s,
        "style.set_bar_role",
        json!({ "role": "task", "thickness": 50, "text_inside": "percent" }),
    );
    call(&mut s, "style.reset_to_template", json!({ "role": "critical" }));
    let doc = style::doc_style(s.project()).unwrap();
    assert!(
        doc.over.colors.is_none(),
        "couleur critique retirée, variantes vides retirées"
    );
    assert!(doc.over.bars.contains_key(&BarRole::Task));
    assert_eq!(doc.over.density, Some(plannercraft_render::style::Density::Airy));
    call(&mut s, "style.reset_to_template", json!({}));
    let doc = style::doc_style(s.project()).unwrap();
    assert_eq!(doc.base, TemplateId::Sobre);
    assert!(doc.over.is_empty());
    assert_eq!(payload(&s).as_deref(), Some(r#"{"base":"sobre","v":1}"#));
    call(&mut s, "history.undo", json!({}));
    assert!(!style::doc_style(s.project()).unwrap().over.is_empty());
    // Sans style : rien à faire, rien d'écrit.
    let mut fresh = session();
    call(&mut fresh, "style.reset_to_template", json!({}));
    assert!(payload(&fresh).is_none());
}

#[test]
fn style_get_and_list_views() {
    let mut s = session();
    let v = call(&mut s, "style.get", json!({}));
    assert_eq!(v["source"], "default");
    assert_eq!(v["base"], "classique");
    assert_eq!(v["resolved"]["name"], template(TemplateId::Classique).name);
    let l = call(&mut s, "style.list", json!({}));
    assert_eq!(l["templates"].as_array().unwrap().len(), 6);
    assert_eq!(l["templates"][1]["id"], "sobre");
    assert_eq!(l["document"], Value::Null);
    assert_eq!(l["default"], "classique");
    call(
        &mut s,
        "style.apply",
        json!({ "template": "chantier", "patch": { "density": "airy" } }),
    );
    let v = call(&mut s, "style.get", json!({}));
    assert_eq!(v["source"], "document");
    assert_eq!(v["base"], "chantier");
    assert_eq!(v["over"], json!({ "density": "airy" }));
    assert_eq!(v["resolved"]["density"], "airy");
    let l = call(&mut s, "style.list", json!({}));
    assert_eq!(l["document"], json!({ "base": "chantier", "customized": true }));
}

/// H-UI-61 : une couleur qui fait tomber un contraste sous H-UI-45 est
/// signalée, avec la couleur conforme la plus proche.
#[test]
fn style_get_reports_contrast_issue_with_suggestion() {
    let mut s = session();
    call(&mut s, "style.apply", json!({ "template": "sobre" }));
    let v = call(&mut s, "style.get", json!({}));
    assert_eq!(v["contrast_issues"], json!([]), "Sobre conforme");
    call(
        &mut s,
        "style.set_bar_role",
        json!({ "role": "task", "color": "#FFF59D" }),
    );
    let v = call(&mut s, "style.get", json!({}));
    let issues = v["contrast_issues"].as_array().unwrap();
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert_eq!(issues[0]["role"], "task");
    assert_eq!(issues[0]["variant"], "light");
    assert_eq!(issues[0]["min"], 3000);
    let suggested = Rgb::parse(issues[0]["suggestion"].as_str().unwrap()).unwrap();
    assert!(plannercraft_render::style::contrast_ratio(suggested, Rgb(0xFF, 0xFF, 0xFF)) >= 3000);
}

#[test]
fn style_save_as_template_reads_only() {
    let mut s = session();
    call(&mut s, "style.apply", json!({ "template": "presentation" }));
    let before = s.project().clone();
    let generation = s.generation();
    let v = call(
        &mut s,
        "style.save_as_template",
        json!({ "name": "  Client A  " }),
    );
    assert_eq!(
        v,
        json!({ "name": "Client A", "style": { "v": 1, "base": "presentation" } })
    );
    assert_eq!(s.project(), &before);
    assert_eq!(s.generation(), generation);
    for name in ["", "   ", &"x".repeat(65)] {
        let err = call_with(&mut s, &env(), "style.save_as_template", json!({ "name": name })).unwrap_err();
        assert_eq!(err.code, codes::PARAM_INVALID);
    }
}

/// `render.gantt` sans `style` dessine le style du document.
#[test]
fn render_gantt_uses_document_style() {
    let mut s = session();
    let classique = call(&mut s, "render.gantt", json!({}));
    assert!(
        classique["svg"]
            .as_str()
            .unwrap()
            .contains("font-family:sans-serif")
    );
    call(&mut s, "style.apply", json!({ "template": "sobre" }));
    let doc = call(&mut s, "render.gantt", json!({}));
    let sobre = call(&mut s, "render.gantt", json!({ "style": "sobre" }));
    assert_eq!(doc["svg"], sobre["svg"]);
    assert!(doc["svg"].as_str().unwrap().contains("Inter,sans-serif"));
}

/// H-R12 : version inconnue à la lecture ⇒ ignorée, diagnostic
/// `style.ignored`, modèle par défaut.
#[test]
fn h_r12_unknown_version_ignored() {
    let s = session();
    let base = xml(&s);
    for bad in [
        r#"{"base":"sobre","v":2}"#,
        r#"{"base":"sobre","v":1,"extra":true}"#,
        r#"{"base":"inconnu","v":1}"#,
        "pas du json",
    ] {
        let text = base.replacen("<Project", &format!("<?plannercraft-style {bad}?>\n<Project"), 1);
        let (opened, diags) = reopen(text.as_bytes());
        assert!(payload(&opened).is_none(), "{bad}");
        let d = diags
            .iter()
            .find(|d| d.code.code() == "style.ignored")
            .unwrap_or_else(|| panic!("{bad} : {diags:?}"));
        assert_eq!(
            d.code,
            ImportCode::StyleIgnored {
                reason: style_pi::StyleIgnoredReason::Invalid
            }
        );
        assert_eq!(
            style::effective_style(opened.project(), HEADLESS_DEFAULT),
            template(HEADLESS_DEFAULT)
        );
        // Réécrit sans instruction.
        assert_eq!(xml(&opened), base, "{bad}");
    }
}

/// P56 (aller-retour) : MSPDI avec style lu puis écrit = mêmes octets ; le
/// style relu égale le style écrit.
#[test]
fn p56_styled_session_roundtrip_same_bytes() {
    let mut s = session();
    call(
        &mut s,
        "style.apply",
        json!({ "template": "contraste_eleve", "patch": { "density": "compact" } }),
    );
    call(
        &mut s,
        "style.set_bar_role",
        json!({ "role": "milestone", "color": "#102030", "text_right": "finish" }),
    );
    let first = xml(&s);
    let (opened, diags) = reopen(first.as_bytes());
    assert!(
        diags.iter().all(|d| d.code.code() != "style.ignored"),
        "{diags:?}"
    );
    assert_eq!(style::doc_style(opened.project()), style::doc_style(s.project()));
    assert_eq!(xml(&opened), first);
    // Préférence « ne jamais écrire » : fichier sans style, octets J7.
    let off = WriteOptions {
        style: false,
        ..WriteOptions::default()
    };
    let plain = String::from_utf8(opened.save_bytes(&off).xml).unwrap();
    assert!(!plain.contains("plannercraft-style"));
}

/// H-UI-43 : document sans style ⇒ modèle par défaut de l'appelant.
#[test]
fn h_ui_43_default_template_for_unstyled_document() {
    let mut s = session();
    assert_eq!(
        style::effective_style(s.project(), TemplateId::Sobre),
        template(TemplateId::Sobre)
    );
    call(&mut s, "style.apply", json!({ "template": "presentation" }));
    assert_eq!(
        style::effective_style(s.project(), TemplateId::Sobre),
        template(TemplateId::Presentation)
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// P56 (lecture) : toute charge d'instruction donne le style du fichier
    /// (valide) ou le modèle par défaut avec diagnostic, jamais de panique.
    #[test]
    fn p56_any_pi_payload_opens(text in "[ -~]{0,200}", valid in any::<bool>()) {
        let body = if valid {
            r#"{"base":"sobre","over":{"density":"airy"},"v":1}"#.to_owned()
        } else {
            strip_pi_end(&text)
        };
        let ignored = open_with_pi(&body)?;
        if valid {
            prop_assert!(!ignored);
        }
    }
}

/// Retire `?>` jusqu'au point fixe : une seule passe en laisse (`??>>` ⇒
/// `?>`), qui fermerait l'instruction avant la fin de la charge.
fn strip_pi_end(text: &str) -> String {
    let mut out = text.to_owned();
    while out.contains("?>") {
        out = out.replace("?>", "");
    }
    out
}

/// Ouvre la session de base précédée de `<?plannercraft-style {body}?>` ; vrai si
/// le style est ignoré (diagnostic), faux s'il est appliqué.
fn open_with_pi(body: &str) -> Result<bool, TestCaseError> {
    let base = xml(&session());
    let doc = base.replacen("<Project", &format!("<?plannercraft-style {body}?>\n<Project"), 1);
    let (opened, diags) = reopen(doc.as_bytes());
    let ignored = diags.iter().any(|d| d.code.code() == "style.ignored");
    match style::doc_style(opened.project()) {
        Some(_) => prop_assert!(!ignored),
        None => prop_assert!(ignored || body.trim().is_empty() && payload(&opened).is_none()),
    }
    Ok(ignored)
}

/// P56, cas réduits par la CI (run 40644375476) : `??>><` laissait un `?>`
/// après une passe de retrait ; une charge avec `&`, `<` ou `&amp;` est
/// prise telle quelle (XML 1.0 § 2.6) : le fichier s'ouvre, style ignoré
/// (JSON invalide), diagnostic `style.ignored`.
#[test]
fn p56_pi_payload_regressions() {
    assert_eq!(strip_pi_end("??>><"), "<");
    assert_eq!(strip_pi_end("a??>>?>b"), "ab");
    for raw in ["??>><", "a&b", "x<y", "&amp;", "&#38; &lt; <Project>"] {
        let body = strip_pi_end(raw);
        let ignored = open_with_pi(&body).map_err(|e| e.to_string());
        assert_eq!(ignored, Ok(true), "{raw:?}");
    }
}

/// ADR-0017 (lot L0), P56 étendue : chronologie (`timeline`) et case de
/// mise en page (`page.timeline`) écrites par `style.apply`, relues à
/// l'identique ; MSPDI stylé lu puis écrit = mêmes octets ; une entrée
/// d'annulation.
#[test]
fn style_timeline_roundtrip() {
    let mut s = session();
    let unstyled = xml(&s);
    let mut page = serde_json::to_value(template(TemplateId::Sobre).page).unwrap();
    page["timeline"] = json!(true);
    call(
        &mut s,
        "style.apply",
        json!({ "template": "sobre", "patch": { "timeline": { "tasks": [1, 2] }, "page": page } }),
    );
    let resolved = style::effective_style(s.project(), HEADLESS_DEFAULT);
    assert_eq!(resolved.timeline.tasks, vec![1, 2]);
    assert!(resolved.page.timeline);
    let first = xml(&s);
    let (opened, diags) = reopen(first.as_bytes());
    assert!(
        diags.iter().all(|d| d.code.code() != "style.ignored"),
        "{diags:?}"
    );
    assert_eq!(style::doc_style(opened.project()), style::doc_style(s.project()));
    assert_eq!(xml(&opened), first);
    // Une surcharge seule remplace la liste en entier.
    call(
        &mut s,
        "style.apply",
        json!({ "patch": { "timeline": { "tasks": [2] } } }),
    );
    let resolved = style::effective_style(s.project(), HEADLESS_DEFAULT);
    assert_eq!(resolved.timeline.tasks, vec![2]);
    assert!(resolved.page.timeline, "page gardée");
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(xml(&s), unstyled, "MSPDI sans style : octets inchangés");
}

/// H-UI-79 : tâche de plus grand UID sur la frise, supprimée, enregistrer,
/// rouvrir, ajouter une tâche (qui reprend cet UID) ⇒ frise vide ; les UID
/// des tâches présentes sont gardés.
#[test]
fn h_ui_79_deleted_uid_dropped_on_reopen() {
    let mut s = session();
    let add = Batch::from_json(json!({
        "format_version": 1,
        "ops": [ { "op": "task.add", "name": "Toit", "duration": { "text": "2j" } } ]
    }))
    .unwrap();
    s.apply(&add, ApplyMode::Commit).unwrap();
    let last = s.project().tasks.iter().map(|t| t.uid().get()).max().unwrap();
    call(
        &mut s,
        "style.apply",
        json!({ "patch": { "timeline": { "tasks": [1, last] } } }),
    );
    let delete = Batch::from_json(json!({
        "format_version": 1,
        "ops": [ { "op": "task.delete", "uid": last } ]
    }))
    .unwrap();
    s.apply(&delete, ApplyMode::Commit).unwrap();
    let (mut opened, _) = reopen(xml(&s).as_bytes());
    assert_eq!(
        style::effective_style(opened.project(), HEADLESS_DEFAULT)
            .timeline
            .tasks,
        vec![1]
    );
    opened.apply(&add, ApplyMode::Commit).unwrap();
    assert!(
        opened
            .project()
            .tasks
            .get(plannercraft_core::TaskUid::new(last))
            .is_some(),
        "UID repris"
    );
    assert_eq!(
        style::effective_style(opened.project(), HEADLESS_DEFAULT)
            .timeline
            .tasks,
        vec![1]
    );
}

/// ADR-0017 D7 : `style.apply { template }` (agent MCP) garde la liste de
/// la chronologie et les couleurs de phase ; `patch` les remplace ;
/// `reset` les retire.
#[test]
fn style_apply_template_keeps_timeline() {
    let mut s = session();
    call(
        &mut s,
        "style.apply",
        json!({ "patch": { "timeline": { "tasks": [1, 2] } } }),
    );
    call(&mut s, "style.apply", json!({ "template": "chantier" }));
    let resolved = style::effective_style(s.project(), HEADLESS_DEFAULT);
    assert_eq!(resolved.timeline.tasks, vec![1, 2]);
    call(
        &mut s,
        "style.apply",
        json!({ "template": "sobre", "patch": { "timeline": { "tasks": [2] } } }),
    );
    let resolved = style::effective_style(s.project(), HEADLESS_DEFAULT);
    assert_eq!(resolved.timeline.tasks, vec![2]);
    call(&mut s, "style.apply", json!({ "reset": true }));
    assert!(
        style::effective_style(s.project(), HEADLESS_DEFAULT)
            .timeline
            .is_empty()
    );
}

/// ADR-0017 (lot L0) : 201 UID ou doublons ⇒ `style.apply` refusé
/// (`style.invalid`), rien écrit ; charge du fichier hors bornes ⇒ style
/// retiré, diagnostic `style.ignored` ; jamais de panique.
#[test]
fn style_timeline_bounded() {
    let mut s = session();
    let before = xml(&s);
    let many: Vec<u32> = (1..=201).collect();
    for tasks in [json!(many), json!([2, 2]), json!([3, 1])] {
        let err = call_with(
            &mut s,
            &env(),
            "style.apply",
            json!({ "template": "sobre", "patch": { "timeline": { "tasks": tasks } } }),
        )
        .unwrap_err();
        assert_eq!(err.code, codes::STYLE_INVALID, "{err:?}");
        assert_eq!(xml(&s), before);
    }
    let payload = r#"{"base":"sobre","over":{"timeline":{"tasks":[2,2]}},"v":1}"#;
    let bad = before.replacen(
        "<Project",
        &format!("<?plannercraft-style {payload}?>\n<Project"),
        1,
    );
    let (opened, diags) = reopen(bad.as_bytes());
    assert!(
        diags.iter().any(|d| d.code.code() == "style.ignored"),
        "{diags:?}"
    );
    assert!(style::doc_style(opened.project()).is_none());
}

/// ADR-0017 (lot L0) : squelette de `timeline::input` ; extrémités = dates
/// du planning (règle 3), aucune autre donnée avant L1.
#[test]
fn timeline_input_skeleton_uses_engine_bounds() {
    let s = session();
    let spec = plannercraft_render::style::TimelineSpec { tasks: vec![1, 99] };
    let input = plannercraft_automation::timeline::input(&s, &spec, Language::Fr);
    assert_eq!(input.start, s.schedule().project_start);
    assert_eq!(input.finish, s.schedule().project_finish);
}

/// H-R12 : un nom de style contenant `?>`, `&` et `<` ne ferme pas
/// l'instruction (`>` écrit `>` par le JSON canonique) ; le fichier
/// se relit avec le même style et les mêmes octets.
#[test]
fn h_r12_hostile_style_name_roundtrips() {
    let mut s = session();
    call(
        &mut s,
        "style.apply",
        json!({ "template": "sobre", "patch": { "name": "a?>b &amp; <c>" } }),
    );
    let first = xml(&s);
    let line = first.lines().nth(1).unwrap_or_default();
    assert!(
        line.starts_with("<?plannercraft-style ") && line.ends_with("?>"),
        "{line}"
    );
    assert_eq!(line.matches("?>").count(), 1, "{line}");
    let (opened, diags) = reopen(first.as_bytes());
    assert!(
        diags.iter().all(|d| d.code.code() != "style.ignored"),
        "{diags:?}"
    );
    assert_eq!(style::doc_style(opened.project()), style::doc_style(s.project()));
    assert_eq!(xml(&opened), first);
}
