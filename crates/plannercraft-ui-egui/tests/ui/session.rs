//! Document sur une session d'édition (lot L11, ADR-0012 D11) : lots de
//! commandes, annuler/rétablir, rafraîchissement, raccourcis vérifiés
//! contre le registre, refus traduits.

use std::cell::RefCell;
use std::rc::Rc;

use plannercraft_automation::{Batch, REGISTRY, registry};
use plannercraft_core::TaskUid;
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts, error_key, error_text_id};
use plannercraft_ui_egui::keymap::{Binding, KEYMAP, KeyContext, UiBinding};
use plannercraft_ui_egui::rows::RowKind;
use plannercraft_ui_egui::{
    Action, App, DocumentObserver, EditOrigin, Env, LanguagePref, OpenDocument, ThemePref, UiState,
};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;

use crate::{Reply, fr, harness, harness_for, now, open_demo, scripted};

/// Lot lu depuis son JSON (format versionné, ADR-0012 D6).
fn batch(json: &str) -> Batch {
    Batch::from_json_str(json).unwrap()
}

/// Lot qui change la durée de `uid`.
fn set_duration(uid: TaskUid, text: &str) -> Batch {
    batch(&format!(
        r#"{{"format_version":1,"ops":[{{"op":"task.update","uid":{},"duration":{{"text":"{text}"}}}}]}}"#,
        uid.get()
    ))
}

/// Première feuille du document : (indice de ligne, UID).
fn first_leaf(app: &App) -> (usize, TaskUid) {
    let rows = app.rows().unwrap();
    (0..rows.len())
        .find_map(|i| {
            let row = rows.row(i)?;
            (row.kind == RowKind::Leaf).then_some((i, row.uid))
        })
        .unwrap()
}

/// Exemple ouvert, puis une durée modifiée par un lot : rend l'UID modifié.
fn demo_with_one_edit(harness: &mut Harness<'static, App>) -> TaskUid {
    open_demo(harness);
    let (_, uid) = first_leaf(harness.state());
    harness
        .state_mut()
        .dispatch(Action::Exec(set_duration(uid, "17j")));
    harness.run();
    assert_eq!(harness.state().document.as_ref().unwrap().generation(), 1);
    uid
}

/// Nombre d'entrées annulables et rétablissables du document.
fn history_len(app: &App) -> (usize, usize) {
    let history = app.document.as_ref().unwrap().session.history();
    (history.entries.len(), history.redo.len())
}

#[test]
fn ui_undo_redo_restores_rows() {
    let mut harness = harness();
    open_demo(&mut harness);
    let before = harness.state().rows().unwrap().clone();
    assert!(!harness.state().window_title().starts_with('*'));
    let (row, uid) = first_leaf(harness.state());
    harness
        .state_mut()
        .dispatch(Action::Exec(set_duration(uid, "17j")));
    harness.run();
    let edited = harness.state().rows().unwrap().clone();
    assert_ne!(edited, before);
    assert!(
        edited.text(row, Column::Duration).contains("17"),
        "{}",
        edited.text(row, Column::Duration)
    );
    assert_ne!(edited.row(row).unwrap().bar, before.row(row).unwrap().bar);
    assert!(harness.state().window_title().starts_with('*'));
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );

    harness.state_mut().dispatch(Action::Undo);
    harness.run();
    assert_eq!(harness.state().rows().unwrap(), &before);
    assert!(!harness.state().window_title().starts_with('*'));
    assert_eq!(history_len(harness.state()), (0, 1));

    harness.state_mut().dispatch(Action::Redo);
    harness.run();
    assert_eq!(harness.state().rows().unwrap(), &edited);
    assert!(harness.state().window_title().starts_with('*'));
    assert_eq!(history_len(harness.state()), (1, 0));

    // Rien à rétablir : aucun effet, aucun message.
    harness.state_mut().dispatch(Action::Redo);
    harness.run();
    assert_eq!(harness.state().rows().unwrap(), &edited);
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );
}

#[test]
fn ui_refresh_keeps_selection_by_uid() {
    let mut harness = harness();
    open_demo(&mut harness);
    let (row, uid) = first_leaf(harness.state());
    let first = harness.state().rows().unwrap().row(0).unwrap().uid;
    harness.state_mut().dispatch(Action::Select(Some(uid)));
    harness.state_mut().dispatch(Action::ScrollGanttX(120.0));
    harness.run();
    let scroll = harness.state().view().gantt_scroll_x;
    assert!(scroll > 0.0);

    // Une tâche insérée en tête décale les ID : la sélection suit l'UID.
    harness.state_mut().dispatch(Action::Exec(batch(&format!(
        r#"{{"format_version":1,"ops":[{{"op":"task.add","name":"Nouvelle","at":{{"before":{}}}}}]}}"#,
        first.get()
    ))));
    harness.run();
    let app = harness.state();
    assert!(app.messages().is_empty(), "{:?}", app.messages());
    assert_eq!(app.selection, Some(uid));
    assert_eq!(app.rows().unwrap().index_of(uid), Some(row + 1));
    assert!((app.view().gantt_scroll_x - scroll).abs() < f32::EPSILON);

    // La tâche sélectionnée supprimée : la tâche qui prend sa ligne devient
    // active (relecture finale J7, F4).
    let next = harness.state().rows().unwrap().row(row + 2).unwrap().uid;
    harness.state_mut().dispatch(Action::Exec(batch(&format!(
        r#"{{"format_version":1,"ops":[{{"op":"task.delete","uid":{}}}]}}"#,
        uid.get()
    ))));
    harness.run();
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );
    assert_eq!(harness.state().selection, Some(next));
    assert_eq!(harness.state().rows().unwrap().index_of(next), Some(row + 1));
    assert_eq!(harness.state().rows().unwrap().index_of(uid), None);
}

#[test]
fn ui_keymap_commands_exist_in_registry() {
    let mut commands = 0;
    for (_, shortcut, binding) in KEYMAP {
        if let Binding::Command(id) = binding {
            commands += 1;
            let spec =
                registry::lookup(id).unwrap_or_else(|| panic!("{shortcut:?} : « {id} » absent du registre"));
            assert!(spec.is_listed(), "« {id} » réservé à un jalon ultérieur");
            assert!(REGISTRY.iter().any(|entry| entry.id == *id));
        }
    }
    assert!(commands > 0);
}

/// Appuie sur le raccourci de l'entrée `i` de [`KEYMAP`].
fn press(harness: &mut Harness<'static, App>, i: usize) {
    let (_, shortcut, _) = KEYMAP[i];
    harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
    harness.run();
}

/// Vérifie que l'entrée `i` de [`KEYMAP`] déclenche son action, et qu'une
/// entrée de contexte `Document` est sans effet sans document.
fn check_keymap_entry(i: usize) {
    let (context, _, binding) = KEYMAP[i];
    if context == KeyContext::Document {
        let mut harness = harness();
        harness.run();
        press(&mut harness, i);
        assert!(harness.state().document.is_none());
        assert!(
            harness.state().messages().is_empty(),
            "{:?}",
            harness.state().messages()
        );
    }
    match binding {
        Binding::Command("history.undo") => {
            let mut harness = harness();
            demo_with_one_edit(&mut harness);
            press(&mut harness, i);
            assert_eq!(history_len(harness.state()), (0, 1));
        }
        Binding::Command("history.redo") => {
            let mut harness = harness();
            demo_with_one_edit(&mut harness);
            harness.state_mut().dispatch(Action::Undo);
            harness.run();
            press(&mut harness, i);
            assert_eq!(history_len(harness.state()), (1, 0));
        }
        Binding::Ui(UiBinding::OpenFile) => {
            let (mut harness, _held) = scripted(&[Reply::Demo]);
            harness.run();
            press(&mut harness, i);
            assert!(
                harness.state().document.is_some(),
                "{:?}",
                harness.state().messages()
            );
        }
        Binding::Command("project.new") => {
            let mut harness = harness();
            harness.run();
            press(&mut harness, i);
            let document = harness.state().document.as_ref().expect("projet créé");
            assert_eq!(document.project().tasks.len(), 0);
        }
        Binding::Command("project.save") => {
            let mut sim = crate::files::sim(crate::files::SaveReply::Ok);
            open_demo(&mut sim.harness);
            crate::files::edit_first_leaf(&mut sim.harness, "17j");
            press(&mut sim.harness, i);
            assert_eq!(sim.requests.borrow().len(), 1);
            assert!(!sim.harness.state().document.as_ref().unwrap().is_dirty());
        }
        Binding::Ui(UiBinding::SaveAs) => {
            let mut sim = crate::files::sim(crate::files::SaveReply::Ok);
            open_demo(&mut sim.harness);
            press(&mut sim.harness, i);
            let requests = sim.requests.borrow();
            assert_eq!(requests.len(), 1);
            assert!(matches!(
                requests[0].target,
                plannercraft_ui_egui::io::SaveTarget::Dialog { .. }
            ));
        }
        Binding::Ui(UiBinding::Quit) => crate::files::check_quit_shortcut(i),
        // « Informations sur la tâche » (J7 lot L9) : `task_info.rs`.
        Binding::Ui(UiBinding::TaskInformation) => crate::task_info::check_keymap_entry(i),
        // Navigation, échelle de temps, zoom, `task.move` (J7 lot L1) :
        // vérifiées par `nav.rs`.
        binding if crate::nav::is_nav_entry(binding) => crate::nav::check_keymap_entry(i),
        // Structure (lot L14) : vérifiées par `structure.rs`.
        Binding::Command(id) if crate::structure::COMMANDS.contains(&id) => {
            crate::structure::check_keymap_entry(id, KEYMAP[i].1);
        }
        // Filtres, tri, recherche, repli (J8 lot L9) : `filters.rs`.
        binding if crate::filters::is_view_entry(binding) => crate::filters::check_keymap_entry(i),
        // Passe UX (ADR-0018, L0) : sélection, remplir, effacer.
        binding if crate::select_keys::is_ux_entry(binding) => crate::select_keys::check_keymap_entry(i),
        // Seconde passe UX (ADR-0019, L0) : fermer, imprimer, défiler,
        // remplir à droite, Paramètres de champ, F6.
        binding if crate::ux2_contract::is_ux2_entry(binding) => crate::ux2_contract::check_keymap_entry(i),
        // Ruban (ADR-0017, L0) : Ctrl/Cmd+F1 replie puis déplie le ruban.
        Binding::Ui(UiBinding::ToggleRibbon) => {
            let mut harness = harness();
            harness.run();
            press(&mut harness, i);
            assert!(harness.state().state.ribbon.collapsed());
            press(&mut harness, i);
            assert!(!harness.state().state.ribbon.collapsed());
        }
        // Finitions P2 (H-UI-127) : Ctrl/Cmd+B, Ctrl/Cmd+I (P4) sur la tâche
        // active ; une entrée d'annulation chacune.
        Binding::Ui(UiBinding::ToggleBold | UiBinding::ToggleItalic) => {
            let mut harness = harness();
            open_demo(&mut harness);
            let uid = harness.state().selection.expect("tâche active");
            press(&mut harness, i);
            let doc = harness.state().document.as_ref().unwrap();
            let t = plannercraft_automation::command::text_style::task_text_of(doc.project(), uid);
            let bold = binding == Binding::Ui(UiBinding::ToggleBold);
            assert_eq!((t.bold, t.italic), (bold, !bold), "{t:?}");
            assert_eq!(history_len(harness.state()), (1, 0));
        }
        // Lot A5 (H-UI-162) : Ctrl/Cmd+J, `assistant_panel.rs`.
        Binding::Ui(UiBinding::ToggleAssistant) => crate::assistant_panel::check_keymap_entry(i),
        // Lot U3 (H-UI-177) : Ctrl/Cmd+K, `palette.rs`.
        Binding::Ui(UiBinding::CommandPalette) => crate::palette::check_keymap_entry(i),
        other => panic!("entrée {i} sans vérification : {other:?}"),
    }
}

/// Un test par entrée de [`KEYMAP`] (le nombre d'entrées est vérifié).
macro_rules! keymap_entry_tests {
    ($($name:ident => $i:literal),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                check_keymap_entry($i);
            }
        )*

        #[test]
        fn ui_keymap_each_shortcut_dispatches_covers_all() {
            assert_eq!(KEYMAP.len(), [$($i),*].len(), "ajouter un test par entrée de KEYMAP");
        }
    };
}

keymap_entry_tests! {
    ui_keymap_each_shortcut_dispatches_0 => 0,
    ui_keymap_each_shortcut_dispatches_1 => 1,
    ui_keymap_each_shortcut_dispatches_2 => 2,
    ui_keymap_each_shortcut_dispatches_3 => 3,
    ui_keymap_each_shortcut_dispatches_4 => 4,
    ui_keymap_each_shortcut_dispatches_5 => 5,
    ui_keymap_each_shortcut_dispatches_6 => 6,
    ui_keymap_each_shortcut_dispatches_7 => 7,
    ui_keymap_each_shortcut_dispatches_8 => 8,
    ui_keymap_each_shortcut_dispatches_9 => 9,
    ui_keymap_each_shortcut_dispatches_10 => 10,
    ui_keymap_each_shortcut_dispatches_11 => 11,
    ui_keymap_each_shortcut_dispatches_12 => 12,
    ui_keymap_each_shortcut_dispatches_13 => 13,
    ui_keymap_each_shortcut_dispatches_14 => 14,
    ui_keymap_each_shortcut_dispatches_15 => 15,
    ui_keymap_each_shortcut_dispatches_16 => 16,
    ui_keymap_each_shortcut_dispatches_17 => 17,
    ui_keymap_each_shortcut_dispatches_18 => 18,
    ui_keymap_each_shortcut_dispatches_19 => 19,
    ui_keymap_each_shortcut_dispatches_20 => 20,
    ui_keymap_each_shortcut_dispatches_21 => 21,
    ui_keymap_each_shortcut_dispatches_22 => 22,
    ui_keymap_each_shortcut_dispatches_23 => 23,
    ui_keymap_each_shortcut_dispatches_24 => 24,
    ui_keymap_each_shortcut_dispatches_25 => 25,
    ui_keymap_each_shortcut_dispatches_26 => 26,
    ui_keymap_each_shortcut_dispatches_27 => 27,
    ui_keymap_each_shortcut_dispatches_28 => 28,
    ui_keymap_each_shortcut_dispatches_29 => 29,
    ui_keymap_each_shortcut_dispatches_30 => 30,
    ui_keymap_each_shortcut_dispatches_31 => 31,
    ui_keymap_each_shortcut_dispatches_32 => 32,
    ui_keymap_each_shortcut_dispatches_33 => 33,
    ui_keymap_each_shortcut_dispatches_34 => 34,
    ui_keymap_each_shortcut_dispatches_35 => 35,
    ui_keymap_each_shortcut_dispatches_36 => 36,
    ui_keymap_each_shortcut_dispatches_37 => 37,
    ui_keymap_each_shortcut_dispatches_38 => 38,
    ui_keymap_each_shortcut_dispatches_39 => 39,
    ui_keymap_each_shortcut_dispatches_40 => 40,
    ui_keymap_each_shortcut_dispatches_41 => 41,
    ui_keymap_each_shortcut_dispatches_42 => 42,
    ui_keymap_each_shortcut_dispatches_43 => 43,
    ui_keymap_each_shortcut_dispatches_44 => 44,
    ui_keymap_each_shortcut_dispatches_45 => 45,
    ui_keymap_each_shortcut_dispatches_46 => 46,
    ui_keymap_each_shortcut_dispatches_47 => 47,
    ui_keymap_each_shortcut_dispatches_48 => 48,
    ui_keymap_each_shortcut_dispatches_49 => 49,
    ui_keymap_each_shortcut_dispatches_50 => 50,
    ui_keymap_each_shortcut_dispatches_51 => 51,
    ui_keymap_each_shortcut_dispatches_52 => 52,
    // J8 L9.
    ui_keymap_each_shortcut_dispatches_53 => 53,
    ui_keymap_each_shortcut_dispatches_54 => 54,
    ui_keymap_each_shortcut_dispatches_55 => 55,
    ui_keymap_each_shortcut_dispatches_56 => 56,
    ui_keymap_each_shortcut_dispatches_57 => 57,
    ui_keymap_each_shortcut_dispatches_58 => 58,
    ui_keymap_each_shortcut_dispatches_59 => 59,
    ui_keymap_each_shortcut_dispatches_60 => 60,
    ui_keymap_each_shortcut_dispatches_61 => 61,
    ui_keymap_each_shortcut_dispatches_62 => 62,
    ui_keymap_each_shortcut_dispatches_63 => 63,
    ui_keymap_each_shortcut_dispatches_64 => 64,
    ui_keymap_each_shortcut_dispatches_65 => 65,
    ui_keymap_each_shortcut_dispatches_66 => 66,
    ui_keymap_each_shortcut_dispatches_67 => 67,
    ui_keymap_each_shortcut_dispatches_68 => 68,
    ui_keymap_each_shortcut_dispatches_69 => 69,
    ui_keymap_each_shortcut_dispatches_70 => 70,
    ui_keymap_each_shortcut_dispatches_71 => 71,
    ui_keymap_each_shortcut_dispatches_72 => 72,
    ui_keymap_each_shortcut_dispatches_73 => 73,
    ui_keymap_each_shortcut_dispatches_74 => 74,
    ui_keymap_each_shortcut_dispatches_75 => 75,
    ui_keymap_each_shortcut_dispatches_76 => 76,
    ui_keymap_each_shortcut_dispatches_77 => 77,
    ui_keymap_each_shortcut_dispatches_78 => 78,
    ui_keymap_each_shortcut_dispatches_79 => 79,
    ui_keymap_each_shortcut_dispatches_80 => 80,
    ui_keymap_each_shortcut_dispatches_81 => 81,
    ui_keymap_each_shortcut_dispatches_82 => 82,
    ui_keymap_each_shortcut_dispatches_83 => 83,
    ui_keymap_each_shortcut_dispatches_84 => 84,
    ui_keymap_each_shortcut_dispatches_85 => 85,
    ui_keymap_each_shortcut_dispatches_86 => 86,
    // Lot A5 d'ADR-0024.
    ui_keymap_each_shortcut_dispatches_87 => 87,
    ui_keymap_each_shortcut_dispatches_88 => 88,
}

/// Lot refusé : UID absent.
fn unknown_task() -> Batch {
    batch(r#"{"format_version":1,"ops":[{"op":"task.update","uid":999999,"name":"X"}]}"#)
}

#[test]
fn ui_command_error_banner_translated() {
    for (pref, language) in [(LanguagePref::Fr, Language::Fr), (LanguagePref::En, Language::En)] {
        let state = UiState {
            language: pref,
            theme: ThemePref::Light,
            ..UiState::default()
        };
        let mut harness = harness_for(App::new(state, Env::fixed(now())));
        harness.run();
        harness.state_mut().open_demo();
        harness.run();
        let texts = Texts::new(language);
        let error = harness
            .state_mut()
            .execute(&unknown_task(), EditOrigin::User)
            .unwrap_err();
        assert_eq!(error.code, "task.not_found");
        harness.run();
        let lang = match language {
            Language::Fr => plannercraft_automation::text::Language::Fr,
            Language::En => plannercraft_automation::text::Language::En,
        };
        // Seconde passe UX (H-UI-105) : l'UID d'une tâche introuvable n'a
        // pas d'ID ⇒ phrase générique, code et texte du moteur au détail.
        let expected = texts.get(TextId::RefusalGeneric);
        let messages = harness.state().messages();
        assert_eq!(messages.len(), 1, "{messages:?}");
        assert_eq!(messages[0].title, Some(TextId::CommandErrorTitle));
        assert_eq!(messages[0].text, expected);
        let detail = messages[0].detail.clone().unwrap();
        assert!(detail.contains("task.not_found"), "{detail}");
        assert!(detail.contains(&error.message(lang)), "{detail}");
        assert!(
            harness
                .query_by_label(&texts.get(TextId::CommandErrorTitle))
                .is_some()
        );
        assert!(harness.query_by_label(&expected).is_some());
        // Document intact.
        assert_eq!(harness.state().document.as_ref().unwrap().generation(), 0);

        // Une édition réussie retire le refus devenu caduc.
        let (_, uid) = first_leaf(harness.state());
        harness
            .state_mut()
            .dispatch(Action::Exec(set_duration(uid, "4j")));
        harness.run();
        assert!(
            harness.state().messages().is_empty(),
            "{:?}",
            harness.state().messages()
        );
    }
    // Un refus venu d'un agent est rendu à l'appelant, sans bandeau.
    let mut harness = harness();
    open_demo(&mut harness);
    assert!(
        harness
            .state_mut()
            .execute(&unknown_task(), EditOrigin::Agent)
            .is_err()
    );
    harness.run();
    assert!(
        harness.state().messages().is_empty(),
        "{:?}",
        harness.state().messages()
    );
    // Sans document : `session.none`, traduit.
    let mut harness = crate::harness();
    harness.run();
    let error = harness
        .state_mut()
        .execute(&unknown_task(), EditOrigin::User)
        .unwrap_err();
    assert_eq!(error.code, "session.none");
    assert_eq!(error_text_id(error.code), TextId::ErrorSessionNone);
    // Code sans phrase dédiée (H-UI-105) : phrase générique, code au détail.
    let message = &harness.state().messages()[0];
    assert_eq!(message.text, fr(TextId::RefusalGeneric));
    assert!(message.detail.as_deref().unwrap().contains("session.none"));
    assert!(fr(TextId::CommandErrorTitle).starts_with("Modification"));
}

/// Codes d'erreur stables du moteur, relevés dans ses sources : le module
/// `codes` d'`error.rs` et les codes `text.*` de `TextError::code`.
fn engine_error_codes() -> Vec<String> {
    const ERROR_RS: &str = include_str!("../../../plannercraft-automation/src/error.rs");
    const TEXT_RS: &str = include_str!("../../../plannercraft-automation/src/text/mod.rs");
    let (_, after) = ERROR_RS.split_once("pub mod codes {").unwrap();
    let (module, _) = after.split_once("\n}").unwrap();
    let mut codes: Vec<String> = module
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub const "))
        .filter_map(|rest| rest.split('"').nth(1))
        .map(str::to_owned)
        .collect();
    codes.extend(
        TEXT_RS
            .lines()
            .filter_map(|line| line.split_once("=> \"text."))
            .filter_map(|(_, rest)| rest.split('"').next())
            .map(|code| format!("text.{code}")),
    );
    codes.sort();
    codes.dedup();
    codes
}

#[test]
fn ui_ftl_keys_for_all_error_codes() {
    let codes = engine_error_codes();
    // Garde-fou du relevé : au moins les codes connus au lot L11.
    assert!(codes.len() >= 40, "{codes:?}");
    for required in [
        "task.not_found",
        "history.empty",
        "file.open_in_app",
        "text.bad_date",
    ] {
        assert!(codes.iter().any(|code| code == required), "{required} non relevé");
    }
    let raw = |language: Language, key: &str| -> bool {
        let source = match language {
            Language::Fr => include_str!("../../locales/fr.ftl"),
            Language::En => include_str!("../../locales/en.ftl"),
        };
        source.lines().any(|line| line.starts_with(&format!("{key} = ")))
    };
    for code in &codes {
        let id = error_text_id(code);
        assert_ne!(
            id,
            TextId::ErrorUnknown,
            "« {code} » sans clé {}",
            error_key(code)
        );
        assert_eq!(id.key(), error_key(code));
        for language in Language::ALL {
            assert!(raw(language, id.key()), "{language:?} : clé {} absente", id.key());
            let text = Texts::new(language).get_args(id, &[("detail", Arg::Str("x".into()))]);
            assert!(
                text.ends_with('x') && text != id.key(),
                "{language:?} {code} : {text}"
            );
        }
    }
    assert_eq!(error_text_id("code.inconnu"), TextId::ErrorUnknown);
}

/// Clés de cache vues par l'observateur (`None` : aucun document).
type Seen = Rc<RefCell<Vec<Option<(u64, u64)>>>>;

/// Observateur de test : clés de cache vues à chaque appel.
#[derive(Default)]
struct Recorder {
    seen: Seen,
}

impl DocumentObserver for Recorder {
    fn document_changed(&mut self, document: Option<&OpenDocument>) {
        self.seen.borrow_mut().push(document.map(OpenDocument::cache_key));
    }
}

#[test]
fn ui_observer_sees_open_edit_undo_close() {
    let mut harness = harness();
    let recorder = Recorder::default();
    let seen = Rc::clone(&recorder.seen);
    harness.state_mut().set_observer(Box::new(recorder));
    demo_with_one_edit(&mut harness);
    harness.state_mut().dispatch(Action::Undo);
    harness.state_mut().dispatch(Action::CloseDocument);
    harness.run();
    assert_eq!(
        *seen.borrow(),
        [None, Some((1, 0)), Some((1, 1)), Some((1, 2)), None]
    );
}

/// Menu Édition : Annuler puis Rétablir (raccourci affiché dans le libellé).
#[test]
fn ui_edit_menu_undo_redo() {
    let mut harness = harness();
    demo_with_one_edit(&mut harness);
    for (item, expected) in [(TextId::MenuUndo, (0, 1)), (TextId::MenuRedo, (1, 0))] {
        crate::click(&mut harness, &fr(TextId::MenuEdit));
        let label = fr(item);
        crate::click_where(&mut harness, |text| text.contains(&label));
        assert_eq!(history_len(harness.state()), expected, "{item:?}");
    }
}
