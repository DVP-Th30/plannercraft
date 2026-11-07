//! Boucle d'outils sans réseau (lots A1 et A1b) : réponses SSE écrites à la
//! main d'après la documentation d'OpenRouter, exécutées sur un vrai
//! projet ; tours, mémoire, raisonnement, reprise, persistance.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests : un échec doit s'arrêter net"
)]

use plannercraft_assistant::archive::Archive;
use plannercraft_assistant::conversation::{
    Activity, Conversation, MAX_ROUNDS, RequestConfig, SavedConversation, TurnState,
};
use plannercraft_assistant::error::AssistantError;
use plannercraft_assistant::http::HttpEvent;
use plannercraft_assistant::testing::{
    ScriptedTransport, error_response, reasoning_chunks, sse_response, text_chunks, tool_call_chunks,
};
use plannercraft_assistant::tools::SessionHost;
use plannercraft_automation::text::Language;
use plannercraft_automation::{CallEnv, Workspace};
use plannercraft_core::Moment;
use serde_json::{Value, json};

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

/// A 1 j, B 2 j, C après A et B.
fn workspace() -> (Workspace, Value) {
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Maison", "start": "2026-10-19T08:00" }),
    )
    .unwrap();
    let out = ws
        .call(
            "batch.apply",
            json!({ "format_version": 1, "ops": [
                { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "1j" } },
                { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "2j" } },
                { "op": "task.add", "name": "C", "as": "$c", "duration": { "text": "1j" } },
                { "op": "link.add", "pred": "$a", "succ": "$c" },
                { "op": "link.add", "pred": "$b", "succ": "$c" }
            ] }),
        )
        .unwrap();
    (ws, out["labels"].clone())
}

fn config() -> RequestConfig {
    RequestConfig::new(KEY, "anthropic/claude-sonnet-5.5", Language::Fr, 512)
}

const KEY: &str = "sk-or-v1-test-secret";

/// Dernier état de tour.
fn last_state(conv: &Conversation) -> TurnState {
    conv.turns().last().unwrap().state.clone()
}

fn host(ws: &Workspace) -> SessionHost<'_> {
    SessionHost::new(
        ws.session(),
        CallEnv::new(now(), Language::Fr),
        json!({ "active": 3, "view": "gantt" }),
    )
}

fn body(req: &plannercraft_assistant::HttpRequest) -> Value {
    serde_json::from_slice(&req.body).unwrap()
}

/// Appel d'outil → exécution par le moteur sur le projet → réponse finale.
#[test]
fn tool_loop_reads_engine_then_answers() {
    let (ws, labels) = workspace();
    let c = labels["$c"].clone();
    let transport = ScriptedTransport::new(vec![
        sse_response(&tool_call_chunks(
            "call_1",
            "explain_date",
            &json!({ "task": c }).to_string(),
        )),
        sse_response(&text_chunks(
            "La tâche 3 C commence le 21/10 car B la pilote.",
            0.0042,
        )),
    ]);
    let mut conv = Conversation::new();
    let ws_snapshot = ws.session().unwrap().clone();
    conv.send("Pourquoi C commence le 21 ?", config(), &transport)
        .unwrap();
    assert!(conv.busy());
    let mut h = host(&ws);
    while conv.poll(&mut h, &transport) {}
    assert!(!conv.busy());
    let requests = transport.requests();
    assert_eq!(requests.len(), 2);
    let first = body(&requests[0]);
    assert_eq!(
        first["provider"],
        json!({ "zdr": true, "data_collection": "deny" })
    );
    assert_eq!(first["messages"][0]["role"], "system");
    // Point de cache n° 1 (modèle Anthropic) : consignes en bloc marqué.
    assert_eq!(
        first["messages"][0]["content"][0]["cache_control"],
        json!({ "type": "ephemeral" })
    );
    assert!(
        first["messages"][0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("ne jamais en calculer")
    );
    assert_eq!(
        first["messages"][1],
        json!({ "role": "user", "content": "Pourquoi C commence le 21 ?" })
    );
    let names: Vec<&str> = first["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["function"]["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"explain_date") && names.contains(&"ui_state"));
    assert!(!names.contains(&"add_task") && !names.contains(&"apply_batch"));
    // Second tour : l'appel et son résultat, calculé par le moteur.
    let second = body(&requests[1]);
    let msgs = second["messages"].as_array().unwrap();
    assert_eq!(msgs[2]["tool_calls"][0]["function"]["name"], "explain_date");
    assert_eq!(msgs[3]["role"], "tool");
    assert_eq!(msgs[3]["tool_call_id"], "call_1");
    let result: Value = serde_json::from_str(msgs[3]["content"].as_str().unwrap()).unwrap();
    assert_eq!(result["date"], json!("2026-10-21T08:00"));
    // Tour : question, lecture, réponse, coût (somme des tours d'API).
    let turn = &conv.turns()[0];
    assert_eq!(turn.question, "Pourquoi C commence le 21 ?");
    assert_eq!(turn.tools.len(), 1);
    assert_eq!(turn.tools[0].name, "explain_date");
    assert_eq!(turn.tools[0].error, None);
    assert_eq!(turn.answer, "La tâche 3 C commence le 21/10 car B la pilote.");
    assert_eq!(turn.state, TurnState::Done);
    assert_eq!(turn.wire.len(), 4, "question, appel, résultat, réponse");
    let usage = turn.usage.unwrap();
    assert_eq!(usage.prompt_tokens, 2200);
    assert!((usage.cost.unwrap() - 0.0052).abs() < 1e-9);
    // Lecture seule : la session n'a pas bougé.
    assert_eq!(
        ws.session().unwrap().history_position(),
        ws_snapshot.history_position()
    );
    assert_eq!(ws.session().unwrap().generation(), ws_snapshot.generation());
}

#[test]
fn ui_state_served_by_host() {
    let (ws, _) = workspace();
    let transport = ScriptedTransport::new(vec![
        sse_response(&tool_call_chunks("u", "ui_state", "{}")),
        sse_response(&text_chunks("Tâche 3.", 0.0)),
    ]);
    let mut conv = Conversation::new();
    conv.send("Quelle tâche est sélectionnée ?", config(), &transport)
        .unwrap();
    let mut h = host(&ws);
    while conv.poll(&mut h, &transport) {}
    let second = body(&transport.requests()[1]);
    assert_eq!(
        second["messages"][3]["content"],
        json!({ "active": 3, "view": "gantt" }).to_string()
    );
}

#[test]
fn refused_without_key_nothing_sent() {
    let transport = ScriptedTransport::new(vec![]);
    let mut conv = Conversation::new();
    let mut cfg = config();
    cfg.api_key = "  ".to_owned();
    assert_eq!(conv.send("x", cfg, &transport), Err(AssistantError::NoKey));
    assert_eq!(transport.requests().len(), 0);
    assert!(conv.is_empty());
}

#[test]
fn http_errors_become_entries() {
    let (ws, _) = workspace();
    for (response, expected) in [
        (
            error_response(401, &[], "No auth credentials found"),
            AssistantError::Unauthorized,
        ),
        (
            error_response(402, &[], "Insufficient credits"),
            AssistantError::PaymentRequired,
        ),
        (
            error_response(429, &[("Retry-After", "7")], "Rate limited"),
            AssistantError::RateLimited { retry_after: Some(7) },
        ),
    ] {
        let transport = ScriptedTransport::new(vec![response]);
        let mut conv = Conversation::new();
        conv.send("x", config(), &transport).unwrap();
        let mut h = host(&ws);
        while conv.poll(&mut h, &transport) {}
        assert_eq!(last_state(&conv), TurnState::Failed(expected));
        assert!(!conv.busy());
    }
}

#[test]
fn network_cut_is_displayable_error() {
    let (ws, _) = workspace();
    // Coupure en plein flux : en-tête, un morceau, puis échec du transport.
    let mut cut = sse_response(&text_chunks("Début de réponse", 0.0));
    cut.truncate(3);
    cut.push(HttpEvent::Failed("connection reset".to_owned()));
    // Flux fermé sans [DONE] ni raison de fin : coupure aussi.
    let mut closed = sse_response(&text_chunks("Rien", 0.0));
    closed.truncate(3);
    closed.push(HttpEvent::End);
    for response in [cut, closed, vec![HttpEvent::Failed("dns".to_owned())]] {
        let transport = ScriptedTransport::new(vec![response]);
        let mut conv = Conversation::new();
        conv.send("x", config(), &transport).unwrap();
        let mut h = host(&ws);
        while conv.poll(&mut h, &transport) {}
        assert!(
            matches!(
                last_state(&conv),
                TurnState::Failed(AssistantError::Network { .. })
            ),
            "{:?}",
            conv.turns()
        );
    }
}

#[test]
fn mid_stream_provider_error() {
    let (ws, _) = workspace();
    let transport = ScriptedTransport::new(vec![sse_response(&[json!({
        "error": { "code": "server_error", "message": "Provider disconnected unexpectedly" },
        "choices": [{ "index": 0, "delta": { "content": "" }, "finish_reason": "error" }]
    })])]);
    let mut conv = Conversation::new();
    conv.send("x", config(), &transport).unwrap();
    let mut h = host(&ws);
    while conv.poll(&mut h, &transport) {}
    assert_eq!(
        last_state(&conv),
        TurnState::Failed(AssistantError::Provider {
            message: "Provider disconnected unexpectedly".to_owned()
        })
    );
}

#[test]
fn rounds_bounded_last_round_without_tools() {
    let (ws, _) = workspace();
    let responses = (0..MAX_ROUNDS)
        .map(|i| sse_response(&tool_call_chunks(&format!("c{i}"), "project_summary", "{}")))
        .collect();
    let transport = ScriptedTransport::new(responses);
    let mut conv = Conversation::new();
    conv.send("boucle", config(), &transport).unwrap();
    let mut h = host(&ws);
    while conv.poll(&mut h, &transport) {}
    let requests = transport.requests();
    assert_eq!(requests.len(), MAX_ROUNDS as usize);
    assert_eq!(body(&requests[0])["tool_choice"], "auto");
    assert_eq!(body(requests.last().unwrap())["tool_choice"], "none");
    assert_eq!(
        last_state(&conv),
        TurnState::Failed(AssistantError::TooManyRounds)
    );
    assert_eq!(conv.turns()[0].tools.len(), MAX_ROUNDS as usize - 1);
}

/// Flux pas à pas : texte affiché au fil de l'eau, arrêt, évènements
/// tardifs ignorés, nouvelle demande possible.
#[test]
fn streaming_then_stop() {
    let (ws, _) = workspace();
    let transport = ScriptedTransport::manual(vec![sse_response(&text_chunks("un deux trois quatre", 0.0))]);
    let mut conv = Conversation::new();
    conv.send("compte", config(), &transport).unwrap();
    let mut h = host(&ws);
    assert_eq!(conv.activity(), Activity::Waiting);
    transport.step(4); // en-tête, commentaire, « un », « deux »
    assert!(conv.poll(&mut h, &transport));
    assert_eq!(conv.activity(), Activity::Writing);
    assert_eq!(conv.turns()[0].answer, "un deux ");
    conv.stop();
    assert!(!conv.busy());
    assert_eq!(last_state(&conv), TurnState::Stopped);
    // Le reste du flux est refusé par le puits (arrêt de la connexion).
    transport.step(10);
    assert!(!conv.poll(&mut h, &transport));
    assert_eq!(transport.pending_events(), 0);
    assert_eq!(last_state(&conv), TurnState::Stopped);
    assert_eq!(conv.turns()[0].answer, "un deux ");
}

#[test]
fn reading_plan_activity_after_tools() {
    let (ws, _) = workspace();
    let transport = ScriptedTransport::manual(vec![
        sse_response(&tool_call_chunks("c", "list_tasks", "{}")),
        sse_response(&text_chunks("Trois tâches.", 0.0)),
    ]);
    let mut conv = Conversation::new();
    conv.send("liste", config(), &transport).unwrap();
    let mut h = host(&ws);
    // Appel d'outil annoncé dans le flux, pas encore terminé : lecture en
    // cours (nom de l'outil).
    transport.step(3);
    conv.poll(&mut h, &transport);
    assert_eq!(conv.pending_tools(), vec!["list_tasks".to_owned()]);
    assert_eq!(conv.activity(), Activity::Reading("list_tasks".to_owned()));
    transport.step(100);
    conv.poll(&mut h, &transport);
    assert_eq!(conv.activity(), Activity::Reading("list_tasks".to_owned()));
    transport.step(100);
    while conv.poll(&mut h, &transport) {}
    assert_eq!(conv.activity(), Activity::Idle);
}

/// Essai réel facultatif (jamais en CI) : `OPENROUTER_API_KEY` requise.
#[test]
#[ignore = "réseau réel : OPENROUTER_API_KEY=… cargo test -p plannercraft-assistant -- --ignored live"]
fn live_openrouter_round_trip() {
    let Ok(key) = std::env::var("OPENROUTER_API_KEY") else {
        return;
    };
    let (ws, _) = workspace();
    let transport = plannercraft_assistant::EhttpTransport;
    let mut conv = Conversation::new();
    let mut cfg = config();
    cfg.api_key = key;
    cfg.model = "anthropic/claude-haiku-5.5".to_owned();
    conv.send(
        "Quelle est la date de fin du projet ? Utilise les outils.",
        cfg,
        &transport,
    )
    .unwrap();
    let mut h = host(&ws);
    for _ in 0..600 {
        conv.poll(&mut h, &transport);
        if !conv.busy() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    println!("{:#?}", conv.turns());
    assert_ne!(conv.turns()[0].answer, "");
}

/// Deux tours : le second renvoie l'historique (premier tour intact,
/// point de cache n° 2 sur sa réponse) ; le contexte de l'interface n'est
/// joint qu'à la question en cours.
#[test]
fn second_turn_sends_history_with_context_last_only() {
    let (ws, _) = workspace();
    let transport = ScriptedTransport::new(vec![
        sse_response(&tool_call_chunks("c1", "critical_path", "{}")),
        sse_response(&text_chunks("A puis C.", 0.001)),
        sse_response(&text_chunks("Oui.", 0.001)),
    ]);
    let mut conv = Conversation::new();
    let mut h = host(&ws);
    let mut cfg = config();
    cfg.context = Some("[Interface] {\"active\":3}".to_owned());
    conv.send("Chemin critique ?", cfg.clone(), &transport).unwrap();
    while conv.poll(&mut h, &transport) {}
    conv.send("Et la tâche active ?", cfg, &transport).unwrap();
    while conv.poll(&mut h, &transport) {}
    let third = body(&transport.requests()[2]);
    let msgs = third["messages"].as_array().unwrap();
    let roles: Vec<&str> = msgs.iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(
        roles,
        ["system", "user", "assistant", "tool", "assistant", "user"]
    );
    assert_eq!(
        msgs[1]["content"], "Chemin critique ?",
        "contexte retiré de l'historique"
    );
    assert_eq!(
        msgs[4]["content"],
        json!([{ "type": "text", "text": "A puis C.", "cache_control": { "type": "ephemeral" } }])
    );
    assert_eq!(
        msgs[5]["content"],
        "Et la tâche active ?\n\n[Interface] {\"active\":3}"
    );
    assert_eq!(conv.turns().len(), 2);
}

/// Raisonnement en flux : affiché (activité « réflexion »), durée mesurée
/// sur l'horloge de l'interface, renvoyé avec l'appel d'outil.
#[test]
fn reasoning_streamed_timed_and_echoed_with_tool_call() {
    let (ws, _) = workspace();
    let mut first = reasoning_chunks("Je lis le chemin critique.");
    first.extend(tool_call_chunks("c1", "critical_path", "{}"));
    let transport = ScriptedTransport::manual(vec![
        sse_response(&first),
        sse_response(&text_chunks("A puis C.", 0.0)),
    ]);
    let mut conv = Conversation::new();
    let mut h = host(&ws);
    conv.set_now(10.0);
    conv.send("Chemin ?", config(), &transport).unwrap();
    transport.step(4); // en-tête, commentaire, « Je », « lis »
    conv.poll(&mut h, &transport);
    assert_eq!(conv.activity(), Activity::Reasoning);
    assert_eq!(conv.turns()[0].reasoning, "Je lis ");
    conv.set_now(16.2);
    transport.step(100);
    conv.poll(&mut h, &transport);
    let turn = &conv.turns()[0];
    assert_eq!(turn.reasoning, "Je lis le chemin critique.");
    assert_eq!(turn.reasoning_secs, Some(6));
    transport.step(100);
    while conv.poll(&mut h, &transport) {}
    let second = body(&transport.requests()[1]);
    let call = &second["messages"][2];
    assert_eq!(call["reasoning_details"][0]["text"], "Je lis le chemin critique.");
    assert_eq!(call["reasoning_details"][0]["signature"], "sig-test");
    assert_eq!(conv.turns()[0].answer, "A puis C.");
}

/// Arrêt puis « Continuer » : le texte partiel est gardé, la suite s'y
/// ajoute ; la consigne de reprise part sans être affichée.
#[test]
fn stop_then_continue_appends() {
    let (ws, _) = workspace();
    let transport = ScriptedTransport::manual(vec![
        sse_response(&text_chunks("un deux trois", 0.0)),
        sse_response(&text_chunks("trois quatre", 0.0)),
    ]);
    let mut conv = Conversation::new();
    let mut h = host(&ws);
    conv.send("compte", config(), &transport).unwrap();
    transport.step(4);
    conv.poll(&mut h, &transport);
    conv.stop();
    assert_eq!(conv.turns()[0].answer, "un deux ");
    conv.continue_last("Continue.", config(), &transport).unwrap();
    assert!(conv.busy());
    transport.step(100);
    while conv.poll(&mut h, &transport) {}
    assert_eq!(conv.turns().len(), 1);
    assert_eq!(conv.turns()[0].answer, "un deux trois quatre");
    assert_eq!(last_state(&conv), TurnState::Done);
    let second = body(&transport.requests()[1]);
    let msgs = second["messages"].as_array().unwrap();
    assert_eq!(msgs[2]["role"], "assistant");
    assert_eq!(msgs[3], json!({ "role": "user", "content": "Continue." }));
}

/// Régénérer (ou Réessayer) : le dernier tour est remplacé.
#[test]
fn regenerate_replaces_last_turn() {
    let (ws, _) = workspace();
    let transport = ScriptedTransport::new(vec![
        error_response(402, &[], "Insufficient credits"),
        sse_response(&text_chunks("Fin le 21/10.", 0.0)),
    ]);
    let mut conv = Conversation::new();
    let mut h = host(&ws);
    conv.send("Fin ?", config(), &transport).unwrap();
    while conv.poll(&mut h, &transport) {}
    assert_eq!(
        last_state(&conv),
        TurnState::Failed(AssistantError::PaymentRequired)
    );
    conv.regenerate(config(), &transport).unwrap();
    while conv.poll(&mut h, &transport) {}
    assert_eq!(conv.turns().len(), 1);
    assert_eq!(conv.turns()[0].question, "Fin ?");
    assert_eq!(conv.turns()[0].answer, "Fin le 21/10.");
    let second = body(&transport.requests()[1]);
    assert_eq!(
        second["messages"].as_array().unwrap().len(),
        2,
        "système et question"
    );
}

/// Persistance : la conversation gardée (et l'ensemble des conversations)
/// ne contient ni la clé ni le contexte de l'interface ; relue, elle
/// reprend ses tours et sert d'historique.
#[test]
fn no_secret_in_archive() {
    let (ws, _) = workspace();
    let transport = ScriptedTransport::new(vec![
        sse_response(&tool_call_chunks("c1", "project_summary", "{}")),
        sse_response(&text_chunks("Le projet compte 3 tâches.", 0.002)),
        sse_response(&text_chunks("Toujours 3.", 0.0)),
    ]);
    let mut conv = Conversation::new();
    let mut h = host(&ws);
    let mut cfg = config();
    cfg.context = Some("[Interface] CONTEXTE-PRIVE".to_owned());
    conv.send("Combien de tâches ?", cfg.clone(), &transport).unwrap();
    while conv.poll(&mut h, &transport) {}
    let saved = conv.saved();
    let mut archive = Archive::default();
    archive.put_active("file:/maison.xml", saved);
    let json = archive.to_json();
    assert!(!json.contains(KEY) && !json.contains("sk-or"), "{json}");
    assert!(!json.contains("CONTEXTE-PRIVE"));
    assert!(!format!("{conv:?}").contains(KEY));
    // Relue : mêmes tours, et l'historique repart avec la question suivante.
    let again = Archive::from_json(&json);
    let restored: SavedConversation = again.active("file:/maison.xml").unwrap().clone();
    let mut conv = Conversation::restore(restored);
    assert_eq!(conv.turns()[0].answer, "Le projet compte 3 tâches.");
    conv.send("Et maintenant ?", cfg, &transport).unwrap();
    while conv.poll(&mut h, &transport) {}
    let third = body(&transport.requests()[2]);
    assert_eq!(third["messages"][1]["content"], "Combien de tâches ?");
}
