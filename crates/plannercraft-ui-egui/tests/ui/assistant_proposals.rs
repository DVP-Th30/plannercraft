//! Propositions de l'assistant (ADR-0024, lot A2, H-UI-149 à H-UI-154) et
//! banc d'essai rejoué (lot A3, partie 1) : réponses SSE écrites à la main,
//! sans réseau, sur le vrai moteur.
//!
//! Banc : trois scénarios (petit planning, décalage d'un lot, correction
//! après un refus du registre), chacun vérifié par son avant/après
//! ([`DiffView`] : dates du moteur, comparées à une application
//! indépendante du même lot), l'acceptation (une seule entrée
//! d'annulation, origine `Agent`), le refus (rien d'appliqué) et l'absence
//! de date dans ce que le modèle propose.

use std::collections::BTreeMap;
use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_assistant::http::HttpEvent;
use plannercraft_assistant::proposal::{PROPOSE_TOOL, ProposalStatus};
use plannercraft_assistant::testing::{ScriptedTransport, sse_response, text_chunks, tool_call_chunks};
use plannercraft_automation::diff::RowChange;
use plannercraft_automation::{ApplyMode, Batch, EditSource};
use plannercraft_core::{Moment, TaskUid};
use plannercraft_ui_egui::App;
use plannercraft_ui_egui::action::Action;
use plannercraft_ui_egui::approvals::ProposalOp;
use plannercraft_ui_egui::assistant::{AssistantOp, AssistantServices};
use plannercraft_ui_egui::i18n::TextId;
use plannercraft_ui_egui::view::side_panel::{AiOutcome, SideOp, SideTab};
use serde_json::{Value, json};

use crate::assistant::{ask, assistant_harness, body, enable_with_key, fr_args, settle};
use crate::fr;

fn propose(id: &str, args: &Value) -> Vec<HttpEvent> {
    sse_response(&tool_call_chunks(id, PROPOSE_TOOL, &args.to_string()))
}

fn read(id: &str, tool: &str) -> Vec<HttpEvent> {
    sse_response(&tool_call_chunks(id, tool, "{}"))
}

fn say(text: &str) -> Vec<HttpEvent> {
    sse_response(&text_chunks(text, 0.002))
}

/// Exemple ouvert, assistant activé, réponses `responses`.
fn ready(responses: Vec<Vec<HttpEvent>>) -> (Harness<'static, App>, Arc<ScriptedTransport>) {
    let transport = Arc::new(ScriptedTransport::new(responses));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    (harness, transport)
}

fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

fn task_name(app: &App, uid: u32) -> String {
    app.document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .iter()
        .find(|t| t.uid() == TaskUid::new(uid))
        .map(|t| t.name.clone())
        .unwrap_or_default()
}

fn card_status(app: &App) -> Option<ProposalStatus> {
    app.assistant()
        .conversation
        .turns()
        .iter()
        .flat_map(|t| &t.tools)
        .filter_map(|s| s.proposal.as_ref())
        .next_back()
        .map(|c| c.status.clone())
}

fn rename(label: &str, uid: u32, name: &str) -> Value {
    json!({ "label": label, "ops": [{ "op": "task.update", "uid": uid, "name": name }] })
}

/// Dernier message envoyé au modèle (texte).
fn last_user_text(transport: &ScriptedTransport) -> String {
    let n = transport.requests().len();
    let req = body(transport, n.saturating_sub(1));
    req["messages"].as_array().unwrap().last().unwrap()["content"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// H-UI-149 : la proposition devient une carte dans la conversation et une
/// écriture en attente du panneau Opérations IA (origine « Assistant »,
/// diff calculé une fois) ; rien n'est appliqué ; « Voir » ouvre l'onglet.
#[test]
fn h_ui_149_proposal_card_and_panel() {
    let (mut harness, _t) = ready(vec![
        propose("p1", &rename("Renommer les plans", 3, "Plans définitifs")),
        say("Je propose de renommer la tâche 3."),
    ]);
    let generation = harness.state().document.as_ref().unwrap().generation();
    ask(&mut harness, "Renomme la tâche 3");
    settle(&mut harness);
    harness.run();
    // Carte et ligne d'outil.
    let card = fr_args(TextId::AssistantProposalTitle, &[("name", "Renommer les plans")]);
    assert!(harness.query_all_by_label(&card).count() >= 1);
    harness.get_by_label(&fr_args(
        TextId::AssistantToolPropose,
        &[("what", "Renommer les plans")],
    ));
    assert!(harness.query_by_label_contains(PROPOSE_TOOL).is_none());
    harness.get_by_label(&fr(TextId::AssistantProposalReason));
    // File : une proposition de l'assistant, avec son diff, rien d'appliqué.
    let approvals = harness.state().approvals();
    assert_eq!(approvals.pending().len(), 1);
    let pending = &approvals.pending()[0];
    assert!(pending.is_assistant());
    assert_eq!(pending.label.as_deref(), Some("Renommer les plans"));
    assert_eq!(pending.tool, "batch.apply");
    assert!(pending.diff.is_some());
    assert_eq!(approvals.diff_builds(), 1);
    let doc = harness.state().document.as_ref().unwrap();
    assert_eq!(doc.generation(), generation);
    assert_eq!(task_name(harness.state(), 3), "Plans de l'architecte");
    // Le bandeau du haut se retire : la carte porte la décision.
    let title = fr_args(TextId::ApprovalTitleAssistant, &[("name", "Renommer les plans")]);
    assert!(harness.query_by_label_contains(&title).is_none());
    // Voir : onglet Opérations IA, origine « Assistant ».
    harness
        .query_all_by_label(&fr(TextId::AssistantProposalView))
        .next()
        .unwrap()
        .click();
    harness.run();
    assert_eq!(harness.state().side_panel().tab, SideTab::AiOperations);
    harness.get_by_label_contains(&title);
    assert!(harness.query_by_label_contains("L'agent propose").is_none());
}

/// H-UI-150 : Accepter depuis la carte applique le lot en une seule entrée
/// d'annulation, origine `Agent` ; la carte passe à « acceptée » ;
/// l'historique du panneau dit « Assistant » ; la décision est dite au
/// modèle au message suivant.
#[test]
fn h_ui_150_accept_single_undo_and_told() {
    let (mut harness, transport) = ready(vec![
        propose(
            "p1",
            &json!({ "label": "Deux renommages", "ops": [
            { "op": "task.update", "uid": 3, "name": "Plans définitifs" },
            { "op": "task.update", "uid": 4, "name": "Dépôt du permis (PC)" }
        ] }),
        ),
        say("Proposé."),
        say("Bien noté."),
    ]);
    let entries = history_len(harness.state());
    ask(&mut harness, "Renomme 3 et 4");
    settle(&mut harness);
    harness.run();
    harness
        .query_all_by_label(&fr(TextId::AssistantProposalAccept))
        .next()
        .unwrap()
        .click();
    harness.run();
    let app = harness.state();
    assert_eq!(task_name(app, 3), "Plans définitifs");
    assert_eq!(task_name(app, 4), "Dépôt du permis (PC)");
    let history = app.document.as_ref().unwrap().session.history();
    assert_eq!(
        history.entries.len(),
        entries.saturating_add(1),
        "une seule entrée"
    );
    assert_eq!(history.entries.last().unwrap().source, EditSource::Agent);
    assert_eq!(card_status(app), Some(ProposalStatus::Accepted));
    assert!(app.approvals().pending().is_empty());
    let entry = app.approvals().history().back().unwrap();
    assert_eq!(entry.outcome, AiOutcome::Accepted);
    assert_eq!(entry.label.as_deref(), Some("Deux renommages"));
    harness.get_by_label(&fr(TextId::AssistantProposalAccepted));
    // Une annulation défait tout le lot.
    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(task_name(harness.state(), 3), "Plans de l'architecte");
    assert_eq!(task_name(harness.state(), 4), "Dépôt du permis de construire");
    assert_eq!(history_len(harness.state()), entries);
    // Décision dite avec la question suivante.
    ask(&mut harness, "Merci");
    settle(&mut harness);
    let text = last_user_text(&transport);
    assert!(
        text.starts_with("[Décision de l'utilisateur sur la proposition « Deux renommages » : acceptée"),
        "{text}"
    );
    // Panneau : origine « Assistant » dans l'historique.
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::AiOperations)));
    harness.run();
    harness.get_by_label_contains("Assistant, « Deux renommages »");
}

/// H-UI-151 : Refuser avec un motif : rien d'appliqué, carte « refusée »,
/// motif dit au modèle.
#[test]
fn h_ui_151_reject_with_reason() {
    let (mut harness, transport) = ready(vec![
        propose("p1", &rename("Renommer", 3, "X")),
        say("Proposé."),
        say("D'accord, je garde le nom."),
    ]);
    let entries = history_len(harness.state());
    let generation = harness.state().document.as_ref().unwrap().generation();
    ask(&mut harness, "Renomme 3");
    settle(&mut harness);
    harness.run();
    harness.get_by_label(&fr(TextId::AssistantProposalReason)).focus();
    harness.run();
    harness
        .get_by_label(&fr(TextId::AssistantProposalReason))
        .type_text("le nom actuel est bon");
    harness.run();
    harness
        .query_all_by_label(&fr(TextId::AssistantProposalReject))
        .next()
        .unwrap()
        .click();
    harness.run();
    let app = harness.state();
    assert_eq!(task_name(app, 3), "Plans de l'architecte");
    assert_eq!(history_len(app), entries);
    assert_eq!(app.document.as_ref().unwrap().generation(), generation);
    assert_eq!(
        card_status(app),
        Some(ProposalStatus::Rejected {
            reason: Some("le nom actuel est bon".to_owned())
        })
    );
    assert_eq!(
        app.approvals().history().back().unwrap().outcome,
        AiOutcome::Rejected
    );
    ask(&mut harness, "Ok");
    settle(&mut harness);
    assert!(last_user_text(&transport).contains("motif de l'utilisateur : le nom actuel est bon"));
}

/// H-UI-152 : mêmes gardes que pour un agent. Planning modifié pendant
/// l'attente ⇒ Accepter donne un nouvel aperçu, rien d'appliqué ; document
/// remplacé ⇒ périmée ; nouvelle conversation ⇒ proposition retirée.
#[test]
fn h_ui_152_guards_stale_preview_document_conversation() {
    let (mut harness, _t) = ready(vec![
        propose("p1", &rename("Renommer 3", 3, "Plans v2")),
        say("Proposé."),
        propose("p2", &rename("Renommer 5", 5, "Instruction")),
        say("Proposé."),
        propose("p3", &rename("Renommer 6", 6, "Consultation")),
        say("Proposé."),
    ]);
    ask(&mut harness, "Renomme 3");
    settle(&mut harness);
    // L'utilisateur modifie le planning pendant l'attente.
    let edit: Batch = serde_json::from_value(json!({
        "format_version": 1, "ops": [{ "op": "task.update", "uid": 4, "name": "Dépôt PC" }]
    }))
    .unwrap();
    harness
        .state_mut()
        .execute(&edit, plannercraft_ui_egui::EditOrigin::User)
        .unwrap();
    harness.run();
    harness
        .state_mut()
        .dispatch(Action::Proposal(ProposalOp::Accept(1)));
    harness.run();
    assert_eq!(
        task_name(harness.state(), 3),
        "Plans de l'architecte",
        "aperçu périmé jamais appliqué"
    );
    assert_eq!(harness.state().approvals().pending().len(), 1);
    assert_eq!(harness.state().approvals().diff_builds(), 2, "nouvel aperçu");
    assert_eq!(card_status(harness.state()), Some(ProposalStatus::Pending));
    harness
        .state_mut()
        .dispatch(Action::Proposal(ProposalOp::Accept(1)));
    harness.run();
    assert_eq!(task_name(harness.state(), 3), "Plans v2");
    // Nouvelle conversation : la proposition en attente est retirée.
    ask(&mut harness, "Renomme 5");
    settle(&mut harness);
    assert_eq!(harness.state().approvals().pending().len(), 1);
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::Clear));
    harness.run();
    assert!(harness.state().approvals().pending().is_empty());
    assert_eq!(
        harness.state().approvals().history().back().unwrap().outcome,
        AiOutcome::Expired
    );
    assert_eq!(task_name(harness.state(), 5), "Instruction du permis");
    // Document remplacé : périmée, rien d'appliqué.
    ask(&mut harness, "Renomme 6");
    settle(&mut harness);
    assert_eq!(harness.state().approvals().pending().len(), 1);
    harness.state_mut().open_demo();
    harness.run();
    assert!(harness.state().approvals().pending().is_empty());
    assert_eq!(
        harness.state().approvals().history().back().unwrap().outcome,
        AiOutcome::Stale
    );
    assert_eq!(task_name(harness.state(), 6), "Consultation des entreprises");
}

/// H-UI-153 (amendé le 2026-11-07) : une proposition de l'assistant
/// n'expire pas ; après plus de 120 s elle attend toujours et s'accepte.
#[test]
fn h_ui_153_assistant_proposal_never_expires() {
    let transport = Arc::new(ScriptedTransport::new(vec![
        propose("p1", &rename("Renommer", 3, "X")),
        say("Proposé."),
    ]));
    let mut app = crate::new_app();
    app.set_assistant_services(AssistantServices::with_transport(Arc::clone(&transport) as _));
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_step_dt(20.0)
        .with_max_steps(16)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.state_mut().open_demo();
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::Assistant)));
    harness.run();
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::Enable));
    "sk-or-v1-test".clone_into(&mut harness.state_mut().assistant_mut().key_draft);
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::SaveKey));
    harness.run();
    "Renomme 3".clone_into(&mut harness.state_mut().assistant_mut().draft);
    harness.state_mut().dispatch(Action::Assistant(AssistantOp::Send));
    for _ in 0..6 {
        harness.run();
    }
    let start = harness.ctx.input(|i| i.time);
    for _ in 0..12 {
        harness.step();
    }
    let waited = harness.ctx.input(|i| i.time) - start;
    assert!(waited > 200.0, "{waited} s");
    // Toujours en attente : aucun agent n'attend, pas de délai.
    assert_eq!(harness.state().approvals().pending().len(), 1);
    assert_eq!(card_status(harness.state()), Some(ProposalStatus::Pending));
    assert_eq!(task_name(harness.state(), 3), "Plans de l'architecte");
    harness
        .state_mut()
        .dispatch(Action::Proposal(ProposalOp::Accept(1)));
    harness.run();
    assert_eq!(card_status(harness.state()), Some(ProposalStatus::Accepted));
    assert_eq!(task_name(harness.state(), 3), "X");
}

/// H-UI-154 : « Montrer la réflexion » (menu « … ») : coupé par défaut,
/// mémorisé dans l'état d'interface, budget minimal envoyé quand coché.
#[test]
fn h_ui_154_show_reasoning_option() {
    let (mut harness, transport) = ready(vec![say("Oui."), say("Oui.")]);
    assert!(!harness.state().state.assistant.show_reasoning);
    ask(&mut harness, "Q1");
    settle(&mut harness);
    assert!(body(&transport, 0).get("reasoning").is_none());
    crate::click(&mut harness, &fr(TextId::AssistantMenu));
    harness.run();
    crate::click(&mut harness, &fr(TextId::AssistantReasoningToggle));
    harness.run();
    assert!(harness.state().state.assistant.show_reasoning);
    // Menu encore ouvert : Échap le ferme ; menu déjà refermé par le clic,
    // Échap fermerait le volet (lot A5, H-UI-162).
    if egui::Popup::is_any_open(&harness.ctx) {
        harness.key_press(egui::Key::Escape);
        harness.run();
    }
    assert!(harness.state().side_panel().open);
    ask(&mut harness, "Q2");
    settle(&mut harness);
    let req = body(&transport, 1);
    assert_eq!(req["reasoning"], json!({ "max_tokens": 1024 }));
    assert_eq!(req["max_tokens"], 9216);
    // Mémorisé, mais jamais rendu aux agents MCP (`app.ui_state`).
    let json = harness.state().state.to_json().unwrap();
    assert!(json.contains("\"show_reasoning\":true"));
    assert!(
        harness.state().ui_state_json()["ui_state"]
            .get("assistant")
            .is_none()
    );
}

// --- Banc d'essai rejoué (lot A3, partie 1) ---------------------------------

/// Arguments sans aucune date ISO (le modèle ne calcule jamais une date).
fn assert_no_date(args: &Value) {
    let text = args.to_string();
    let bytes = text.as_bytes();
    let dated = bytes.windows(10).any(|w| {
        w.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
    });
    assert!(!dated, "date dans la proposition : {text}");
}

/// Dates (début, fin) par nom d'une application **indépendante** du même
/// lot sur une copie de la session (moteur seul).
fn engine_dates(app: &App, args: &Value) -> BTreeMap<String, (Moment, Moment)> {
    let mut copy = app.document.as_ref().unwrap().session.clone();
    let batch: Batch = serde_json::from_value(json!({
        "format_version": 1, "ops": args["ops"], "label": args["label"]
    }))
    .unwrap();
    copy.apply(&batch, ApplyMode::Commit).unwrap();
    copy.project()
        .tasks
        .iter()
        .filter_map(|t| {
            let s = copy.schedule().scheduled(t.uid())?;
            Some((t.name.clone(), (s.start, s.finish)))
        })
        .collect()
}

/// Avant/après de la proposition en tête : chaque date d'après est celle
/// du moteur ; rend les lignes (nom, changement).
fn check_diff(app: &App, args: &Value) -> Vec<(String, RowChange)> {
    let expected = engine_dates(app, args);
    let pending = &app.approvals().pending()[0];
    let diff = pending.diff.as_ref().unwrap();
    assert_ne!(diff.rows.len(), 0);
    for row in &diff.rows {
        let (start, finish) = expected[&row.name];
        assert_eq!(row.start.1, Some(start), "{}", row.name);
        assert_eq!(row.finish.1, Some(finish), "{}", row.name);
    }
    diff.rows.iter().map(|r| (r.name.clone(), r.change)).collect()
}

/// Mène un scénario jusqu'à la proposition en attente.
fn scenario(
    responses: Vec<Vec<HttpEvent>>,
    empty: bool,
    question: &str,
) -> (Harness<'static, App>, Arc<ScriptedTransport>) {
    let (mut harness, transport) = ready(responses);
    if empty {
        harness.state_mut().new_project();
        harness.run();
    }
    ask(&mut harness, question);
    settle(&mut harness);
    harness.run();
    (harness, transport)
}

/// Décide la proposition en attente : accepter (une entrée d'annulation,
/// puis Annuler rend l'état d'avant) ou refuser (rien d'appliqué).
fn decide(harness: &mut Harness<'static, App>, accept: bool) {
    let app = harness.state();
    let before_tasks = app.document.as_ref().unwrap().project().tasks.len();
    let before_entries = history_len(app);
    let before_generation = app.document.as_ref().unwrap().generation();
    let id = app.approvals().pending()[0].id;
    if accept {
        harness
            .state_mut()
            .dispatch(Action::Proposal(ProposalOp::Accept(id)));
        harness.run();
        assert_eq!(
            history_len(harness.state()),
            before_entries.saturating_add(1),
            "une seule annulation"
        );
        assert_eq!(card_status(harness.state()), Some(ProposalStatus::Accepted));
        harness.state_mut().dispatch(Action::Undo);
        harness.run();
        assert_eq!(history_len(harness.state()), before_entries);
        assert_eq!(
            harness.state().document.as_ref().unwrap().project().tasks.len(),
            before_tasks
        );
    } else {
        harness
            .state_mut()
            .dispatch(Action::Proposal(ProposalOp::Reject(id)));
        harness.run();
        let doc = harness.state().document.as_ref().unwrap();
        assert_eq!(doc.generation(), before_generation, "rien d'appliqué");
        assert_eq!(history_len(harness.state()), before_entries);
        assert!(matches!(
            card_status(harness.state()),
            Some(ProposalStatus::Rejected { .. })
        ));
    }
}

/// Scénario 1 : petit planning (5 tâches, 2 liens, 1 jalon) sur un projet
/// vide ; le modèle lit le résumé puis propose des durées et des liens.
fn small_plan() -> (Vec<Vec<HttpEvent>>, Value) {
    let args = json!({ "label": "Petit planning", "ops": [
        { "op": "task.add", "name": "Études", "as": "$a", "duration": { "text": "10d" } },
        { "op": "task.add", "name": "Permis obtenu", "as": "$m", "milestone": true, "duration": { "text": "0d" } },
        { "op": "task.add", "name": "Terrassement", "as": "$b", "duration": { "text": "5d" } },
        { "op": "task.add", "name": "Fondations", "as": "$c", "duration": { "text": "8d" } },
        { "op": "task.add", "name": "Gros œuvre", "as": "$d", "duration": { "text": "15d" } },
        { "op": "link.add", "pred": "$a", "succ": "$m" },
        { "op": "link.add", "pred": "$m", "succ": "$b" }
    ] });
    let responses = vec![
        read("r1", "project_summary"),
        propose("p1", &args),
        say("Je propose un planning de 5 tâches, dont un jalon, en attente de votre validation."),
    ];
    (responses, args)
}

#[test]
fn bench_small_plan_accept_and_reject() {
    for accept in [true, false] {
        let (responses, args) = small_plan();
        assert_no_date(&args);
        let (mut harness, transport) = scenario(responses, true, "Crée un petit planning de maison");
        assert_eq!(transport.requests().len(), 3);
        let rows = check_diff(harness.state(), &args);
        assert_eq!(
            rows.iter().filter(|(_, c)| *c == RowChange::Added).count(),
            5,
            "{rows:?}"
        );
        assert!(
            harness
                .state()
                .document
                .as_ref()
                .unwrap()
                .project()
                .tasks
                .is_empty()
        );
        decide(&mut harness, accept);
    }
}

/// Scénario 2 : décaler le second œuvre de deux semaines (exemple) ; le
/// modèle lit les tâches puis propose `task.shift` sur la récapitulative.
#[test]
fn bench_shift_phase_accept_and_reject() {
    for accept in [true, false] {
        let args = json!({ "label": "Décaler le second œuvre de 2 semaines", "ops": [
            { "op": "task.shift", "tasks": [29], "by": { "text": "10d" } }
        ] });
        assert_no_date(&args);
        let responses = vec![
            read("r1", "list_tasks"),
            propose("p1", &args),
            say("Le second œuvre est décalé de 10 jours ouvrés, en attente de votre validation."),
        ];
        let (mut harness, _t) = scenario(responses, false, "Décale le second œuvre de deux semaines");
        let rows = check_diff(harness.state(), &args);
        for name in ["Électricité", "Plomberie", "Chape"] {
            assert!(
                rows.iter().any(|(n, c)| n == name && *c == RowChange::Changed),
                "{name} : {rows:?}"
            );
        }
        let pending = &harness.state().approvals().pending()[0];
        let row = pending
            .diff
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .find(|r| r.name == "Électricité")
            .unwrap();
        assert!(row.start.1 > row.start.0, "décalé plus tard par le moteur");
        decide(&mut harness, accept);
    }
}

/// Scénario 3 : le registre refuse le premier lot (boucle), le modèle le
/// lit et corrige ; la proposition corrigée est soumise.
#[test]
fn bench_fix_after_registry_refusal() {
    for accept in [true, false] {
        let wrong = json!({ "label": "Lier la réception aux études", "ops": [
            { "op": "link.add", "pred": 49, "succ": 2 }
        ] });
        let fixed = json!({ "label": "Lier le conduit à l'électricité", "ops": [
            { "op": "link.add", "pred": 20, "succ": 30, "lag": { "text": "60d" } }
        ] });
        assert_no_date(&wrong);
        assert_no_date(&fixed);
        let responses = vec![
            propose("p1", &wrong),
            propose("p2", &fixed),
            say("Le premier lien créait une boucle ; je propose plutôt ce lien, à valider."),
        ];
        let (mut harness, transport) = scenario(responses, false, "Ajoute un lien");
        let refused: Value = {
            let req = body(&transport, 1);
            let msg = req["messages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|m| m["tool_call_id"] == "p1")
                .unwrap()
                .clone();
            serde_json::from_str(msg["content"].as_str().unwrap()).unwrap()
        };
        assert_eq!(
            refused["error"]["diagnostics"][0]["code"], "link.cycle",
            "{refused}"
        );
        assert!(refused["hint"].as_str().unwrap().contains("Corrige"));
        let turn = &harness.state().assistant().conversation.turns()[0];
        assert!(turn.tools[0].error.is_some() && turn.tools[0].proposal.is_none());
        assert!(turn.tools[1].error.is_none(), "{:?}", turn.tools[1].error);
        assert_eq!(
            turn.tools[1].proposal.as_ref().unwrap().label,
            "Lier le conduit à l'électricité"
        );
        harness.get_by_label(&fr(TextId::AssistantToolProposeFailed));
        assert_eq!(harness.state().approvals().pending().len(), 1);
        check_diff(harness.state(), &fixed);
        decide(&mut harness, accept);
    }
}

// --- Captures ------------------------------------------------------------

/// Conversation d'une proposition en attente (exemple, clair).
fn proposal_capture() -> Harness<'static, App> {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let args = json!({ "label": "Décaler le second œuvre de 2 semaines", "ops": [
        { "op": "task.shift", "tasks": [29], "by": { "text": "10d" } }
    ] });
    let transport = Arc::new(ScriptedTransport::new(vec![
        read("r1", "list_tasks"),
        propose("p1", &args),
        say(
            "Je propose de **décaler le second œuvre** (tâches 30 à 35) de **10 jours ouvrés**. Le moteur \
             recalcule les dates des tâches qui suivent.\n\nLa proposition attend votre validation : rien \
             n'est appliqué avant.",
        ),
    ]));
    let mut harness = crate::assistant::chat_capture_harness(transport, LanguagePref::Fr, ThemePref::Light);
    crate::assistant::send_now(&mut harness, "Décale le second œuvre de deux semaines");
    for _ in 0..12 {
        harness.run();
    }
    assert!(!harness.state().assistant().conversation.busy());
    assert_eq!(harness.state().approvals().pending().len(), 1);
    harness
}

/// Capture : carte de la proposition dans la conversation.
#[test]
fn snapshot_assistant_proposal_fr_light() {
    let mut harness = proposal_capture();
    crate::common::snapshot(&mut harness, "assistant_proposal_fr_light");
}

/// Capture : onglet Opérations IA ouvert par « Voir », sur l'avant/après
/// de la même proposition (origine « Assistant »).
#[test]
fn snapshot_assistant_proposal_panel_fr_light() {
    let mut harness = proposal_capture();
    harness
        .state_mut()
        .dispatch(Action::Proposal(ProposalOp::View(1)));
    for _ in 0..4 {
        harness.run();
    }
    crate::common::snapshot(&mut harness, "assistant_proposal_panel_fr_light");
}

/// Scénario 4 : « fixe la livraison au 15 mars 2027 ». Le modèle tente
/// d'abord une date qu'il a déduite (refusée, motif rendu, compte comme une
/// correction), puis pose l'échéance à la date écrite par l'utilisateur ;
/// aucune autre date dans les arguments.
#[test]
fn bench_deadline_cited_by_user() {
    for accept in [true, false] {
        let inferred = json!({ "label": "Échéance de livraison", "ops": [
            { "op": "task.set_deadline", "uid": 49, "deadline": "2027-03-12T17:00" }
        ] });
        let cited = json!({ "label": "Échéance de livraison au 15 mars 2027", "ops": [
            { "op": "task.set_deadline", "uid": 49, "deadline": "2027-03-15T17:00" }
        ] });
        let responses = vec![
            propose("p1", &inferred),
            propose("p2", &cited),
            say("L'échéance de la remise des clés est proposée au 15 mars 2027, à valider."),
        ];
        let (mut harness, transport) = scenario(responses, false, "Fixe la livraison au 15 mars 2027");
        // Date déduite : refusée avant tout appel, motif rendu au modèle.
        let req = body(&transport, 1);
        let msg = req["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["tool_call_id"] == "p1")
            .unwrap()
            .clone();
        let refused: Value = serde_json::from_str(msg["content"].as_str().unwrap()).unwrap();
        assert!(
            refused["error"]["message"]
                .as_str()
                .unwrap()
                .contains("ne figure pas dans la demande"),
            "{refused}"
        );
        assert!(
            refused["hint"].as_str().unwrap().contains("2 correction"),
            "{refused}"
        );
        // Seule date des arguments acceptés : celle de l'utilisateur.
        let text = cited.to_string();
        assert_eq!(text.matches("2027-").count(), 1);
        let turn = &harness.state().assistant().conversation.turns()[0];
        assert!(turn.tools[0].error.is_some() && turn.tools[0].proposal.is_none());
        assert!(turn.tools[1].proposal.is_some());
        assert_eq!(harness.state().approvals().pending().len(), 1);
        let deadline = |app: &App| {
            app.document
                .as_ref()
                .unwrap()
                .project()
                .tasks
                .iter()
                .find(|t| t.uid() == TaskUid::new(49))
                .unwrap()
                .deadline
        };
        assert_eq!(deadline(harness.state()), None);
        let id = harness.state().approvals().pending()[0].id;
        if accept {
            let entries = history_len(harness.state());
            harness
                .state_mut()
                .dispatch(Action::Proposal(ProposalOp::Accept(id)));
            harness.run();
            assert_eq!(
                deadline(harness.state()),
                Some("2027-03-15T17:00".parse().unwrap())
            );
            assert_eq!(history_len(harness.state()), entries.saturating_add(1));
            harness.state_mut().dispatch(Action::Undo);
            harness.run();
            assert_eq!(deadline(harness.state()), None);
        } else {
            harness
                .state_mut()
                .dispatch(Action::Proposal(ProposalOp::Reject(id)));
            harness.run();
            assert_eq!(deadline(harness.state()), None);
        }
    }
}
