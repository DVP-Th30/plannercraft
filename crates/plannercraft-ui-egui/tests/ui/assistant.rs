//! Volet Assistant (ADR-0024 ; lot A1, H-UI-138 à H-UI-142 ; lot A1b,
//! H-UI-143 à H-UI-148) sans réseau : transport scripté (réponses SSE
//! écrites à la main d'après la documentation d'OpenRouter), clé en
//! mémoire, navigateur simulé.

use std::sync::Arc;
use std::time::Duration;

use egui::{Event, Key};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_assistant::http::HttpEvent;
use plannercraft_assistant::testing::{
    ScriptedTransport, error_response, reasoning_chunks, sse_response, text_chunks, tool_call_chunks,
};
use plannercraft_ui_egui::App;
use plannercraft_ui_egui::action::Action;
use plannercraft_ui_egui::assistant::{AssistantOp, AssistantServices, ScrollMode};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::view::side_panel::{SideOp, SideTab};
use serde_json::{Value, json};

use crate::{click, click_where, fr, harness, open_demo};

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

/// Ouvre le menu « … » des options de l'assistant.
fn open_options(harness: &mut Harness<'static, App>) {
    click(harness, &fr(TextId::AssistantMenu));
    harness.run();
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
    // Désactiver (menu des options) : retour à la mention et au bouton
    // d'activation.
    open_options(&mut harness);
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
    // Mémoire seule (pas de trousseau) : « pour cette session » (menu des
    // options).
    open_options(&mut harness);
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
    assert!(
        harness
            .query_all_by_label_contains("30/06/27 d'après le moteur")
            .count()
            >= 1
    );
    // Libellé métier, jamais l'identifiant technique.
    harness.get_by_label("A lu\u{a0}: le résumé du projet");
    assert!(harness.query_by_label_contains("project_summary").is_none());
    // Pied : modèle · jetons · coût (somme des tours d'API).
    harness.get_by_label("Sonnet 5.5 · 2,3 k jetons · 0,005 $");
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
    // Rien reçu : barre d'état « réponse en cours » (« En attente… » après
    // 800 ms), chronomètre.
    harness.get_by_role(egui::accesskit::Role::Status);
    transport.step(4);
    harness.run();
    assert_eq!(streamed(&harness), 1);
    harness.get_by_label(&fr(TextId::AssistantWriting));
    assert!(harness.query_by_label(&fr(TextId::AssistantSend)).is_none());
    click(&mut harness, &fr(TextId::AssistantStop));
    harness.run();
    harness.get_by_label(&fr(TextId::AssistantStopped));
    harness.get_by_label(&fr(TextId::AssistantContinue));
    assert!(!harness.state().assistant().conversation.busy());
    transport.step(20);
    harness.run();
    assert_eq!(
        harness.state().assistant().conversation.turns()[0].answer,
        "un deux "
    );
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
        // Un seul bouton : « Réessayer ».
        harness.get_by_label(&fr(TextId::AssistantRetry));
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

/// Réponses d'un tour riche : raisonnement, trois lectures (une par tour
/// d'API), réponse Markdown (gras, liste).
fn rich_turn() -> Vec<Vec<HttpEvent>> {
    let mut first = reasoning_chunks("Je dois lire le chemin critique, puis les tâches et le résumé.");
    first.extend(tool_call_chunks("c1", "critical_path", "{}"));
    vec![
        sse_response(&first),
        sse_response(&tool_call_chunks("c2", "list_tasks", "{}")),
        sse_response(&tool_call_chunks("c3", "project_summary", "{}")),
        sse_response(&text_chunks(RICH_ANSWER, 0.0071)),
    ]
}

const RICH_ANSWER: &str = "Le **chemin critique** passe par trois lots :\n\n- les études et le permis ;\n- \
     le terrassement et les fondations ;\n- le gros œuvre.\n\nUn retard sur l'une de ces tâches décale \
     d'autant la **fin du projet**.";

/// Longue réponse : `n` lignes de liste.
fn long_text(n: usize) -> String {
    use std::fmt::Write as _;
    let mut text = String::new();
    for i in 1..=n {
        let _ = writeln!(text, "- ligne {i} de la réponse");
    }
    text
}

/// H-UI-143 : état vide (transparence, suggestions qui préremplissent la
/// saisie).
#[test]
fn h_ui_143_empty_state_suggestions() {
    let transport = Arc::new(ScriptedTransport::new(vec![]));
    let mut harness = assistant_harness(transport);
    enable_with_key(&mut harness);
    harness.get_by_label(&fr(TextId::AssistantEmptyTitle));
    harness.get_by_label(&fr(TextId::AssistantDisclosure));
    click(&mut harness, &fr(TextId::AssistantSuggestCritical));
    harness.run();
    assert_eq!(
        harness.state().assistant().draft,
        fr(TextId::AssistantSuggestCritical)
    );
}

/// H-UI-143 : tour riche — réflexion repliée (dépliable), trois lectures
/// regroupées au libellé métier, réponse Markdown, pied ; jamais
/// l'identifiant d'un outil ni de JSON.
#[test]
fn h_ui_143_turn_layout() {
    let transport = Arc::new(ScriptedTransport::new(rich_turn()));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "Quel est le chemin critique ?");
    settle(&mut harness);
    let turn = harness.state().assistant().conversation.turns()[0].clone();
    assert_eq!(turn.tools.len(), 3);
    assert!(turn.reasoning.starts_with("Je dois lire"));
    // Réflexion repliée : son texte n'est pas affiché.
    let reasoning = harness.query_all_by_label_contains("Réflexion").count();
    assert_eq!(reasoning, 1);
    assert!(harness.query_by_label_contains("Je dois lire").is_none());
    // Lectures regroupées.
    let group = Texts::new(Language::Fr).get_args(TextId::AssistantToolGroup, &[("count", Arg::Count(3))]);
    harness.get_by_label(&group);
    assert!(harness.query_by_label("A lu\u{a0}: le chemin critique").is_none());
    click(&mut harness, &group);
    harness.get_by_label("A lu\u{a0}: le chemin critique");
    harness.get_by_label("A lu\u{a0}: la liste des tâches");
    for technical in ["critical_path", "list_tasks", "project_summary", "{}"] {
        assert!(
            harness.query_by_label_contains(technical).is_none(),
            "{technical} affiché"
        );
    }
    // Markdown : pas de marques `**`.
    assert!(harness.query_by_label_contains("**").is_none());
    assert!(harness.query_all_by_label_contains("chemin critique").count() >= 1);
    // Déplier la réflexion.
    click_where(&mut harness, |l| l.starts_with("Réflexion"));
    assert!(harness.query_all_by_label_contains("Je dois lire").count() >= 1);
    harness.get_by_label_contains("Sonnet 5.5 · ");
}

/// Position haute de la question `q` à l'écran.
fn question_top(harness: &Harness<'static, App>, q: &str) -> f32 {
    harness.get_by_label(q).rect().top()
}

fn view_top(harness: &Harness<'static, App>) -> f32 {
    harness.state().assistant().scroll.layout.view_rect.unwrap().top()
}

/// H-UI-144 : défilement (cahier § 2). La question envoyée monte en haut
/// de la vue et y reste pendant tout le flux (la vue ne bouge pas quand du
/// texte arrive) ; la pastille apparaît quand la réponse dépasse ; un geste
/// de molette rend la main (Free) ; un clic sur la pastille suit le bas.
#[test]
fn h_ui_144_question_anchored_during_stream() {
    let transport = Arc::new(ScriptedTransport::manual(vec![
        sse_response(&text_chunks(&long_text(30), 0.0)),
        sse_response(&text_chunks(&long_text(120), 0.0)),
    ]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "Question un ?");
    transport.step(10_000);
    settle(&mut harness);
    ask(&mut harness, "Question deux ?");
    harness.run();
    assert_eq!(harness.state().assistant().scroll.mode, ScrollMode::Anchored(1));
    transport.step(8);
    harness.run();
    let anchored = question_top(&harness, "Question deux ?");
    let target = view_top(&harness) + 8.0;
    assert!(
        (anchored - target).abs() <= 1.5,
        "question à {anchored}, attendue à {target}"
    );
    // Le texte arrive : la question ne bouge pas.
    for _ in 0..12 {
        transport.step(25);
        harness.run();
        let top = question_top(&harness, "Question deux ?");
        assert!(
            (top - anchored).abs() <= 0.5,
            "la vue a bougé : {top} ≠ {anchored}"
        );
        assert_eq!(harness.state().assistant().scroll.mode, ScrollMode::Anchored(1));
    }
    // La réponse dépasse la vue : pastille « Suite de la réponse » après
    // 300 ms.
    harness.run_steps(3);
    harness.get_by_label(&fr(TextId::AssistantJumpStream));
    // Geste de molette sur la zone : Free, la vue n'est plus déplacée.
    harness.get_by_label("Question deux ?").hover();
    harness.event(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -60.0),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::NONE,
    });
    harness.run();
    assert_eq!(harness.state().assistant().scroll.mode, ScrollMode::Free);
    let offset = harness.state().assistant().scroll.layout.offset;
    transport.step(25);
    harness.run();
    let after = harness.state().assistant().scroll.layout.offset;
    assert!((after - offset).abs() <= 0.5, "{after} ≠ {offset}");
    // Clic sur la pastille : suit le bas pendant le flux.
    click(&mut harness, &fr(TextId::AssistantJumpStream));
    assert_eq!(harness.state().assistant().scroll.mode, ScrollMode::Following);
    transport.step(40);
    harness.run();
    harness.run();
    let layout = harness.state().assistant().scroll.layout.clone();
    let bottom = layout.content_height - layout.view_height;
    assert!(
        (layout.offset - bottom).abs() <= 1.5,
        "{} ≠ {bottom}",
        layout.offset
    );
}

/// H-UI-144 : redimensionner le volet en mode ancré réancre la question.
#[test]
fn h_ui_144_resize_reanchors() {
    let transport = Arc::new(ScriptedTransport::manual(vec![
        sse_response(&text_chunks(&long_text(30), 0.0)),
        sse_response(&text_chunks(&long_text(10), 0.0)),
    ]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "Question un ?");
    transport.step(10_000);
    settle(&mut harness);
    ask(&mut harness, "Question deux ?");
    transport.step(10_000);
    settle(&mut harness);
    harness.set_size(egui::vec2(1280.0, 600.0));
    harness.run();
    harness.run();
    let top = question_top(&harness, "Question deux ?");
    assert!((top - (view_top(&harness) + 8.0)).abs() <= 1.5);
}

/// H-UI-145 : conversation gardée par document, hors de `UiState` et de
/// `app.ui_state` ; relue à la réouverture (dernière question en haut) ;
/// « Nouvelle conversation » archive ; aucun secret.
#[test]
fn h_ui_145_conversation_per_document() {
    let transport = Arc::new(ScriptedTransport::new(vec![sse_response(&text_chunks(
        "Le projet compte 49 tâches.",
        0.001,
    ))]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "Combien de tâches ?");
    settle(&mut harness);
    let json = harness.state_mut().assistant_archive_json();
    assert!(json.contains("Combien de tâches ?"), "{json}");
    assert!(!json.contains(KEY) && !json.contains("sk-or"), "{json}");
    let app = harness.state();
    assert!(!app.state.to_json().unwrap().contains("Combien de tâches"));
    let ui_state = app.ui_state_json().to_string();
    assert!(
        !ui_state.contains("Combien de tâches") && !ui_state.contains("49 tâches"),
        "{ui_state}"
    );
    // Fermer puis rouvrir le document : la conversation revient.
    harness.state_mut().dispatch(Action::CloseDocument);
    harness.run();
    assert!(harness.state().assistant().conversation.is_empty());
    open_demo(&mut harness);
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::Assistant)));
    harness.run();
    harness.run();
    assert_eq!(harness.state().assistant().conversation.turns().len(), 1);
    assert_eq!(harness.state().assistant().scroll.mode, ScrollMode::Anchored(0));
    harness.get_by_label("Combien de tâches ?");
    // Relue par une autre session de l'application (stockage local).
    let mut other = assistant_harness(Arc::new(ScriptedTransport::new(vec![])));
    other.state_mut().load_assistant_archive(&json);
    other.run();
    other.run();
    assert_eq!(other.state().assistant().conversation.turns().len(), 1);
    // « Nouvelle conversation » : archivée, plus relue.
    click(&mut harness, &fr(TextId::AssistantClear));
    harness.run();
    assert!(harness.state().assistant().conversation.is_empty());
    let archive = harness.state().assistant().archive();
    assert_eq!(archive.entries.len(), 1);
    assert!(!archive.entries[0].active);
    assert!(archive.active("demo").is_none());
}

/// H-UI-147 : « Réduire les animations » (menu Affichage → Thème) coupe
/// les animations d'egui ; préférence gardée, absente d'`app.ui_state`.
#[test]
fn h_ui_147_reduce_motion() {
    let mut harness = harness();
    // kittest coupe déjà les animations : une durée posée ici sert de
    // témoin.
    harness.ctx.all_styles_mut(|style| style.animation_time = 0.1);
    open_demo(&mut harness);
    // Menu Affichage → Thème → « Réduire les animations ».
    click(&mut harness, &fr(TextId::MenuView));
    let label = fr(TextId::MenuTheme);
    {
        let theme = harness.get_by_label(&label);
        theme.hover();
        theme.click();
    }
    harness.run();
    click(&mut harness, &fr(TextId::ReduceMotion));
    harness.run();
    assert_eq!(harness.ctx.global_style().animation_time, 0.0);
    assert!(harness.state().state.reduce_motion);
    assert!(
        harness
            .state()
            .state
            .to_json()
            .unwrap()
            .contains("\"reduce_motion\":true")
    );
    assert!(
        !harness
            .state()
            .ui_state_json()
            .to_string()
            .contains("reduce_motion")
    );
    harness.state_mut().dispatch(Action::SetReduceMotion(false));
    harness.run();
    assert_eq!(harness.ctx.global_style().animation_time, 0.1);
}

/// H-UI-148 : clavier — Tab depuis la saisie atteint Copier et Régénérer
/// (sans souris), Entrée les déclenche ; Échap arrête la réponse.
#[test]
fn h_ui_148_keyboard_actions() {
    let transport = Arc::new(ScriptedTransport::new(vec![
        sse_response(&text_chunks("Réponse courte.", 0.0)),
        sse_response(&text_chunks("Autre réponse.", 0.0)),
    ]));
    let mut harness = assistant_harness(Arc::clone(&transport));
    enable_with_key(&mut harness);
    ask(&mut harness, "Question ?");
    settle(&mut harness);
    harness
        .get_by_role_and_label(
            egui::accesskit::Role::MultilineTextInput,
            &fr(TextId::AssistantPlaceholder),
        )
        .focus();
    harness.run();
    let copy = plannercraft_ui_egui::view::assistant::copy_id(0);
    let regenerate = plannercraft_ui_egui::view::assistant::regenerate_id();
    let mut reached_copy = false;
    for _ in 0..40 {
        harness.key_press(Key::Tab);
        harness.run();
        if harness.ctx.memory(|m| m.has_focus(copy)) {
            reached_copy = true;
            break;
        }
    }
    assert!(reached_copy, "Copier inaccessible au clavier");
    harness.key_press(Key::Enter);
    harness.run();
    assert!(harness.state().assistant().copied.is_some());
    harness.key_press(Key::Tab);
    harness.run();
    assert!(harness.ctx.memory(|m| m.has_focus(regenerate)));
    harness.key_press(Key::Enter);
    settle(&mut harness);
    assert_eq!(transport.requests().len(), 2);
    assert_eq!(
        harness.state().assistant().conversation.turns()[0].answer,
        "Autre réponse."
    );
    // Échap arrête une réponse en cours (saisie active).
    let manual = Arc::new(ScriptedTransport::manual(vec![sse_response(&text_chunks(
        "un deux trois",
        0.0,
    ))]));
    let mut harness = assistant_harness(Arc::clone(&manual));
    enable_with_key(&mut harness);
    ask(&mut harness, "compte");
    harness
        .get_by_role_and_label(
            egui::accesskit::Role::MultilineTextInput,
            &fr(TextId::AssistantPlaceholder),
        )
        .focus();
    harness.run();
    assert!(harness.state().assistant().conversation.busy());
    harness.key_press(Key::Escape);
    harness.run();
    assert!(!harness.state().assistant().conversation.busy());
}

/// Harnais de capture : volet Assistant activé, clé saisie, exemple ouvert.
fn chat_capture_harness(
    transport: Arc<ScriptedTransport>,
    language: plannercraft_ui_egui::LanguagePref,
    theme: plannercraft_ui_egui::ThemePref,
) -> Harness<'static, App> {
    let mut harness = crate::common::harness(language, theme);
    harness
        .state_mut()
        .set_assistant_services(AssistantServices::with_transport(transport as _));
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
    harness
}

fn send_now(harness: &mut Harness<'static, App>, question: &str) {
    question.clone_into(&mut harness.state_mut().assistant_mut().draft);
    harness.state_mut().dispatch(Action::Assistant(AssistantOp::Send));
}

/// Capture du volet activé, état vide (clair).
#[test]
fn snapshot_assistant_fr_light() {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let transport = Arc::new(ScriptedTransport::new(vec![]));
    let mut harness = chat_capture_harness(transport, LanguagePref::Fr, ThemePref::Light);
    for _ in 0..4 {
        harness.run();
    }
    crate::common::snapshot(&mut harness, "assistant_fr_light");
}

/// Capture : conversation de deux tours terminée ; le second (réflexion
/// repliée, trois lectures regroupées, liste et gras, pied) tenu en haut
/// de la vue.
#[test]
fn snapshot_assistant_chat_fr_light() {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let mut responses = vec![sse_response(&text_chunks(
        "Le projet compte 49 tâches, de l'étude au gros œuvre.",
        0.0012,
    ))];
    responses.extend(rich_turn());
    let transport = Arc::new(ScriptedTransport::new(responses));
    let mut harness = chat_capture_harness(transport, LanguagePref::Fr, ThemePref::Light);
    send_now(&mut harness, "Combien de tâches compte le projet ?");
    for _ in 0..6 {
        harness.run();
    }
    send_now(&mut harness, "Quel est le chemin critique ?");
    for _ in 0..12 {
        harness.run();
    }
    assert!(!harness.state().assistant().conversation.busy());
    crate::common::snapshot(&mut harness, "assistant_chat_fr_light");
}

/// Capture : même conversation en anglais, thème sombre, pendant le flux
/// de la seconde réponse (question ancrée en haut, barre d'état, Arrêter).
#[test]
fn snapshot_assistant_chat_en_dark() {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let mut responses = vec![sse_response(&text_chunks(
        "The project has 49 tasks, from design to structural work.",
        0.0012,
    ))];
    let mut first = reasoning_chunks("I need the critical path, then the tasks and the summary.");
    first.extend(tool_call_chunks("c1", "critical_path", "{}"));
    responses.push(sse_response(&first));
    responses.push(sse_response(&tool_call_chunks("c2", "list_tasks", "{}")));
    responses.push(sse_response(&tool_call_chunks("c3", "project_summary", "{}")));
    responses.push(sse_response(&text_chunks(
        "The **critical path** runs through three work packages:\n\n- design and permit;\n- \
         earthworks and foundations;\n- structural work.\n\nAny delay on one of these tasks moves the \
         **project finish** by as much.",
        0.0071,
    )));
    let transport = Arc::new(ScriptedTransport::manual(responses));
    let mut harness = chat_capture_harness(Arc::clone(&transport), LanguagePref::En, ThemePref::Dark);
    send_now(&mut harness, "How many tasks does the project have?");
    harness.run();
    transport.step(10_000);
    for _ in 0..6 {
        harness.run();
    }
    send_now(&mut harness, "What is the critical path?");
    harness.run();
    // Réflexion, trois lectures, puis la moitié de la réponse.
    for _ in 0..3 {
        transport.step(10_000);
        harness.run();
        harness.run();
    }
    transport.step(16);
    for _ in 0..3 {
        harness.run();
    }
    assert!(
        harness.state().assistant().conversation.busy(),
        "{:#?}",
        harness.state().assistant().conversation.turns()
    );
    crate::common::snapshot(&mut harness, "assistant_chat_en_dark");
}
