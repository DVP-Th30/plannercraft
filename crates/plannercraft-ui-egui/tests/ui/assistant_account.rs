//! Mode « Compte PlannerCraft » du volet Assistant (ADR-0024, lot A4,
//! H-UI-155 à H-UI-160) sans réseau : faux relais
//! ([`plannercraft_assistant::testing::FakeRelay`]) qui sert le contrat
//! d'API v1, jeton en mémoire, navigateur simulé.

use std::sync::{Arc, Mutex};

use egui::Event;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_assistant::error::AssistantError;
use plannercraft_assistant::platform::BrowserOpener;
use plannercraft_assistant::testing::{
    FAKE_CHECKOUT_URL, FAKE_CODE, FAKE_LINK_TOKEN, FAKE_SESSION, FakeRelay, text_chunks, tool_call_chunks,
};
use plannercraft_ui_egui::App;
use plannercraft_ui_egui::action::Action;
use plannercraft_ui_egui::assistant::{AccountStage, AssistantOp, AssistantServices, Notice};
use plannercraft_ui_egui::i18n::TextId;
use plannercraft_ui_egui::state::AssistantAccess;
use plannercraft_ui_egui::view::side_panel::{SideOp, SideTab};
use serde_json::Value;

use crate::assistant::{ask, settle};
use crate::{click, fr, harness, open_demo};

const EMAIL: &str = "marie@exemple.fr";

/// Navigateur simulé : garde les adresses ouvertes.
#[derive(Default)]
struct RecordingBrowser(Mutex<Vec<String>>);

impl BrowserOpener for RecordingBrowser {
    fn open(&self, url: &str) -> Result<(), String> {
        self.0.lock().unwrap().push(url.to_owned());
        Ok(())
    }
}

/// Exemple ouvert, volet sur l'onglet Assistant, services sur le faux
/// relais.
fn account_harness(relay: Arc<FakeRelay>, browser: Arc<RecordingBrowser>) -> Harness<'static, App> {
    let mut harness = harness();
    harness
        .state_mut()
        .set_assistant_services(AssistantServices::with_transport(relay).with_browser(browser));
    open_demo(&mut harness);
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::Assistant)));
    harness.run();
    harness
}

/// Attend la fin de la lecture du magasin du jeton (fil de fond).
fn wait_session_loaded(harness: &mut Harness<'static, App>) {
    for _ in 0..50 {
        if !harness.state().assistant().session_loading() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        harness.run();
    }
    assert!(
        !harness.state().assistant().session_loading(),
        "lecture du magasin trop longue"
    );
}

fn run(harness: &mut Harness<'static, App>, frames: usize) {
    for _ in 0..frames {
        harness.run();
    }
}

/// Choisit « Compte PlannerCraft », saisit l'adresse et le code : connecté.
fn sign_in(harness: &mut Harness<'static, App>) {
    click(harness, &fr(TextId::AssistantUseAccount));
    wait_session_loaded(harness);
    harness
        .get_by_role_and_label(
            egui::accesskit::Role::TextInput,
            &fr(TextId::AssistantAccountEmail),
        )
        .focus();
    harness.run();
    harness.event(Event::Text(EMAIL.to_owned()));
    harness.run();
    click(harness, &fr(TextId::AssistantAccountSendLink));
    run(harness, 3);
    harness
        .get_by_role_and_label(
            egui::accesskit::Role::TextInput,
            &fr(TextId::AssistantAccountCode),
        )
        .focus();
    harness.run();
    harness.event(Event::Text("k7qf-3mzd".to_owned()));
    harness.run();
    click(harness, &fr(TextId::AssistantAccountVerify));
    run(harness, 4);
    wait_session_loaded(harness);
    assert_eq!(
        harness.state().assistant().account().stage(),
        AccountStage::SignedIn
    );
}

/// H-UI-155, H-UI-156 : choix de l'accès à l'activation (compte mis en
/// avant), lien magique demandé, code saisi, session ouverte, `/v1/me` lu ;
/// le jeton n'est jamais dans l'état d'interface, `app.ui_state`, le
/// stockage des conversations ni un texte de débogage.
#[test]
fn h_ui_156_sign_in_by_email_code_token_never_stored() {
    let relay = Arc::new(FakeRelay::new(437));
    let mut harness = account_harness(Arc::clone(&relay), Arc::default());
    harness.get_by_label(&fr(TextId::AssistantAccessRecommended));
    harness.get_by_label(&fr(TextId::AssistantUseOwnKey));
    assert!(
        relay.requests().is_empty(),
        "rien n'est envoyé avant l'activation"
    );
    sign_in(&mut harness);
    assert_eq!(relay.mails(), [EMAIL]);
    let routes = relay.routes();
    assert_eq!(
        routes,
        ["Post /v1/auth/magic-link", "Post /v1/auth/exchange", "Get /v1/me"],
        "{routes:?}"
    );
    let exchange: Value = serde_json::from_slice(&relay.requests()[1].body).unwrap();
    assert_eq!(exchange["email"], EMAIL);
    assert_eq!(exchange["code"], "k7qf-3mzd");
    let app = harness.state();
    assert_eq!(app.state.assistant.access, AssistantAccess::Account);
    assert!(app.state.assistant.enabled);
    assert_eq!(app.assistant().account().balance(), Some(437));
    // Solde en tête de l'onglet, mention du relais.
    harness.get_by_label("Solde\u{a0}: 4,37\u{a0}€");
    harness.get_by_label(&fr(TextId::AssistantRelayNotice));
    // Jamais le jeton ailleurs que dans le magasin des secrets.
    let app = harness.state_mut();
    let json = app.state.to_json().unwrap();
    assert!(!json.contains(FAKE_SESSION) && !json.contains("pcs_"), "{json}");
    assert!(json.contains("\"access\":\"account\""), "{json}");
    let ui_state = app.ui_state_json().to_string();
    assert!(
        !ui_state.contains("pcs_") && !ui_state.contains("assistant"),
        "{ui_state}"
    );
    assert!(!format!("{:?}", app.assistant()).contains("pcs_"));
    assert!(!app.assistant_archive_json().contains("pcs_"));
    assert!(!format!("{:?}", relay.requests()).contains("pcs_"));
}

/// H-UI-156 : un code faux est refusé en langage d'utilisateur, l'écran du
/// code reste ; « Changer d'adresse » revient à la saisie de l'adresse.
#[test]
fn h_ui_156_wrong_code_refused() {
    let relay = Arc::new(FakeRelay::new(100));
    let mut harness = account_harness(Arc::clone(&relay), Arc::default());
    click(&mut harness, &fr(TextId::AssistantUseAccount));
    wait_session_loaded(&mut harness);
    EMAIL.clone_into(&mut harness.state_mut().assistant_mut().account_mut().email_draft);
    harness.run();
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::AccountSendLink));
    run(&mut harness, 3);
    "ABCD-EFGH".clone_into(&mut harness.state_mut().assistant_mut().account_mut().code_draft);
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::AccountVerify));
    run(&mut harness, 3);
    let panel = harness.state().assistant();
    assert_eq!(panel.account().stage(), AccountStage::Code(EMAIL.to_owned()));
    assert!(
        matches!(&panel.notice, Some(Notice::Error(AssistantError::Relay { relay_code, .. })) if relay_code == "auth.invalid"),
        "{:?}",
        panel.notice
    );
    harness.get_by_label_contains("Code ou lien refusé");
    click(&mut harness, &fr(TextId::AssistantAccountChangeEmail));
    harness.run();
    assert_eq!(harness.state().assistant().account().stage(), AccountStage::Email);
    assert_eq!(FAKE_CODE.len(), 8);
}

/// H-UI-157 : demande relayée (outil puis texte) : pied en euros, solde mis
/// à jour par le coût annoncé puis relu (`/v1/me`) ; aucune clé
/// OpenRouter, pas de `provider`.
#[test]
fn h_ui_157_relayed_chat_cost_in_euros_balance_updated() {
    let relay = Arc::new(FakeRelay::new(437));
    relay.push_chat(tool_call_chunks("c1", "critical_path", "{}"), 1);
    relay.push_chat(
        text_chunks("Le chemin critique passe par le gros œuvre.", 0.004),
        2,
    );
    let mut harness = account_harness(Arc::clone(&relay), Arc::default());
    sign_in(&mut harness);
    ask(&mut harness, "Quel est le chemin critique ?");
    settle(&mut harness);
    run(&mut harness, 3);
    let app = harness.state();
    let turn = app.assistant().conversation.turns().last().unwrap().clone();
    assert_eq!(turn.answer, "Le chemin critique passe par le gros œuvre.");
    assert_eq!(turn.usage.unwrap().cost_cents, Some(3));
    assert_eq!(turn.usage.unwrap().cost, None);
    assert_eq!(app.assistant().account().balance(), Some(434));
    harness.get_by_label_contains("0,03\u{a0}€");
    harness.get_by_label("Solde\u{a0}: 4,34\u{a0}€");
    let routes = relay.routes();
    assert_eq!(
        routes[3..],
        ["Post /v1/assistant/chat", "Post /v1/assistant/chat", "Get /v1/me"],
        "{routes:?}"
    );
    for request in relay.requests().iter().filter(|r| r.url.ends_with("/chat")) {
        assert!(request.url.starts_with("https://relay.test/"));
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert!(body.get("provider").is_none());
        assert!(!format!("{:?}", request.headers).contains("sk-or"));
        assert_eq!(
            request.header("authorization"),
            Some(format!("Bearer {FAKE_SESSION}").as_str())
        );
    }
}

/// H-UI-158 : crédits épuisés ⇒ « Crédits épuisés » et les packs dans
/// l'erreur ; acheter ⇒ session de paiement, adresse ouverte dans le
/// navigateur ; au retour (Actualiser le solde), solde relu.
#[test]
fn h_ui_158_credits_exhausted_buy_opens_checkout() {
    let relay = Arc::new(FakeRelay::new(0));
    let browser = Arc::new(RecordingBrowser::default());
    let mut harness = account_harness(Arc::clone(&relay), Arc::clone(&browser));
    sign_in(&mut harness);
    relay.set_insufficient(true);
    ask(&mut harness, "Résume le projet");
    settle(&mut harness);
    harness.get_by_label(&fr(TextId::AssistantErrorCredits));
    harness.get_by_label("Solde bas");
    click(&mut harness, "20\u{a0}€ de crédits");
    run(&mut harness, 3);
    assert_eq!(*browser.0.lock().unwrap(), [format!("{FAKE_CHECKOUT_URL}p20")]);
    harness.get_by_label(&fr(TextId::AssistantCheckoutOpened));
    // Paiement fait : le relais crédite, l'utilisateur actualise.
    relay.set_balance(2000);
    relay.set_insufficient(false);
    click(&mut harness, &fr(TextId::AssistantRefreshBalance));
    run(&mut harness, 3);
    assert_eq!(harness.state().assistant().account().balance(), Some(2000));
    harness.get_by_label("Solde\u{a0}: 20,00\u{a0}€");
}

/// H-UI-158 : le retour dans l'application après la page de paiement
/// relit le solde (focus de la fenêtre).
#[test]
fn h_ui_158_balance_refreshed_when_focus_returns() {
    let relay = Arc::new(FakeRelay::new(30));
    let mut harness = account_harness(Arc::clone(&relay), Arc::new(RecordingBrowser::default()));
    sign_in(&mut harness);
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::Checkout("p5")));
    run(&mut harness, 3);
    let before = relay.routes().len();
    relay.set_balance(530);
    harness.state_mut().assistant_mut().set_focused(false);
    harness.state_mut().assistant_mut().set_focused(true);
    run(&mut harness, 3);
    assert_eq!(relay.routes().len(), before + 1);
    assert_eq!(harness.state().assistant().account().balance(), Some(530));
}

/// H-UI-156 : session expirée sur une demande ⇒ retour à l'écran de
/// connexion avec le message ; le jeton est oublié.
#[test]
fn h_ui_156_expired_session_back_to_sign_in() {
    let relay = Arc::new(FakeRelay::new(300));
    let mut harness = account_harness(Arc::clone(&relay), Arc::default());
    sign_in(&mut harness);
    relay.expire_session();
    ask(&mut harness, "Bonjour");
    settle(&mut harness);
    run(&mut harness, 2);
    let panel = harness.state().assistant();
    assert!(!panel.account().signed_in());
    assert_eq!(panel.account().stage(), AccountStage::Email);
    assert_eq!(
        panel.notice,
        Some(Notice::Error(AssistantError::SessionEnded { expired: true }))
    );
    harness.get_by_label(&fr(TextId::AssistantAccountSendLink));
    assert!(
        harness
            .query_all_by_label(&fr(TextId::AssistantErrorSessionExpired))
            .count()
            >= 1
    );
}

/// H-UI-159 : menu « … » : Consommation (liste et total du relais),
/// changement d'accès, déconnexion (jeton révoqué puis oublié).
#[test]
fn h_ui_159_usage_access_switch_and_logout() {
    let relay = Arc::new(FakeRelay::new(437));
    relay.push_chat(text_chunks("Bonjour.", 0.001), 4);
    let mut harness = account_harness(Arc::clone(&relay), Arc::default());
    sign_in(&mut harness);
    ask(&mut harness, "Bonjour");
    settle(&mut harness);
    click(&mut harness, &fr(TextId::AssistantMenu));
    click(&mut harness, &fr(TextId::AssistantUsageMenu));
    run(&mut harness, 3);
    harness.get_by_label(&fr(TextId::AssistantUsageTitle));
    harness.get_by_label("Total\u{a0}: 0,04\u{a0}€");
    click(&mut harness, &fr(TextId::AssistantBack));
    harness.run();
    assert!(!harness.state().assistant().account().usage_open);
    // Changer d'accès : la clé OpenRouter, puis retour au compte (session
    // gardée).
    click(&mut harness, &fr(TextId::AssistantMenu));
    click(&mut harness, &fr(TextId::AssistantAccessOwnKey));
    harness.run();
    assert_eq!(harness.state().state.assistant.access, AssistantAccess::OwnKey);
    // La lecture du trousseau est asynchrone (voir `assistant::wait_key_loaded`).
    super::assistant::wait_key_loaded(&mut harness);
    harness.get_by_label(&fr(TextId::AssistantEnterKey));
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::SetAccess(
            AssistantAccess::Account,
        )));
    run(&mut harness, 2);
    assert!(harness.state().assistant().account().signed_in());
    // Déconnexion.
    click(&mut harness, &fr(TextId::AssistantMenu));
    click(&mut harness, &fr(TextId::AssistantAccountLogout));
    run(&mut harness, 3);
    assert!(relay.revoked());
    assert!(!harness.state().assistant().account().signed_in());
    harness.get_by_label(&fr(TextId::AssistantAccountSendLink));
}

/// Web (H-UI-156) : lien magique `?token=` lu au démarrage de la page :
/// accès « compte » choisi, assistant activé, jeton du lien échangé.
/// (La lecture et le nettoyage de l'adresse sont propres au navigateur :
/// `plannercraft_assistant::web::take_magic_token`.)
#[test]
fn h_ui_156_web_magic_link_signs_in() {
    let relay = Arc::new(FakeRelay::new(50));
    let mut harness = account_harness(Arc::clone(&relay), Arc::default());
    let app = harness.state_mut();
    let mut prefs = app.state.assistant.clone();
    app.assistant_mut().sign_in_with_link(FAKE_LINK_TOKEN, &mut prefs);
    app.state.assistant = prefs;
    run(&mut harness, 4);
    wait_session_loaded(&mut harness);
    run(&mut harness, 2);
    let app = harness.state();
    assert_eq!(app.state.assistant.access, AssistantAccess::Account);
    assert!(app.state.assistant.enabled);
    assert!(app.assistant().account().signed_in());
    assert_eq!(app.assistant().account().balance(), Some(50));
    let exchange: Value = serde_json::from_slice(&relay.requests()[0].body).unwrap();
    assert_eq!(exchange["token"], FAKE_LINK_TOKEN);
}

/// Captures du mode compte : choix de l'accès, saisie du code,
/// conversation (solde en tête, coût en euros).
#[test]
fn snapshot_assistant_account_fr_light() {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let relay = Arc::new(FakeRelay::new(437));
    let mut harness = crate::assistant::capture_harness(relay, LanguagePref::Fr, ThemePref::Light);
    run(&mut harness, 4);
    crate::common::snapshot(&mut harness, "assistant_account_fr_light");
}

#[test]
fn snapshot_assistant_account_login_fr_light() {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let relay = Arc::new(FakeRelay::new(437));
    let mut harness =
        crate::assistant::capture_harness(Arc::clone(&relay) as _, LanguagePref::Fr, ThemePref::Light);
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::ChooseAccess(
            AssistantAccess::Account,
        )));
    run(&mut harness, 2);
    wait_session_loaded(&mut harness);
    EMAIL.clone_into(&mut harness.state_mut().assistant_mut().account_mut().email_draft);
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::AccountSendLink));
    run(&mut harness, 4);
    assert_eq!(
        harness.state().assistant().account().stage(),
        AccountStage::Code(EMAIL.to_owned())
    );
    "K7QF".clone_into(&mut harness.state_mut().assistant_mut().account_mut().code_draft);
    run(&mut harness, 2);
    crate::common::snapshot(&mut harness, "assistant_account_login_fr_light");
}

#[test]
fn snapshot_assistant_account_chat_fr_light() {
    use plannercraft_ui_egui::{LanguagePref, ThemePref};
    let relay = Arc::new(FakeRelay::new(437));
    relay.push_chat(
        text_chunks(
            "Le projet compte **49 tâches**, de l'étude au gros œuvre. La fin prévue vient du moteur : \
             demandez « pourquoi cette date ? » sur une tâche pour le détail.",
            0.004,
        ),
        3,
    );
    let mut harness =
        crate::assistant::capture_harness(Arc::clone(&relay) as _, LanguagePref::Fr, ThemePref::Light);
    harness
        .state_mut()
        .dispatch(Action::Assistant(AssistantOp::ChooseAccess(
            AssistantAccess::Account,
        )));
    run(&mut harness, 2);
    wait_session_loaded(&mut harness);
    let app = harness.state_mut();
    let mut prefs = app.state.assistant.clone();
    app.assistant_mut().sign_in_with_link(FAKE_LINK_TOKEN, &mut prefs);
    app.state.assistant = prefs;
    run(&mut harness, 4);
    crate::assistant::send_now(&mut harness, "Combien de tâches compte le projet ?");
    run(&mut harness, 10);
    assert!(!harness.state().assistant().conversation.busy());
    assert_eq!(harness.state().assistant().account().balance(), Some(434));
    crate::common::snapshot(&mut harness, "assistant_account_chat_fr_light");
}
