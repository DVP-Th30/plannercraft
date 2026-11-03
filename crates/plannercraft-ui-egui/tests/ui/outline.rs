//! Repli dans l'interface (J8, lot L9, H-UI-34) : triangles de la colonne
//! Nom (derrière `App::set_outline_triangles`, faux jusqu'au lot L15),
//! « Afficher jusqu'au niveau n » au menu Affichage, Alt+Maj+−/+ (et =) ;
//! repli à la main non persisté.

use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable as _;
use plannercraft_core::TaskUid;
use plannercraft_ui_egui::i18n::{Arg, Language, TextId, Texts};
use plannercraft_ui_egui::view::task_view::TaskViewOp;
use plannercraft_ui_egui::{Action, App};

use crate::filters::{harness_p, open_p_in, shown, view_op};
use crate::{click, fr, fr_light, harness_for, new_app, now};

fn triangle(id: TextId, name: &str) -> String {
    Texts::new(Language::Fr).get_args(id, &[("name", Arg::Str(name.to_owned().into()))])
}

/// H-UI-34 : triangle de « Gros œuvre » : clic ⇒ 2, 3, 4 masquées et
/// libellé « Déplier » ; nouveau clic ⇒ dépliée. Triangles affichés par
/// défaut depuis le lot L15 ; sans le drapeau, aucun triangle.
#[test]
fn h_ui_34_triangle() {
    let collapse = triangle(TextId::OutlineCollapseRow, "Gros œuvre");
    let mut off = new_app();
    off.set_outline_triangles(false);
    open_p_in(&mut off);
    let mut plain = harness_for(off);
    plain.run();
    assert!(
        plain.query_by_label(&collapse).is_none(),
        "drapeau faux : aucun triangle"
    );
    let mut harness = harness_p();
    assert!(harness.state().outline_triangles(), "triangles par défaut (L15)");
    click(&mut harness, &collapse);
    assert_eq!(shown(harness.state()), [1, 5, 6, 7]);
    let expand = triangle(TextId::OutlineExpandRow, "Gros œuvre");
    click(&mut harness, &expand);
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5, 6, 7]);
    // Niveau 1 : les deux triangles disent « Déplier » ; déplier 5
    // n'ouvre que 5.
    view_op(&mut harness, TaskViewOp::SetOutlineLevel(Some(1)));
    assert!(harness.query_by_label(&expand).is_some());
    click(&mut harness, &triangle(TextId::OutlineExpandRow, "Second œuvre"));
    assert_eq!(shown(harness.state()), [1, 5, 6, 7]);
    assert_eq!(harness.state().document.as_ref().unwrap().generation(), 0);
}

/// H-UI-34 : Affichage → Niveau hiérarchique → Niveau 1 ⇒ lignes 1 et 5
/// seules ; Tous les niveaux ⇒ toutes.
#[test]
fn h_ui_34_level_1() {
    let mut harness = harness_p();
    click(&mut harness, &fr(TextId::MenuView));
    click(&mut harness, &fr(TextId::MenuOutlineLevel));
    let level_1 = Texts::new(Language::Fr).get_args(TextId::OutlineLevelN, &[("count", Arg::Count(1))]);
    click(&mut harness, &level_1);
    assert_eq!(shown(harness.state()), [1, 5]);
    click(&mut harness, &fr(TextId::MenuView));
    click(&mut harness, &fr(TextId::MenuOutlineLevel));
    click(&mut harness, &fr(TextId::OutlineAll));
    assert_eq!(shown(harness.state()).len(), 7);
}

/// H-UI-34 : Alt+Maj+− masque les sous-tâches de 1 ; Alt+Maj++ et
/// Alt+Maj+= les affichent ; sur une sous-tâche (6), Alt+Maj+− replie sa
/// récapitulative 5, qui devient active.
#[test]
fn h_ui_34_alt_shift_minus_plus() {
    let mut harness = harness_p();
    let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
    harness
        .state_mut()
        .dispatch(Action::Select(Some(TaskUid::new(1))));
    harness.run();
    harness.key_press_modifiers(alt_shift, Key::Minus);
    harness.run();
    assert_eq!(shown(harness.state()), [1, 5, 6, 7]);
    harness.key_press_modifiers(alt_shift, Key::Plus);
    harness.run();
    assert_eq!(shown(harness.state()).len(), 7);
    harness.key_press_modifiers(alt_shift, Key::Minus);
    harness.run();
    harness.key_press_modifiers(alt_shift, Key::Equals);
    harness.run();
    assert_eq!(shown(harness.state()).len(), 7);
    harness
        .state_mut()
        .dispatch(Action::Select(Some(TaskUid::new(6))));
    harness.run();
    harness.key_press_modifiers(alt_shift, Key::Minus);
    harness.run();
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5]);
    assert_eq!(harness.state().selection, Some(TaskUid::new(5)));
    // Sous « niveau 1 », Alt+Maj++ sur 1 ne déplie que 1.
    view_op(&mut harness, TaskViewOp::SetOutlineLevel(Some(1)));
    harness
        .state_mut()
        .dispatch(Action::Select(Some(TaskUid::new(1))));
    harness.run();
    harness.key_press_modifiers(alt_shift, Key::Plus);
    harness.run();
    assert_eq!(shown(harness.state()), [1, 2, 3, 4, 5]);
}

/// Le repli à la main n'est pas persisté : ni dans `UiState`, ni à la
/// réouverture du document.
#[test]
fn collapse_not_persisted() {
    let mut app = App::new(fr_light(), plannercraft_ui_egui::Env::fixed(now()));
    open_p_in(&mut app);
    app.apply_task_view(TaskViewOp::ToggleCollapsed(TaskUid::new(1)));
    assert_eq!(shown(&app), [1, 5, 6, 7]);
    // Hors des préférences du ruban (ADR-0017 : `ribbon.collapsed`, le
    // repli du ruban, pas celui des tâches).
    let mut value = serde_json::to_value(&app.state).unwrap();
    value.as_object_mut().unwrap().remove("ribbon");
    let json = value.to_string();
    assert!(!json.contains("collapsed"), "{json}");
    assert_eq!(app.state.task_view.outline_level, None);
    open_p_in(&mut app);
    assert!(app.collapsed().is_empty());
    assert_eq!(shown(&app).len(), 7);
}
