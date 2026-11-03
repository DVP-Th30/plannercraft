//! Seconde passe UX (ADR-0019, contrat L0) : touches nouvelles, actions du
//! réducteur et état posés par le lot L0. Les touches servies par un lot
//! suivant répondent « Ce geste n'est pas encore disponible. » et n'écrivent
//! rien ; Ctrl/Cmd+W, Ctrl+F4, Ctrl/Cmd+P et Ctrl+Maj+F5 passent déjà par
//! leurs chemins existants (fermeture gardée, mode impression, défilement).

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use plannercraft_ui_egui::action::{ColumnOp, ConfirmChoice, MessageOp, PaneOp};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::i18n::TextId;
use plannercraft_ui_egui::keymap::{Binding, KEYMAP, KeyContext, UiBinding};
use plannercraft_ui_egui::state::{DetailsForm, UiState};
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::view::table::table_id;
use plannercraft_ui_egui::{Action, App};

use crate::{harness, open_demo};

/// Vrai si l'entrée de [`KEYMAP`] est vérifiée par [`check_keymap_entry`].
pub fn is_ux2_entry(binding: Binding) -> bool {
    matches!(
        binding,
        Binding::Ui(
            UiBinding::CloseDocument
                | UiBinding::PrintMode
                | UiBinding::ScrollToTask
                | UiBinding::FillRight
                | UiBinding::FieldSettings
                | UiBinding::FocusNextPane
                | UiBinding::FocusPreviousPane
        )
    )
}

/// Exemple ouvert, tâche de la ligne `i` sélectionnée, table focalisée.
fn demo_on_row(i: usize) -> Harness<'static, App> {
    let mut harness = harness();
    open_demo(&mut harness);
    let uid = harness.state().rows().unwrap().row(i).unwrap().uid;
    harness.state_mut().dispatch(Action::Select(Some(uid)));
    harness.state_mut().dispatch(Action::FocusPane(Pane::Table));
    harness.run();
    assert!(harness.ctx.memory(|memory| memory.has_focus(table_id())));
    harness
}

/// Document de l'exemple modifié (une durée), une entrée d'annulation.
fn make_dirty(harness: &mut Harness<'static, App>) {
    crate::files::edit_first_leaf(harness, "17j");
    assert!(harness.state().document.as_ref().unwrap().is_dirty());
}

fn press(harness: &mut Harness<'static, App>, i: usize) {
    let (_, shortcut, _) = KEYMAP[i];
    harness.key_press_modifiers(shortcut.modifiers, shortcut.logical_key);
    harness.run();
}

fn texts(app: &App) -> Vec<String> {
    app.messages().iter().map(|m| m.text.clone()).collect()
}

/// Vérifie l'entrée `i` de [`KEYMAP`] de la seconde passe (contrat L0).
pub fn check_keymap_entry(i: usize) {
    let (context, shortcut, binding) = KEYMAP[i];
    match binding {
        Binding::Ui(UiBinding::CloseDocument) => {
            assert_eq!(context, KeyContext::Global);
            // Document intact : fermé, fenêtre gardée (écran d'accueil).
            let mut harness = demo_on_row(1);
            press(&mut harness, i);
            assert!(harness.state().document.is_none(), "{shortcut:?}");
            assert!(!crate::files::viewport_commands(&harness).contains(&egui::ViewportCommand::Close));
            // Modifié : bandeau de confirmation d'abord (H-UI-109).
            let mut harness = demo_on_row(1);
            make_dirty(&mut harness);
            press(&mut harness, i);
            assert!(harness.state().confirmation_pending(), "{shortcut:?}");
            assert!(harness.state().document.is_some());
            harness
                .state_mut()
                .dispatch(Action::Confirm(ConfirmChoice::Discard));
            harness.run();
            assert!(harness.state().document.is_none());
        }
        Binding::Ui(UiBinding::PrintMode) => {
            assert_eq!(context, KeyContext::Global);
            let mut harness = demo_on_row(1);
            press(&mut harness, i);
            assert!(harness.state().view().dialog.is_some(), "{shortcut:?}");
            assert_eq!(texts(harness.state()), Vec::<String>::new());
        }
        Binding::Ui(UiBinding::ScrollToTask) => {
            assert_eq!(context, KeyContext::Document);
            let mut harness = demo_on_row(30);
            let selection = harness.state().selection;
            press(&mut harness, i);
            assert_eq!(harness.state().selection, selection);
            assert_eq!(texts(harness.state()), Vec::<String>::new());
            assert_eq!(harness.state().document.as_ref().unwrap().generation(), 0);
        }
        Binding::Ui(UiBinding::FillRight) => {
            // Servi par le lot L2 (R-258 amendée) : cellule du Nom, à droite
            // de l'ID (jamais une source) ⇒ rien à recopier, aucun message.
            assert_eq!(context, KeyContext::Table);
            let mut harness = demo_on_row(3);
            press(&mut harness, i);
            let app = harness.state();
            assert_eq!(texts(app), Vec::<String>::new(), "{shortcut:?}");
            assert_eq!(app.document.as_ref().unwrap().generation(), 0);
        }
        Binding::Ui(UiBinding::FocusNextPane | UiBinding::FocusPreviousPane) => {
            // Servi par le lot L4 (H-UI-111) : clavier au volet suivant ou
            // précédent (chronologie affichée), aucun message.
            assert_eq!(context, KeyContext::Document);
            let mut harness = demo_on_row(3);
            press(&mut harness, i);
            harness.run();
            let app = harness.state();
            assert_eq!(texts(app), Vec::<String>::new(), "{shortcut:?}");
            assert_eq!(app.document.as_ref().unwrap().generation(), 0);
            assert!(app.view().edit.editing.is_none());
            assert!(
                harness
                    .ctx
                    .memory(|m| m.has_focus(plannercraft_ui_egui::view::timeline::close_id())),
                "{shortcut:?}"
            );
        }
        Binding::Ui(UiBinding::FieldSettings) => {
            // Servi par le lot L6 (H-UI-115) : « Paramètres de champ » de
            // la colonne de la cellule active, rien d'écrit, aucun message.
            let mut harness = demo_on_row(3);
            let filtered = harness.state().task_view().clone();
            press(&mut harness, i);
            let app = harness.state();
            assert_eq!(texts(app), Vec::<String>::new(), "{shortcut:?}");
            assert!(app.view().picker.as_ref().is_some_and(|p| p.settings.is_some()));
            assert_eq!(app.document.as_ref().unwrap().generation(), 0);
            // Alt+F3 n'est pas pris par F3 (retirer le filtre).
            assert_eq!(*app.task_view(), filtered);
            assert!(app.view().edit.editing.is_none());
        }
        other => panic!("entrée {i} hors de la seconde passe : {other:?}"),
    }
}

/// H-UI-109 : Ctrl+F4 ferme le document hors macOS seulement ; Cmd+W
/// partout (harnais macOS : Ctrl+F4 sans effet).
#[test]
fn ux2_ctrl_f4_not_on_mac() {
    let app = App::new(
        UiState {
            language: plannercraft_ui_egui::LanguagePref::Fr,
            ..UiState::default()
        },
        plannercraft_ui_egui::Env::fixed("2027-01-15T08:00".parse().unwrap()),
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_os(egui::os::OperatingSystem::Mac)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.state_mut().open_demo();
    harness.run();
    harness.key_press_modifiers(Modifiers::CTRL, Key::F4);
    harness.run();
    assert!(harness.state().document.is_some());
    harness.key_press_modifiers(Modifiers::MAC_CMD | Modifiers::COMMAND, Key::W);
    harness.run();
    assert!(harness.state().document.is_none());
}

/// Contrat L0 (`ColumnOp`) : insérer et masquer servis sur
/// `UiState.columns` (préférence, aucune entrée d'annulation) ; déplacer
/// servi depuis le lot L6.
#[test]
fn ux2_column_op_insert_hide() {
    let mut harness = demo_on_row(1);
    let order = |app: &App| app.state.columns.iter().map(|c| c.column).collect::<Vec<_>>();
    harness.state_mut().dispatch(Action::Column(ColumnOp::Insert {
        at: 2,
        column: Column::Work,
    }));
    harness.run();
    assert_eq!(
        order(harness.state()),
        [
            Column::Id,
            Column::Name,
            Column::Work,
            Column::Duration,
            Column::Start,
            Column::Finish,
            Column::Predecessors
        ]
    );
    // Nouvelle colonne : valeur du moteur dès l'image suivante (lot L7 :
    // travail R-184 de la tâche, sans affectation ici).
    let uid = harness.state().selection.unwrap();
    let row = harness.state().rows().unwrap().index_of(uid).unwrap();
    assert_eq!(harness.state().rows().unwrap().text(row, Column::Work), "0 heure");
    // Déjà affichée : déplacée (en fin).
    harness.state_mut().dispatch(Action::Column(ColumnOp::Insert {
        at: 99,
        column: Column::Work,
    }));
    harness.run();
    assert_eq!(order(harness.state()).last(), Some(&Column::Work));
    // Masquer : retirée ; ID et Nom refusés sans message.
    harness.state_mut().dispatch(Action::Column(ColumnOp::Hide(6)));
    harness.state_mut().dispatch(Action::Column(ColumnOp::Hide(0)));
    harness.state_mut().dispatch(Action::Column(ColumnOp::Hide(1)));
    harness.run();
    assert_eq!(order(harness.state()).len(), 6);
    assert!(!harness.state().state.shown(Column::Work));
    assert_eq!(texts(harness.state()), Vec::<String>::new());
    assert_eq!(harness.state().document.as_ref().unwrap().generation(), 0);
    // Déplacer : servi par le lot L6 (H-UI-116), sans message.
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::Move { from: 2, to: 3 }));
    harness.run();
    assert_eq!(texts(harness.state()), Vec::<String>::new());
    assert_eq!(order(harness.state()).get(3), Some(&Column::Duration));
}

/// Contrat L0 (`PaneOp`) : hauteurs bornées, visibilité et forme du volet
/// écrites dans `UiState` ; rien de dessiné (L4, L5) ; F6 « non
/// disponible ».
#[test]
fn ux2_pane_op_writes_state() {
    let mut harness = demo_on_row(1);
    let actions = [
        Action::Pane(PaneOp::SetTimelineHeight(5)),
        Action::Pane(PaneOp::ShowDetails(true)),
        Action::Pane(PaneOp::SetDetailsHeight(u16::MAX)),
        Action::Pane(PaneOp::SetDetailsForm(DetailsForm::PredecessorsSuccessors)),
    ];
    for action in actions {
        harness.state_mut().dispatch(action);
    }
    harness.run();
    let state = &harness.state().state;
    assert_eq!(
        state.timeline_height,
        plannercraft_ui_egui::state::TIMELINE_HEIGHT_MIN
    );
    assert!(state.details.visible);
    assert_eq!(state.details.height, plannercraft_ui_egui::state::PANE_HEIGHT_MAX);
    assert_eq!(state.details.form, DetailsForm::PredecessorsSuccessors);
    assert_eq!(texts(harness.state()), Vec::<String>::new());
    // Relu après relance.
    let (read, warning) = UiState::from_json_lenient(&state.to_json().unwrap());
    assert!(warning.is_none(), "{warning:?}");
    assert_eq!(read.details, state.details);
    assert_eq!(read.timeline_height, state.timeline_height);
}

/// Contrat L0 (messages), servi par L1 : `MessageOp::Dismiss` ne ferme que
/// son message ; `DismissMessage` ferme tous les messages affichés ;
/// `ToggleHistory` ouvre l'historique sans rien publier.
#[test]
fn ux2_message_dismiss_one() {
    let mut harness = demo_on_row(1);
    for _ in 0..2 {
        // Même texte deux fois : un seul message (doublon remplacé).
        harness.state_mut().dispatch(Action::About);
        harness.run();
    }
    harness
        .state_mut()
        .report_state_load(&plannercraft_ui_egui::state::StateLoadError::NotObject);
    harness.run();
    assert_eq!(harness.state().messages().len(), 2);
    let first = harness.state().messages()[0].id;
    harness
        .state_mut()
        .dispatch(Action::Message(MessageOp::Dismiss(first)));
    harness.run();
    assert_eq!(harness.state().messages().len(), 1);
    assert_ne!(harness.state().messages()[0].id, first);
    harness
        .state_mut()
        .dispatch(Action::Message(MessageOp::ToggleHistory));
    harness.run();
    assert_eq!(harness.state().messages().len(), 1);
    assert!(harness.state().message_store().history_open());
    harness.state_mut().dispatch(Action::DismissMessage);
    harness.run();
    assert_eq!(harness.state().messages().len(), 0);
}

/// Contrat L0 (`App::shell_quit`, menu natif macOS de L3) : même garde que
/// Ctrl/Cmd+Q : document modifié ⇒ bandeau ; intact ⇒ fermeture.
#[test]
fn ux2_shell_quit_guarded() {
    let mut harness = demo_on_row(1);
    make_dirty(&mut harness);
    harness.state_mut().shell_quit();
    harness.run();
    assert!(harness.state().confirmation_pending());
    assert!(!crate::files::viewport_commands(&harness).contains(&egui::ViewportCommand::Close));

    let mut harness = demo_on_row(1);
    harness.state_mut().shell_quit();
    harness.step();
    assert!(crate::files::viewport_commands(&harness).contains(&egui::ViewportCommand::Close));
}

/// Contrat L0 (ruban) : la case Détails bascule le volet dans `UiState`
/// (sans dessin avant L4) ; Insérer une colonne et Paramètres de champ
/// ouvrent leur fenêtre (lot L6).
#[test]
fn ux2_ribbon_cmds_contract() {
    use plannercraft_ui_egui::ribbon::spec::RibbonCmd;
    let mut harness = demo_on_row(1);
    harness
        .state_mut()
        .dispatch(Action::Pane(PaneOp::ShowDetails(true)));
    harness.run();
    assert!(harness.state().state.details.visible);
    for cmd in [RibbonCmd::InsertColumn, RibbonCmd::FieldSettings] {
        let action = match cmd {
            RibbonCmd::InsertColumn => Action::Column(ColumnOp::OpenPicker {
                at: plannercraft_ui_egui::action::ACTIVE_COLUMN,
            }),
            _ => Action::Column(ColumnOp::OpenSettings(
                plannercraft_ui_egui::action::ACTIVE_COLUMN,
            )),
        };
        harness.state_mut().dispatch(action);
        harness.run();
        // Servis par le lot L6 : sélecteur, puis « Paramètres de champ ».
        let picker = harness.state().view().picker.as_ref().unwrap();
        assert_eq!(picker.settings.is_some(), cmd == RibbonCmd::FieldSettings);
    }
    assert_eq!(texts(harness.state()), Vec::<String>::new());
    assert_eq!(RibbonCmd::DetailsPane.menu_label(), TextId::MenuDetailsPane);
    assert_eq!(RibbonCmd::CustomFields.menu_label(), TextId::MenuCustomFields);
}
