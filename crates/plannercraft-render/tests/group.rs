//! Regroupement des tâches (finitions P3, ADR-0022 D4) : groupes, ordre,
//! bornes lues du planning (H-R17), scène et SVG regroupés (H-UI-129).
//!
//! Projet « P » (`common::project_p`) : 1 « Gros œuvre » ⊃ 2 « Fondations »,
//! 3 « Murs », 4 jalon « Livraison » ; 5 « Second œuvre » ⊃ 6 « Plâtrerie »,
//! 7 « Peinture » ; liens 2→3, 3→4, 3→6, 6→7.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

mod common;

use common::{project_p, uid, xml_texts};
use plannercraft_core::{
    Assignment, AssignmentUid, CustomFieldId, CustomValue, Project, Resource, ResourceUid, Schedule,
    TaskOutcome,
};
use plannercraft_render::geometry::{Class, NoTexts, Prim, Scene, Viewport, layout};
use plannercraft_render::group::{GroupKey, group_label, group_tasks};
use plannercraft_render::style::{BarRole, GroupBy, PlanStyle, TemplateId, Variant, template};
use plannercraft_render::{GanttRequest, Lang, RangeSpec, RenderLimits, RowFilter, Theme, Zoom, svg_styled};

/// P avec Texte1 : 2, 3 « Lot 10 » ; 4, 7 « Lot 2 » ; 6 sans valeur.
fn p_lots() -> Project {
    let mut p = project_p();
    for (n, lot) in [(2, "Lot 10"), (3, "Lot 10"), (4, "Lot 2"), (7, "Lot 2")] {
        p.tasks
            .get_mut(uid(n))
            .unwrap()
            .custom
            .insert(CustomFieldId::TEXT1, CustomValue::Text(lot.to_owned()));
    }
    p
}

fn grouped(by: GroupBy) -> PlanStyle {
    let mut st = template(TemplateId::Classique);
    st.group_by = Some(by);
    st
}

fn scene(p: &Project, s: &Schedule, st: &PlanStyle) -> Scene {
    layout(p, s, st, &NoTexts, &Viewport::new(Variant::Light, Lang::Fr)).0
}

fn texts(sc: &Scene) -> Vec<&str> {
    sc.items
        .iter()
        .filter(|i| i.class == Class::TextStrong)
        .filter_map(|i| match &i.prim {
            Prim::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn span(s: &Schedule, n: u32) -> (plannercraft_core::Moment, plannercraft_core::Moment) {
    match s.tasks.get(&uid(n)) {
        Some(TaskOutcome::Scheduled(t)) => (t.start, t.finish),
        _ => panic!("tâche {n} non planifiée"),
    }
}

/// H-R17 : groupes en ordre naturel (« Lot 2 » avant « Lot 10 »), sans
/// valeur en dernier, feuilles en ordre des ID, récapitulatives écartées ;
/// bornes = plus petit début et plus grande fin du moteur.
#[test]
fn h_r17_custom_text_groups_order_and_bounds() {
    let p = p_lots();
    let s = plannercraft_core::schedule(&p);
    let groups = group_tasks(&p, &s, GroupBy::Custom(CustomFieldId::TEXT1), &|_| true);
    let positions: Vec<Vec<usize>> = groups.iter().map(|g| g.tasks.clone()).collect();
    assert_eq!(positions, [vec![3, 6], vec![1, 2], vec![5]]);
    let labels: Vec<String> = groups
        .iter()
        .map(|g| group_label(GroupBy::Custom(CustomFieldId::TEXT1), g, Lang::Fr))
        .collect();
    assert_eq!(labels, ["Lot 2", "Lot 10", "(sans valeur)"]);
    assert_eq!(groups[2].key, GroupKey::Empty);
    // Lot 10 : début de 2, fin de 3, lus du planning.
    let lot10 = &groups[1];
    assert_eq!(lot10.start, Some((span(&s, 2).0, 1)));
    assert_eq!(lot10.finish, Some((span(&s, 3).1, 2)));
    // Filtre appliqué avant le groupement : 7 écartée.
    let filtered = group_tasks(&p, &s, GroupBy::Custom(CustomFieldId::TEXT1), &|i| i != 6);
    assert_eq!(filtered[0].tasks, [3]);
}

/// H-R17 : Critique avant non critique ; Jalons d'abord.
#[test]
fn h_r17_flag_groups_order() {
    let p = project_p();
    let s = plannercraft_core::schedule(&p);
    let crit = group_tasks(&p, &s, GroupBy::Critical, &|_| true);
    assert_eq!(crit[0].key, GroupKey::Flag(true));
    assert_eq!(group_label(GroupBy::Critical, &crit[0], Lang::Fr), "Critique");
    let miles = group_tasks(&p, &s, GroupBy::Milestone, &|_| true);
    assert_eq!(miles[0].key, GroupKey::Flag(true));
    assert_eq!(miles[0].tasks, [3]);
    assert_eq!(
        group_label(GroupBy::Milestone, &miles[1], Lang::En),
        "Other tasks"
    );
}

/// H-R17 : une tâche à deux ressources est sous chacune ; sans ressource :
/// « (sans valeur) » en dernier ; noms en ordre naturel.
#[test]
fn h_r17_resource_groups_repeat_tasks() {
    let mut p = project_p();
    for (r, name) in [(1, "Maçon"), (2, "électricien")] {
        p.resources
            .push(Resource::new(ResourceUid::new(r), name.to_owned()))
            .unwrap();
    }
    for (a, task, r) in [(1, 2, 1), (2, 3, 1), (3, 3, 2)] {
        p.assignments
            .push(Assignment::new(
                AssignmentUid::new(a),
                uid(task),
                Some(ResourceUid::new(r)),
            ))
            .unwrap();
    }
    let s = plannercraft_core::schedule(&p);
    let groups = group_tasks(&p, &s, GroupBy::Resource, &|_| true);
    let keys: Vec<(GroupKey, Vec<usize>)> = groups.iter().map(|g| (g.key.clone(), g.tasks.clone())).collect();
    assert_eq!(
        keys,
        [
            (GroupKey::Resource(2), vec![2]),
            (GroupKey::Resource(1), vec![1, 2]),
            (GroupKey::Empty, vec![3, 5, 6]),
        ]
    );
}

/// H-UI-129 (impression, PDF, SVG) : scène regroupée — en-tête par groupe
/// (valeur et nombre, Début et Fin), barre récapitulative du plus petit
/// début à la plus grande fin, feuilles au niveau 2, aucun lien.
#[test]
fn h_ui_129_scene_grouped_rows() {
    let p = p_lots();
    let s = plannercraft_core::schedule(&p);
    let st = grouped(GroupBy::Custom(CustomFieldId::TEXT1));
    let sc = scene(&p, &s, &st);
    let shape: Vec<(Option<u32>, u16, bool)> = sc
        .rows
        .iter()
        .map(|r| (r.uid.map(plannercraft_core::TaskUid::get), r.level, r.summary))
        .collect();
    assert_eq!(
        shape,
        [
            (None, 1, true),
            (Some(4), 2, false),
            (Some(7), 2, false),
            (None, 1, true),
            (Some(2), 2, false),
            (Some(3), 2, false),
            (None, 1, true),
            (Some(6), 2, false),
        ]
    );
    assert_eq!(sc.links, []);
    let header = sc.rows[3].bar.unwrap();
    assert_eq!(header.role, BarRole::Summary);
    assert_eq!(header.rect.x, sc.rows[4].bar.unwrap().rect.x);
    assert_eq!(header.rect.right(), sc.rows[5].bar.unwrap().rect.right());
    let strong = texts(&sc);
    for t in ["Lot 2 (2 tâches)", "Lot 10 (2 tâches)", "(sans valeur) (1 tâche)"] {
        assert!(strong.contains(&t), "{t} absent de {strong:?}");
    }
    // Sans regroupement : la scène d'avant, octet pour octet.
    let plain = scene(&p, &s, &template(TemplateId::Classique));
    assert_eq!(plain.rows.len(), 7);
    assert_ne!(plain.links, []);
}

/// H-UI-129 : le SVG suit le style regroupé (`render_gantt`).
#[test]
fn h_ui_129_svg_grouped() {
    let p = p_lots();
    let s = plannercraft_core::schedule(&p);
    let st = grouped(GroupBy::Custom(CustomFieldId::TEXT1));
    let req = GanttRequest {
        range: RangeSpec::Auto,
        zoom: Zoom::Week,
        filter: RowFilter::default(),
        language: Lang::Fr,
        theme: Theme::Light,
        limits: RenderLimits::default(),
    };
    let (svg, report) = svg_styled(&p, &s, &req, &st, &NoTexts);
    let texts = xml_texts(&svg);
    assert!(texts.iter().any(|t| t == "Lot 10 (2 tâches)"), "{texts:?}");
    assert_eq!(report.shown, 8);
    // Récapitulatives 1 et 5 masquées.
    assert_eq!(report.hidden, 2);
}

/// Un champ personnalisé autre que texte ne regroupe pas (style refusé).
#[test]
fn h_ui_129_group_by_number_field_invalid() {
    let mut st = template(TemplateId::Classique);
    st.group_by = Some(GroupBy::Custom("number1".parse().unwrap()));
    assert!(st.validate().is_err());
    st.group_by = Some(GroupBy::Custom(CustomFieldId::TEXT1));
    assert!(st.validate().is_ok());
    let json = serde_json::to_string(&st.group_by).unwrap();
    assert_eq!(json, r#"{"custom":"text1"}"#);
}
