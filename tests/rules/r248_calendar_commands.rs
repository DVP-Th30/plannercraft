//! R-248 — Créer, modifier, supprimer un calendrier (`calendar.add`,
//! `calendar.update`, `calendar.delete`) : de base à la semaine du Standard
//! intégré, dérivé d'une base, ou copie ; nom non vide, unique sans tenir
//! compte de la casse (`calendar.name_taken`) ; base qui doit être un
//! calendrier de base, calendrier qui sert de base jamais rendu dérivé
//! (`calendar.base_not_base`) ; suppression d'un calendrier utilisé refusée
//! (`calendar.in_use`, utilisateurs listés). Repère : projet neuf,
//! calendriers fournis 1 (Standard, du projet), 2 (24 heures), 3 (Équipe de
//! nuit), début lun 19/10/2026 08:00.

use plannercraft_automation::error::codes;
use plannercraft_automation::{CommandErrorKind, Session, SideEffect};
use plannercraft_core::calendar::{Calendar, CalendarKind, DayRule, DaySetting};
use plannercraft_core::{CalendarUid, ResourceUid, Subject, TaskUid, Weekday};
use plannercraft_tests::resource_edit::{fresh, with_tasks};
use plannercraft_tests::task_edit::{apply, try_apply};
use serde_json::json;

fn cal(s: &Session, uid: u32) -> &Calendar {
    s.project().calendars.get(CalendarUid::new(uid)).unwrap()
}

#[test]
fn r248_add_derived_copy_base() {
    let mut s = fresh();
    // Exception sur le Standard, recopiée par `copy_of`.
    apply(
        &mut s,
        json!([{ "op": "calendar.set_exception", "cal": 1, "from": "2026-11-11", "to": "2026-11-11", "name": "Armistice" }]),
    );
    // Cahier : `add { name: "Équipe B", copy_of: 1 }` ⇒ UID suivant, mêmes
    // jours que S.
    let o = apply(
        &mut s,
        json!([{ "op": "calendar.add", "name": "Équipe B", "copy_of": 1 }]),
    );
    assert_eq!(o.side_effects, [SideEffect::CalendarCreated(CalendarUid::new(4))]);
    let (src, copy) = (cal(&s, 1), cal(&s, 4));
    assert_eq!(copy.name, "Équipe B");
    assert_eq!(copy.kind(), src.kind());
    assert_eq!(copy.exceptions(), src.exceptions());
    assert_eq!(copy.guid, None);
    // Dérivé : tous les jours hérités, aucune exception.
    apply(
        &mut s,
        json!([{ "op": "calendar.add", "name": "Dérivé", "base": 2 }]),
    );
    let CalendarKind::Derived { base, week } = cal(&s, 5).kind() else {
        panic!("dérivé attendu");
    };
    assert_eq!(*base, CalendarUid::new(2));
    assert!(Weekday::ALL.iter().all(|d| week.get(*d) == &DayRule::Inherit));
    assert_eq!(cal(&s, 5).exceptions(), []);
    // Copie d'un dérivé : même base.
    apply(
        &mut s,
        json!([{ "op": "calendar.add", "name": "Dérivé 2", "copy_of": 5 }]),
    );
    assert!(matches!(cal(&s, 6).kind(), CalendarKind::Derived { base, .. } if *base == CalendarUid::new(2)));
    // Ni l'un ni l'autre : calendrier de base à la semaine du Standard.
    apply(&mut s, json!([{ "op": "calendar.add", "name": "Neuf" }]));
    let CalendarKind::Base { week } = cal(&s, 7).kind() else {
        panic!("base attendue");
    };
    assert_eq!(week.get(Weekday::Saturday), &DaySetting::NonWorking);
    assert!(matches!(week.get(Weekday::Monday), DaySetting::Working(h) if h.total().get() == 480));
    // Les deux : refus ; copie d'un inconnu : `calendar.not_found`.
    let err = try_apply(
        &mut s,
        json!([{ "op": "calendar.add", "name": "X", "base": 1, "copy_of": 1 }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::PARAM_INVALID, Some("copy_of"))
    );
    let err = try_apply(
        &mut s,
        json!([{ "op": "calendar.add", "name": "X", "copy_of": 99 }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::CALENDAR_NOT_FOUND, Some("copy_of"))
    );
    // Annulé puis recréé : UID jamais réutilisé.
    s.undo().unwrap();
    apply(&mut s, json!([{ "op": "calendar.add", "name": "Neuf" }]));
    assert!(s.project().calendars.contains(CalendarUid::new(8)));
    assert!(!s.project().calendars.contains(CalendarUid::new(7)));
}

#[test]
fn r248_name_taken() {
    let mut s = fresh();
    let before = s.project().clone();
    for (name, existing) in [("standard", 1), ("24 HEURES", 2)] {
        let err = try_apply(&mut s, json!([{ "op": "calendar.add", "name": name }])).unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref(), err.subject),
            (
                codes::CALENDAR_NAME_TAKEN,
                Some("name"),
                Some(Subject::Calendar(CalendarUid::new(existing)))
            ),
            "{name}"
        );
    }
    for name in ["", "   "] {
        let err = try_apply(&mut s, json!([{ "op": "calendar.add", "name": name }])).unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (codes::PARAM_INVALID, Some("name"))
        );
    }
    let long = "x".repeat(513);
    let err = try_apply(&mut s, json!([{ "op": "calendar.add", "name": long }])).unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    assert_eq!(s.project(), &before);
    // Renommer : son propre nom (autre casse) accepté, celui d'un autre
    // refusé, `null` refusé.
    apply(
        &mut s,
        json!([{ "op": "calendar.update", "uid": 1, "name": "STANDARD" }]),
    );
    assert_eq!(cal(&s, 1).name, "STANDARD");
    let err = try_apply(
        &mut s,
        json!([{ "op": "calendar.update", "uid": 2, "name": "Standard" }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::CALENDAR_NAME_TAKEN);
    let err = try_apply(
        &mut s,
        json!([{ "op": "calendar.update", "uid": 2, "name": null }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::PARAM_INVALID, Some("name"))
    );
    // Deux créations du même nom dans un lot : la seconde refusée.
    let err = try_apply(
        &mut s,
        json!([
            { "op": "calendar.add", "name": "Équipe" },
            { "op": "calendar.add", "name": "équipe" }
        ]),
    )
    .unwrap_err();
    assert_eq!((err.code, err.op_index), (codes::CALENDAR_NAME_TAKEN, Some(1)));
}

#[test]
fn r248_base_not_base() {
    let mut s = fresh();
    apply(&mut s, json!([{ "op": "calendar.add", "name": "D", "base": 1 }]));
    // Base dérivée : refus à la création et à la modification.
    let err = try_apply(&mut s, json!([{ "op": "calendar.add", "name": "E", "base": 4 }])).unwrap_err();
    assert_eq!(
        (err.code, &err.kind),
        (
            codes::CALENDAR_BASE_NOT_BASE,
            &CommandErrorKind::CalendarBaseNotBase {
                base: CalendarUid::new(4)
            }
        )
    );
    let err = try_apply(&mut s, json!([{ "op": "calendar.update", "uid": 2, "base": 4 }])).unwrap_err();
    assert_eq!(err.code, codes::CALENDAR_BASE_NOT_BASE);
    // Le Standard sert de base à 4 : il ne devient pas dérivé.
    let err = try_apply(&mut s, json!([{ "op": "calendar.update", "uid": 1, "base": 2 }])).unwrap_err();
    assert_eq!(
        (err.code, &err.kind),
        (
            codes::CALENDAR_BASE_NOT_BASE,
            &CommandErrorKind::CalendarServesAsBase {
                calendar: CalendarUid::new(1),
                count: 1
            }
        )
    );
    // Sa propre base : refus.
    let err = try_apply(&mut s, json!([{ "op": "calendar.update", "uid": 2, "base": 2 }])).unwrap_err();
    assert_eq!(err.code, codes::CALENDAR_BASE_NOT_BASE);
    // 4 change de base (jours réglés gardés), puis redevient de base
    // (jours hérités résolus sur l'ancienne base).
    apply(
        &mut s,
        json!([
            { "op": "calendar.set_week", "calendar": 4, "days": [{ "day": "monday", "setting": "non_working" }] },
            { "op": "calendar.update", "uid": 4, "base": 2 }
        ]),
    );
    let CalendarKind::Derived { base, week } = cal(&s, 4).kind() else {
        panic!("dérivé attendu");
    };
    assert_eq!(
        (*base, week.get(Weekday::Monday)),
        (CalendarUid::new(2), &DayRule::NonWorking)
    );
    apply(
        &mut s,
        json!([{ "op": "calendar.update", "uid": 4, "base": null }]),
    );
    let CalendarKind::Base { week } = cal(&s, 4).kind() else {
        panic!("base attendue");
    };
    assert_eq!(week.get(Weekday::Monday), &DaySetting::NonWorking);
    assert!(matches!(week.get(Weekday::Sunday), DaySetting::Working(h) if h.total().get() == 1_440));
    // Un calendrier de base qui ne sert de base à aucun devient dérivé :
    // chacun de ses jours devient un réglage propre.
    apply(&mut s, json!([{ "op": "calendar.update", "uid": 4, "base": 1 }]));
    let CalendarKind::Derived { week, .. } = cal(&s, 4).kind() else {
        panic!("dérivé attendu");
    };
    assert!(matches!(week.get(Weekday::Sunday), DayRule::Working(h) if h.total().get() == 1_440));
}

#[test]
fn r248_delete_in_use() {
    let mut s = with_tasks(&[("T1", "1j"), ("T2", "1j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "Maçon", "calendar": 2 },
            { "op": "calendar.add", "name": "Libre" }
        ]),
    );
    let mut p = s.project().clone();
    p.tasks.get_mut(TaskUid::new(2)).unwrap().calendar = Some(CalendarUid::new(2));
    let mut s = plannercraft_tests::session::session(p);
    // Cahier : `delete 1` (calendrier du projet) ⇒ `calendar.in_use`.
    let err = try_apply(&mut s, json!([{ "op": "calendar.delete", "uid": 1 }])).unwrap_err();
    assert_eq!(err.code, codes::CALENDAR_IN_USE);
    assert!(matches!(
        &err.kind,
        CommandErrorKind::CalendarInUse { users, total: 1, .. } if users == &[Subject::Project]
    ));
    // 2 : tâche 2 et ressource 1.
    let err = try_apply(&mut s, json!([{ "op": "calendar.delete", "uid": 2 }])).unwrap_err();
    let CommandErrorKind::CalendarInUse { users, total, .. } = &err.kind else {
        panic!("calendar.in_use attendu : {err:?}");
    };
    assert_eq!(
        (users.as_slice(), *total),
        (
            &[
                Subject::Task(TaskUid::new(2)),
                Subject::Resource(ResourceUid::new(1))
            ][..],
            2
        )
    );
    // Base d'un dérivé : refusée aussi.
    apply(&mut s, json!([{ "op": "calendar.add", "name": "D", "base": 3 }]));
    let err = try_apply(&mut s, json!([{ "op": "calendar.delete", "uid": 3 }])).unwrap_err();
    assert!(matches!(
        &err.kind,
        CommandErrorKind::CalendarInUse { users, .. } if users == &[Subject::Calendar(CalendarUid::new(5))]
    ));
    // Inutilisé : supprimé, effet signalé ; annuler le rend.
    let o = apply(&mut s, json!([{ "op": "calendar.delete", "uid": 4 }]));
    assert_eq!(o.side_effects, [SideEffect::CalendarDeleted(CalendarUid::new(4))]);
    assert!(!s.project().calendars.contains(CalendarUid::new(4)));
    s.undo().unwrap();
    assert_eq!(cal(&s, 4).name, "Libre");
    let err = try_apply(&mut s, json!([{ "op": "calendar.delete", "uid": 99 }])).unwrap_err();
    assert_eq!(err.code, codes::CALENDAR_NOT_FOUND);
    // Au plus 20 utilisateurs listés, le total compté.
    let ops: Vec<serde_json::Value> = (0..25)
        .map(|i| json!({ "op": "resource.add", "name": format!("R{i}"), "calendar": 3 }))
        .collect();
    apply(&mut s, serde_json::Value::Array(ops));
    let err = try_apply(&mut s, json!([{ "op": "calendar.delete", "uid": 3 }])).unwrap_err();
    let CommandErrorKind::CalendarInUse { users, total, .. } = &err.kind else {
        panic!("calendar.in_use attendu");
    };
    assert_eq!((users.len(), *total), (20, 26));
    assert!(
        err.message(plannercraft_automation::text::Language::Fr)
            .contains(", …")
    );
}
