//! Mise en forme du texte des lignes (finitions P2, ADR-0022 D3, H-UI-126) :
//! `task.set_text_style` (Patch : absent inchangé, `null` retiré), une
//! entrée d'annulation par lot, stockage dans le style du document
//! (`<?plannercraft-style?>` écrit seulement si choisi, fichier sans style
//! écrit octet pour octet comme avant), `style.get`, UID disparus retirés à
//! l'ouverture, modèles qui gardent la mise en forme du projet.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use plannercraft_automation::command::style;
use plannercraft_automation::command::text_style::{TaskSetTextStyle, task_text_of, would_change};
use plannercraft_automation::command::{Patch, TaskRef};
use plannercraft_automation::error::codes;
use plannercraft_automation::session::DocOrigin;
use plannercraft_automation::text::Language;
use plannercraft_automation::{AnchorSpec, ApplyMode, Batch, CallEnv, NewProject, Session, call_with};
use plannercraft_core::TaskUid;
use plannercraft_mspdi::style_pi;
use plannercraft_mspdi::write::WriteOptions;
use plannercraft_render::style::{Rgb, TaskText, Variant, task_text_look, template};
use serde_json::{Value, json};

fn now() -> plannercraft_core::Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn env() -> CallEnv {
    CallEnv::new(now(), Language::Fr)
}

/// Projet de trois tâches (UID 1 à 3).
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
            { "op": "task.add", "name": "Murs", "duration": { "text": "10j" } },
            { "op": "task.add", "name": "Toiture", "duration": { "text": "3j" } }
        ]
    }))
    .unwrap();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    s
}

fn call(s: &mut Session, id: &str, params: Value) -> Value {
    call_with(s, &env(), id, params).unwrap_or_else(|e| panic!("{id} : {e:?}"))
}

fn xml(s: &Session) -> String {
    String::from_utf8(s.save_bytes(&WriteOptions::default()).xml).unwrap()
}

fn reopen(bytes: &[u8]) -> Session {
    let origin = DocOrigin {
        name: "p.xml".to_owned(),
        path: None,
    };
    Session::open_bytes(bytes, origin, now(), Language::Fr).unwrap().0
}

fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

#[test]
fn h_ui_126_set_text_style_patch_and_undo() {
    let mut s = session();
    let before = xml(&s);
    assert!(!before.contains("plannercraft-style"));
    let entries = s.history().entries.len();
    call(
        &mut s,
        "task.set_text_style",
        json!({ "tasks": [1, 2], "bold": true, "color": "#C00000" }),
    );
    assert_eq!(s.history().entries.len(), entries + 1, "une entrée d'annulation");
    let t1 = task_text_of(s.project(), uid(1));
    assert!(t1.bold && !t1.italic);
    assert_eq!(t1.color, Some(Rgb(0xC0, 0, 0)));
    assert_eq!(task_text_of(s.project(), uid(2)), t1);
    assert_eq!(task_text_of(s.project(), uid(3)), TaskText::default());
    assert!(
        xml(&s).contains("<?plannercraft-style "),
        "style écrit dès qu'il est choisi"
    );
    // Absent : inchangé ; `null` : retiré.
    call(
        &mut s,
        "task.set_text_style",
        json!({ "tasks": [1], "italic": true, "color": null, "fill": "#FFF2CC" }),
    );
    let t1 = task_text_of(s.project(), uid(1));
    assert_eq!(
        t1,
        TaskText {
            bold: true,
            italic: true,
            color: None,
            fill: Some(Rgb(0xFF, 0xF2, 0xCC)),
        }
    );
    // Deux annulations : fichier d'avant, octet pour octet.
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(xml(&s), before);
}

#[test]
fn h_ui_126_unstyled_file_same_bytes_and_styled_roundtrip() {
    // Fichier sans style : relu et écrit octet pour octet.
    let s = session();
    let plain = xml(&s);
    let opened = reopen(plain.as_bytes());
    assert_eq!(xml(&opened), plain);
    // Avec mise en forme : relue à l'identique, mêmes octets.
    let mut s = session();
    call(
        &mut s,
        "task.set_text_style",
        json!({ "tasks": [2], "bold": true, "italic": true, "color": "#0072B2", "fill": "#FFF2CC" }),
    );
    let styled = xml(&s);
    let opened = reopen(styled.as_bytes());
    assert_eq!(
        task_text_of(opened.project(), uid(2)),
        task_text_of(s.project(), uid(2))
    );
    assert_eq!(xml(&opened), styled);
    // Rien dans le MSPDI hors de l'instruction de style.
    let without = String::from_utf8(
        opened
            .save_bytes(&WriteOptions {
                style: false,
                ..WriteOptions::default()
            })
            .xml,
    )
    .unwrap();
    assert_eq!(without, plain);
}

#[test]
fn h_ui_126_get_style_exposes_task_text() {
    let mut s = session();
    call(
        &mut s,
        "task.set_text_style",
        json!({ "tasks": [2], "bold": true, "fill": "#FFF2CC" }),
    );
    let view = call(&mut s, "style.get", json!({}));
    assert_eq!(view["source"], "document");
    assert_eq!(
        view["resolved"]["task_text"]["2"],
        json!({ "bold": true, "fill": "#FFF2CC" })
    );
    assert_eq!(view["over"]["task_text"]["2"]["bold"], true);
}

#[test]
fn h_ui_126_refusals_leave_session_intact() {
    let mut s = session();
    let before = s.project().clone();
    for (params, code) in [
        (json!({ "tasks": [] , "bold": true }), codes::PARAM_INVALID),
        (json!({ "tasks": [1] }), codes::PARAM_INVALID),
        (json!({ "tasks": [1], "color": "rouge" }), codes::PARAM_INVALID),
        (json!({ "tasks": [1], "fill": "#12345" }), codes::PARAM_INVALID),
        (json!({ "tasks": [99], "bold": true }), codes::TASK_NOT_FOUND),
        (json!({ "tasks": [1], "underline": true }), codes::PARAM_INVALID),
    ] {
        let err = call_with(&mut s, &env(), "task.set_text_style", params.clone()).unwrap_err();
        assert_eq!(err.code, code, "{params}");
        assert_eq!(s.project(), &before, "{params}");
    }
}

#[test]
fn h_ui_126_would_change_detects_no_op() {
    let mut s = session();
    let bold = |tasks: Vec<u32>, on: bool| TaskSetTextStyle {
        tasks: tasks.into_iter().map(|n| TaskRef::Uid(uid(n))).collect(),
        bold: Patch::Set(on),
        ..TaskSetTextStyle::default()
    };
    assert!(would_change(s.project(), &bold(vec![1], true)));
    assert!(
        !would_change(s.project(), &bold(vec![1], false)),
        "déjà sans gras"
    );
    call(
        &mut s,
        "task.set_text_style",
        json!({ "tasks": [1], "bold": true }),
    );
    assert!(!would_change(s.project(), &bold(vec![1], true)), "déjà en gras");
    assert!(would_change(s.project(), &bold(vec![1, 2], true)));
    assert!(!would_change(
        s.project(),
        &TaskSetTextStyle::clear(vec![TaskRef::Uid(uid(3))])
    ));
    // Pinceau : même mise en forme exactement.
    let t = task_text_of(s.project(), uid(1));
    assert!(would_change(
        s.project(),
        &TaskSetTextStyle::exactly(vec![TaskRef::Uid(uid(2))], &t)
    ));
    assert!(!would_change(
        s.project(),
        &TaskSetTextStyle::exactly(vec![TaskRef::Uid(uid(1))], &t)
    ));
}

#[test]
fn h_ui_126_templates_keep_task_text() {
    let mut s = session();
    call(
        &mut s,
        "task.set_text_style",
        json!({ "tasks": [1], "bold": true }),
    );
    // Changer de modèle garde la mise en forme du projet.
    call(&mut s, "style.apply", json!({ "template": "sobre" }));
    assert!(task_text_of(s.project(), uid(1)).bold);
    // « Revenir au modèle » aussi.
    call(&mut s, "style.reset_to_template", json!({}));
    assert!(task_text_of(s.project(), uid(1)).bold);
    // Un modèle personnel ne la porte pas.
    let named = call(&mut s, "style.save_as_template", json!({ "name": "Mien" }));
    assert!(named["style"]["over"].get("task_text").is_none(), "{named}");
    // Tout effacé : la section disparaît du style.
    call(
        &mut s,
        "task.set_text_style",
        json!({ "tasks": [1], "bold": null, "italic": null, "color": null, "fill": null }),
    );
    let doc = style::doc_style(s.project()).unwrap();
    assert!(doc.over.task_text.is_none(), "{doc:?}");
}

#[test]
fn h_ui_126_deleted_task_dropped_on_open() {
    let mut s = session();
    call(
        &mut s,
        "task.set_text_style",
        json!({ "tasks": [3], "italic": true }),
    );
    call(&mut s, "task.delete", json!({ "uid": 3 }));
    let opened = reopen(xml(&s).as_bytes());
    let doc = style::doc_style(opened.project()).unwrap();
    assert!(doc.over.task_text.is_none(), "UID disparu retiré : {doc:?}");
    assert!(style_pi::entry(&opened.project().extras).is_some());
}

#[test]
fn h_ui_126_dark_theme_keeps_contrasted_color_else_lighter() {
    let st = template(plannercraft_render::style::TemplateId::Classique);
    let dark_bg = st.tokens(Variant::Dark).bg;
    let min = plannercraft_render::style::min_text_contrast(&st);
    // Bleu foncé illisible sur le fond sombre : variante claire, même teinte.
    let navy = Rgb(0x1F, 0x3A, 0x93);
    let look = task_text_look(
        &st,
        Variant::Dark,
        &TaskText {
            color: Some(navy),
            ..TaskText::default()
        },
    );
    let shown = look.color.unwrap();
    assert!(plannercraft_render::style::contrast_ratio(shown, dark_bg) >= min);
    assert!(
        shown.2 >= shown.0 && shown.2 >= shown.1,
        "teinte bleue gardée : {shown:?}"
    );
    // Couleur déjà lisible : gardée telle quelle.
    let light = Rgb(0xF0, 0xE4, 0x42);
    let look = task_text_look(
        &st,
        Variant::Dark,
        &TaskText {
            color: Some(light),
            ..TaskText::default()
        },
    );
    assert_eq!(look.color, Some(light));
    // Clair : couleur telle que choisie, même peu contrastée.
    let look = task_text_look(
        &st,
        Variant::Light,
        &TaskText {
            color: Some(light),
            fill: Some(navy),
            ..TaskText::default()
        },
    );
    assert_eq!((look.color, look.fill), (Some(light), Some(navy)));
    // Sombre : surlignage ramené vers le fond, textes lisibles dessus.
    let yellow = Rgb(0xFF, 0xF2, 0xCC);
    let look = task_text_look(
        &st,
        Variant::Dark,
        &TaskText {
            fill: Some(yellow),
            ..TaskText::default()
        },
    );
    let fill = look.fill.unwrap();
    assert!(plannercraft_render::style::contrast_ratio(st.tokens(Variant::Dark).text, fill) >= min);
}
