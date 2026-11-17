//! Suggestions d'action de l'état vide de l'assistant (H-UI-237,
//! `docs/features/ui.md` § 1.41, `docs/features/assistant.md` § 7) :
//! choisies d'après le planning ouvert, cartes à verbe, clic qui
//! préremplit la saisie sans l'envoyer.

use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_assistant::testing::ScriptedTransport;
use plannercraft_automation::Batch;
use plannercraft_ui_egui::assistant::AssistantServices;
use plannercraft_ui_egui::format::format_date;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::view::assistant::{MAX_SUGGESTIONS, question_id};
use plannercraft_ui_egui::view::side_panel::{SideOp, SideTab};
use plannercraft_ui_egui::{Action, App, DocOrigin, EditOrigin};

use crate::assistant::{assistant_harness, enable_with_key};
use crate::{click, fr, harness};

const TRACKING: &[u8] = include_bytes!("../../../../tests/mcp/expected/tracking_j6.xml");
const RESOURCES: &[u8] = include_bytes!("../../../../tests/mcp/expected/resources_j5.xml");

/// Ressource « Maçon » de `resources_j5.xml`.
const MASON: u32 = 1;

/// Suggestion attendue, par ses deux clés Fluent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    House,
    Renovation,
    CatchUp,
    Level,
    Phase,
    Shift,
    Shorten,
    Finish,
}

const ALL: [Expect; 8] = [
    Expect::House,
    Expect::Renovation,
    Expect::CatchUp,
    Expect::Level,
    Expect::Phase,
    Expect::Shift,
    Expect::Shorten,
    Expect::Finish,
];

impl Expect {
    const fn keys(self) -> (TextId, TextId) {
        match self {
            Self::House => (
                TextId::AssistantSuggestHouseTitle,
                TextId::AssistantSuggestHouseDetail,
            ),
            Self::Renovation => (
                TextId::AssistantSuggestRenovationTitle,
                TextId::AssistantSuggestRenovationDetail,
            ),
            Self::CatchUp => (
                TextId::AssistantSuggestCatchUpTitle,
                TextId::AssistantSuggestCatchUpDetail,
            ),
            Self::Level => (
                TextId::AssistantSuggestLevelTitle,
                TextId::AssistantSuggestLevelDetail,
            ),
            Self::Phase => (
                TextId::AssistantSuggestPhaseTitle,
                TextId::AssistantSuggestPhaseDetail,
            ),
            Self::Shift => (
                TextId::AssistantSuggestShiftTitle,
                TextId::AssistantSuggestShiftDetail,
            ),
            Self::Shorten => (
                TextId::AssistantSuggestShortenTitle,
                TextId::AssistantSuggestShortenDetail,
            ),
            Self::Finish => (
                TextId::AssistantSuggestFinishTitle,
                TextId::AssistantSuggestFinishDetail,
            ),
        }
    }

    /// Phrase complète en français, `date` déjà formatée.
    fn phrase(self, date: &str) -> String {
        let texts = Texts::new(Language::Fr);
        let (title, detail) = self.keys();
        format!(
            "{} {}",
            texts.get(title),
            texts.get_args(detail, &[("date", Arg::Str(date.into()))])
        )
    }
}

/// Date de fin du projet ouvert, calculée par le moteur et formatée comme
/// la barre d'état.
fn engine_finish(harness: &Harness<'static, App>) -> String {
    let app = harness.state();
    let doc = app.document.as_ref().unwrap();
    format_date(doc.schedule().project_finish, app.texts().format_labels())
}

/// Suggestions affichées, de haut en bas.
fn shown(harness: &Harness<'static, App>, date: &str) -> Vec<Expect> {
    let mut found: Vec<(f32, Expect)> = ALL
        .iter()
        .filter_map(|&e| {
            harness
                .query_by_label(&e.phrase(date))
                .map(|node| (node.rect().top(), e))
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, e)| e).collect()
}

/// Clique la suggestion `e` : la saisie reçoit la phrase complète et le
/// focus, rien n'est envoyé.
fn click_fills_input(harness: &mut Harness<'static, App>, e: Expect, date: &str) {
    let phrase = e.phrase(date);
    click(harness, &phrase);
    harness.run();
    assert_eq!(harness.state().assistant().draft, phrase);
    assert!(
        harness.ctx.memory(|m| m.has_focus(question_id())),
        "la saisie a le focus"
    );
    assert!(
        harness.state().assistant().conversation.is_empty(),
        "rien n'est envoyé"
    );
}

/// Ouvre `bytes` sous le nom `name`.
fn open(harness: &mut Harness<'static, App>, bytes: &[u8], name: &str) {
    let origin = DocOrigin {
        name: name.to_owned(),
        path: None,
        demo: false,
    };
    harness.state_mut().open_bytes(bytes, origin);
    harness.run();
    assert!(
        harness.state().document.is_some(),
        "{:?}",
        harness.state().messages()
    );
}

/// Harnais sans document, onglet Assistant ouvert et activé.
fn bare_harness() -> Harness<'static, App> {
    let mut harness = harness();
    harness
        .state_mut()
        .set_assistant_services(AssistantServices::with_transport(Arc::new(
            ScriptedTransport::new(vec![]),
        )));
    harness
        .state_mut()
        .dispatch(Action::SidePanel(SideOp::Open(SideTab::Assistant)));
    harness.run();
    enable_with_key(&mut harness);
    harness
}

/// H-UI-237 : sans document, aucune suggestion, le message « Ouvrez un
/// projet… » reste.
#[test]
fn h_ui_237_no_document_no_suggestion() {
    let harness = bare_harness();
    assert!(harness.state().document.is_none());
    harness.get_by_label(&fr(TextId::AssistantNoDocument));
    assert_eq!(shown(&harness, ""), Vec::<Expect>::new());
}

/// H-UI-237 : projet neuf sans tâche : deux créations de planning, sans
/// question sur la date de fin.
#[test]
fn h_ui_237_empty_project_create_suggestions() {
    let mut harness = bare_harness();
    harness.state_mut().dispatch(Action::NewProject);
    harness.run();
    harness.run();
    let doc = harness.state().document.as_ref().unwrap();
    assert!(doc.project().tasks.iter().all(|t| t.is_null));
    let date = engine_finish(&harness);
    assert_eq!(shown(&harness, &date), [Expect::House, Expect::Renovation]);
    click_fills_input(&mut harness, Expect::House, &date);
    assert_eq!(
        harness.state().assistant().draft,
        "Crée un planning de chantier de maison individuelle"
    );
}

/// H-UI-237 : exemple sain (ni retard ni surcharge selon le moteur) : phase
/// 2, décaler, raccourcir et « Pourquoi le projet finit le … ? », la date
/// étant celle du moteur.
#[test]
fn h_ui_237_healthy_project_engine_date() {
    let transport = Arc::new(ScriptedTransport::new(vec![]));
    let mut harness = assistant_harness(transport);
    enable_with_key(&mut harness);
    {
        let doc = harness.state().document.as_ref().unwrap();
        assert!(!doc.schedule().resources.values().any(|l| l.overallocated));
    }
    let date = engine_finish(&harness);
    assert_eq!(
        shown(&harness, &date),
        [Expect::Phase, Expect::Shift, Expect::Shorten, Expect::Finish]
    );
    assert!(shown(&harness, &date).len() <= MAX_SUGGESTIONS);
    click_fills_input(&mut harness, Expect::Finish, &date);
    let draft = harness.state().assistant().draft.clone();
    assert!(draft.contains(&date), "{draft}");
    assert!(draft.starts_with("Pourquoi le projet finit le "), "{draft}");
}

/// H-UI-237 : une ressource surutilisée selon le moteur (Maçon ramené à
/// 50 %, comme H-UI-23) : « Lisse les surcharges de ressources ».
#[test]
fn h_ui_237_overallocation_level_suggestion() {
    let mut harness = bare_harness();
    open(&mut harness, RESOURCES, "resources_j5.xml");
    let date = engine_finish(&harness);
    assert!(!shown(&harness, &date).contains(&Expect::Level));
    let lot: Batch = serde_json::from_value(serde_json::json!({
        "format_version": 1,
        "ops": [{ "op": "resource.update", "uid": MASON, "max_units": { "text": "50%" } }]
    }))
    .unwrap();
    harness.state_mut().execute(&lot, EditOrigin::User).unwrap();
    harness.run();
    harness.run();
    let date = engine_finish(&harness);
    assert_eq!(shown(&harness, &date), [Expect::Level, Expect::Finish]);
    click_fills_input(&mut harness, Expect::Level, &date);
    assert_eq!(
        harness.state().assistant().draft,
        "Lisse les surcharges de ressources"
    );
}

/// H-UI-237 : une tâche dont la fin calculée dépasse sa fin de référence
/// (variation de fin positive, R-224) : « Rattrape le retard sans décaler
/// la fin du projet » et la question sur la date de fin.
#[test]
fn h_ui_237_late_tasks_catch_up_suggestion() {
    let mut harness = bare_harness();
    open(&mut harness, TRACKING, "tracking_j6.xml");
    let late = {
        let rows = harness.state().rows().unwrap();
        (0..rows.len()).filter_map(|i| rows.row(i)).any(|r| {
            r.active
                && r.finish_variance
                    .is_some_and(|v| v > plannercraft_core::Minutes::ZERO)
        })
    };
    assert!(late, "le fichier de suivi a des tâches en retard");
    let date = engine_finish(&harness);
    let suggestions = shown(&harness, &date);
    assert_eq!(suggestions.first(), Some(&Expect::CatchUp));
    assert_eq!(suggestions.last(), Some(&Expect::Finish));
    assert!(!suggestions.contains(&Expect::Phase));
    click_fills_input(&mut harness, Expect::CatchUp, &date);
    assert_eq!(
        harness.state().assistant().draft,
        "Rattrape le retard sans décaler la fin du projet"
    );
}
