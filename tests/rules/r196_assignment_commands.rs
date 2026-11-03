//! R-196 — Commandes `assignment.*` : paire en double refusée, unités par
//! défaut min(100 %, capacité), travail fourni (R-189 « travail »),
//! quantité des matérielles (W = q × 60 min), champs contraires au type
//! refusés, unités nulles refusées en travail fixe, affectation fictive
//! −65 535 retirée, équilibrage R-190 à R-192 à l'ajout et au retrait.
//! Repère : calendrier Standard, début lun 19/10/2026 08:00 (1 j = 480 min).

use plannercraft_automation::error::codes;
use plannercraft_automation::{ApplyMode, SideEffect};
use plannercraft_core::{Assignment, AssignmentUid, Minutes, TaskUid, Work};
use plannercraft_tests::moment;
use plannercraft_tests::resource_edit::{duration, units_work, with_tasks, work};
use plannercraft_tests::task_edit::{apply, batch, try_apply};
use serde_json::json;

#[test]
fn r196_duplicate_refused() {
    let mut s = with_tasks(&[("A", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "R", "as": "$r" },
            { "op": "assignment.add", "task": 1, "resource": "$r" }
        ]),
    );
    let err = try_apply(
        &mut s,
        json!([{ "op": "assignment.add", "task": 1, "resource": 1 }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::ASSIGNMENT_DUPLICATE, Some("resource"))
    );
    // Désignations inconnues.
    let err = try_apply(
        &mut s,
        json!([{ "op": "assignment.add", "task": 1, "resource": 9 }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::RESOURCE_NOT_FOUND);
    let err = try_apply(
        &mut s,
        json!([{ "op": "assignment.remove", "task": 1, "resource": 2 }]),
    );
    assert_eq!(err.unwrap_err().code, codes::RESOURCE_NOT_FOUND);
    apply(&mut s, json!([{ "op": "resource.add", "name": "S" }]));
    let err = try_apply(
        &mut s,
        json!([{ "op": "assignment.remove", "task": 1, "resource": 2 }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::ASSIGNMENT_NOT_FOUND);
}

#[test]
fn r196_default_units_and_given_work() {
    // Capacité 50 % ⇒ unités par défaut 50 % ; tâche de 2 j non pilotée :
    // W = 960 × 0,5 = 480 (8 h). Travail fourni 24 h à 100 % sur une tâche
    // de 2 j à unités fixes ⇒ R-189 « travail » : durée 3 j (1 440).
    let mut s = with_tasks(&[("A", "2j"), ("B", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "Mi-temps", "max_units": { "text": "50%" } },
            { "op": "resource.add", "name": "Plein" },
            { "op": "assignment.add", "task": 1, "resource": 1 },
            { "op": "assignment.add", "task": 2, "resource": 2, "work": { "text": "24h" },
              "delay": { "text": "1j" } }
        ]),
    );
    assert_eq!(units_work(&s, 1, 1), (5_000, 480));
    assert_eq!((duration(&s, 1), work(&s, 1)), (960, 480));
    assert_eq!(units_work(&s, 2, 2), (10_000, 1_440));
    assert_eq!((duration(&s, 2), work(&s, 2)), (1_440, 1_440));
    assert_eq!(
        plannercraft_tests::resource_edit::assignment(&s, 2, 2).delay,
        Minutes::new(480)
    );
}

#[test]
fn r196_placeholder_removed() {
    // Tâche relue avec l'affectation fictive (−65 535 : sans ressource,
    // O24) : retirée par l'affectation d'une vraie ressource, effet
    // signalé ; jamais recréée au retrait.
    let mut p = plannercraft_tests::mspdi::new_project("2026-10-19T08:00");
    plannercraft_tests::session::push_tasks(&mut p, &[1], 2);
    let mut placeholder = Assignment::new(AssignmentUid::new(7), TaskUid::new(1), None);
    placeholder.work = Work::new(Minutes::new(960)).unwrap();
    p.assignments.push(placeholder).unwrap();
    let mut s = plannercraft_tests::session::session(p);
    let o = apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "R" },
            { "op": "assignment.add", "task": 1, "resource": 1 }
        ]),
    );
    assert!(
        o.side_effects
            .contains(&SideEffect::PlaceholderRemoved(AssignmentUid::new(7)))
    );
    // UID d'affectation suivant le plus grand présent (7).
    assert_eq!(
        s.project()
            .assignments
            .iter()
            .map(|a| a.uid().get())
            .collect::<Vec<_>>(),
        [8]
    );
    apply(
        &mut s,
        json!([{ "op": "assignment.remove", "task": 1, "resource": 1 }]),
    );
    assert!(s.project().assignments.is_empty());
}

#[test]
fn r196_effort_driven_add_dry_run() {
    // Cahier : unités fixes 4 j pilotée par l'effort avec Maçon (32 h),
    // `add` Électricien en `dry_run` ⇒ diff : durée 4 j → 2 j, fin avancée
    // du jeu 22/10 17:00 (lun-jeu) au mar 20/10 17:00, rien appliqué.
    let mut s = with_tasks(&[("Fondations", "4j")]);
    apply(
        &mut s,
        json!([
            { "op": "task.update", "uid": 1, "effort_driven": true },
            { "op": "resource.add", "name": "Maçon", "calendar": "project" },
            { "op": "resource.add", "name": "Électricien", "calendar": "project" },
            { "op": "assignment.add", "task": 1, "resource": 1 }
        ]),
    );
    assert_eq!(units_work(&s, 1, 1), (10_000, 1_920));
    let before = s.project().clone();
    let o = s
        .apply(
            &batch(json!([{ "op": "assignment.add", "task": 1, "resource": 2 }])),
            ApplyMode::DryRun,
        )
        .unwrap();
    assert!(!o.committed);
    assert_eq!(s.project(), &before);
    let change = &o.diff.changed[0];
    assert_eq!(change.uid, TaskUid::new(1));
    assert_eq!(
        change.finish,
        (Some(moment("2026-10-22T17:00")), Some(moment("2026-10-20T17:00")))
    );
    // Validé : 2 j, 16 h chacune (R-190).
    apply(
        &mut s,
        json!([{ "op": "assignment.add", "task": 1, "resource": 2 }]),
    );
    assert_eq!(duration(&s, 1), 960);
    assert_eq!(units_work(&s, 1, 1), (10_000, 960));
    assert_eq!(units_work(&s, 1, 2), (10_000, 960));
}

#[test]
fn r196_material_quantity() {
    // Matérielle « m³ » : unités 100 %, travail = quantité × 60 min (O30) ;
    // 3 m³ ⇒ 180 ; défaut 1 ⇒ 60 ; la durée de la tâche ne bouge pas.
    let mut s = with_tasks(&[("A", "2j"), ("B", "1j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "Béton", "kind": "material", "material_label": "m³" },
            { "op": "assignment.add", "task": 1, "resource": 1, "quantity": { "text": "3" } },
            { "op": "assignment.add", "task": 2, "resource": 1 }
        ]),
    );
    assert_eq!(units_work(&s, 1, 1), (10_000, 180));
    assert_eq!(units_work(&s, 2, 1), (10_000, 60));
    assert_eq!((duration(&s, 1), work(&s, 1)), (960, 0));
    apply(
        &mut s,
        json!([{ "op": "assignment.update", "task": 1, "resource": 1, "quantity": { "value": 12_500 } }]),
    );
    // 1,25 m³ ⇒ 75 min.
    assert_eq!(units_work(&s, 1, 1), (10_000, 75));
    for (op, field) in [
        (
            json!({ "op": "assignment.update", "task": 1, "resource": 1, "units": { "text": "50%" } }),
            "units",
        ),
        (
            json!({ "op": "assignment.update", "task": 1, "resource": 1, "quantity": { "text": "50%" } }),
            "quantity",
        ),
    ] {
        let err = try_apply(&mut s, json!([op.clone()])).unwrap_err();
        assert_eq!(err.field.as_deref(), Some(field), "{op}");
    }
    // Une quantité sur une ressource de travail : type contraire.
    apply(&mut s, json!([{ "op": "resource.add", "name": "R" }]));
    let err = try_apply(
        &mut s,
        json!([{ "op": "assignment.add", "task": 1, "resource": 2, "quantity": { "value": 10_000 } }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::ASSIGNMENT_KIND_MISMATCH, Some("quantity"))
    );
}

#[test]
fn r196_cost_kind_mismatch() {
    // Ressource de coût : ni unités, ni travail, ni quantité ; affectée,
    // travail nul, dates inchangées.
    let mut s = with_tasks(&[("A", "2j")]);
    apply(
        &mut s,
        json!([{ "op": "resource.add", "name": "Location", "kind": "cost" }]),
    );
    for (op, field) in [
        (
            json!({ "op": "assignment.add", "task": 1, "resource": 1, "units": { "value": 10_000 } }),
            "units",
        ),
        (
            json!({ "op": "assignment.add", "task": 1, "resource": 1, "work": { "text": "8h" } }),
            "work",
        ),
        (
            json!({ "op": "assignment.add", "task": 1, "resource": 1, "quantity": { "value": 1 } }),
            "quantity",
        ),
    ] {
        let err = try_apply(&mut s, json!([op.clone()])).unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (codes::ASSIGNMENT_KIND_MISMATCH, Some(field)),
            "{op}"
        );
    }
    let finish = s.schedule().scheduled(TaskUid::new(1)).unwrap().finish;
    apply(
        &mut s,
        json!([{ "op": "assignment.add", "task": 1, "resource": 1 }]),
    );
    assert_eq!(units_work(&s, 1, 1), (10_000, 0));
    assert_eq!(s.schedule().scheduled(TaskUid::new(1)).unwrap().finish, finish);
}

#[test]
fn r196_zero_units_fixed_work_refused() {
    let mut s = with_tasks(&[("A", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "task.update", "uid": 1, "type": "fixed_work" },
            { "op": "resource.add", "name": "R" }
        ]),
    );
    let err = try_apply(
        &mut s,
        json!([{ "op": "assignment.add", "task": 1, "resource": 1, "units": { "value": 0 } }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::ASSIGNMENT_ZERO_UNITS_FIXED_WORK, Some("units"))
    );
    // Accepté à 100 %, puis refusé à 0 % par `assignment.update`.
    apply(
        &mut s,
        json!([{ "op": "assignment.add", "task": 1, "resource": 1 }]),
    );
    let err = try_apply(
        &mut s,
        json!([{ "op": "assignment.update", "task": 1, "resource": 1, "units": { "text": "0%" } }]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::ASSIGNMENT_ZERO_UNITS_FIXED_WORK);
}

#[test]
fn r196_remove_rebalances() {
    // Pilotée (R-191) : unités fixes 2 j, R1 et R2 à 16 h ; retrait de R2 ⇒
    // R1 32 h, 4 j. Non pilotée (R-192) : 4 j, R1 32 h, + R2 ⇒ 64 h ; retrait
    // ⇒ 4 j, 32 h.
    let mut s = with_tasks(&[("A", "4j"), ("B", "4j")]);
    apply(
        &mut s,
        json!([
            { "op": "task.update", "uid": 1, "effort_driven": true },
            { "op": "resource.add", "name": "R1" },
            { "op": "resource.add", "name": "R2" },
            { "op": "assignment.add", "task": 1, "resource": 1 },
            { "op": "assignment.add", "task": 1, "resource": 2 },
            { "op": "assignment.add", "task": 2, "resource": 1 },
            { "op": "assignment.add", "task": 2, "resource": 2 }
        ]),
    );
    assert_eq!((duration(&s, 1), work(&s, 1)), (960, 1_920));
    assert_eq!((duration(&s, 2), work(&s, 2)), (1_920, 3_840));
    let o = apply(
        &mut s,
        json!([
            { "op": "assignment.remove", "task": 1, "resource": 2 },
            { "op": "assignment.remove", "task": 2, "resource": 2 }
        ]),
    );
    assert_eq!(
        o.side_effects
            .iter()
            .filter(|e| matches!(e, SideEffect::AssignmentRemoved(_)))
            .count(),
        2
    );
    assert_eq!(units_work(&s, 1, 1), (10_000, 1_920));
    assert_eq!(duration(&s, 1), 1_920);
    assert_eq!((duration(&s, 2), work(&s, 2)), (1_920, 1_920));
    // Retrait de la dernière : durée gardée, travail nul, aucune
    // affectation fictive recréée.
    apply(
        &mut s,
        json!([{ "op": "assignment.remove", "task": 1, "resource": 1 }]),
    );
    assert_eq!((duration(&s, 1), work(&s, 1)), (1_920, 0));
    assert!(s.project().assignments.iter().all(|a| a.task != TaskUid::new(1)));
}

#[test]
fn r196_update_units_work_delay() {
    // Unités fixes 2 j, R 100 % 16 h : unités → 50 % ⇒ 4 j (travail gardé,
    // R-189) ; travail → 24 h ⇒ d = 1 440 × 10 000 / 5 000 = 2 880 (6 j) ;
    // délai : aucun rééquilibrage.
    let mut s = with_tasks(&[("A", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "R" },
            { "op": "assignment.add", "task": 1, "resource": 1 }
        ]),
    );
    apply(
        &mut s,
        json!([{ "op": "assignment.update", "task": 1, "resource": 1, "units": { "text": "50%" } }]),
    );
    assert_eq!((units_work(&s, 1, 1), duration(&s, 1)), ((5_000, 960), 1_920));
    apply(
        &mut s,
        json!([{ "op": "assignment.update", "task": 1, "resource": 1, "work": { "text": "24h" } }]),
    );
    assert_eq!((units_work(&s, 1, 1), duration(&s, 1)), ((5_000, 1_440), 2_880));
    apply(
        &mut s,
        json!([{ "op": "assignment.update", "task": 1, "resource": 1, "delay": { "text": "2j" } }]),
    );
    assert_eq!((units_work(&s, 1, 1), duration(&s, 1)), ((5_000, 1_440), 2_880));
    assert_eq!(
        plannercraft_tests::resource_edit::assignment(&s, 1, 1).delay,
        Minutes::new(960)
    );
    let err = try_apply(
        &mut s,
        json!([{ "op": "assignment.update", "task": 1, "resource": 1, "units": null }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::PARAM_INVALID, Some("units"))
    );
}

#[test]
fn r196_summary_and_manual_tasks() {
    // Récapitulative (R-179) : affectée sans rééquilibrage, travail = durée
    // calculée × unités (Lot couvre A, lun – mar : 2 j, et non le 1 j saisi
    // avant l'indentation) ; tâche manuelle : durée tenue (hypothèse),
    // travail = D × U.
    let mut s = with_tasks(&[("Lot", "1j"), ("A", "2j")]);
    apply(&mut s, json!([{ "op": "task.indent", "uid": 2 }]));
    let mut p = s.project().clone();
    let t = p.tasks.get_mut(TaskUid::new(2)).unwrap();
    t.mode = plannercraft_core::SchedulingMode::Manual {
        start: Some(moment("2026-10-19T08:00")),
        finish: None,
        duration: Some(t.duration),
    };
    let mut s = plannercraft_tests::session::session(p);
    let stored = duration(&s, 1);
    apply(
        &mut s,
        json!([
            { "op": "task.update", "uid": 2, "effort_driven": true },
            { "op": "resource.add", "name": "R1" },
            { "op": "resource.add", "name": "R2" },
            { "op": "assignment.add", "task": 1, "resource": 1, "units": { "text": "50%" } },
            { "op": "assignment.add", "task": 2, "resource": 1 },
            { "op": "assignment.add", "task": 2, "resource": 2 }
        ]),
    );
    assert_eq!(stored, 480);
    assert_eq!(units_work(&s, 1, 1), (5_000, 480));
    assert_eq!(duration(&s, 1), stored);
    // Manuelle pilotée : 2 j gardés, 16 h chacune.
    assert_eq!(duration(&s, 2), 960);
    assert_eq!(
        (units_work(&s, 2, 1), units_work(&s, 2, 2)),
        ((10_000, 960), (10_000, 960))
    );
}

#[test]
fn r196_summary_read_from_file_uses_computed_duration() {
    // Récapitulative automatique relue d'un fichier : sa durée du modèle est
    // nulle (valeur calculée) ; l'affectation sans travail fourni prend la
    // durée calculée, 3 j (A 1 j puis B 2 j) × 100 % = 24 h, et non 0.
    let mut s = with_tasks(&[("Lot", "1j"), ("A", "1j"), ("B", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "task.indent", "uid": 2 },
            { "op": "task.indent", "uid": 3 },
            { "op": "link.add", "pred": 2, "succ": 3 }
        ]),
    );
    let xml = s
        .save_bytes(&plannercraft_mspdi::write::WriteOptions::default())
        .xml;
    let origin = plannercraft_automation::DocOrigin {
        name: "p.xml".to_owned(),
        path: None,
    };
    let (mut r, _) = plannercraft_automation::Session::open_bytes(
        &xml,
        origin,
        moment("2026-10-19T08:00"),
        plannercraft_automation::text::Language::Fr,
    )
    .unwrap();
    assert_eq!(duration(&r, 1), 0);
    apply(
        &mut r,
        json!([
            { "op": "resource.add", "name": "R1" },
            { "op": "assignment.add", "task": 1, "resource": 1 }
        ]),
    );
    assert_eq!(units_work(&r, 1, 1), (10_000, 1_440));
}
