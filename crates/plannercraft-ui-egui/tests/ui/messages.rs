//! Seconde passe UX (ADR-0019, lot L1) : notifications, bandeau des
//! décisions, historique et refus en langage d'utilisateur (H-UI-104,
//! H-UI-105).
//!
//! Repère : projet « P » (`plannercraft_tests::daily::plan_p`) précédé d'une
//! tâche « Préparation » (UID 8, ID 1) : les ID diffèrent des UID (UID 2
//! « Fondations » ⇒ ID 3, UID 3 ⇒ 4, UID 6 ⇒ 7, UID 7 ⇒ 8).

use egui::Event;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_automation::error::codes;
use plannercraft_automation::text::TextError;
use plannercraft_automation::{Batch, CommandError, CommandErrorKind};
use plannercraft_core::{Diagnostic, Subject, TaskUid};
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::action::{MessageOp, PressMode, PressTarget, SelectOp};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::error_text::{UI_CODES, user_message};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::notify::{MAX_HISTORY, MAX_TOASTS, MessageKind};
use plannercraft_ui_egui::state::StateLoadError;
use plannercraft_ui_egui::view::chrome::TOAST_CLOSE_GLYPH;
use plannercraft_ui_egui::{Action, App, DocOrigin, EditOrigin};
use serde_json::json;

use crate::{fr, harness_for, new_app};

/// Images par seconde des tests de durée (H-UI-104 : « 300 images à
/// 60 Hz »).
const HZ: f32 = 60.0;

/// Application ouverte sur P précédé de « Préparation » (une entrée
/// d'annulation).
fn p_app() -> App {
    let mut app = new_app();
    app.open_project(
        plan_p().project().clone(),
        DocOrigin {
            name: "P".to_owned(),
            path: None,
            demo: false,
        },
    );
    app.execute(
        &batch(&[json!({ "op": "task.add", "name": "Préparation", "at": { "before": 1 } })]),
        EditOrigin::User,
    )
    .unwrap();
    app
}

/// Harnais du projet, images de 0,25 s (harnais commun).
fn p_harness() -> Harness<'static, App> {
    let mut harness = harness_for(p_app());
    harness.run();
    harness
}

/// Harnais à 60 images par seconde (durées des notifications).
fn timed_harness(app: App) -> Harness<'static, App> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / HZ)
        .with_max_steps(16)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.run();
    harness
}

fn batch(ops: &[serde_json::Value]) -> Batch {
    serde_json::from_value(json!({ "format_version": 1, "ops": ops })).unwrap()
}

/// `link.add` de l'UID `pred` vers l'UID `succ`, refusé par le registre.
fn refuse_link(harness: &mut Harness<'static, App>, pred: u32, succ: u32) -> CommandError {
    let error = harness
        .state_mut()
        .execute(
            &batch(&[json!({ "op": "link.add", "pred": pred, "succ": succ })]),
            EditOrigin::User,
        )
        .unwrap_err();
    harness.run();
    error
}

/// Texte du dernier message affiché.
fn last_text(app: &App) -> String {
    app.messages().last().unwrap().text.clone()
}

/// Nom accessible de la ligne d'ID `id`.
fn row_label(id: i64, name: &str) -> String {
    Texts::new(Language::Fr).get_args(
        TextId::RowLabel,
        &[("id", Arg::Count(id)), ("name", Arg::Str(name.to_owned().into()))],
    )
}

/// Rectangles des nœuds d'accessibilité nommés `label`.
fn rects(harness: &Harness<'static, App>, label: &str) -> Vec<egui::Rect> {
    harness.query_all_by_label(label).map(|n| n.rect()).collect()
}

/// H-UI-104 : le compte rendu d'un collage (liens non repris) est une
/// notification `Info` : aucun bandeau, ruban et table à la même place
/// avant et après (onglets du ruban, lignes au-dessus du collage).
#[test]
fn h_ui_104_report_is_toast_not_banner() {
    let mut harness = p_harness();
    let press = |harness: &mut Harness<'static, App>, n: u32, mode| {
        harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
            row: RowRef::Item(TaskUid::new(n)),
            column: Column::Name,
            target: PressTarget::RowHeader,
            mode,
        }));
        harness.run();
    };
    // Murs et Livraison (UID 3, 4) copiés : Fondations → Murs non repris.
    press(&mut harness, 3, PressMode::Replace);
    press(&mut harness, 4, PressMode::Extend);
    harness.event(Event::Copy);
    harness.step();
    let copied = harness
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    harness.run();
    press(&mut harness, 6, PressMode::Replace);
    let watched = [
        fr(TextId::RibbonTabTask),
        fr(TextId::RibbonTabView),
        row_label(1, "Préparation"),
        row_label(3, "Fondations"),
    ];
    let before: Vec<Vec<egui::Rect>> = watched.iter().map(|l| rects(&harness, l)).collect();
    assert!(before.iter().all(|r| !r.is_empty()), "{before:?}");
    harness.event(Event::Paste(copied));
    harness.run();
    let report = harness
        .state()
        .messages()
        .iter()
        .find(|m| m.title == Some(TextId::PasteReportTitle))
        .cloned()
        .unwrap();
    assert_eq!(report.kind, MessageKind::Info);
    assert_eq!(harness.state().message_store().banner(), []);
    assert_eq!(harness.state().message_store().toasts().len(), 1);
    // Notification affichée (« × »), sans « Fermer le message » du bandeau.
    assert!(harness.query_by_label(&report.text).is_some());
    assert!(harness.query_by_label(TOAST_CLOSE_GLYPH).is_some());
    assert!(harness.query_by_label(&fr(TextId::MessageDismiss)).is_none());
    let after: Vec<Vec<egui::Rect>> = watched.iter().map(|l| rects(&harness, l)).collect();
    assert_eq!(before, after, "aucune vue ne bouge");
    // « × » de la notification : partie, historique gardé.
    harness.get_by_label(TOAST_CLOSE_GLYPH).click();
    harness.run();
    assert_eq!(harness.state().messages(), []);
    assert_eq!(harness.state().message_store().history().len(), 1);
}

/// H-UI-104 : une notification `Info` part après 300 images à 60 Hz
/// (5 s), pas avant ; survolée, elle reste ; le pointeur parti, elle part
/// 5 s plus tard.
#[test]
fn h_ui_104_toast_expires() {
    let mut harness = timed_harness(new_app());
    harness.state_mut().dispatch(Action::About);
    harness.run();
    assert_eq!(harness.state().messages().len(), 1);
    assert_eq!(harness.state().messages()[0].kind, MessageKind::Info);
    harness.run_steps(270);
    assert_eq!(harness.state().messages().len(), 1, "partie trop tôt");
    harness.run_steps(32);
    assert_eq!(harness.state().messages(), [], "300 images : partie");
    assert_eq!(harness.state().message_store().history().len(), 1);

    // Survol : gardée bien au-delà de 5 s.
    harness.state_mut().dispatch(Action::About);
    harness.run();
    let text = last_text(harness.state());
    let center = harness.get_by_label(&text).rect().center();
    harness.hover_at(center);
    harness.run_steps(400);
    assert_eq!(harness.state().messages().len(), 1, "survolée : gardée");
    harness.remove_cursor();
    harness.run_steps(280);
    assert_eq!(harness.state().messages().len(), 1);
    harness.run_steps(40);
    assert_eq!(harness.state().messages(), []);
}

/// H-UI-104 : un refus reste 10 s (600 images), un avertissement de
/// collage est une information (5 s).
#[test]
fn h_ui_104_refusal_lasts_ten_seconds() {
    let mut harness = timed_harness(p_app());
    refuse_link(&mut harness, 2, 2);
    assert_eq!(harness.state().messages()[0].kind, MessageKind::Refusal);
    harness.run_steps(560);
    assert_eq!(harness.state().messages().len(), 1);
    harness.run_steps(50);
    assert_eq!(harness.state().messages(), []);
}

/// H-UI-104 : 3 notifications au plus, la plus ancienne part ; même
/// texte : remplacée (minuterie relancée), pas de doublon.
#[test]
fn h_ui_104_three_toasts_max() {
    let mut harness = p_harness();
    refuse_link(&mut harness, 2, 2); // vers soi
    let first = last_text(harness.state());
    refuse_link(&mut harness, 2, 3); // en double
    refuse_link(&mut harness, 1, 2); // récapitulative
    refuse_link(&mut harness, 7, 2); // boucle
    let store = harness.state().message_store();
    assert_eq!(store.toasts().len(), MAX_TOASTS);
    assert!(store.toasts().iter().all(|m| m.text != first));
    assert_eq!(store.history().len(), 4);
    refuse_link(&mut harness, 7, 2);
    assert_eq!(harness.state().message_store().toasts().len(), MAX_TOASTS);
    assert_eq!(harness.state().message_store().history().len(), 5);
}

/// H-UI-104 : seuls les messages qui attendent une décision ou une
/// lecture (ouverture échouée, état illisible) vont au bandeau ; « Fermer
/// le message » ne ferme que le sien ; la fermeture d'un document modifié
/// demande confirmation dans son bandeau.
#[test]
fn h_ui_104_banner_only_blocking() {
    let (mut harness, _) = crate::scripted(&[crate::Reply::Demo, crate::Reply::ReadError]);
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    assert!(harness.state().document.is_some());
    harness.state_mut().dispatch(Action::OpenDialog);
    harness.run();
    harness.state_mut().report_state_load(&StateLoadError::NotObject);
    harness.state_mut().dispatch(Action::About);
    harness.run();
    let store = harness.state().message_store();
    assert_eq!(store.banner().len(), 2, "{:?}", store.banner());
    assert!(store.banner().iter().all(|m| m.kind == MessageKind::Blocking));
    assert_eq!(store.toasts().len(), 1);
    let open_error = store.banner()[0].id;
    // Le bandeau ne s'efface pas seul.
    harness.run_steps(80);
    assert_eq!(harness.state().message_store().banner().len(), 2);
    let buttons: Vec<egui::Rect> = rects(&harness, &fr(TextId::MessageDismiss));
    assert_eq!(buttons.len(), 2);
    harness
        .query_all_by_label(&fr(TextId::MessageDismiss))
        .next()
        .unwrap()
        .click();
    harness.run();
    let banner = harness.state().message_store().banner();
    assert_eq!(banner.len(), 1);
    assert_ne!(banner[0].id, open_error);

    // Document modifié, fermeture : bandeau de confirmation.
    crate::files::edit_first_leaf(&mut harness, "17j");
    harness.state_mut().dispatch(Action::CloseDocumentGuarded);
    harness.run();
    assert!(harness.state().confirmation_pending());
    let title = harness.state().texts().get_args(
        TextId::ConfirmUnsavedTitle,
        &[("name", Arg::Str("maison.xml".into()))],
    );
    assert!(harness.query_by_label(&title).is_some());
}

/// H-UI-104 : l'historique garde les 20 derniers messages ; le bouton de
/// la barre d'état affiche les non lus, l'ouvre et les marque lus.
#[test]
fn h_ui_104_history_keeps_20() {
    let mut harness = p_harness();
    assert!(harness.query_by_label(&fr(TextId::NotifyHistory)).is_none());
    for _ in 0..25 {
        refuse_link(&mut harness, 2, 3);
    }
    let store = harness.state().message_store();
    assert_eq!(store.history().len(), MAX_HISTORY);
    assert_eq!(store.toasts().len(), 1, "même texte : une notification");
    assert_eq!(store.unread(), MAX_HISTORY);
    let texts = Texts::new(Language::Fr);
    let count = i64::try_from(MAX_HISTORY).unwrap();
    let label = format!(
        "{} · {}",
        texts.get(TextId::NotifyHistory),
        texts.get_args(TextId::NotifyUnread, &[("count", Arg::Count(count))])
    );
    harness.get_by_label(&label).click();
    harness.run();
    let store = harness.state().message_store();
    assert!(store.history_open());
    assert_eq!(store.unread(), 0);
    assert!(harness.query_by_label(&fr(TextId::NotifyHistoryTitle)).is_some());
    // Fenêtre ouverte : les notifications laissent la place.
    assert_eq!(rects(&harness, &last_text(harness.state())).len(), MAX_HISTORY);
    harness
        .state_mut()
        .dispatch(Action::Message(MessageOp::ToggleHistory));
    harness.run();
    assert!(!harness.state().message_store().history_open());
    assert!(harness.query_by_label(&fr(TextId::NotifyHistoryTitle)).is_none());
}

/// H-UI-105 : lien déjà présent, sujets en ID (UID 2 → ID 3, UID 3 →
/// ID 4), sans nom de commande ni numéro d'opération.
#[test]
fn h_ui_105_duplicate_link_sentence() {
    let mut harness = p_harness();
    let error = refuse_link(&mut harness, 2, 3);
    assert_eq!(error.code, codes::LINK_DUPLICATE);
    let text = last_text(harness.state());
    let expected = harness.state().texts().get_args(
        TextId::RefusalLinkDuplicate,
        &[
            ("successor", Arg::Str("4".into())),
            ("predecessor", Arg::Str("3".into())),
        ],
    );
    assert_eq!(text, expected);
    assert_eq!(text, "La tâche 4 a déjà la tâche 3 pour prédécesseur.");
    for jargon in ["link.update", "link.", "opération", "(", "UID"] {
        assert!(!text.contains(jargon), "{jargon} : {text}");
    }
    assert_eq!(
        harness.state().messages().last().unwrap().kind,
        MessageKind::Refusal
    );
    assert!(harness.query_by_label(&text).is_some());
}

/// H-UI-105 : boucle nommée par les ID de ses tâches (3, 4, 7, 8), pas
/// par leurs UID (2, 3, 6, 7).
#[test]
fn h_ui_105_cycle_sentence_ids() {
    let mut harness = p_harness();
    let error = refuse_link(&mut harness, 7, 2);
    assert_eq!(error.code, codes::BATCH_WOULD_CREATE_ERRORS);
    let text = last_text(harness.state());
    let expected = harness.state().texts().get_args(
        TextId::RefusalLinkCycle,
        &[("tasks", Arg::Str("3, 4, 7, 8".into()))],
    );
    assert_eq!(text, expected);
    assert!(!text.contains("2,") && !text.contains("6,"), "{text}");
    assert!(!text.contains("link.cycle"), "{text}");
}

/// H-UI-105 : « Détails » replié par défaut ; déplié, il montre le code
/// (`link.cycle`) et l'opération.
#[test]
fn h_ui_105_details_fold() {
    let mut harness = p_harness();
    refuse_link(&mut harness, 7, 2);
    let message = harness.state().messages().last().unwrap().clone();
    let detail = message.detail.clone().unwrap();
    assert!(detail.contains("link.cycle"), "{detail}");
    assert!(detail.contains("opération 0"), "{detail}");
    assert!(harness.query_by_label(&detail).is_none(), "replié par défaut");
    harness.get_by_label(&fr(TextId::NotifyDetails)).click();
    harness.run();
    assert!(harness.query_by_label(&detail).is_some(), "déplié");
}

/// H-UI-105 : aucun pluriel entre parenthèses (« erreur(s) ») dans les
/// textes FR et EN.
#[test]
fn h_ui_105_plural_no_parenthesis() {
    for (lang, ftl) in [
        ("fr", include_str!("../../locales/fr.ftl")),
        ("en", include_str!("../../locales/en.ftl")),
    ] {
        for (n, line) in ftl.lines().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            for (i, window) in chars.windows(3).enumerate() {
                if window[0] == '(' && window[2] == ')' && ['s', 'x', 'e'].contains(&window[1]) {
                    let letter_before = i > 0 && chars[i - 1].is_alphabetic();
                    assert!(!letter_before, "{lang}.ftl:{} : {line}", n + 1);
                }
            }
            assert!(!line.contains("(es)"), "{lang}.ftl:{} : {line}", n + 1);
        }
    }
}

/// H-UI-105 : chaque code produit par un lot de l'interface a sa phrase
/// (jamais la phrase générique), en FR et en EN, sans son code.
#[test]
fn h_ui_105_every_ui_code_has_sentence() {
    let app = p_app();
    let project = app.document.as_ref().unwrap().project();
    let link = Subject::Link {
        predecessor: TaskUid::new(2),
        successor: TaskUid::new(3),
    };
    let mut samples: Vec<CommandError> = UI_CODES
        .iter()
        .map(|&code| {
            let kind = match code {
                codes::LINK_DUPLICATE => CommandErrorKind::DuplicateLink,
                codes::LINK_SELF => CommandErrorKind::SelfLink,
                codes::LINK_SUMMARY_DESCENDANT => CommandErrorKind::SummaryDescendantLink,
                codes::TASK_NOT_FOUND => CommandErrorKind::UnknownTask(TaskUid::new(6)),
                codes::BATCH_WOULD_CREATE_ERRORS => {
                    CommandErrorKind::WouldCreateErrors(vec![Diagnostic::LinkCycle {
                        tasks: vec![TaskUid::new(2), TaskUid::new(3)],
                    }])
                }
                codes::LIMIT_EXCEEDED => CommandErrorKind::Limit { what: "ops", max: 10 },
                codes::PARAM_CONFLICT => {
                    CommandErrorKind::ParamConflict("percent_work_complete", "assignments")
                }
                codes::CALENDAR_IN_USE => CommandErrorKind::CalendarInUse {
                    calendar: plannercraft_core::CalendarUid::new(1),
                    users: vec![Subject::Project, Subject::Task(TaskUid::new(2))],
                    total: 3,
                },
                codes::ASSIGNMENT_DUPLICATE => CommandErrorKind::DuplicateAssignment {
                    task: TaskUid::new(2),
                    resource: plannercraft_core::ResourceUid::new(1),
                },
                c if c.starts_with("text.") => CommandErrorKind::Text(TextError::BadNumber { at: 1 }),
                _ => CommandErrorKind::InvalidParam("x".to_owned()),
            };
            CommandError::new(code, kind).with_subject(link).with_op_index(0)
        })
        .collect();
    samples.push(CommandError::new(
        "text.unknown_unit",
        CommandErrorKind::Text(TextError::UnknownUnit {
            unit: "jx".to_owned(),
        }),
    ));
    samples.push(CommandError::new(
        "custom.value_type",
        CommandErrorKind::InvalidParam("x".to_owned()),
    ));
    for lang in [Language::Fr, Language::En] {
        let texts = Texts::new(lang);
        let generic = texts.get(TextId::RefusalGeneric);
        for error in &samples {
            let (text, detail) = user_message(
                error,
                Some(project),
                &texts,
                plannercraft_automation::text::Language::Fr,
            );
            assert_ne!(text, generic, "{}", error.code);
            assert!(!text.contains(error.code), "{text}");
            assert!(detail.contains(error.code), "{detail}");
        }
        // Morceau fautif cité ; tâche supprimée nommée comme telle.
        let unit = samples.iter().find(|e| e.code == "text.unknown_unit").unwrap();
        assert!(
            user_message(
                unit,
                Some(project),
                &texts,
                plannercraft_automation::text::Language::Fr
            )
            .0
            .contains("jx")
        );
        let gone = CommandError::new(codes::LINK_DUPLICATE, CommandErrorKind::DuplicateLink).with_subject(
            Subject::Link {
                predecessor: TaskUid::new(99),
                successor: TaskUid::new(3),
            },
        );
        let (text, _) = user_message(
            &gone,
            Some(project),
            &texts,
            plannercraft_automation::text::Language::Fr,
        );
        assert!(text.contains(&texts.get(TextId::RefusalDeletedTask)), "{text}");
        // Code inconnu de la table : phrase générique, code au détail.
        let other = CommandError::new(
            codes::EXPLAIN_NOT_SCHEDULED,
            CommandErrorKind::InvalidParam("x".to_owned()),
        );
        let (text, detail) = user_message(
            &other,
            Some(project),
            &texts,
            plannercraft_automation::text::Language::Fr,
        );
        assert_eq!(text, generic);
        assert!(detail.contains(codes::EXPLAIN_NOT_SCHEDULED));
    }
}

/// H-UI-105 (relecture finale ux2, C3) : un refus courant dit sa raison,
/// pas la phrase générique : Indenter la première tâche.
#[test]
fn h_ui_105_outline_refusal_says_why() {
    let mut harness = p_harness();
    let error = harness
        .state_mut()
        .execute(
            &batch(&[json!({ "op": "task.indent", "uid": 8 })]),
            EditOrigin::User,
        )
        .unwrap_err();
    harness.run();
    assert_eq!(error.code, codes::TASK_OUTLINE_REFUSED);
    let text = last_text(harness.state());
    assert_eq!(text, fr(TextId::RefusalOutline));
    assert_ne!(text, fr(TextId::RefusalGeneric));
}

/// H-UI-105 (relecture finale ux2, C4) : lot refusé pour un diagnostic
/// autre que la boucle : ses tâches sont nommées par leur ID (UID 2 ⇒ ID
/// 3), jamais par leur UID.
#[test]
fn h_ui_105_batch_diagnostic_task_ids() {
    let app = p_app();
    let project = app.document.as_ref().unwrap().project();
    let error = CommandError::new(
        codes::BATCH_WOULD_CREATE_ERRORS,
        CommandErrorKind::WouldCreateErrors(vec![Diagnostic::TaskNegativeProgress {
            task: TaskUid::new(2),
        }]),
    );
    let texts = Texts::new(Language::Fr);
    let (text, _) = user_message(
        &error,
        Some(project),
        &texts,
        plannercraft_automation::text::Language::Fr,
    );
    assert!(text.contains("tâche 3"), "{text}");
    assert!(!text.contains("tâche 2"), "{text}");
}
