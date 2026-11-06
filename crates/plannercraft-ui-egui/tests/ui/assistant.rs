//! Volet Assistant (ADR-0024, lot A1, H-UI-138 à H-UI-142) sans réseau :
//! transport scripté (réponses SSE écrites à la main d'après la
//! documentation d'OpenRouter), clé en mémoire, navigateur simulé.

use std::sync::Arc;
use std::time::Duration;

use egui::{Event, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_assistant::http::HttpEvent;
use plannercraft_assistant::testing::{
    ScriptedTransport, error_response, sse_response, text_chunks, tool_call_chunks,
};
use plannercraft_ui_egui::App;
use plannercraft_ui_egui::action::Action;
use plannercraft_ui_egui::assistant::{AssistantOp, AssistantServices};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::view::side_panel::{SideOp, SideTab};
use serde_json::{Value, json};

use crate::{click, fr, harness, open_demo};

const KEY: &str = "sk-or-v1-0123456789abcdef-test";

/// Exemple ouvert, volet sur l'onglet Assistant, services sur `transport`.
fn assistant_harness(transport: Arc<ScriptedTransport>) -> Harness<'static, App> {
    let mut harness = harness();
    harness
        .state_mut()
        .set_assistant_services(AssistantServices::with_transport(transport));
    open_demo(&mut harness);
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::Assistant)));
    harness.run();
    harness
}

/// Active l'assistant et saisit la clé (connexion simulée).
fn enable_with_key(harness: &mut Harness<'static, App>) {
    click(harness, &fr(TextId::AssistantEnable));
    harness.run();
    KEY.clone_into(&mut harness.state_mut().assistant_mut().key_draft);
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::SaveKey));
    harness.run();
    harness.run();
    assert!(harness.state().assistant().connected());
}

/// Écrit `question` et clique Envoyer.
fn ask(harness: &mut Harness<'static, App>, question: &str) {
    question.clone_into(&mut harness.state_mut().assistant_mut().draft);
    harness.run();
    click(harness, &fr(TextId::AssistantSend));
    harness.run();
}

/// Mène la conversation jusqu'à la fin (transport immédiat).
fn settle(harness: &mut Harness<'static, App>) {
    for _ in 0..20 {
        harness.run();
        if !harness.state().assistant().conversation.busy() {
            break;
        }
    }
    assert!(!harness.state().assistant().conversation.busy());
}

fn body(transport: &ScriptedTransport, i: usize) -> Value {
    serde_json::from_slice(&transport.requests()[i].body).unwrap()
}

/// H-UI-138 : rien n'est envoyé ni lu avant l'activation explicite, qui
/// affiche la mention « vous échangez avec une IA » (AI Act art. 50).
#[test]
fn h_ui_138_explicit_activation_with_disclosure() {
    let transport = Arc::new(ScriptedTransport::new(vec![]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    assert!(!harness.state().state.assistant.enabled);
    harness.get_by_label(&fr(TextId::AssistantDisclosure));
    assert!(harness.query_by_label(&fr(TextId::AssistantLogin)).is_none());
    click(&mut harness, &fr(TextId::AssistantEnable));
    harness.run();
    assert!(harness.state().state.assistant.enabled);
    // Mention toujours visible une fois activé ; connexion proposée.
    harness.get_by_label(&fr(TextId::AssistantDisclosure));
    harness.get_by_label(&fr(TextId::AssistantLogin));
    harness.get_by_label(&fr(TextId::AssistantEnterKey));
    assert_eq!(transport.requests().len(), 0);
    // Désactiver : retour à la mention et au bouton d'activation.
    click(&mut harness, &fr(TextId::AssistantDisable));
    harness.run();
    assert!(!harness.state().state.assistant.enabled);
    harness.get_by_label(&fr(TextId::AssistantEnable));
}

/// H-UI-138 : Affichage → Assistant IA (ruban et menu) ouvre et referme
/// l'onglet.
#[test]
fn h_ui_138_ribbon_toggles_assistant_tab() {
    let mut harness = harness();
    open_demo(&mut harness);
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::ToggleTab(SideTab::Assistant)));
    harness.run();
    assert!(harness.state().side_panel().open);
    assert_eq!(harness.state().side_panel().tab, SideTab::Assistant);
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::ToggleTab(SideTab::Assistant)));
    harness.run();
    assert!(!harness.state().side_panel().open);
    // Depuis un autre onglet ouvert : bascule vers l'Assistant.
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::AiOperations)));
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::ToggleTab(SideTab::Assistant)));
    harness.run();
    assert_eq!(harness.state().side_panel().tab, SideTab::Assistant);
    assert!(harness.state().side_panel().open);
}

/// H-UI-139 : saisie d'une clé (champ masqué) ; la clé n'est ni dans
/// l'état d'interface sérialisé, ni dans `app.ui_state`, ni dans le texte
/// de débogage ; déconnexion.
#[test]
fn h_ui_139_key_never_in_ui_state() {
    let transport = Arc::new(ScriptedTransport::new(vec![]));
    let mut harness = assistant_harness(transport);
    click(&mut harness, &fr(TextId::AssistantEnable));
    harness.run();
    click(&mut harness, &fr(TextId::AssistantEnterKey));
    harness.run();
    harness
        .get_by_role_and_label(egui::accesskit::Role::TextInput, &fr(TextId::AssistantKeyHint))
        .focus();
    harness.run();
    harness.event(Event::Text(KEY.to_owned()));
    harness.run();
    assert_eq!(harness.state().assistant().key_draft, KEY);
    click(&mut harness, &fr(TextId::AssistantKeySave));
    harness.run();
    harness.run();
    let app = harness.state();
    assert!(app.assistant().connected());
    assert_eq!(app.assistant().key_draft, "");
    let json = app.state.to_json().unwrap();
    assert!(!json.contains(KEY) && !json.contains("sk-or"), "{json}");
    assert!(json.contains("\"assistant\":{\"enabled\":true"), "{json}");
    let ui_state = app.ui_state_json().to_string();
    assert!(
        !ui_state.contains("sk-or") && !ui_state.contains("assistant"),
        "{ui_state}"
    );
    assert!(!format!("{:?}", app.assistant()).contains("sk-or"));
    // Mémoire seule (pas de trousseau) : « pour cette session ».
    harness.get_by_label(&fr(TextId::AssistantConnectedSession));
    click(&mut harness, &fr(TextId::AssistantLogout));
    harness.run();
    assert!(!harness.state().assistant().connected());
    harness.get_by_label(&fr(TextId::AssistantLogin));
}

/// H-UI-139 : sans clé, Envoyer ne part pas (message en langage
/// d'utilisateur).
#[test]
fn h_ui_139_refused_without_key() {
    let transport = Arc::new(ScriptedTransport::new(vec![]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    click(&mut harness, &fr(TextId::AssistantEnable));
    harness.run();
    harness.state_mut().assistant_mut().draft = "Quand finit le projet ?".to_owned();
    harness.state_mut().dispatch(Action::Assistant(AssistantOp::Send));
    harness.run();
    assert_eq!(transport.requests().len(), 0);
    harness.get_by_label(&fr(TextId::AssistantErrorNoKey));
}

/// Navigateur simulé : suit la redirection vers l'écouteur local comme le
/// ferait OpenRouter après l'autorisation.
#[derive(Default)]
struct FakeBrowser(std::sync::Mutex<Vec<String>>);

impl plannercraft_assistant::platform::BrowserOpener for FakeBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        use std::io::{Read as _, Write as _};
        self.0.lock().unwrap().push(url.to_owned());
        let query = url.split_once('?').unwrap().1;
        let callback = plannercraft_assistant::pkce::query_param(query, "callback_url").unwrap();
        let state = plannercraft_assistant::pkce::query_param(query, "state").unwrap();
        let port: u16 = callback
            .trim_start_matches("http://localhost:")
            .trim_end_matches("/callback")
            .parse()
            .unwrap();
        // Fil de test (natif) : la redirection arrive pendant l'attente.
        let _client = std::thread::Builder::new().spawn(move || {
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(
                stream,
                "GET /callback?code=oauth-code&state={state} HTTP/1.1\r\n\r\n"
            )
            .unwrap();
            let mut page = String::new();
            let _ = stream.read_to_string(&mut page);
        });
        Ok(())
    }
}

/// H-UI-139 : « Se connecter à OpenRouter » (PKCE), navigateur et
/// OpenRouter simulés : écouteur local, échange du code, clé reçue.
#[test]
fn h_ui_139_login_simulated() {
    let transport = Arc::new(ScriptedTransport::new(vec![vec![
        HttpEvent::Head {
            status: 200,
            headers: vec![],
        },
        HttpEvent::Chunk(json!({ "key": KEY }).to_string().into_bytes()),
        HttpEvent::End,
    ]]));
    let browser = Arc::new(FakeBrowser::default());
    let mut harness = harness();
    harness.state_mut().set_assistant_services(
        AssistantServices::with_transport(Arc::clone(&transport) as _)
            .with_browser(Arc::clone(&browser) as _),
    );
    open_demo(&mut harness);
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::Assistant)));
    harness.run();
    click(&mut harness, &fr(TextId::AssistantEnable));
    harness.run();
    click(&mut harness, &fr(TextId::AssistantLogin));
    harness.run();
    for _ in 0..200 {
        harness.run();
        if harness.state().assistant().connected() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        harness.state().assistant().connected(),
        "{:?}",
        harness.state().assistant()
    );
    let url = browser.0.lock().unwrap()[0].clone();
    assert!(url.starts_with("https://openrouter.ai/auth?callback_url=http%3A%2F%2Flocalhost%3A"));
    assert!(url.contains("code_challenge_method=S256"));
    let exchange = body(&transport, 0);
    assert_eq!(exchange["code"], "oauth-code");
    assert!(!harness.state().state.to_json().unwrap().contains(KEY));
}

/// H-UI-141 : appel d'outil exécuté sur l'exemple (lecture), réponse en
/// flux, coût affiché ; le document n'a pas changé.
#[test]
fn h_ui_141_tool_loop_on_demo_read_only() {
    let transport = Arc::new(ScriptedTransport::new(vec![
        sse_response(&tool_call_chunks("call_1", "project_summary", "{}")),
        sse_response(&text_chunks(
            "Le projet finit le 30/06/27 d'après le moteur.",
            0.0042,
        )),
    ]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    let generation = harness.state().document.as_ref().unwrap().generation();
    ask(&mut harness, "Quand finit le projet ?");
    settle(&mut harness);
    // Requête : clé dans l'en-tête seulement, attribution, confidentialité.
    let requests = transport.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].header("authorization"),
        Some(format!("Bearer {KEY}").as_str())
    );
    assert_eq!(requests[0].header("x-openrouter-title"), Some("PlannerCraft"));
    let first = body(&transport, 0);
    assert_eq!(first["model"], "anthropic/claude-sonnet-5.5");
    assert_eq!(
        first["provider"],
        json!({ "zdr": true, "data_collection": "deny" })
    );
    // Le résultat de l'outil vient du moteur, sur le projet ouvert.
    let second = body(&transport, 1);
    let result: Value = serde_json::from_str(second["messages"][3]["content"].as_str().unwrap()).unwrap();
    assert!(
        result.get("finish").is_some() || result.get("name").is_some(),
        "{result}"
    );
    harness.get_by_label("Le projet finit le 30/06/27 d'après le moteur.");
    harness.get_by_label(&fr_args(
        TextId::AssistantToolsUsed,
        &[("detail", "project_summary")],
    ));
    let cost = harness.query_all_by_label_contains("0,0052").count();
    assert_eq!(cost, 1);
    assert_eq!(
        harness.state().document.as_ref().unwrap().generation(),
        generation
    );
    assert!(!harness.state().document.as_ref().unwrap().is_dirty());
}

fn fr_args(id: TextId, args: &[(&'static str, &str)]) -> String {
    let args: Vec<(&str, Arg<'_>)> = args.iter().map(|(k, v)| (*k, Arg::Str((*v).into()))).collect();
    Texts::new(Language::Fr).get_args(id, &args)
}

/// Réponses partielles « un deux » affichées.
fn streamed(harness: &Harness<'static, App>) -> usize {
    harness.query_all_by_label_contains("un deux").count()
}

/// H-UI-141 : réponse affichée au fil du flux, « Arrêter » coupe, le
/// reste du flux est ignoré, une nouvelle question part ensuite.
#[test]
fn h_ui_141_streaming_then_stop() {
    let transport = Arc::new(ScriptedTransport::manual(vec![
        sse_response(&text_chunks("un deux trois quatre", 0.0)),
        sse_response(&text_chunks("encore", 0.0)),
    ]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "compte");
    harness.get_by_label(&fr(TextId::AssistantThinking));
    transport.step(4);
    harness.run();
    assert_eq!(streamed(&harness), 1);
    harness.get_by_label(&fr(TextId::AssistantWriting));
    assert!(harness.query_by_label(&fr(TextId::AssistantSend)).is_none());
    click(&mut harness, &fr(TextId::AssistantStop));
    harness.run();
    harness.get_by_label(&fr(TextId::AssistantStopped));
    assert!(!harness.state().assistant().conversation.busy());
    transport.step(20);
    harness.run();
    assert_eq!(streamed(&harness), 1);
    assert!(harness.query_by_label_contains("quatre").is_none());
    // Entrée envoie la question suivante.
    harness
        .get_by_role_and_label(
            egui::accesskit::Role::MultilineTextInput,
            &fr(TextId::AssistantPlaceholder),
        )
        .focus();
    harness.run();
    harness.event(Event::Text("suite".to_owned()));
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(transport.requests().len(), 2);
    assert_eq!(harness.state().assistant().draft, "");
}

/// H-UI-142 : erreurs en langage d'utilisateur.
#[test]
fn h_ui_142_errors_in_user_language() {
    for (response, id) in [
        (
            error_response(402, &[], "Insufficient credits"),
            TextId::AssistantErrorPayment,
        ),
        (
            error_response(401, &[], "No auth"),
            TextId::AssistantErrorUnauthorized,
        ),
    ] {
        let transport = Arc::new(ScriptedTransport::new(vec![response]));
        let mut harness = assistant_harness(Arc::clone(&transport));
        enable_with_key(&mut harness);
        ask(&mut harness, "x");
        settle(&mut harness);
        harness.get_by_label(&fr(id));
    }
    let transport = Arc::new(ScriptedTransport::new(vec![error_response(
        429,
        &[("Retry-After", "7")],
        "slow",
    )]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "x");
    settle(&mut harness);
    harness.get_by_label(
        &Texts::new(Language::Fr).get_args(TextId::AssistantErrorRateWait, &[("count", Arg::Count(7))]),
    );
    // Coupure réseau.
    let transport = Arc::new(ScriptedTransport::new(vec![vec![HttpEvent::Failed(
        "connection reset".to_owned(),
    )]]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "x");
    settle(&mut harness);
    harness.get_by_label(&fr_args(
        TextId::AssistantErrorNetwork,
        &[("detail", "connection reset")],
    ));
}

/// H-UI-140 : liste fermée de modèles, choix mémorisé dans `UiState` et
/// envoyé dans la requête.
#[test]
fn h_ui_140_model_choice_remembered() {
    let transport = Arc::new(ScriptedTransport::new(vec![sse_response(&text_chunks(
        "ok", 0.0,
    ))]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::SetModel(
            "google/gemini-3.8-flash",
        )));
    harness.run();
    assert_eq!(harness.state().state.assistant.model, "google/gemini-3.8-flash");
    ask(&mut harness, "x");
    settle(&mut harness);
    assert_eq!(body(&transport, 0)["model"], "google/gemini-3.8-flash");
    // Un identifiant hors liste dans l'état relu retombe sur le défaut.
    harness.state_mut().state.assistant.model = "openai/autre".to_owned();
    transport.push(sse_response(&text_chunks("ok", 0.0)));
    ask(&mut harness, "y");
    settle(&mut harness);
    assert_eq!(body(&transport, 1)["model"], "anthropic/claude-sonnet-5.5");
}

/// Capture du volet activé, conversation d'exemple simulée (clair).
#[test]
fn snapshot_assistant_fr_light() {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let transport = Arc::new(ScriptedTransport::new(vec![
        sse_response(&tool_call_chunks("call_1", "critical_path", "{}")),
        sse_response(&text_chunks(
            "D'après critical_path, le chemin critique passe par les études, les fondations, \
             le gros œuvre puis la charpente : un retard sur l'une de ces tâches retarde la fin du projet.",
            0.0061,
        )),
    ]));
    let mut harness = crate::common::harness(LanguagePref::Fr, ThemePref::Light);
    harness
        .state_mut()
        .set_assistant_services(AssistantServices::with_transport(Arc::clone(&transport) as _));
    harness.run();
    harness.state_mut().open_demo();
    harness.run();
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::Assistant)));
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::Enable));
    harness.run();
    KEY.clone_into(&mut harness.state_mut().assistant_mut().key_draft);
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::SaveKey));
    harness.run();
    harness.state_mut().assistant_mut().draft = "Quelles tâches sont critiques ?".to_owned();
    harness.state_mut().dispatch(Action::Assistant(AssistantOp::Send));
    for _ in 0..10 {
        harness.run();
    }
    assert!(!harness.state().assistant().conversation.busy());
    crate::common::snapshot(&mut harness, "assistant_fr_light");
}
