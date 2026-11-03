//! Seconde passe UX (ADR-0019) : champs personnalisés : colonnes, saisie, renommage (H-UI-118).
//! Module posé vide par le contrat (lot L0), rempli par le lot L9.
//!
//! Repères : projet « P » (`plannercraft_tests::daily::plan_p`, UID n = ID n :
//! 1 « Gros œuvre » ⊃ 2 « Fondations », 3 « Murs », 4 « Livraison » ; 5
//! « Second œuvre » ⊃ 6 « Plâtrerie », 7 « Peinture »).

use egui::accesskit::Role;
use egui::{Event, Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use plannercraft_automation::Batch;
use plannercraft_core::model::custom::{CustomFieldId, CustomKind, CustomValue};
use plannercraft_core::{Moment, Project, TaskUid};
use plannercraft_tests::daily::plan_p;
use plannercraft_ui_egui::action::{ColumnOp, DialogKind, PressMode, PressTarget, SelectOp};
use plannercraft_ui_egui::column::Column;
use plannercraft_ui_egui::field::{CustomCatalog, FieldCategory, category, choices_in, describe, search_in};
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::nav::RowRef;
use plannercraft_ui_egui::ribbon::spec::RibbonCmd;
use plannercraft_ui_egui::view::Pane;
use plannercraft_ui_egui::{Action, App, DocOrigin, Env, LanguagePref, UiState};

use crate::{fr, harness_for};

/// UID `n`.
const fn uid(n: u32) -> TaskUid {
    TaskUid::new(n)
}

/// Champ `kind` numéro `n`.
fn field(kind: CustomKind, n: u8) -> CustomFieldId {
    CustomFieldId::new(kind, n).unwrap()
}

/// Application dans la langue `language` ouvrant `project`.
fn app_with(project: Project, language: LanguagePref) -> App {
    let mut app = App::new(
        UiState {
            language,
            ..crate::fr_light()
        },
        Env::fixed(crate::now()),
    );
    app.open_project(
        project,
        DocOrigin {
            name: "P".to_owned(),
            path: None,
            demo: false,
        },
    );
    app
}

/// Harnais sur le projet « P », en français.
fn p_harness() -> Harness<'static, App> {
    let mut harness = harness_for(app_with(plan_p().project().clone(), LanguagePref::Fr));
    harness.run();
    harness
}

/// Insère `columns` juste après le Nom, dans cet ordre.
fn show_columns(harness: &mut Harness<'static, App>, columns: &[Column]) {
    for (k, column) in columns.iter().enumerate() {
        harness.state_mut().dispatch(Action::Column(ColumnOp::Insert {
            at: k.saturating_add(2),
            column: *column,
        }));
    }
    harness.run();
}

/// Projet du document ouvert.
fn project(app: &App) -> &Project {
    app.document.as_ref().unwrap().project()
}

/// Entrées d'annulation.
fn history_len(app: &App) -> usize {
    app.document.as_ref().unwrap().session.history().entries.len()
}

/// Texte affiché de la cellule (tâche `uid`, `column`).
fn cell(app: &App, uid: TaskUid, column: Column) -> String {
    let rows = app.rows().unwrap();
    rows.text(rows.index_of(uid).unwrap(), column).to_owned()
}

/// Valeur du champ `id` de la tâche `n`.
fn value(app: &App, n: u32, id: CustomFieldId) -> Option<CustomValue> {
    project(app).tasks.get(uid(n)).unwrap().custom.get(&id).cloned()
}

/// Cellule active (`n`, `column`), table focalisée.
fn select(harness: &mut Harness<'static, App>, n: u32, column: Column, mode: PressMode) {
    harness.state_mut().dispatch(Action::SelectOp(SelectOp::Press {
        row: RowRef::Item(uid(n)),
        column,
        target: PressTarget::Cell,
        mode,
    }));
    harness.state_mut().dispatch(Action::FocusPane(Pane::Table));
    harness.run();
}

/// Frappe `text` dans la cellule (`n`, `column`) puis Entrée.
fn type_into(harness: &mut Harness<'static, App>, n: u32, column: Column, text: &str) {
    select(harness, n, column, PressMode::Replace);
    harness.event(Event::Text(text.to_owned()));
    harness.run();
    harness.key_press(Key::Enter);
    harness.run();
}

/// Annule (Ctrl/Cmd+Z).
fn undo(harness: &mut Harness<'static, App>) {
    harness.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    harness.run();
}

/// Applique un lot JSON au document (préparation d'un scénario).
fn exec(harness: &mut Harness<'static, App>, ops: &serde_json::Value) {
    let batch = Batch::from_json(serde_json::json!({ "format_version": 1, "ops": ops })).unwrap();
    harness
        .state_mut()
        .execute(&batch, plannercraft_ui_egui::app::EditOrigin::User)
        .unwrap();
    harness.run();
}

/// Libellé accessible du titre de colonne `title`.
fn header_label_of(title: &str) -> String {
    Texts::new(Language::Fr).get_args(TextId::ColumnHeaderLabel, &[("column", Arg::Str(title.into()))])
}

/// Tape `text` dans le widget focalisé (une image).
fn type_text(harness: &mut Harness<'static, App>, text: &str) {
    harness.event(Event::Text(text.to_owned()));
    harness.run();
}

/// MSPDI de deux tâches : Text1 renommé « Lot », Text2 à formule (champ
/// défini par le fichier, R-265).
fn formula_doc() -> Project {
    let defs = "<ExtendedAttributes>\
        <ExtendedAttribute><FieldID>188743731</FieldID><FieldName>Text1</FieldName><Alias>Lot</Alias></ExtendedAttribute>\
        <ExtendedAttribute><FieldID>188743734</FieldID><FieldName>Text2</FieldName>\
        <Formula>[Name] &amp; \"!\"</Formula></ExtendedAttribute></ExtendedAttributes>";
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID>{defs}\
         <Calendars><Calendar><UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar></Calendar></Calendars>\
         <Tasks><Task><UID>1</UID><ID>1</ID><Name>A</Name><Duration>PT8H0M0S</Duration></Task>\
         <Task><UID>2</UID><ID>2</ID><Name>B</Name><Duration>PT8H0M0S</Duration></Task></Tasks></Project>"
    );
    plannercraft_ui_egui::document::load(xml.as_bytes(), crate::now())
        .unwrap()
        .session
        .project()
        .clone()
}

/// H-UI-118 : le sélecteur liste Texte1-30, Nombre1-20, Date1-10,
/// Indicateur1-20 dans la catégorie Personnalisés, modifiables, après les
/// autres catégories ; « texte3 » trouve Texte3 et Texte30 ; à l'écran, la
/// catégorie et l'entrée « Indicateur20 », qui s'insère.
#[test]
fn h_ui_118_picker_lists_custom() {
    let none = CustomCatalog::default();
    let all = choices_in("", Language::Fr, &none);
    let custom: Vec<CustomFieldId> = all
        .iter()
        .filter_map(|c| match c {
            Column::Custom(id) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(custom.len(), 80);
    for (kind, max) in [
        (CustomKind::Text, 30),
        (CustomKind::Number, 20),
        (CustomKind::Date, 10),
        (CustomKind::Flag, 20),
    ] {
        for n in 1..=max {
            assert!(custom.contains(&field(kind, n)), "{kind:?}{n}");
        }
    }
    // Personnalisés en dernier.
    let first_custom = all.iter().position(|c| matches!(c, Column::Custom(_))).unwrap();
    assert!(all[first_custom..].iter().all(|c| matches!(c, Column::Custom(_))));
    let text3 = Column::Custom(field(CustomKind::Text, 3));
    assert_eq!(category(text3), FieldCategory::Custom);
    assert_eq!(
        describe(text3).map(|d| d.availability),
        Some(plannercraft_ui_egui::field::Availability::Ready)
    );
    assert_eq!(
        search_in("texte3", Language::Fr, &none),
        [text3, Column::Custom(field(CustomKind::Text, 30))]
    );
    assert_eq!(
        search_in("number7", Language::En, &none),
        [Column::Custom(field(CustomKind::Number, 7))]
    );

    let mut harness = p_harness();
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::OpenPicker { at: 2 }));
    harness.run();
    type_text(&mut harness, "indicateur20");
    assert!(harness.query_by_label(&fr(TextId::FieldCatCustom)).is_some());
    crate::click(&mut harness, "Indicateur20");
    let flag20 = Column::Custom(field(CustomKind::Flag, 20));
    assert_eq!(harness.state().state.columns[2].column, flag20);
    // Indicateur vide : « Non », comme MS Project.
    assert_eq!(cell(harness.state(), uid(2), flag20), "Non");
    let _ = harness.get_by_label(&header_label_of("Indicateur20"));
}

/// H-UI-118 : titre = alias (« Lot (Texte1) ») dans l'en-tête, le
/// sélecteur et la recherche ; il suit l'alias à chaque image (renommage
/// annulé ⇒ « Texte1 ») ; le titre de « Paramètres de champ » l'emporte.
#[test]
fn h_ui_118_alias_title() {
    let mut harness = p_harness();
    let text1 = Column::Custom(CustomFieldId::TEXT1);
    show_columns(&mut harness, &[text1]);
    let _ = harness.get_by_label(&header_label_of("Texte1"));
    exec(
        &mut harness,
        &serde_json::json!([{ "op": "custom_field.rename", "field": "text1", "alias": "Lot" }]),
    );
    let _ = harness.get_by_label(&header_label_of("Lot (Texte1)"));
    assert!(harness.query_by_label(&header_label_of("Texte1")).is_none());
    let custom = CustomCatalog::of(project(harness.state()));
    assert!(search_in("lot", Language::Fr, &custom).contains(&text1));
    assert_eq!(search_in("lot (", Language::Fr, &custom), [text1]);
    assert!(!search_in("lot", Language::Fr, &CustomCatalog::default()).contains(&text1));

    // Sélecteur : entrée nommée par l'alias, marquée déjà affichée.
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::OpenPicker { at: 2 }));
    harness.run();
    type_text(&mut harness, "lot");
    let shown = format!("Lot (Texte1) — {}", fr(TextId::FieldPickerShown));
    assert!(harness.query_by_label(&shown).is_some());
    harness.key_press(Key::Escape);
    harness.run();

    undo(&mut harness);
    let _ = harness.get_by_label(&header_label_of("Texte1"));

    // Titre de « Paramètres de champ » : gardé quel que soit l'alias.
    let mut settings = harness.state().state.column(text1).unwrap().clone();
    settings.title = Some("Lot de travaux".to_owned());
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::Settings { at: 2, settings }));
    harness.run();
    let _ = harness.get_by_label(&header_label_of("Lot de travaux"));
}

/// H-UI-118 : saisie au clavier d'un texte, d'un nombre (« 12,5 » en
/// français), d'une date (comme Début) et d'un indicateur (Oui) : une
/// entrée d'annulation chacune, valeur relue à l'identique ; ressaisir la
/// même valeur ne fait rien ; vide efface ; nombre illisible refusé sous
/// la cellule. En anglais, « 12.5 ».
#[test]
fn h_ui_118_edit_text_number_date_flag() {
    let text1 = Column::Custom(CustomFieldId::TEXT1);
    let number1 = Column::Custom(field(CustomKind::Number, 1));
    let date1 = Column::Custom(field(CustomKind::Date, 1));
    let flag1 = Column::Custom(field(CustomKind::Flag, 1));
    let mut harness = p_harness();
    show_columns(&mut harness, &[text1, number1, date1, flag1]);

    type_into(&mut harness, 2, text1, "Gros œuvre");
    assert_eq!(history_len(harness.state()), 1);
    assert_eq!(
        value(harness.state(), 2, CustomFieldId::TEXT1),
        Some(CustomValue::Text("Gros œuvre".to_owned()))
    );
    assert_eq!(cell(harness.state(), uid(2), text1), "Gros œuvre");

    type_into(&mut harness, 2, number1, "12,5");
    assert_eq!(history_len(harness.state()), 2);
    assert_eq!(
        value(harness.state(), 2, field(CustomKind::Number, 1)),
        Some(CustomValue::Number(125_000))
    );
    assert_eq!(cell(harness.state(), uid(2), number1), "12,5");

    type_into(&mut harness, 3, date1, "04/01/2027");
    assert_eq!(history_len(harness.state()), 3);
    let start: Moment = "2027-01-04T08:00".parse().unwrap();
    assert_eq!(
        value(harness.state(), 3, field(CustomKind::Date, 1)),
        Some(CustomValue::Date(start))
    );
    let labels = Texts::new(Language::Fr);
    assert_eq!(
        cell(harness.state(), uid(3), date1),
        plannercraft_ui_egui::format::format_date(start, labels.format_labels())
    );

    // Récapitulative : champ saisi, comme MS Project.
    type_into(&mut harness, 1, flag1, "oui");
    assert_eq!(history_len(harness.state()), 4);
    assert_eq!(
        value(harness.state(), 1, field(CustomKind::Flag, 1)),
        Some(CustomValue::Flag(true))
    );
    assert_eq!(cell(harness.state(), uid(1), flag1), "Oui");

    // Même valeur : aucun lot (R-260 d) ; nombre au point en français : la
    // même valeur aussi (lecture de R-140).
    type_into(&mut harness, 2, number1, "12,50");
    type_into(&mut harness, 2, text1, "Gros œuvre");
    assert_eq!(history_len(harness.state()), 4);

    // Illisible : refus sous la cellule, rien écrit ; Échap ferme.
    type_into(&mut harness, 2, number1, "douze");
    assert!(
        harness
            .state()
            .view()
            .edit
            .editing
            .as_ref()
            .unwrap()
            .error
            .is_some()
    );
    harness.key_press(Key::Escape);
    harness.run();
    assert_eq!(history_len(harness.state()), 4);

    // Vide efface (Suppr sur la cellule, R-259) : une entrée.
    select(&mut harness, 2, text1, PressMode::Replace);
    harness.key_press(Key::Delete);
    harness.run();
    assert_eq!(value(harness.state(), 2, CustomFieldId::TEXT1), None);
    assert_eq!(cell(harness.state(), uid(2), text1), "");
    assert_eq!(history_len(harness.state()), 5);

    // Annuler : une saisie à la fois.
    undo(&mut harness);
    assert_eq!(cell(harness.state(), uid(2), text1), "Gros œuvre");
    undo(&mut harness);
    assert_eq!(cell(harness.state(), uid(1), flag1), "Non");

    // Anglais : « 12.5 », affiché « 12.5 ».
    let mut harness = harness_for(app_with(plan_p().project().clone(), LanguagePref::En));
    harness.run();
    show_columns(&mut harness, &[number1]);
    type_into(&mut harness, 3, number1, "12.5");
    assert_eq!(
        value(harness.state(), 3, field(CustomKind::Number, 1)),
        Some(CustomValue::Number(125_000))
    );
    assert_eq!(cell(harness.state(), uid(3), number1), "12.5");
    assert_eq!(history_len(harness.state()), 1);
}

/// H-UI-118 : Projet → Champs personnalisés… : Texte1 renommé
/// « Lot », type Nombre puis Nombre1 renommé « Quantité », OK ⇒ **un** lot
/// (une entrée d'annulation) ; Annuler (Ctrl+Z) retire les deux ; alias de
/// 51 caractères refusé dans la fenêtre, rien appliqué.
#[test]
fn h_ui_118_rename_one_undo() {
    let mut harness = p_harness();
    assert_eq!(RibbonCmd::CustomFields.menu_label(), TextId::MenuCustomFields);
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::CustomFields));
    harness.run();
    let _ = harness.get_by_label(&fr(TextId::CustomFieldsTitle));
    harness.get_by_role_and_label(Role::TextInput, "Texte1").click();
    harness.run();
    type_text(&mut harness, "Lot");
    crate::click(&mut harness, &fr(TextId::CustomTypeNumber));
    harness.get_by_role_and_label(Role::TextInput, "Nombre1").click();
    harness.run();
    type_text(&mut harness, "Quantité");
    assert_eq!(history_len(harness.state()), 0);
    crate::click(&mut harness, &fr(TextId::TaskInfoOk));
    assert!(harness.state().view().dialog.is_none());
    assert_eq!(history_len(harness.state()), 1);
    let defs = &project(harness.state()).custom_fields;
    assert_eq!(
        defs.get(&CustomFieldId::TEXT1).and_then(|d| d.alias.as_deref()),
        Some("Lot")
    );
    assert_eq!(
        defs.get(&field(CustomKind::Number, 1))
            .and_then(|d| d.alias.as_deref()),
        Some("Quantité")
    );
    undo(&mut harness);
    assert!(project(harness.state()).custom_fields.is_empty());

    assert_eq!(history_len(harness.state()), 0);

    // OK sans changement : aucun lot.
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::CustomFields));
    harness.run();
    crate::click(&mut harness, &fr(TextId::TaskInfoOk));
    assert!(harness.state().view().dialog.is_none());
    assert_eq!(history_len(harness.state()), 0);

    // Trop long : refus dans la fenêtre.
    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::CustomFields));
    harness.run();
    harness.get_by_role_and_label(Role::TextInput, "Texte2").click();
    harness.run();
    type_text(&mut harness, &"x".repeat(51));
    crate::click(&mut harness, &fr(TextId::TaskInfoOk));
    assert!(harness.state().view().dialog.is_some());
    assert!(
        harness
            .query_by_label(&fr(TextId::CustomFieldsAliasTooLong))
            .is_some()
    );
    harness.key_press(Key::Escape);
    harness.run();
    assert!(harness.state().view().dialog.is_none());
    assert!(project(harness.state()).custom_fields.is_empty());
}

/// H-UI-118 : alias et valeurs (texte, nombre, date, indicateur)
/// enregistrés en MSPDI puis relus à l'identique ; mêmes textes de
/// cellules dans le document relu.
#[test]
fn h_ui_118_save_reload_identical() {
    let text1 = Column::Custom(CustomFieldId::TEXT1);
    let number3 = Column::Custom(field(CustomKind::Number, 3));
    let date2 = Column::Custom(field(CustomKind::Date, 2));
    let flag4 = Column::Custom(field(CustomKind::Flag, 4));
    let columns = [text1, number3, date2, flag4];
    let mut harness = p_harness();
    show_columns(&mut harness, &columns);
    exec(
        &mut harness,
        &serde_json::json!([{ "op": "custom_field.rename", "field": "text1", "alias": "Lot" }]),
    );
    type_into(&mut harness, 2, text1, "Gros œuvre");
    type_into(&mut harness, 3, number3, "-0,25");
    type_into(&mut harness, 6, date2, "15/01/2027");
    type_into(&mut harness, 7, flag4, "Oui");
    assert_eq!(history_len(harness.state()), 5);
    let before = project(harness.state()).clone();
    let xml = harness
        .state()
        .document
        .as_ref()
        .unwrap()
        .session
        .save_bytes(&plannercraft_mspdi::write::WriteOptions::default())
        .xml;
    let back = plannercraft_ui_egui::document::load(&xml, crate::now())
        .unwrap()
        .session
        .project()
        .clone();
    assert_eq!(back.custom_fields, before.custom_fields);
    for (a, b) in back.tasks.iter().zip(before.tasks.iter()) {
        assert_eq!(a.custom, b.custom, "{}", a.name);
    }
    let mut again = harness_for(app_with(back, LanguagePref::Fr));
    again.run();
    show_columns(&mut again, &columns);
    for n in 1..=7 {
        for column in columns {
            assert_eq!(
                cell(again.state(), uid(n), column),
                cell(harness.state(), uid(n), column),
                "{n} {column:?}"
            );
        }
    }
    assert_eq!(cell(again.state(), uid(3), number3), "-0,25");
    let _ = again.get_by_label(&header_label_of("Lot (Texte1)"));
}

/// H-UI-118 : un champ à formule (Text2, R-265) est absent du sélecteur et
/// grisé dans « Champs personnalisés » ; Text1 (alias du fichier) est
/// proposé sous son alias.
#[test]
fn h_ui_118_formula_field_hidden() {
    let project = formula_doc();
    let text2 = field(CustomKind::Text, 2);
    let custom = CustomCatalog::of(&project);
    assert!(custom.hidden.contains(&text2));
    let all = choices_in("", Language::Fr, &custom);
    assert!(!all.contains(&Column::Custom(text2)));
    assert!(all.contains(&Column::Custom(CustomFieldId::TEXT1)));
    assert_eq!(all.iter().filter(|c| matches!(c, Column::Custom(_))).count(), 79);

    let mut harness = harness_for(app_with(project, LanguagePref::Fr));
    harness.run();
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::OpenPicker { at: 2 }));
    harness.run();
    type_text(&mut harness, "texte");
    assert!(harness.query_by_label("Texte2").is_none());
    assert!(harness.query_by_label("Texte3").is_some());
    assert!(harness.query_by_label("Lot (Texte1)").is_some());
    harness.key_press(Key::Escape);
    harness.run();

    harness
        .state_mut()
        .dispatch(Action::ShowDialog(DialogKind::CustomFields));
    harness.run();
    assert!(
        harness
            .get_by_role_and_label(Role::TextInput, "Texte2")
            .accesskit_node()
            .is_disabled()
    );
    assert!(
        !harness
            .get_by_role_and_label(Role::TextInput, "Texte1")
            .accesskit_node()
            .is_disabled()
    );
}

/// H-UI-118 : Ctrl/Cmd+D sur Texte1 (tâches 2 à 4) recopie la valeur de
/// la tâche 2 vers le bas : un lot, une entrée d'annulation ; recopier les
/// mêmes valeurs ne fait rien ; copier Nombre1 et Texte1 de la tâche 2 puis
/// coller sur la tâche 6 : un lot (même chemin que la saisie).
#[test]
fn h_ui_118_paste_fill_custom() {
    let text1 = Column::Custom(CustomFieldId::TEXT1);
    let mut harness = p_harness();
    show_columns(&mut harness, &[text1]);
    type_into(&mut harness, 2, text1, "GO");
    assert_eq!(history_len(harness.state()), 1);
    select(&mut harness, 2, text1, PressMode::Replace);
    select(&mut harness, 4, text1, PressMode::Extend);
    harness.key_press_modifiers(Modifiers::COMMAND, Key::D);
    harness.run();
    for n in 2..=4 {
        assert_eq!(
            value(harness.state(), n, CustomFieldId::TEXT1),
            Some(CustomValue::Text("GO".to_owned())),
            "{n}"
        );
    }
    assert_eq!(history_len(harness.state()), 2);
    // Recopier encore : valeurs identiques, aucun lot (R-260 d).
    harness.key_press_modifiers(Modifiers::COMMAND, Key::D);
    harness.run();
    assert_eq!(history_len(harness.state()), 2);
    undo(&mut harness);
    assert_eq!(value(harness.state(), 3, CustomFieldId::TEXT1), None);
    assert_eq!(cell(harness.state(), uid(2), text1), "GO");

    // Copier Texte1 et Nombre1 de la tâche 2, coller sur la tâche 6.
    let number1 = Column::Custom(field(CustomKind::Number, 1));
    show_columns(&mut harness, &[number1]);
    type_into(&mut harness, 2, number1, "3,5");
    let entries = history_len(harness.state());
    select(&mut harness, 2, number1, PressMode::Replace);
    select(&mut harness, 2, text1, PressMode::Extend);
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
        .expect("texte copié");
    harness.run();
    assert_eq!(copied, "3,5\tGO");
    select(&mut harness, 6, number1, PressMode::Replace);
    harness.event(Event::Paste(copied));
    harness.run();
    assert_eq!(
        history_len(harness.state()),
        entries + 1,
        "{:?}",
        harness.state().messages()
    );
    assert_eq!(
        value(harness.state(), 6, field(CustomKind::Number, 1)),
        Some(CustomValue::Number(35_000))
    );
    assert_eq!(
        value(harness.state(), 6, CustomFieldId::TEXT1),
        Some(CustomValue::Text("GO".to_owned()))
    );
}

/// H-UI-114 (relecture finale ux2, C6) : Texte1 renommé « Lot », « lot »
/// puis Entrée ⇒ « Lot (Texte1) » inséré (correspondance en début de
/// titre), pas « Pilotée par l'effort » (« pi-lot-ée », premier du
/// catalogue).
#[test]
fn h_ui_114_picker_best_match_on_enter() {
    let mut harness = p_harness();
    let text1 = Column::Custom(CustomFieldId::TEXT1);
    exec(
        &mut harness,
        &serde_json::json!([{ "op": "custom_field.rename", "field": "text1", "alias": "Lot" }]),
    );
    let effort_before = harness.state().state.column(Column::EffortDriven).is_some();
    assert!(harness.state().state.column(text1).is_none());
    harness
        .state_mut()
        .dispatch(Action::Column(ColumnOp::OpenPicker { at: 2 }));
    harness.run();
    type_text(&mut harness, "lot");
    let custom = CustomCatalog::of(project(harness.state()));
    let list = choices_in("lot", Language::Fr, &custom);
    assert!(list.contains(&Column::EffortDriven), "{list:?}");
    harness.key_press(Key::Enter);
    harness.run();
    let state = &harness.state().state;
    assert!(state.column(text1).is_some(), "Lot (Texte1) non inséré");
    assert_eq!(state.column(Column::EffortDriven).is_some(), effort_before);
    // Fonction pure : rangs de la meilleure correspondance.
    let best = plannercraft_ui_egui::field::best_choice("lot", Language::Fr, &custom, &list);
    assert_eq!(list[best], text1);
    assert_eq!(
        plannercraft_ui_egui::field::best_choice("", Language::Fr, &custom, &list),
        0
    );
}
