//! Panneau d'opérations IA et « Pourquoi cette date ? » (J8, lot L12,
//! ADR-0016 D10) : H-UI-41 (volet ouvert à chaque demande, table
//! avant/après bornée, Accepter / Refuser sur la tête de file, historique
//! de session, diff calculé une fois), H-UI-58 (explication de la tâche
//! active, pilotes cliquables) ; scénario `relay_j8.jsonl` rejoué avec un
//! vrai `plannercraft mcp`, décisions prises dans le volet.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::Batch;
use plannercraft_automation::command::TaskRef;
use plannercraft_automation::command::params::ExplainParams;
use plannercraft_automation::control::{CallState, RelayCall, RelayKind, Reply};
use plannercraft_automation::diff::RowChange;
use plannercraft_automation::error::codes;
use plannercraft_automation::explain::explain_date;
use plannercraft_core::TaskUid;
use plannercraft_ui_egui::i18n::{Arg, TextId};
use plannercraft_ui_egui::view::side_panel::{AiOutcome, MAX_DIFF_ROWS, MAX_HISTORY, SideOp, SideTab};
use plannercraft_ui_egui::{Action, App, EditOrigin};
use serde_json::{Value, json};

use crate::{fr, harness, open_demo};

/// Identifiant d'appel unique.
fn next_id() -> u64 {
    static N: AtomicU64 = AtomicU64::new(5_000);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Exemple ouvert, file des appels branchée.
fn relay_harness() -> (Harness<'static, App>, SyncSender<RelayCall>) {
    let mut harness = harness();
    let (tx, rx) = sync_channel(128);
    harness.state_mut().control_inbox(rx);
    open_demo(&mut harness);
    (harness, tx)
}

/// Appel relayé en file.
struct Sent {
    id: u64,
    state: Arc<CallState>,
    replies: Receiver<Reply>,
}

impl Sent {
    fn drain(&self) -> Vec<Reply> {
        self.replies.try_iter().collect()
    }
}

fn execute(tx: &SyncSender<RelayCall>, tool: &str, params: Value) -> Sent {
    let id = next_id();
    let (reply, replies) = sync_channel(4);
    let state = Arc::new(CallState::new());
    tx.send(RelayCall {
        id,
        kind: RelayKind::Execute {
            tool: tool.to_owned(),
            params,
        },
        state: Arc::clone(&state),
        reply,
    })
    .unwrap();
    Sent { id, state, replies }
}

fn rename(uid: u32, name: &str) -> Value {
    json!({ "uid": uid, "name": name })
}

fn generation(app: &App) -> u64 {
    app.document.as_ref().unwrap().generation()
}

fn pending_ids(app: &App) -> Vec<u64> {
    app.relay().unwrap().pending().iter().map(|p| p.id).collect()
}

fn outcomes(app: &App) -> Vec<AiOutcome> {
    app.relay()
        .unwrap()
        .history()
        .iter()
        .map(|e| e.outcome.clone())
        .collect()
}

/// H-UI-41 : volet fermé par défaut ; ouvert d'office sur « Opérations
/// IA » à la demande, sans prendre le focus ; il remplace le bandeau (un
/// seul « Accepter ») ; refermé par Affichage → Opérations IA, le bandeau
/// J7 revient.
#[test]
fn h_ui_41_panel_opens_on_request() {
    let (mut harness, tx) = relay_harness();
    let tab = fr(TextId::SideTabWhy);
    assert!(!harness.state().side_panel().open);
    assert!(harness.query_by_label(&tab).is_none(), "volet fermé par défaut");
    let sent = execute(&tx, "task.update", rename(3, "Proposé"));
    harness.run();
    assert_eq!(sent.drain(), [Reply::PendingApproval { id: sent.id }]);
    let side = harness.state().side_panel();
    assert!(side.open);
    assert_eq!(side.tab, SideTab::AiOperations);
    harness.get_by_label(&tab);
    let accept = fr(TextId::ApprovalAccept);
    // Un seul bouton « Accepter » (celui du volet) et jamais le focus.
    assert!(!harness.get_by_label(&accept).is_focused());
    // Affichage → Opérations IA : volet refermé, bandeau J7 de retour.
    crate::click(&mut harness, &fr(TextId::MenuView));
    harness
        .get_by_role_and_label(egui::accesskit::Role::CheckBox, &fr(TextId::MenuAiPanel))
        .click();
    harness.run();
    assert!(!harness.state().side_panel().open);
    harness.key_press_modifiers(Modifiers::NONE, Key::Escape);
    harness.run();
    assert!(harness.query_by_label(&tab).is_none());
    harness.get_by_label(&accept);
    assert_eq!(pending_ids(harness.state()), [sent.id], "rien de décidé");
    // Une nouvelle demande le rouvre.
    let second = execute(&tx, "task.update", rename(4, "Encore"));
    harness.run();
    assert_eq!(second.drain(), [Reply::PendingApproval { id: second.id }]);
    assert!(harness.state().side_panel().open);
}

/// H-UI-41 : ligne « + Nettoyage » d'une tâche ajoutée, tâche décalée,
/// au plus 200 lignes et « n autres tâches modifiées ».
#[test]
fn h_ui_41_diff_rows_and_more() {
    let (mut harness, tx) = relay_harness();
    // Exemple : la tâche ajoutée derrière la dernière tâche du réseau.
    let batch = json!({ "ops": [
        { "op": "task.add", "name": "Nettoyage", "as": "$n", "duration": { "text": "1j" } },
        { "op": "task.update", "uid": 3, "duration": { "text": "15j" } },
    ] });
    let sent = execute(&tx, "batch.apply", batch);
    harness.run();
    assert_eq!(sent.drain(), [Reply::PendingApproval { id: sent.id }]);
    let app = harness.state();
    let diff = app.relay().unwrap().pending()[0].diff.clone().unwrap();
    let added = diff.rows.iter().find(|r| r.change == RowChange::Added).unwrap();
    assert_eq!(added.name, "Nettoyage");
    assert!(
        diff.rows
            .iter()
            .any(|r| r.uid == TaskUid::new(3) && r.change == RowChange::Changed)
    );
    let texts = app.texts();
    let label = texts.get_args(TextId::AiAdded, &[("name", Arg::Str("Nettoyage".into()))]);
    harness.get_by_label(&label);
    assert_eq!(generation(harness.state()), 0, "aperçu seulement");

    // 250 tâches décalées : 200 lignes, « 50 autres tâches modifiées ».
    harness.state_mut().dispatch(Action::Reject(sent.id));
    harness.run();
    let ops: Vec<Value> = (0..250)
        .map(|i| json!({ "op": "task.add", "name": format!("Lot {i}"), "duration": { "text": "1j" } }))
        .collect();
    let batch: Batch = serde_json::from_value(json!({ "format_version": 1, "ops": ops })).unwrap();
    harness.state_mut().execute(&batch, EditOrigin::User).unwrap();
    harness.run();
    let uids: Vec<u32> = harness
        .state()
        .document
        .as_ref()
        .unwrap()
        .project()
        .tasks
        .iter()
        .filter(|t| t.name.starts_with("Lot "))
        .map(|t| plannercraft_core::HasUid::uid(t).get())
        .collect();
    assert_eq!(uids.len(), 250);
    let ops: Vec<Value> = uids
        .iter()
        .map(|uid| json!({ "op": "task.update", "uid": uid, "duration": { "text": "2j" } }))
        .collect();
    let many = execute(&tx, "batch.apply", json!({ "ops": ops }));
    harness.run();
    assert_eq!(many.drain(), [Reply::PendingApproval { id: many.id }]);
    let app = harness.state();
    let diff = app.relay().unwrap().pending()[0].diff.clone().unwrap();
    assert_eq!(diff.rows.len(), MAX_DIFF_ROWS);
    assert_eq!(diff.more, 50);
    let more = app.texts().get_args(TextId::AiMore, &[("count", Arg::Count(50))]);
    harness.get_by_label(&more);
}

/// H-UI-41 : Accepter et Refuser du volet ne visent que la tête de file ;
/// une décision portant l'`id` d'un appel en attente derrière elle est
/// sans effet ; Ctrl/Cmd+Entrée accepte depuis le volet.
#[test]
fn h_ui_41_accept_reject_head_only() {
    let (mut harness, tx) = relay_harness();
    let first = execute(&tx, "task.update", rename(3, "Premier"));
    let second = execute(&tx, "task.update", rename(4, "Second"));
    let third = execute(&tx, "task.update", rename(5, "Troisième"));
    harness.run();
    assert_eq!(pending_ids(harness.state()), [first.id, second.id, third.id]);
    // Décision sur un appel qui n'est pas en tête : rien.
    harness.state_mut().dispatch(Action::Approve(second.id));
    harness.state_mut().dispatch(Action::Reject(third.id));
    harness.run();
    assert_eq!(pending_ids(harness.state()), [first.id, second.id, third.id]);
    assert_eq!(generation(harness.state()), 0);
    // Accepter du volet : la tête seulement.
    crate::click(&mut harness, &fr(TextId::ApprovalAccept));
    assert_eq!(pending_ids(harness.state()), [second.id, third.id]);
    assert_eq!(generation(harness.state()), 1);
    assert_eq!(first.drain().len(), 2, "pending puis ok");
    // Refuser du volet : la nouvelle tête.
    crate::click(&mut harness, &fr(TextId::ApprovalReject));
    assert_eq!(pending_ids(harness.state()), [third.id]);
    assert_eq!(generation(harness.state()), 1);
    let replies = second.drain();
    assert!(
        matches!(replies.last(), Some(Reply::Err { error, .. }) if error.code == codes::CONTROL_REJECTED)
    );
    // Ctrl/Cmd+Entrée, volet ouvert : l'aperçu du troisième date d'avant
    // le premier lot accepté, d'où un nouvel aperçu (C-CTL-07, diff
    // recalculé), puis l'application au second appui.
    let builds = harness.state().relay().unwrap().diff_builds();
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    harness.run();
    assert_eq!(pending_ids(harness.state()), [third.id], "nouvel aperçu");
    assert_eq!(harness.state().relay().unwrap().diff_builds(), builds + 1);
    assert_eq!(generation(harness.state()), 1);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter);
    harness.run();
    assert_eq!(pending_ids(harness.state()).len(), 0);
    assert_eq!(generation(harness.state()), 2);
    assert_eq!(
        outcomes(harness.state()),
        [AiOutcome::Accepted, AiOutcome::Rejected, AiOutcome::Accepted]
    );
}

/// H-UI-41 : historique de session (acceptée, refusée, expirée, périmée),
/// affiché le plus récent d'abord ; lien d'annulation du dernier lot de
/// l'agent ; 50 entrées au plus.
#[test]
fn h_ui_41_history_outcomes() {
    let (mut harness, tx) = relay_harness();
    let accepted = execute(&tx, "task.update", rename(3, "Acceptée"));
    harness.run();
    harness.state_mut().dispatch(Action::Approve(accepted.id));
    harness.run();
    // Lien d'annulation : le lot accepté est le dernier annulable.
    let undo = fr(TextId::AiUndoEntry);
    harness.get_by_label(&undo);
    let rejected = execute(&tx, "task.update", rename(4, "Refusée"));
    harness.run();
    harness.state_mut().dispatch(Action::Reject(rejected.id));
    harness.run();
    // Expirée : délai de l'écouteur écoulé avant la décision.
    let expired = execute(&tx, "task.update", rename(5, "Expirée"));
    harness.run();
    assert!(expired.state.expire());
    harness.run();
    assert_eq!(pending_ids(harness.state()).len(), 0);
    // Annuler par le lien : projet d'avant, lien retiré.
    crate::click(&mut harness, &undo);
    assert_eq!(generation(harness.state()), 2, "lot annulé");
    assert!(harness.query_by_label(&undo).is_none());
    // Périmée : document remplacé pendant l'attente.
    let stale = execute(&tx, "task.update", rename(6, "Périmée"));
    harness.run();
    harness.state_mut().dispatch(Action::OpenDemo);
    harness.run();
    harness.run();
    let replies = stale.drain();
    assert!(
        matches!(replies.last(), Some(Reply::Err { error, .. }) if error.code == codes::CONTROL_REJECTED)
    );
    assert_eq!(
        outcomes(harness.state()),
        [
            AiOutcome::Accepted,
            AiOutcome::Rejected,
            AiOutcome::Expired,
            AiOutcome::Stale
        ]
    );
    let app = harness.state();
    let entry = &app.relay().unwrap().history()[0];
    assert_eq!(entry.tool, "task.update");
    assert_eq!(entry.at, crate::now());
    for id in [
        TextId::AiOutcomeAccepted,
        TextId::AiOutcomeRejected,
        TextId::AiOutcomeExpired,
        TextId::AiOutcomeStale,
    ] {
        let outcome = fr(id);
        assert!(harness.query_by_label_contains(&outcome).is_some(), "{outcome}");
    }
    // Plafond : 50 entrées, les plus anciennes retirées.
    for i in 0..MAX_HISTORY {
        let sent = execute(&tx, "task.update", rename(3, &format!("R{i}")));
        harness.run();
        harness.state_mut().dispatch(Action::Reject(sent.id));
        harness.run();
    }
    let history = harness.state().relay().unwrap().history();
    assert_eq!(history.len(), MAX_HISTORY);
    assert!(history.iter().all(|e| e.outcome == AiOutcome::Rejected));
}

/// H-UI-41 : le diff affiché est calculé une fois à l'arrivée, jamais par
/// image ; un nouvel aperçu (génération changée, C-CTL-07) le recalcule
/// une fois.
#[test]
fn h_ui_41_diff_computed_once() {
    let (mut harness, tx) = relay_harness();
    let sent = execute(
        &tx,
        "task.update",
        json!({ "uid": 3, "duration": { "text": "9j" } }),
    );
    for _ in 0..30 {
        harness.run();
    }
    let relay = harness.state().relay().unwrap();
    assert_eq!(relay.diff_builds(), 1);
    assert!(relay.pending()[0].diff.is_some());
    // L'utilisateur modifie le projet pendant l'attente : Accepter donne un
    // nouvel aperçu, rien n'est appliqué.
    let edit: Batch = serde_json::from_value(json!({
        "format_version": 1,
        "ops": [{ "op": "task.update", "uid": 4, "name": "Revu" }]
    }))
    .unwrap();
    harness.state_mut().execute(&edit, EditOrigin::User).unwrap();
    harness.run();
    harness.state_mut().dispatch(Action::Approve(sent.id));
    for _ in 0..10 {
        harness.run();
    }
    let relay = harness.state().relay().unwrap();
    assert_eq!(relay.diff_builds(), 2);
    assert_eq!(pending_ids(harness.state()), [sent.id]);
    // Sans aperçu (enregistrer) : rien n'est calculé.
    harness.state_mut().dispatch(Action::Reject(sent.id));
    harness.run();
    let save = execute(&tx, "project.save", json!({}));
    harness.run();
    assert_eq!(save.drain(), [Reply::PendingApproval { id: save.id }]);
    let relay = harness.state().relay().unwrap();
    assert_eq!(relay.diff_builds(), 2);
    assert!(relay.pending()[0].diff.is_none());
    let no_preview = harness.state().texts().get_args(
        TextId::AiNoPreview,
        &[("detail", Arg::Str("project.save".into()))],
    );
    harness.get_by_label(&no_preview);
}

/// H-UI-58 : « Pourquoi cette date ? » montre le résumé du moteur de la
/// tâche active ; un clic sur un pilote sélectionne cette tâche ;
/// l'explication n'est relue qu'au changement de tâche.
#[test]
fn h_ui_58_why_date_drivers_click() {
    let mut harness = harness();
    open_demo(&mut harness);
    // Première tâche de l'exemple dont le début a un pilote amont.
    let (uid, view) = {
        let session = &harness.state().document.as_ref().unwrap().session;
        session
            .project()
            .tasks
            .iter()
            .find_map(|t| {
                let uid = plannercraft_core::HasUid::uid(t);
                let params = ExplainParams {
                    task: TaskRef::Uid(uid),
                    side: None,
                    depth: None,
                };
                explain_date(session, &params)
                    .ok()
                    .filter(|v| !v.chain.is_empty())
                    .map(|v| (uid, v))
            })
            .unwrap()
    };
    harness.state_mut().dispatch(Action::Select(Some(uid)));
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::WhyThisDate)));
    harness.run();
    for _ in 0..10 {
        harness.run();
    }
    assert_eq!(
        harness.state().side_panel().explain_builds(),
        1,
        "une fois par tâche"
    );
    harness.get_by_label(&view.summary_fr);
    let driver = &view.chain[0];
    let label = format!("{} {}", driver.id.unwrap(), driver.name);
    crate::click(&mut harness, &label);
    harness.run();
    assert_eq!(harness.state().selection, Some(TaskUid::new(driver.uid)));
    assert_eq!(harness.state().side_panel().explain_builds(), 2);
    // Sans tâche active : invitation à en choisir une.
    harness.state_mut().dispatch(Action::Select(None));
    harness.run();
    harness.get_by_label(&fr(TextId::WhyNoTask));
}

/// Scénario `relay_j8.jsonl` avec un vrai `plannercraft mcp` (comme
/// `relay_app_real_mcp`) : chaque décision est prise **dans le volet**
/// (clic sur Accepter, clic sur Refuser, Ctrl/Cmd+Entrée) ; journal égal au
/// scénario. J8 lot L16 (vérification 6) : `apply_style` et `export_pdf`
/// validés dans le volet, une écriture laissée sans réponse jusqu'à
/// l'expiration (délai de validation court, `RELAY_J8_APPROVAL_SECS`),
/// socket Unix et TCP en boucle locale.
#[cfg(unix)]
#[test]
fn relay_j8_panel_real_mcp() {
    relay_j8_scenario(plannercraft_automation::control::Transport::Unix, "relayj8");
}

/// Même scénario relayé en TCP sur la boucle locale (transport de Windows,
/// J8 lot L8) : même journal.
#[cfg(unix)]
#[test]
fn relay_j8_panel_real_mcp_tcp() {
    relay_j8_scenario(plannercraft_automation::control::Transport::Tcp, "relayj8tcp");
}

/// Délai de validation du scénario `relay_j8` (écouteur et serveur) : assez
/// long pour qu'une décision prise à la première image arrive, assez court
/// pour attendre l'expiration de l'appel 7.
#[cfg(unix)]
const RELAY_J8_APPROVAL_SECS: u64 = 4;

#[cfg(unix)]
#[allow(
    clippy::disallowed_methods,
    clippy::too_many_lines,
    reason = "test natif : processus fils et attentes, comme `plannercraft mcp` en vrai ; scénario lu d'un bloc"
)]
fn relay_j8_scenario(transport: plannercraft_automation::control::Transport, tag: &str) {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;

    use plannercraft_core::Moment;

    use crate::e2e::relay_app::{MAX_FRAMES, client_thread, mcp_binary, wired_with};
    use crate::e2e::replay::{self, Scenario, TempDir, normalized, repo_root, substitute};

    /// Décision prise dans le volet pour l'appel d'indice `i`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Decide {
        Click(TextId),
        Keys,
        Wait,
    }
    let decide = |i: usize| match i {
        0 => Decide::Click(TextId::ApprovalAccept),
        1 => Decide::Click(TextId::ApprovalReject),
        6 => Decide::Wait,
        _ => Decide::Keys,
    };

    let Some(binary) = mcp_binary().filter(|p| p.is_file()) else {
        let required = std::env::var("PLANNERCRAFT_RELAY").is_ok_and(|v| v == "required");
        assert!(
            !required,
            "PLANNERCRAFT_RELAY=required : binaire plannercraft introuvable (PLANNERCRAFT_MCP_BIN)"
        );
        eprintln!(
            "relay_j8_panel_real_mcp : binaire plannercraft absent, test non exécuté (cargo build -p plannercraft-cli)"
        );
        return;
    };
    let root = repo_root(env!("CARGO_MANIFEST_DIR"));
    let scenario = Scenario::load(&root, "relay_j8");
    let now: Moment = replay::NOW.parse().unwrap();
    let tmp = TempDir::new(tag);
    let (mut harness, handle, guard, control, _sockets) =
        wired_with(&tmp, now, transport, RELAY_J8_APPROVAL_SECS);
    assert_eq!(
        handle.endpoint().starts_with("tcp:127.0.0.1:"),
        transport == plannercraft_automation::control::Transport::Tcp,
        "{}",
        handle.endpoint()
    );
    let journal = tmp.path().join("run.jsonl");
    let mut child = Command::new(&binary)
        .args(["--now", replay::NOW, "--lang", "fr", "mcp", "--journal"])
        .arg(&journal)
        .args(["--approval-timeout", &RELAY_J8_APPROVAL_SECS.to_string()])
        .env("PLANNERCRAFT_CONTROL_DIR", &control)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|e| panic!("{} : {e}", binary.display()));
    let (calls_tx, calls_rx) = sync_channel::<Option<(String, Value)>>(1);
    let (results_tx, results_rx) = sync_channel::<Value>(1);
    client_thread(&mut child, calls_rx, results_tx);
    let mut got = Vec::new();
    for (i, call) in scenario.calls.iter().enumerate() {
        let params = substitute(&call.params, tmp.path());
        calls_tx.send(Some((call.tool.clone(), params))).unwrap();
        let mut decided = false;
        let mut result = None;
        for _ in 0..MAX_FRAMES {
            if let Ok(r) = results_rx.try_recv() {
                result = Some(r);
                break;
            }
            let pending = harness
                .state()
                .relay()
                .is_some_and(|relay| !relay.pending().is_empty());
            if pending && !decided {
                harness.run();
                assert!(harness.state().side_panel().shows_ai(), "volet ouvert d'office");
                match decide(i) {
                    Decide::Click(id) => harness.get_by_label(&fr(id)).click(),
                    Decide::Keys => harness.key_press_modifiers(Modifiers::COMMAND, Key::Enter),
                    Decide::Wait => {}
                }
                decided = true;
            }
            harness.step();
            thread::sleep(Duration::from_millis(5));
        }
        got.push(normalized(
            &result.unwrap_or_else(|| panic!("aucun résultat de l'appel relayé d'indice {i}")),
            tmp.path(),
        ));
    }
    calls_tx.send(None).unwrap();
    drop(calls_tx);
    let mut status = None;
    for _ in 0..3_000 {
        if let Some(s) = child.try_wait().unwrap() {
            status = Some(s);
            break;
        }
        harness.step();
        thread::sleep(Duration::from_millis(10));
    }
    let status = status.expect("plannercraft mcp ne s'arrête pas");
    assert!(status.success(), "{status}");
    drop((handle, guard));
    let produced = std::fs::read_to_string(&journal).unwrap();
    if replay::blessing() {
        replay::bless(&root, &scenario, &produced, tmp.path(), None);
        return;
    }
    replay::assert_results(&scenario, &got);
    replay::assert_journal(&scenario, &produced, tmp.path());
    let app = harness.state();
    assert_eq!(
        outcomes(app),
        [
            AiOutcome::Accepted,
            AiOutcome::Rejected,
            AiOutcome::Accepted,
            AiOutcome::Accepted,
            AiOutcome::Accepted,
            AiOutcome::Expired
        ]
    );
    // Style validé dans le document de l'application ; PDF écrit.
    let style = plannercraft_automation::command::style::doc_style(app.document.as_ref().unwrap().project());
    assert_eq!(
        style.map(|d| d.base),
        Some(plannercraft_render::style::TemplateId::Presentation)
    );
    let pdf = std::fs::read(tmp.path().join("relay_j8.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
}
