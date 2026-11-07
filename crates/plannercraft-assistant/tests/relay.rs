//! Mode « Compte PlannerCraft » (lot A4) sans réseau : conversation menée
//! par le faux relais ([`FakeRelay`]) sur un vrai projet ; essai réel
//! facultatif contre un relais local.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::expect_used,
    reason = "tests : un échec doit s'arrêter net"
)]

use plannercraft_assistant::conversation::{Conversation, Endpoint, RequestConfig, TurnState};
use plannercraft_assistant::error::AssistantError;
use plannercraft_assistant::relay;
use plannercraft_assistant::testing::{FAKE_SESSION, FakeRelay, text_chunks, tool_call_chunks};
use plannercraft_assistant::tools::SessionHost;
use plannercraft_automation::text::Language;
use plannercraft_automation::{CallEnv, Workspace};
use plannercraft_core::Moment;
use serde_json::{Value, json};

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

fn workspace() -> Workspace {
    let mut ws = Workspace::new(now(), Language::Fr);
    ws.call(
        "project.new",
        json!({ "name": "Maison", "start": "2026-10-19T08:00" }),
    )
    .unwrap();
    ws
}

fn host(ws: &Workspace) -> SessionHost<'_> {
    SessionHost::new(ws.session(), CallEnv::new(now(), Language::Fr), json!({}))
}

fn config(token: &str) -> RequestConfig {
    RequestConfig {
        endpoint: Endpoint::relay("https://relay.test/"),
        ..RequestConfig::new(token, "anthropic/claude-haiku-5.5", Language::Fr, 512)
    }
}

/// Fake relay with a signed-in session.
fn signed_in(balance: i64) -> FakeRelay {
    let relay = FakeRelay::new(balance);
    let exchange = relay::exchange_token_request(
        "https://relay.test",
        plannercraft_assistant::testing::FAKE_LINK_TOKEN,
    );
    let pending = plannercraft_assistant::login::PendingResponse::send(&relay, exchange, None);
    let (status, headers, body) = pending.poll().unwrap().unwrap();
    let session = relay::read_exchange(status, &headers, &body).unwrap();
    assert_eq!(session.token, FAKE_SESSION);
    relay
}

fn run(conv: &mut Conversation, ws: &Workspace, relay: &FakeRelay) {
    for _ in 0..10 {
        let mut h = host(ws);
        conv.poll(&mut h, relay);
        if !conv.busy() {
            break;
        }
    }
}

/// Deux complétions (outil puis texte) : même identifiant de demande,
/// tours 1 et 2, pas de `provider`, coût en centimes additionné, solde lu
/// dans le commentaire du relais, coût en dollars d'OpenRouter écarté.
#[test]
fn relayed_chat_reads_cost_and_balance() {
    let ws = workspace();
    let relay = signed_in(437);
    relay.push_chat(tool_call_chunks("c1", "project_summary", "{}"), 1);
    relay.push_chat(text_chunks("Le projet est vide.", 0.002), 2);
    let mut conv = Conversation::new();
    conv.send("Résume le projet", config(FAKE_SESSION), &relay)
        .unwrap();
    run(&mut conv, &ws, &relay);
    let turn = conv.turns().last().unwrap();
    assert_eq!(turn.state, TurnState::Done, "{:?}", turn.state);
    assert_eq!(turn.answer, "Le projet est vide.");
    let usage = turn.usage.unwrap();
    assert_eq!(usage.cost_cents, Some(3));
    assert_eq!(
        usage.cost, None,
        "coût en dollars d'OpenRouter jamais montré en mode compte"
    );
    assert_eq!(conv.take_balance(), Some(434));
    assert_eq!(conv.take_balance(), None);
    let chats: Vec<_> = relay
        .requests()
        .into_iter()
        .filter(|r| r.url.ends_with("/v1/assistant/chat"))
        .collect();
    assert_eq!(chats.len(), 2);
    let id = chats[0].header("x-plannercraft-request").unwrap().to_owned();
    assert_eq!(id.len(), 36);
    assert_eq!(chats[1].header("x-plannercraft-request"), Some(id.as_str()));
    assert_eq!(chats[0].header("x-plannercraft-turn"), Some("1"));
    assert_eq!(chats[1].header("x-plannercraft-turn"), Some("2"));
    for chat in &chats {
        assert_eq!(chat.url, "https://relay.test/v1/assistant/chat");
        let body: Value = serde_json::from_slice(&chat.body).unwrap();
        assert!(body.get("provider").is_none());
        assert_eq!(
            chat.header("authorization"),
            Some(format!("Bearer {FAKE_SESSION}").as_str())
        );
        // Aucune clé OpenRouter dans ce mode.
        assert!(!format!("{:?}", chat.headers).contains("sk-or"));
    }
    // Demande suivante : nouvel identifiant.
    relay.push_chat(text_chunks("Oui.", 0.001), 0);
    conv.send("Et ensuite ?", config(FAKE_SESSION), &relay).unwrap();
    run(&mut conv, &ws, &relay);
    let last = relay.requests().into_iter().last().unwrap();
    assert_ne!(last.header("x-plannercraft-request"), Some(id.as_str()));
    assert_eq!(last.header("x-plannercraft-turn"), Some("1"));
    assert_eq!(conv.turns().last().unwrap().usage.unwrap().cost_cents, Some(0));
}

/// Erreurs du relais traduites ; session refusée signalée une fois ; sans
/// jeton, rien ne part.
#[test]
fn relay_errors_mapped() {
    let ws = workspace();
    let relay = signed_in(10);
    let mut conv = Conversation::new();
    assert_eq!(conv.send("x", config(""), &relay), Err(AssistantError::NoSession));
    relay.set_insufficient(true);
    conv.send("Bonjour", config(FAKE_SESSION), &relay).unwrap();
    run(&mut conv, &ws, &relay);
    assert_eq!(
        conv.turns().last().unwrap().state,
        TurnState::Failed(AssistantError::CreditsExhausted)
    );
    assert!(!conv.take_session_ended());
    relay.set_insufficient(false);
    relay.expire_session();
    conv.send("Bonjour", config(FAKE_SESSION), &relay).unwrap();
    run(&mut conv, &ws, &relay);
    assert_eq!(
        conv.turns().last().unwrap().state,
        TurnState::Failed(AssistantError::SessionEnded { expired: true })
    );
    assert!(conv.take_session_ended());
    assert!(!conv.take_session_ended());
}

/// Essai réel contre un relais local (`PLANNERCRAFT_RELAY_URL`, par
/// exemple `http://127.0.0.1:8080`) avec un jeton de session obtenu à la
/// main (`PLANNERCRAFT_RELAY_SESSION`) : `/v1/me`, puis une complétion
/// relayée dont le coût est lu. Lancer : `cargo test -p
/// plannercraft-assistant --test relay -- --ignored live_relay`.
#[test]
#[ignore = "réseau : relais local et jeton de session requis"]
fn live_relay_round_trip() {
    use plannercraft_assistant::EhttpTransport;
    use plannercraft_assistant::login::PendingResponse;
    let base = relay::normalize_base(&std::env::var(relay::RELAY_URL_ENV).expect("PLANNERCRAFT_RELAY_URL"));
    let token = std::env::var("PLANNERCRAFT_RELAY_SESSION").expect("PLANNERCRAFT_RELAY_SESSION");
    let transport = EhttpTransport;
    let wait = |pending: &PendingResponse| {
        for _ in 0..600 {
            if let Some(result) = pending.poll() {
                return result.unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        panic!("pas de réponse du relais");
    };
    let me = PendingResponse::send(&transport, relay::me_request(&base, &token), None);
    let (status, headers, body) = wait(&me);
    let me = relay::read_me(status, &headers, &body).unwrap();
    assert_ne!(me.models, Vec::new());
    let ws = workspace();
    let mut conv = Conversation::new();
    let config = RequestConfig {
        endpoint: Endpoint::relay(&base),
        ..RequestConfig::new(&token, "anthropic/claude-haiku-5.5", Language::Fr, 512)
    };
    conv.send("Réponds juste « ok ».", config, &transport).unwrap();
    for _ in 0..1200 {
        let mut h = host(&ws);
        conv.poll(&mut h, &transport);
        if !conv.busy() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let turn = conv.turns().last().unwrap();
    assert_eq!(turn.state, TurnState::Done, "{:?}", turn.state);
    assert!(turn.usage.unwrap().cost_cents.is_some());
    assert!(conv.take_balance().is_some());
}
