//! Seconde passe UX (ADR-0019 D4) : Cmd+Q et gestes de fenêtre (H-UI-108,
//! H-UI-109). Ctrl/Cmd+Q, Ctrl/Cmd+W, Ctrl+F4 (hors macOS) et Ctrl/Cmd+P
//! sont lus dans tous les états : saisie de cellule validée d'abord (refus ⇒
//! on reste), fenêtre fermée comme par Annuler, puis garde des
//! modifications. Le système est simulé par `Harness::with_os` (macOS et
//! Linux ; Windows pour Ctrl+F4).

use egui::os::OperatingSystem;
use egui::{Event, Key, Modifiers, ViewportCommand};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use plannercraft_ui_egui::action::{ConfirmChoice, DialogKind};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::dialog::DialogBody;
use plannercraft_ui_egui::dialog::filter::FilterDialogMode;
use plannercraft_ui_egui::i18n::TextId;
use plannercraft_ui_egui::keymap::{KeyScope, KeyState, web_intercepts};
use plannercraft_ui_egui::rows::RowKind;
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::view::task_view::TaskViewOp;
use plannercraft_ui_egui::{Action, App, Env, ViewKind};

use crate::files::{SaveReply, edit_first_leaf, sim, viewport_commands};
use crate::{fr, fr_light, now, open_demo};

/// Harnais 1280×720 sans rendu, système `os`.
fn harness_os(os: OperatingSystem) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 720.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(os)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, App::new(fr_light(), Env::fixed(now())))
}

/// Exemple ouvert sur le système `os`.
fn demo_os(os: OperatingSystem) -> Harness<'static, App> {
    let mut h = harness_os(os);
    open_demo(&mut h);
    h
}

/// Exemple ouvert et modifié (durée de la première tâche simple).
fn dirty_demo(os: OperatingSystem) -> Harness<'static, App> {
    let mut h = demo_os(os);
    edit_first_leaf(&mut h, "17j");
    h
}

/// Appui (avec `modifiers`) dans une seule image, sans relâcher : la
/// réponse est lue dans la sortie de cette image.
fn press_once(h: &mut Harness<'static, App>, modifiers: Modifiers, key: Key) {
    h.input_mut().events.push(Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    });
    h.step();
}

/// Ctrl/Cmd+Q dans une image.
fn cmd_q(h: &mut Harness<'static, App>) {
    press_once(h, Modifiers::COMMAND, Key::Q);
}

/// La fenêtre se ferme dans la dernière image.
fn closed(h: &Harness<'static, App>) -> bool {
    viewport_commands(h).contains(&ViewportCommand::Close)
}

/// Aucune fenêtre ouverte (fenêtres de l'application, « Trier », « Filtre »,
/// sélecteur de champ et « Paramètres de champ »).
fn no_window(app: &App) -> bool {
    app.view().dialog.is_none()
        && app.sort_dialog().is_none()
        && app.filter_dialog().is_none()
        && app.view().picker.is_none()
}

/// Indice de la première tâche simple de l'exemple.
fn first_leaf(app: &App) -> usize {
    let rows = app.rows().unwrap();
    (0..rows.len())
        .find(|&i| rows.row(i).is_some_and(|row| row.kind == RowKind::Leaf))
        .unwrap()
}

/// Saisie ouverte sur la Durée de la première tâche simple, texte `text`.
fn start_duration_edit(h: &mut Harness<'static, App>, text: &str) {
    let i = first_leaf(h.state());
    crate::select_mouse::click_cell(h, i, Column::Duration, Modifiers::NONE);
    h.event(Event::Text(text.to_owned()));
    h.run();
    let editing = h.state().view().edit.editing.as_ref().expect("saisie ouverte");
    assert_eq!(editing.buffer, text);
}

/// Ouvre la fenêtre `which` sur l'exemple ; vérifie qu'elle est ouverte.
fn open_window(h: &mut Harness<'static, App>, which: &str) {
    match which {
        "sort" => {
            h.state_mut()
                .dispatch(Action::TaskView(TaskViewOp::OpenSortDialog));
        }
        "filter" => {
            h.state_mut()
                .dispatch(Action::TaskView(TaskViewOp::OpenFilterDialog(
                    FilterDialogMode::Custom,
                )));
        }
        "paste" => {
            // Tampon interne d'abord : ligne copiée depuis la table.
            let i = first_leaf(h.state());
            let uid = h.state().rows().unwrap().row(i).unwrap().uid;
            h.state_mut().dispatch(Action::Select(Some(uid)));
            h.state_mut().dispatch(Action::FocusPane(Pane::Table));
            h.run();
            h.event(Event::Copy);
            h.run();
            h.state_mut()
                .dispatch(Action::ShowDialog(DialogKind::PasteSpecial));
        }
        "info" => {
            let i = first_leaf(h.state());
            let uid = h.state().rows().unwrap().row(i).unwrap().uid;
            h.state_mut()
                .dispatch(Action::ShowDialog(DialogKind::TaskInformation(uid)));
        }
        "picker" => {
            // Sélecteur « Insérer une colonne » (egui::Modal).
            h.state_mut().view_mut().picker = Some(plannercraft_ui_egui::field::FieldPicker::insert_at(1));
        }
        _ => panic!("fenêtre inconnue {which}"),
    }
    h.run();
    assert!(!no_window(h.state()), "{which} : fenêtre non ouverte");
}

/// H-UI-108 : document modifié, Ctrl/Cmd+Q ⇒ bandeau « Enregistrer les
/// modifications ? », la fenêtre reste ouverte ; table focalisée, Gantt
/// focalisé, feuille des ressources ; macOS et Linux.
#[test]
fn h_ui_108_cmd_q_dirty_confirms() {
    for os in [OperatingSystem::Mac, OperatingSystem::Nix] {
        for state in ["table", "gantt", "resources"] {
            let mut h = dirty_demo(os);
            match state {
                "table" => h.state_mut().dispatch(Action::FocusPane(Pane::Table)),
                "gantt" => h.state_mut().dispatch(Action::FocusPane(Pane::Gantt)),
                _ => h.state_mut().dispatch(Action::SetView(ViewKind::ResourceSheet)),
            }
            h.run();
            cmd_q(&mut h);
            assert!(h.state().confirmation_pending(), "{os:?} {state}");
            assert!(!closed(&h), "{os:?} {state}");
            h.run();
            let name = h.state().document.as_ref().unwrap().origin.name.clone();
            let title = plannercraft_ui_egui::i18n::Texts::new(plannercraft_ui_egui::Language::Fr).get_args(
                TextId::ConfirmUnsavedTitle,
                &[("name", plannercraft_ui_egui::i18n::Arg::Str(name.into()))],
            );
            assert!(
                h.query_by_label(&title).is_some(),
                "{os:?} {state} : bandeau absent"
            );
        }
    }
}

/// H-UI-108 : document intact (ou aucun), Ctrl/Cmd+Q ⇒ la fenêtre se ferme.
#[test]
fn h_ui_108_cmd_q_clean_closes() {
    for os in [OperatingSystem::Mac, OperatingSystem::Nix] {
        let mut h = demo_os(os);
        cmd_q(&mut h);
        assert!(closed(&h), "{os:?}");
        assert!(!h.state().confirmation_pending());
        let mut h = harness_os(os);
        h.run();
        cmd_q(&mut h);
        assert!(closed(&h), "{os:?} : accueil");
    }
}

/// H-UI-108 (QUIT-01 passe 2) : fenêtre ouverte (Trier, Filtre, Collage
/// spécial, Informations, sélecteur de champ), Cmd+Q ⇒ fenêtre fermée comme
/// par Annuler, puis bandeau ; rien d'appliqué par la fenêtre.
#[test]
fn h_ui_108_cmd_q_in_modal() {
    for which in ["sort", "filter", "paste", "info", "picker"] {
        let mut h = dirty_demo(OperatingSystem::Mac);
        let entries = h
            .state()
            .document
            .as_ref()
            .unwrap()
            .session
            .history()
            .entries
            .len();
        open_window(&mut h, which);
        cmd_q(&mut h);
        assert!(no_window(h.state()), "{which} : fenêtre encore ouverte");
        assert!(h.state().confirmation_pending(), "{which} : pas de bandeau");
        assert!(!closed(&h), "{which}");
        assert_eq!(
            h.state()
                .document
                .as_ref()
                .unwrap()
                .session
                .history()
                .entries
                .len(),
            entries,
            "{which} : la fenêtre a écrit"
        );
        assert!(!h.state().task_view().is_filtered(), "{which}");
        // Document intact : la fenêtre se ferme aussi, puis l'application.
        let mut h = demo_os(OperatingSystem::Nix);
        open_window(&mut h, which);
        cmd_q(&mut h);
        assert!(no_window(h.state()), "{which}");
        assert!(closed(&h), "{which} : intact, fermeture attendue");
    }
}

/// H-UI-108 (QUIT-02 passe 2) : saisie ouverte ⇒ validée d'abord (lot
/// appliqué, une entrée d'annulation), puis bandeau ; saisie refusée ⇒ le
/// refus reste sous la cellule, rien d'autre (ni bandeau, ni fermeture).
#[test]
fn h_ui_108_cmd_q_while_editing() {
    for os in [OperatingSystem::Mac, OperatingSystem::Nix] {
        // Saisie valide sur un document intact.
        let mut h = demo_os(os);
        start_duration_edit(&mut h, "9");
        let entries = h
            .state()
            .document
            .as_ref()
            .unwrap()
            .session
            .history()
            .entries
            .len();
        cmd_q(&mut h);
        assert!(h.state().view().edit.editing.is_none(), "{os:?}");
        assert!(!closed(&h), "{os:?} : fermée sans garde");
        h.run();
        let doc = h.state().document.as_ref().unwrap();
        assert!(doc.is_dirty(), "{os:?} : saisie non appliquée");
        assert_eq!(doc.session.history().entries.len(), entries + 1, "{os:?}");
        assert!(h.state().confirmation_pending(), "{os:?} : pas de bandeau");

        // Saisie refusée : on reste.
        let mut h = demo_os(os);
        start_duration_edit(&mut h, "zz");
        cmd_q(&mut h);
        let editing = h.state().view().edit.editing.as_ref().expect("saisie gardée");
        assert!(editing.error.is_some(), "{os:?} : refus non affiché");
        assert!(!h.state().confirmation_pending(), "{os:?}");
        assert!(!closed(&h), "{os:?}");
        h.run();
        assert!(!closed(&h), "{os:?}");
        assert!(!h.state().document.as_ref().unwrap().is_dirty());
    }
}

/// H-UI-108 : le raccourci de sortie intégré d'egui est coupé ; pendant
/// une saisie (où le `KEYMAP` ne lit rien), Cmd+Q ne produit aucun
/// `ViewportCommand::Close` sans passer par la garde.
#[test]
fn h_ui_108_no_builtin_quit_shortcut() {
    let mut h = dirty_demo(OperatingSystem::Mac);
    assert!(h.ctx.options(|options| options.quit_shortcuts.is_empty()));
    start_duration_edit(&mut h, "9");
    cmd_q(&mut h);
    assert!(!closed(&h));
    assert!(h.state().confirmation_pending());
    // Saisie dans un autre champ de texte (zone de recherche) : même garde.
    let mut h = dirty_demo(OperatingSystem::Nix);
    h.state_mut().dispatch(Action::TaskView(TaskViewOp::OpenSearch));
    h.run();
    assert!(h.ctx.text_edit_focused(), "recherche sans focus");
    cmd_q(&mut h);
    assert!(!closed(&h));
    assert!(h.state().confirmation_pending());
}

/// H-UI-108 : « Quitter PlannerCraft » du menu natif (`shell_quit`) suit le
/// même chemin que Cmd+Q : fenêtre fermée, saisie validée, garde.
#[test]
fn h_ui_108_shell_quit_guarded() {
    let mut h = dirty_demo(OperatingSystem::Mac);
    open_window(&mut h, "sort");
    h.state_mut().shell_quit();
    h.step();
    assert!(no_window(h.state()));
    assert!(h.state().confirmation_pending());
    assert!(!closed(&h));
    h.state_mut().dispatch(Action::Confirm(ConfirmChoice::Discard));
    h.step();
    assert!(closed(&h));

    let mut h = demo_os(OperatingSystem::Mac);
    start_duration_edit(&mut h, "zz");
    h.state_mut().shell_quit();
    h.step();
    assert!(h.state().view().edit.editing.is_some(), "saisie refusée gardée");
    assert!(!closed(&h));
    assert!(!h.state().confirmation_pending());

    let mut h = demo_os(OperatingSystem::Mac);
    h.state_mut().shell_quit();
    h.step();
    assert!(closed(&h), "document intact : fermeture");
}

/// H-UI-109 : Ctrl/Cmd+W ⇒ Fermer le document, gardé comme Quitter ;
/// « Enregistrer » ⇒ écran d'accueil, la fenêtre reste ouverte.
#[test]
fn h_ui_109_cmd_w_closes_document_guarded() {
    let mut s = sim(SaveReply::Ok);
    let h = &mut s.harness;
    open_demo(h);
    edit_first_leaf(h, "17j");
    open_window(h, "filter");
    press_once(h, Modifiers::COMMAND, Key::W);
    assert!(no_window(h.state()));
    assert!(h.state().confirmation_pending());
    assert!(h.state().document.is_some());
    h.run();
    crate::click(h, &fr(TextId::ConfirmSave));
    h.run();
    assert_eq!(s.requests.borrow().len(), 1);
    let h = &mut s.harness;
    assert!(h.state().document.is_none(), "document non fermé");
    assert!(!closed(h));
    assert!(h.query_by_label(&fr(TextId::WelcomeOpenDemo)).is_some());

    // Document intact (macOS) : fermé tout de suite, sans bandeau.
    let mut h = demo_os(OperatingSystem::Mac);
    press_once(&mut h, Modifiers::COMMAND, Key::W);
    h.run();
    assert!(h.state().document.is_none());
    assert!(!closed(&h));
}

/// H-UI-109 : Ctrl+F4 ferme le document sous Windows (et Linux), jamais
/// sous macOS (touche laissée au système).
#[test]
fn h_ui_109_ctrl_f4_windows() {
    let mut h = dirty_demo(OperatingSystem::Windows);
    press_once(&mut h, Modifiers::CTRL, Key::F4);
    assert!(h.state().confirmation_pending());
    h.state_mut().dispatch(Action::Confirm(ConfirmChoice::Discard));
    h.run();
    assert!(h.state().document.is_none());
    assert!(!closed(&h));

    let mut h = dirty_demo(OperatingSystem::Mac);
    press_once(&mut h, Modifiers::CTRL, Key::F4);
    h.run();
    assert!(!h.state().confirmation_pending());
    assert!(h.state().document.is_some());
}

/// H-UI-109 : Ctrl/Cmd+P ⇒ « Mise en page » en mode impression, depuis la
/// table, une fenêtre ouverte (fermée d'abord) ou une saisie (validée
/// d'abord) ; sans document : rien.
#[test]
fn h_ui_109_cmd_p_print_mode() {
    let print_mode = |app: &App| {
        app.view()
            .dialog
            .as_ref()
            .is_some_and(|d| matches!(&d.body, DialogBody::PageSetup(state) if state.print_mode))
    };
    for os in [OperatingSystem::Mac, OperatingSystem::Nix] {
        let mut h = demo_os(os);
        press_once(&mut h, Modifiers::COMMAND, Key::P);
        h.run();
        assert!(print_mode(h.state()), "{os:?}");

        let mut h = demo_os(os);
        open_window(&mut h, "sort");
        press_once(&mut h, Modifiers::COMMAND, Key::P);
        h.run();
        assert!(h.state().sort_dialog().is_none(), "{os:?}");
        assert!(print_mode(h.state()), "{os:?}");

        let mut h = demo_os(os);
        start_duration_edit(&mut h, "9");
        press_once(&mut h, Modifiers::COMMAND, Key::P);
        h.run();
        assert!(h.state().view().edit.editing.is_none(), "{os:?}");
        assert!(h.state().document.as_ref().unwrap().is_dirty(), "{os:?}");
        assert!(print_mode(h.state()), "{os:?}");

        let mut h = harness_os(os);
        h.run();
        press_once(&mut h, Modifiers::COMMAND, Key::P);
        h.run();
        assert!(h.state().view().dialog.is_none(), "{os:?} : accueil");
    }
}

/// H-UI-109 : sur le web, Ctrl/Cmd+W, P et Q restent au navigateur
/// (fermer l'onglet, imprimer, quitter) : jamais retenus par la garde
/// `keydown`, quel que soit l'état.
#[test]
fn h_ui_109_web_leaves_w_p() {
    let all = KeyScope::All(KeyState {
        document: true,
        table: true,
    });
    for key in [Key::W, Key::P, Key::Q] {
        assert!(!web_intercepts(all, Modifiers::COMMAND, key), "{key:?}");
        assert!(!web_intercepts(all, Modifiers::CTRL, key), "{key:?}");
    }
    assert!(!web_intercepts(all, Modifiers::CTRL, Key::F4));
    // Une touche ordinaire reste retenue (contrôle du test).
    assert!(web_intercepts(all, Modifiers::COMMAND, Key::F));
}

/// H-UI-108 (relecture finale ux2, C1) : document intact, brouillon du
/// volet de détails illisible (Durée « abc »), Cmd+Q ⇒ OK implicite
/// refusé, on reste : refus sous les champs, brouillon gardé, ni bandeau
/// ni fermeture.
#[test]
fn h_ui_108_cmd_q_details_refusal_stays() {
    for os in [OperatingSystem::Mac, OperatingSystem::Nix] {
        let mut h = demo_os(os);
        let i = first_leaf(h.state());
        let uid = h.state().rows().unwrap().row(i).unwrap().uid;
        h.state_mut()
            .dispatch(Action::Pane(plannercraft_ui_egui::action::PaneOp::ShowDetails(
                true,
            )));
        h.state_mut().dispatch(Action::Select(Some(uid)));
        h.run();
        {
            let st = h
                .state_mut()
                .view_mut()
                .details
                .as_mut()
                .expect("volet synchronisé");
            st.draft.as_mut().expect("brouillon").general.duration = "abc".to_owned();
            st.dirty = true;
        }
        cmd_q(&mut h);
        assert!(!closed(&h), "{os:?} : fermée malgré le refus");
        h.run();
        assert!(!closed(&h), "{os:?}");
        let app = h.state();
        assert!(!app.confirmation_pending(), "{os:?}");
        let st = app.view().details.as_ref().unwrap();
        assert!(st.dirty, "{os:?} : brouillon perdu");
        assert!(!st.errors.is_empty(), "{os:?} : refus non affiché");
        assert!(!app.document.as_ref().unwrap().is_dirty());
    }
}
