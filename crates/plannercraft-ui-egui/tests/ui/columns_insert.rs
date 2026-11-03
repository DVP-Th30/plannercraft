//! Seconde passe UX (ADR-0019, lot L6) : insertion de colonnes, sélecteur
//! de champs, « Paramètres de champ », déplacement d'une colonne par son
//! en-tête, catalogue des champs (H-UI-114 à H-UI-117). Le pointeur est
//! conduit par de vrais événements egui (`select_mouse.rs`). Repère :
//! projet « P » (UID n = ID n, ligne affichée n − 1), colonnes par défaut
//! ID, Nom, Durée, Début, Fin, Prédécesseurs.

#![allow(
    clippy::arithmetic_side_effects,
    reason = "tests : positions du pointeur et rangs bornés par l'exemple"
)]

use egui::{CursorIcon, Event, Key, Modifiers, Pos2, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::action::{ACTIVE_COLUMN, ColumnOp};
use plannercraft_ui_egui::column::{CellAlign, Column, ColumnState, MAX_COLUMNS};
use plannercraft_ui_egui::field::{Availability, FieldCategory, catalog, choices, describe};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::selection::SelectionKind;
use plannercraft_ui_egui::view::table;
use plannercraft_ui_egui::{Action, App, DocOrigin};

use crate::select_mouse::{move_to, press_at, release_at};
use crate::{fr, new_app};

/// Harnais sur le projet « P », images de 1/60 s.
fn p_harness() -> Harness<'static, App> {
    let mut app = new_app();
    app.open_project(
        plan_p().project().clone(),
        DocOrigin {
            name: "P".to_owned(),
            path: None,
            demo: false,
        },
    );
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .with_pixels_per_point(1.0)
        .with_theme(egui::Theme::Light)
        .with_os(egui::os::OperatingSystem::Nix)
        .with_step_dt(1.0 / 60.0)
        .with_max_steps(8)
        .build_ui_state(plannercraft_ui_egui::ui, app);
    harness.run();
    harness
}

/// Colonnes affichées, dans l'ordre.
fn order(app: &App) -> Vec<Column> {
    app.state.columns.iter().map(|c| c.column).collect()
}

/// Libellé « Colonne Durée » d'un titre (`title` : titre affiché).
fn header_label_of(title: &str) -> String {
    Texts::new(Language::Fr).get_args(TextId::ColumnHeaderLabel, &[("column", Arg::Str(title.into()))])
}

/// Libellé du titre de `column` (titre par défaut).
fn header_label(column: Column) -> String {
    header_label_of(&fr(column.title()))
}

/// Centre du titre de `column`.
fn header_point(harness: &Harness<'static, App>, column: Column) -> Pos2 {
    harness.get_by_label(&header_label(column)).rect().center()
}

/// Ouvre le menu de l'en-tête `column`.
fn header_menu(harness: &mut Harness<'static, App>, column: Column) {
    harness.get_by_label(&header_label(column)).click_secondary();
    harness.run();
}

/// Messages affichés.
fn texts(app: &App) -> Vec<String> {
    app.messages().iter().map(|m| m.text.clone()).collect()
}

/// Génération du document (rien d'écrit : inchangée) et entrées annulables.
fn untouched(app: &App) -> bool {
    let doc = app.document.as_ref().unwrap();
    doc.generation() == 0 && doc.session.history().entries.is_empty()
}

/// Le sélecteur est ouvert (zone de recherche présente).
fn picker_open(harness: &Harness<'static, App>) -> bool {
    harness.state().view().picker.is_some()
        && harness.query_by_label(&fr(TextId::FieldPickerSearch)).is_some()
}

/// Tape `text` dans le widget focalisé (une image).
fn type_text(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// Nœud d'entrée du sélecteur pour `label` (bouton de la liste).
fn entry_disabled(harness: &Harness<'static, App>, label: &str) -> bool {
    harness.get_by_label(label).accesskit_node().is_disabled()
}

/// H-UI-114 (COL-01) : clic droit sur « Durée » → Insérer une colonne →
/// « % ach » → Entrée ⇒ « % achevé » **à gauche** de Durée, pas en fin ;
/// aucune entrée d'annulation, rien d'écrit.
#[test]
fn h_ui_114_insert_left_of_clicked() {
    let mut harness = p_harness();
    header_menu(&mut harness, Column::Duration);
    crate::click(&mut harness, &fr(TextId::MenuInsertColumn));
    assert!(picker_open(&harness), "sélecteur fermé");
    type_text(&mut harness, "% ach");
    harness.key_press(Key::Enter);
    harness.run();
    assert!(harness.state().view().picker.is_none());
    assert_eq!(
        order(harness.state()),
        [
            Column::Id,
            Column::Name,
            Column::PercentComplete,
            Column::Duration,
            Column::Start,
            Column::Finish,
            Column::Predecessors,
        ]
    );
    assert!(untouched(harness.state()));
    assert_eq!(texts(harness.state()), Vec::<String>::new());
    // Le titre de la colonne insérée est affiché.
    let _ = harness.get_by_label(&header_label(Column::PercentComplete));
}

/// H-UI-114 : colonne « Ajouter une nouvelle colonne » en bout de table
/// (allumée pour le test) : clic ⇒ sélecteur ; « Marge libre » ⇒ colonne
/// ajoutée à sa place, en fin.
#[test]
fn h_ui_114_add_new_column_slot() {
    let mut harness = p_harness();
    // Affichée par défaut depuis le lot L10.
    assert!(table::add_column_slot(&harness.ctx), "affichée depuis L10");
    // Table élargie : la colonne tient à droite des colonnes par défaut
    // (642 points, largeur par défaut de la table).
    harness.state_mut().dispatch(Action::SetTableWidth(900));
    harness.run();
    let slot_label = fr(TextId::AddNewColumn);
    let slot = harness.get_by_label(&slot_label);
    let last = header_point(&harness, Column::Predecessors);
    assert!(slot.rect().center().x > last.x, "à droite de la dernière colonne");
    slot.click();
    harness.run();
    assert!(picker_open(&harness));
    type_text(&mut harness, "marge libre");
    crate::click(&mut harness, &fr(TextId::ColFreeSlack));
    assert_eq!(order(harness.state()).last(), Some(&Column::FreeSlack));
    assert_eq!(order(harness.state()).len(), 7);
    assert!(untouched(harness.state()));
}

/// H-UI-114 : recherche par sous-chaîne sans accents ni casse : « trav »
/// ⇒ Travail, Travail réel, Travail restant… ; « duree » ⇒ Durée ; rien ⇒
/// « Aucun champ ne correspond. ».
#[test]
fn h_ui_114_picker_search() {
    let none = std::collections::BTreeMap::new();
    let found = choices("trav", Language::Fr, &none);
    for column in [
        Column::Work,
        Column::ActualWork,
        Column::RemainingWork,
        Column::BaselineWork,
        Column::WorkVariance,
        Column::PercentWorkComplete,
    ] {
        assert!(found.contains(&column), "{column:?}");
    }
    assert!(!found.contains(&Column::Duration));
    assert!(choices("duree", Language::Fr, &none).contains(&Column::Duration));
    assert!(choices("WORK", Language::En, &none).contains(&Column::Work));

    let mut harness = p_harness();
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::OpenPicker { at: 2 }));
    harness.run();
    assert!(picker_open(&harness));
    type_text(&mut harness, "trav");
    for id in [TextId::ColWork, TextId::ColActualWork, TextId::ColRemainingWork] {
        assert!(harness.query_by_label(&fr(id)).is_some(), "{id:?}");
    }
    // « Durée » (déjà affichée) n'est plus listée.
    let shown = format!("{} — {}", fr(TextId::ColDuration), fr(TextId::FieldPickerShown));
    assert!(harness.query_by_label(&shown).is_none());
    type_text(&mut harness, "zzz");
    assert!(harness.query_by_label(&fr(TextId::FieldPickerEmpty)).is_some());
}

/// H-UI-114 : clavier du sélecteur : ↓ ↓ puis Entrée insère la troisième
/// entrée choisissable ; ↑ au-delà du début reste sur la première ; Échap
/// ferme sans rien insérer.
#[test]
fn h_ui_114_picker_keyboard() {
    let mut harness = p_harness();
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::OpenPicker { at: 3 }));
    harness.run();
    type_text(&mut harness, "plus t");
    let none = std::collections::BTreeMap::new();
    let list = choices("plus t", Language::Fr, &none);
    assert!(list.len() >= 3, "{list:?}");
    harness.key_press(Key::ArrowUp);
    harness.run();
    assert_eq!(harness.state().view().picker.as_ref().unwrap().highlighted, 0);
    harness.key_press(Key::ArrowDown);
    harness.run();
    harness.key_press(Key::ArrowDown);
    harness.run();
    assert_eq!(harness.state().view().picker.as_ref().unwrap().highlighted, 2);
    harness.key_press(Key::Enter);
    harness.run();
    assert_eq!(order(harness.state()).get(3), Some(&list[2]));
    // Échap : fermé, rien inséré.
    let before = order(harness.state());
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::OpenPicker { at: 2 }));
    harness.run();
    assert!(picker_open(&harness));
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().view().picker.is_none());
    assert_eq!(order(harness.state()), before);
}

/// H-UI-114 : un champ déjà affiché est marqué « déjà affiché » ;
/// l'insérer le **déplace** (une colonne au plus une fois).
#[test]
fn h_ui_114_existing_field_moves() {
    let mut harness = p_harness();
    header_menu(&mut harness, Column::Duration);
    crate::click(&mut harness, &fr(TextId::MenuInsertColumn));
    type_text(&mut harness, "fin");
    let label = format!("{} — {}", fr(TextId::ColFinish), fr(TextId::FieldPickerShown));
    crate::click(&mut harness, &label);
    assert_eq!(
        order(harness.state()),
        [
            Column::Id,
            Column::Name,
            Column::Finish,
            Column::Duration,
            Column::Start,
            Column::Predecessors,
        ]
    );
}

/// H-UI-117 : les coûts sont listés grisés avec leur raison (« Coûts : non
/// calculés dans cette version ») et ne s'insèrent pas, ni au clic ni par
/// Entrée.
#[test]
fn h_ui_114_costs_greyed_reason() {
    for column in [
        Column::Cost,
        Column::FixedCost,
        Column::ActualCost,
        Column::RemainingCost,
        Column::BaselineCost,
        Column::CostVariance,
    ] {
        assert_eq!(
            describe(column).map(|d| d.availability),
            Some(Availability::Unavailable(TextId::FieldCostsNotComputed)),
            "{column:?}"
        );
    }
    let mut harness = p_harness();
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::OpenPicker { at: 2 }));
    harness.run();
    type_text(&mut harness, "coût fixe");
    assert!(entry_disabled(&harness, &fr(TextId::ColFixedCost)));
    harness.key_press(Key::Enter);
    harness.run();
    assert!(!harness.state().state.shown(Column::FixedCost));
    let none = std::collections::BTreeMap::new();
    assert_eq!(choices("coût", Language::Fr, &none), Vec::<Column>::new());
}

/// H-UI-114, P63 : 64 colonnes au plus ; la 65e est refusée par un
/// message nommé, rien n'est ajouté.
#[test]
fn h_ui_114_max_64() {
    let mut harness = p_harness();
    let candidates: Vec<Column> = catalog()
        .iter()
        .filter(|d| !matches!(d.availability, Availability::Unavailable(_)))
        .map(|d| d.column)
        .filter(|c| !harness.state().state.shown(*c))
        .collect();
    assert!(candidates.len() > MAX_COLUMNS);
    // Actions mises en file, appliquées à l'image suivante : juste assez
    // pour atteindre 64.
    let room = MAX_COLUMNS - harness.state().state.columns.len();
    for (k, column) in candidates.iter().take(room).enumerate() {
        let at = harness.state().state.columns.len() + k;
        harness
            .state_mut()
            .dispatch(Action::Column(ColumnOp::Insert { at, column: *column }));
    }
    harness.run();
    assert_eq!(harness.state().state.columns.len(), MAX_COLUMNS);
    assert_eq!(texts(harness.state()), Vec::<String>::new());
    let extra = candidates
        .iter()
        .copied()
        .find(|c| !harness.state().state.shown(*c))
        .unwrap();
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::Insert { at: 2, column: extra }));
    harness.run();
    assert_eq!(harness.state().state.columns.len(), MAX_COLUMNS);
    assert!(!harness.state().state.shown(extra));
    assert_eq!(texts(harness.state()), [fr(TextId::ColumnsMaxReached)]);
    assert!(untouched(harness.state()));
}

/// H-UI-115 : « Paramètres de champ » par Alt+F3 (cellule active dans
/// Durée) puis par le menu d'en-tête : titre « Durée prévue » ⇒ en-tête
/// renommé, valeurs inchangées ; titre vidé ⇒ titre par défaut ;
/// alignements gardés dans `ColumnState` ; Annuler ⇒ rien.
#[test]
fn h_ui_115_field_settings_title_align() {
    let mut harness = p_harness();
    let p = crate::select_mouse::cell_point(&harness, 1, Column::Duration);
    crate::select_mouse::click_at(&mut harness, p, Modifiers::NONE);
    assert_eq!(harness.state().view().cursor.column, Column::Duration);
    let before = harness
        .state()
        .rows()
        .unwrap()
        .text(1, Column::Duration)
        .to_owned();
    harness.key_press_modifiers(Modifiers::ALT, Key::F3);
    harness.run();
    let picker = harness.state().view().picker.as_ref().unwrap();
    assert_eq!(picker.at, 2);
    assert!(picker.settings.is_some());
    let _ = harness.get_by_label(&fr(TextId::FieldSettingsTitle));
    type_text(&mut harness, "Durée prévue");
    crate::click(&mut harness, &fr(TextId::TaskInfoOk));
    assert!(harness.state().view().picker.is_none());
    let state = harness.state().state.column(Column::Duration).unwrap().clone();
    assert_eq!(state.title.as_deref(), Some("Durée prévue"));
    let _ = harness.get_by_label(&header_label_of("Durée prévue"));
    assert!(harness.query_by_label(&header_label(Column::Duration)).is_none());
    assert_eq!(harness.state().rows().unwrap().text(1, Column::Duration), before);
    assert!(untouched(harness.state()));

    // Menu d'en-tête : alignements posés par la fenêtre (action), titre
    // vidé ⇒ défaut.
    harness
        .get_by_label(&header_label_of("Durée prévue"))
        .click_secondary();
    harness.run();
    crate::click_where(&mut harness, |l| l.starts_with(&fr(TextId::MenuFieldSettings)));
    assert!(harness.state().view().picker.as_ref().unwrap().settings.is_some());
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().view().picker.is_none());
    assert_eq!(harness.state().state.column(Column::Duration).unwrap(), &state);
    harness.state_mut().dispatch(Action::Column(ColumnOp::Settings {
        at: 2,
        settings: ColumnState {
            title: None,
            align: Some(CellAlign::Left),
            header_align: Some(CellAlign::Right),
            ..state
        },
    }));
    harness.run();
    let state = harness.state().state.column(Column::Duration).unwrap();
    assert_eq!(state.data_align(), CellAlign::Left);
    assert_eq!(state.title_align(), CellAlign::Right);
    let _ = harness.get_by_label(&header_label(Column::Duration));
    // Ribbon / menu : même action (OpenSettings sur la colonne active).
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::OpenSettings(ACTIVE_COLUMN)));
    harness.run();
    assert_eq!(harness.state().view().picker.as_ref().unwrap().at, 2);
}

/// Sélectionne la colonne `column` entière par un clic sur son titre.
fn select_column(harness: &mut Harness<'static, App>, column: Column) {
    let p = header_point(harness, column);
    press_at(harness, p, Modifiers::NONE);
    release_at(harness, p);
    let app = harness.state();
    assert_eq!(app.selected().kind(), SelectionKind::Cells);
    assert_eq!(app.view().cursor.column, column);
}

/// Point à `dx` du bord gauche du titre de `column`.
fn header_left(harness: &Harness<'static, App>, column: Column, dx: f32) -> Pos2 {
    let rect = harness.get_by_label(&header_label(column)).rect();
    pos2(rect.left() + dx, rect.center().y)
}

/// Glisse depuis le titre de `from` jusqu'à `to` en quatre pas (bouton
/// tenu).
fn drag_header(harness: &mut Harness<'static, App>, from: Column, to: Pos2) {
    let start = header_point(harness, from);
    press_at(harness, start, Modifiers::NONE);
    for k in 1..=4_u8 {
        let t = f32::from(k) / 4.0;
        move_to(harness, start + (to - start) * t);
    }
}

/// H-UI-116 : « Fin » sélectionnée, glissée avant « Durée » ⇒ ordre Nom,
/// Fin, Durée, Début ; trait d'insertion et curseur pendant le glisser ;
/// aucune entrée d'annulation.
#[test]
fn h_ui_116_drag_selected_header_moves() {
    let mut harness = p_harness();
    select_column(&mut harness, Column::Finish);
    let to = header_left(&harness, Column::Duration, 12.0);
    drag_header(&mut harness, Column::Finish, to);
    let moving = table::column_move(&harness.ctx).expect("déplacement ouvert");
    assert_eq!((moving.from, moving.to, moving.refused), (4, 2, false));
    assert_eq!(harness.output().platform_output.cursor_icon, CursorIcon::Grabbing);
    release_at(&mut harness, to);
    assert!(table::column_move(&harness.ctx).is_none());
    assert_eq!(
        order(harness.state()),
        [
            Column::Id,
            Column::Name,
            Column::Finish,
            Column::Duration,
            Column::Start,
            Column::Predecessors,
        ]
    );
    assert!(untouched(harness.state()));
    // Échap pendant le glisser : rien ne bouge.
    select_column(&mut harness, Column::Predecessors);
    let to = header_left(&harness, Column::Name, 12.0);
    drag_header(&mut harness, Column::Predecessors, to);
    assert!(table::column_move(&harness.ctx).is_some());
    harness.key_press(Key::Escape);
    harness.step();
    release_at(&mut harness, to);
    assert_eq!(order(harness.state()).last(), Some(&Column::Predecessors));
}

/// H-UI-116 : l'ID reste en premier : un lâcher devant lui est refusé
/// (curseur interdit, rien ne bouge) ; `Move` vers le rang 0 ⇒ refus nommé.
#[test]
fn h_ui_116_id_stays_first() {
    let mut harness = p_harness();
    select_column(&mut harness, Column::Duration);
    let to = header_left(&harness, Column::Id, 6.0);
    drag_header(&mut harness, Column::Duration, to);
    let moving = table::column_move(&harness.ctx).unwrap();
    assert!(moving.refused && moving.to == 0);
    assert_eq!(
        harness.output().platform_output.cursor_icon,
        CursorIcon::NotAllowed
    );
    release_at(&mut harness, to);
    let default = order(harness.state());
    assert_eq!(default.first(), Some(&Column::Id));
    assert_eq!(default.get(2), Some(&Column::Duration));
    assert_eq!(texts(harness.state()), Vec::<String>::new());
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::Move { from: 2, to: 0 }));
    harness.run();
    assert_eq!(order(harness.state()), default);
    assert_eq!(texts(harness.state()), [fr(TextId::ColumnIdStaysFirst)]);
}

/// H-UI-116, H-UI-90 gardée : sur une colonne non sélectionnée, l'appui
/// sélectionne et le glisser étend la sélection de colonnes ; l'ordre ne
/// change pas.
#[test]
fn h_ui_116_unselected_header_selects() {
    let mut harness = p_harness();
    let before = order(harness.state());
    let to = header_point(&harness, Column::Finish);
    drag_header(&mut harness, Column::Duration, to);
    assert!(table::column_move(&harness.ctx).is_none());
    release_at(&mut harness, to);
    let app = harness.state();
    assert_eq!(order(app), before);
    assert_eq!(app.view().cursor.column, Column::Finish);
    assert_eq!(app.view().cursor.anchor_column, Some(Column::Duration));
    assert_eq!(app.selected().len(), app.rows().unwrap().len());
}

/// H-UI-117 : chaque champ de la liste de H-UI-117 est au catalogue, une
/// seule fois, dans sa catégorie, titré en français et en anglais.
#[test]
fn h_ui_117_catalog_complete() {
    use FieldCategory as C;
    let mut expected = vec![
        (Column::Name, C::General),
        (Column::Duration, C::Scheduling),
        (Column::Start, C::Scheduling),
        (Column::Finish, C::Scheduling),
        (Column::Predecessors, C::Scheduling),
        (Column::Successors, C::Scheduling),
        (Column::PercentComplete, C::Progress),
        (Column::Mode, C::Scheduling),
        (Column::Active, C::General),
        (Column::Priority, C::General),
        (Column::ConstraintType, C::Scheduling),
        (Column::ConstraintDate, C::Scheduling),
        (Column::Deadline, C::Scheduling),
        (Column::Work, C::Scheduling),
        (Column::TaskType, C::Scheduling),
        (Column::EffortDriven, C::Scheduling),
        (Column::Milestone, C::General),
        (Column::Notes, C::General),
        (Column::ActualStart, C::Progress),
        (Column::ActualFinish, C::Progress),
        (Column::ActualDuration, C::Progress),
        (Column::RemainingDuration, C::Progress),
        (Column::PercentWorkComplete, C::Progress),
        (Column::UniqueId, C::General),
        (Column::Wbs, C::General),
        (Column::OutlineLevel, C::General),
        (Column::Summary, C::General),
        (Column::Critical, C::Scheduling),
        (Column::TotalSlack, C::Slack),
        (Column::FreeSlack, C::Slack),
        (Column::EarlyStart, C::Scheduling),
        (Column::EarlyFinish, C::Scheduling),
        (Column::LateStart, C::Scheduling),
        (Column::LateFinish, C::Scheduling),
        (Column::StartVariance, C::Variance),
        (Column::FinishVariance, C::Variance),
        (Column::DurationVariance, C::Variance),
        (Column::WorkVariance, C::Variance),
        (Column::ResourceNames, C::Resources),
        (Column::Created, C::General),
        (Column::ActualWork, C::Progress),
        (Column::RemainingWork, C::Progress),
        (Column::Cost, C::Costs),
        (Column::FixedCost, C::Costs),
        (Column::ActualCost, C::Costs),
        (Column::RemainingCost, C::Costs),
        (Column::BaselineCost, C::Costs),
        (Column::CostVariance, C::Costs),
    ];
    // Lignes de base 0 à 10 : Début, Fin, Durée, Travail (Début et Fin de
    // la ligne de base 0 sont des colonnes d'avant la passe).
    expected.extend(
        [Column::BaselineStart, Column::BaselineFinish]
            .into_iter()
            .map(|c| (c, C::Baseline)),
    );
    expected.extend(
        Column::FIELDS
            .into_iter()
            .filter(|c| format!("{c:?}").starts_with("Baseline") && *c != Column::BaselineCost)
            .map(|c| (c, C::Baseline)),
    );
    assert_eq!(
        expected.iter().filter(|(_, c)| *c == C::Baseline).count(),
        44,
        "4 champs × 11 lignes de base"
    );
    let fr_texts = Texts::new(Language::Fr);
    let en_texts = Texts::new(Language::En);
    for (column, cat) in &expected {
        let desc = describe(*column).unwrap_or_else(|| panic!("{column:?} absent"));
        assert_eq!(desc.category, *cat, "{column:?}");
        for t in [&fr_texts, &en_texts] {
            let title = t.get(desc.title);
            assert!(!title.is_empty() && title != desc.title.key(), "{column:?}");
        }
        assert_eq!(catalog().iter().filter(|d| d.column == *column).count(), 1);
    }
    assert!(catalog().len() >= 80, "{}", catalog().len());
}

/// Format → Colonnes (ruban et barre de menus, COL-02) : « Insérer une
/// colonne » ouvre le sélecteur sur la colonne active ; les cases
/// d'affichage passent dans le sous-menu « Affichage du tableau ».
#[test]
fn h_ui_114_format_columns_menu() {
    let mut harness = p_harness();
    let p = crate::select_mouse::cell_point(&harness, 1, Column::Start);
    crate::select_mouse::click_at(&mut harness, p, Modifiers::NONE);
    crate::ribbon_click(
        &mut harness,
        plannercraft_ui_egui::ribbon::RibbonTab::Format,
        &fr(TextId::RibbonColumns),
    );
    assert!(harness.query_by_label(&fr(TextId::MenuTableDisplay)).is_some());
    assert!(
        harness.query_by_label(&fr(TextId::ColCritical)).is_none(),
        "cases repliées"
    );
    crate::click(&mut harness, &fr(TextId::MenuInsertColumn));
    let picker = harness.state().view().picker.as_ref().expect("sélecteur");
    assert_eq!(picker.at, 3, "à gauche de Début, colonne active");
    assert!(picker.settings.is_none());
}
