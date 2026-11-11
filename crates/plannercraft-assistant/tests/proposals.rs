//! Propositions d'écriture de l'assistant (lot A2) sans réseau : l'outil
//! `propose_changes` est essayé en `dry_run` sur une copie, mis en file
//! (rien d'appliqué), le refus du registre est rendu au modèle (deux
//! corrections au plus), une seule proposition par demande, la décision de
//! l'utilisateur est dite au message suivant.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests : un échec doit s'arrêter net"
)]

use plannercraft_assistant::conversation::{Conversation, RequestConfig, SavedConversation, TurnState};
use plannercraft_assistant::openrouter;
use plannercraft_assistant::proposal::{PROPOSE_TOOL, ProposalStatus};
use plannercraft_assistant::testing::{ScriptedTransport, sse_response, text_chunks, tool_call_chunks};
use plannercraft_assistant::tools::SessionHost;
use plannercraft_automation::text::Language;
use plannercraft_automation::{CallEnv, Workspace};
use plannercraft_core::Moment;
use serde_json::{Value, json};

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

/// Projet « Maison » : A (1 j) puis B (2 j).
fn workspace() -> Workspace {
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Maison", "start": "2026-10-19T08:00" }),
    )
    .unwrap();
    ws.call(
        "batch.apply",
        json!({ "format_version": 1, "ops": [
            { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "1j" } },
            { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "2j" } },
            { "op": "link.add", "pred": "$a", "succ": "$b" }
        ] }),
    )
    .unwrap();
    ws
}

fn config() -> RequestConfig {
    RequestConfig::new("sk-or-v1-test", "anthropic/claude-sonnet-5.5", Language::Fr, 1024)
}

fn host(ws: &Workspace) -> SessionHost<'_> {
    SessionHost::new(ws.session(), CallEnv::new(now(), Language::Fr), json!({}))
}

fn body(req: &plannercraft_assistant::HttpRequest) -> Value {
    serde_json::from_slice(&req.body).unwrap()
}

fn propose(id: &str, args: &Value) -> Vec<plannercraft_assistant::HttpEvent> {
    sse_response(&tool_call_chunks(id, PROPOSE_TOOL, &args.to_string()))
}

fn answer(text: &str) -> Vec<plannercraft_assistant::HttpEvent> {
    sse_response(&text_chunks(text, 0.001))
}

/// Contenu du message `role: tool` de l'appel `call_id` dans la requête.
fn tool_result(req: &Value, call_id: &str) -> Value {
    let msg = req["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool" && m["tool_call_id"] == call_id)
        .unwrap();
    serde_json::from_str(msg["content"].as_str().unwrap()).unwrap()
}

fn run(conv: &mut Conversation, h: &mut SessionHost<'_>, t: &ScriptedTransport) {
    while conv.poll(h, t) {}
}

/// Proposition acceptée en file : résumé du `dry_run` rendu au modèle,
/// carte en attente, projet intact.
#[test]
fn proposal_queued_never_applied() {
    let ws = workspace();
    let before = ws.session().unwrap().clone();
    let args = json!({ "label": "Ajouter C", "ops": [
        { "op": "task.add", "name": "C", "as": "$c", "duration": { "text": "3d" } },
        { "op": "link.add", "pred": 2, "succ": "$c" }
    ] });
    let t = ScriptedTransport::new(vec![propose("p1", &args), answer("Je propose d'ajouter C.")]);
    let mut conv = Conversation::new();
    conv.send("Ajoute C après B", config(), &t).unwrap();
    let mut h = host(&ws);
    run(&mut conv, &mut h, &t);
    assert_eq!(conv.turns()[0].state, TurnState::Done);
    let result = tool_result(&body(&t.requests()[1]), "p1");
    assert_eq!(result["status"], "pending_approval");
    assert_eq!(result["proposal"], 1);
    assert_eq!(result["created_tasks"], 1);
    assert!(result["note"].as_str().unwrap().contains("Rien n'est appliqué"));
    let card = conv.turns()[0].tools[0].proposal.clone().unwrap();
    assert_eq!((card.id, card.label.as_str()), (1, "Ajouter C"));
    assert_eq!(card.status, ProposalStatus::Pending);
    assert!(card.changed_tasks >= 1);
    assert_eq!(h.proposals.len(), 1);
    assert_eq!(h.proposals[0].params["ops"].as_array().unwrap().len(), 2);
    assert_eq!(h.next_proposal, 2);
    assert_eq!(conv.pending_proposals(), [1]);
    // Rien d'appliqué : même projet, même historique.
    let after = ws.session().unwrap();
    assert_eq!(after.project(), before.project());
    assert_eq!(after.history().entries.len(), before.history().entries.len());
}

/// Lot d'avancement seul (défaut du 2026-11-10, « 0 tâche touchée ») : la
/// carte et le résultat rendu au modèle portent les tâches touchées
/// (`touched_tasks`, R-163 amendée) à côté des dates déplacées
/// (`changed_tasks`, inchangé).
#[test]
fn proposal_counts_touched_tasks_without_moved_dates() {
    let ws = workspace();
    let args = json!({ "label": "Avancement", "ops": [
        { "op": "progress.update", "task": 1, "percent_complete": { "text": "100%" } },
        { "op": "progress.update", "task": 2, "percent_complete": { "text": "100%" } }
    ] });
    let t = ScriptedTransport::new(vec![propose("p1", &args), answer("Je propose l'avancement.")]);
    let mut conv = Conversation::new();
    conv.send("Passe A et B à 100 %", config(), &t).unwrap();
    let mut h = host(&ws);
    run(&mut conv, &mut h, &t);
    let result = tool_result(&body(&t.requests()[1]), "p1");
    assert_eq!(result["touched_tasks"], 2);
    assert_eq!(result["changed_tasks"], 0);
    let card = conv.turns()[0].tools[0].proposal.clone().unwrap();
    assert_eq!((card.touched_tasks, card.changed_tasks), (2, 0));
    assert_eq!(card.touched(), 2);
    // Carte enregistrée avant le champ : relue à 0, jamais sous les dates
    // déplacées.
    let mut old = serde_json::to_value(&card).unwrap();
    old.as_object_mut().unwrap().remove("touched_tasks");
    old["changed_tasks"] = json!(3);
    let old: plannercraft_assistant::proposal::ProposalCard = serde_json::from_value(old).unwrap();
    assert_eq!((old.touched_tasks, old.touched()), (0, 3));
}

/// Refus du registre (boucle) rendu en clair ; deux corrections au plus,
/// puis plus d'appel à l'hôte.
#[test]
fn registry_refusal_then_two_corrections_at_most() {
    let ws = workspace();
    let cycle = json!({ "label": "Boucle", "ops": [{ "op": "link.add", "pred": 2, "succ": 1 }] });
    let dated = json!({ "label": "Daté", "ops": [
        { "op": "task.add", "name": "D", "start": "2026-11-02T08:00" }
    ] });
    let t = ScriptedTransport::new(vec![
        propose("p1", &cycle),
        propose("p2", &dated),
        propose("p3", &cycle),
        propose("p4", &cycle),
        answer("Je n'arrive pas à proposer ce lien."),
    ]);
    let mut conv = Conversation::new();
    conv.send("Lie B à A", config(), &t).unwrap();
    let mut h = host(&ws);
    run(&mut conv, &mut h, &t);
    let reqs = t.requests();
    let first = tool_result(&body(&reqs[1]), "p1");
    assert_eq!(first["error"]["code"], "batch.would_create_errors", "{first}");
    assert_eq!(first["error"]["diagnostics"][0]["code"], "link.cycle");
    assert!(
        first["error"]["message"]
            .as_str()
            .unwrap()
            .contains("liens circulaires")
    );
    assert!(first["hint"].as_str().unwrap().contains("2 correction"));
    let second = tool_result(&body(&reqs[2]), "p2");
    assert_eq!(second["error"]["field"], "start");
    assert!(second["hint"].as_str().unwrap().contains("1 correction"));
    let third = tool_result(&body(&reqs[3]), "p3");
    assert!(third["hint"].as_str().unwrap().contains("Plus de correction"));
    let fourth = tool_result(&body(&reqs[4]), "p4");
    assert_eq!(fourth["error"]["code"], "proposal.blocked");
    assert_eq!(h.proposals.len(), 0);
    assert!(conv.turns()[0].tools.iter().all(|s| s.proposal.is_none()));
    assert_eq!(conv.turns()[0].state, TurnState::Done);
}

/// Une seule proposition soumise par demande.
#[test]
fn one_proposal_per_request() {
    let ws = workspace();
    let a = json!({ "label": "Un", "ops": [{ "op": "task.update", "uid": 1, "name": "A1" }] });
    let b = json!({ "label": "Deux", "ops": [{ "op": "task.update", "uid": 2, "name": "B1" }] });
    let t = ScriptedTransport::new(vec![propose("p1", &a), propose("p2", &b), answer("Fait.")]);
    let mut conv = Conversation::new();
    conv.send("Renomme", config(), &t).unwrap();
    let mut h = host(&ws);
    run(&mut conv, &mut h, &t);
    let second = tool_result(&body(&t.requests()[2]), "p2");
    assert_eq!(second["error"]["code"], "proposal.blocked");
    assert_eq!(h.proposals.len(), 1);
}

/// La décision de l'utilisateur est jointe à la question suivante, puis
/// oubliée ; une proposition déjà tranchée ne change plus.
#[test]
fn decision_told_with_next_message() {
    let ws = workspace();
    let args = json!({ "label": "Renommer A", "ops": [{ "op": "task.update", "uid": 1, "name": "Études" }] });
    let t = ScriptedTransport::new(vec![
        propose("p1", &args),
        answer("Proposé."),
        answer("D'accord."),
    ]);
    let mut conv = Conversation::new();
    conv.send("Renomme A en Études", config(), &t).unwrap();
    let mut h = host(&ws);
    run(&mut conv, &mut h, &t);
    let status = ProposalStatus::Rejected {
        reason: Some("garde le nom".to_owned()),
    };
    assert!(conv.set_proposal_status(1, status.clone()));
    assert!(
        !conv.set_proposal_status(1, ProposalStatus::Accepted),
        "déjà tranchée"
    );
    assert_eq!(conv.turns()[0].tools[0].proposal.as_ref().unwrap().status, status);
    assert_eq!(conv.untold_decisions(), 1);
    conv.send("Et maintenant ?", config(), &t).unwrap();
    run(&mut conv, &mut h, &t);
    let req = body(&t.requests()[2]);
    let last = req["messages"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["role"], "user");
    let text = last["content"].as_str().unwrap();
    assert!(
        text.starts_with(
            "[Décision de l'utilisateur sur la proposition « Renommer A » : refusée, rien n'a été appliqué \
             (motif de l'utilisateur : garde le nom).]"
        ),
        "{text}"
    );
    assert!(text.ends_with("Et maintenant ?"));
    assert_eq!(
        conv.turns()[1].question,
        "Et maintenant ?",
        "question affichée intacte"
    );
    assert_eq!(conv.untold_decisions(), 0);
}

/// Décision prise pendant la demande : dite au tour d'API suivant, en
/// message à part après les résultats d'outils.
#[test]
fn decision_told_mid_request() {
    let ws = workspace();
    let args =
        json!({ "label": "Renommer B", "ops": [{ "op": "task.update", "uid": 2, "name": "Fondations" }] });
    let t = ScriptedTransport::manual(vec![
        propose("p1", &args),
        sse_response(&tool_call_chunks("r1", "project_summary", "{}")),
        answer("Bien noté."),
    ]);
    let mut conv = Conversation::new();
    conv.send("Renomme B", config(), &t).unwrap();
    let mut h = host(&ws);
    while t.step(1) > 0 {
        conv.poll(&mut h, &t);
        if conv.pending_proposals() == [1] && conv.untold_decisions() == 0 && t.requests().len() == 2 {
            conv.set_proposal_status(1, ProposalStatus::Accepted);
        }
    }
    let req = body(&t.requests()[2]);
    let msgs = req["messages"].as_array().unwrap();
    let note = msgs.last().unwrap();
    assert_eq!(note["role"], "user");
    assert!(note["content"].as_str().unwrap().contains("acceptée"), "{note}");
}

/// Conversation relue : une proposition en attente est expirée (la file
/// n'est pas gardée).
#[test]
fn restored_pending_proposal_expires() {
    let ws = workspace();
    let args = json!({ "label": "X", "ops": [{ "op": "task.update", "uid": 1, "name": "X" }] });
    let t = ScriptedTransport::new(vec![propose("p1", &args), answer("Proposé.")]);
    let mut conv = Conversation::new();
    conv.send("X", config(), &t).unwrap();
    let mut h = host(&ws);
    run(&mut conv, &mut h, &t);
    let saved: SavedConversation =
        serde_json::from_str(&serde_json::to_string(&conv.saved()).unwrap()).unwrap();
    let restored = Conversation::restore(saved);
    let card = restored.turns()[0].tools[0].proposal.clone().unwrap();
    assert_eq!(card.status, ProposalStatus::Expired);
    assert_eq!(restored.pending_proposals(), Vec::<u64>::new());
}

/// « Montrer la réflexion » : `reasoning.max_tokens` et plafond relevé ;
/// coupée : aucun paramètre.
#[test]
fn reasoning_budget_only_when_asked() {
    let ws = workspace();
    let t = ScriptedTransport::new(vec![answer("Oui."), answer("Oui.")]);
    let mut conv = Conversation::new();
    conv.send("Q", config(), &t).unwrap();
    run(&mut conv, &mut host(&ws), &t);
    let mut with = config();
    with.reasoning_budget = Some(openrouter::REASONING_BUDGET);
    with.max_tokens = openrouter::max_tokens_for(with.reasoning_budget);
    conv.send("Q2", with, &t).unwrap();
    run(&mut conv, &mut host(&ws), &t);
    let off = body(&t.requests()[0]);
    assert!(off.get("reasoning").is_none());
    let on = body(&t.requests()[1]);
    assert_eq!(on["reasoning"], json!({ "max_tokens": 1024 }));
    assert_eq!(on["max_tokens"], openrouter::MAX_TOKENS + 1024);
    assert!(on["max_tokens"].as_u64().unwrap() > 1024);
}

/// L'outil d'écriture est décrit au modèle, sans les schémas complets du
/// registre (`apply_batch` absent).
#[test]
fn propose_tool_described() {
    let ws = workspace();
    let t = ScriptedTransport::new(vec![answer("Oui.")]);
    let mut conv = Conversation::new();
    conv.send("Q", config(), &t).unwrap();
    run(&mut conv, &mut host(&ws), &t);
    let req = body(&t.requests()[0]);
    let names: Vec<&str> = req["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&PROPOSE_TOOL));
    assert!(!names.contains(&"apply_batch"));
    let system = req["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(system.contains("Ne calcule jamais une date"));
}

/// Une échéance à la date écrite par l'utilisateur passe ; une date que le
/// modèle déduit est refusée et compte comme une correction.
#[test]
fn deadline_only_at_cited_date() {
    let ws = workspace();
    let inferred = json!({ "label": "Échéance", "ops": [
        { "op": "task.set_deadline", "uid": 2, "deadline": "2026-11-20T17:00" }
    ] });
    let cited = json!({ "label": "Échéance", "ops": [
        { "op": "task.set_deadline", "uid": 2, "deadline": "2026-11-27T17:00" }
    ] });
    let t = ScriptedTransport::new(vec![
        propose("p1", &inferred),
        propose("p2", &cited),
        answer("Proposé."),
    ]);
    let mut conv = Conversation::new();
    conv.send("B doit finir au plus tard le 27/11/2026", config(), &t)
        .unwrap();
    let mut h = host(&ws);
    run(&mut conv, &mut h, &t);
    let refused = tool_result(&body(&t.requests()[1]), "p1");
    assert_eq!(refused["error"]["code"], "param.invalid");
    assert!(refused["hint"].as_str().unwrap().contains("2 correction"));
    let ok = tool_result(&body(&t.requests()[2]), "p2");
    assert_eq!(ok["status"], "pending_approval");
    assert_eq!(h.proposals.len(), 1);
}
