//! P52 (J8, lot L3) : copier puis coller (tampon interne) redonne les mêmes
//! champs copiés (R-244), aux UID et ID près, et les liens internes
//! (1 024 cas). Projet tiré par des commandes : hiérarchie, durées
//! (estimées ou non, jalons), contraintes, échéances, notes, mode manuel,
//! inactivité, liens et affectations de travail ; sélection tirée, collée
//! en fin de projet, puis recopiée depuis les tâches créées.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]
#![allow(
    clippy::struct_excessive_bools,
    clippy::needless_pass_by_value,
    reason = "tests : tirage proptest à drapeaux, opérations JSON construites à la volée"
)]

use plannercraft_automation::clip::{PasteOptions, RowClip, paste_batch};
use plannercraft_automation::{AnchorSpec, ApplyMode, Batch, NewProject, Placement, Session};
use plannercraft_core::{Moment, TaskUid};
use proptest::prelude::*;
use serde_json::{Value, json};

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

/// Tâche tirée : parent (indice d'une tâche antérieure, modulo), jours,
/// estimée, contrainte (0 aucune, 1 snet, 2 fnlt, 3 mso), échéance,
/// notes, manuelle, inactive.
#[derive(Clone, Debug)]
struct Spec {
    parent: Option<usize>,
    days: u8,
    estimated: bool,
    constraint: u8,
    deadline: bool,
    notes: bool,
    manual: bool,
    inactive: bool,
}

fn spec() -> impl Strategy<Value = Spec> {
    (
        proptest::option::weighted(0.5, any::<usize>()),
        0u8..6,
        any::<bool>(),
        0u8..4,
        proptest::bool::weighted(0.2),
        proptest::bool::weighted(0.3),
        proptest::bool::weighted(0.25),
        proptest::bool::weighted(0.15),
    )
        .prop_map(
            |(parent, days, estimated, constraint, deadline, notes, manual, inactive)| Spec {
                parent,
                days,
                estimated,
                constraint,
                deadline,
                notes,
                manual,
                inactive,
            },
        )
}

/// Applique `op` seul ; un refus (lien entre une récapitulative et sa
/// descendante, doublon…) est ignoré : le projet tiré reste valide.
fn try_op(s: &mut Session, op: Value) {
    let b = Batch::from_json(json!({ "format_version": 1, "ops": [op] })).unwrap();
    let _ = s.apply(&b, ApplyMode::Commit);
}

fn build(specs: &[Spec], links: &[(usize, usize, u8)], assigns: &[(usize, u8, u16)]) -> Session {
    let init = NewProject {
        name: "P52".to_owned(),
        anchor: AnchorSpec::Start(now()),
        lang: None,
    };
    let mut s = Session::new_project(init, now()).unwrap();
    try_op(&mut s, json!({ "op": "resource.add", "name": "Maçon" }));
    try_op(&mut s, json!({ "op": "resource.add", "name": "Grue" }));
    let n = specs.len();
    for (i, t) in specs.iter().enumerate() {
        let uid = i + 1;
        let text = format!("{}j{}", t.days, if t.estimated { "?" } else { "" });
        let mut op = json!({ "op": "task.add", "name": format!("T{uid}"), "duration": { "text": text } });
        if let Some(p) = t.parent.filter(|_| i > 0) {
            op["at"] = json!({ "last_child_of": (p % i) + 1 });
        }
        if t.notes {
            op["notes"] = json!(format!("note {uid}\nligne 2"));
        }
        try_op(&mut s, op);
        let date = format!("2026-10-{:02}T08:00", 20 + (i % 9));
        match t.constraint {
            1 => try_op(
                &mut s,
                json!({ "op": "task.set_constraint", "uid": uid, "kind": "snet", "date": date }),
            ),
            2 => try_op(
                &mut s,
                json!({ "op": "task.set_constraint", "uid": uid, "kind": "fnlt", "date": "2026-12-31T17:00" }),
            ),
            3 => try_op(
                &mut s,
                json!({ "op": "task.set_constraint", "uid": uid, "kind": "mso", "date": date }),
            ),
            _ => {}
        }
        if t.deadline {
            try_op(
                &mut s,
                json!({ "op": "task.set_deadline", "uid": uid, "deadline": "2026-11-30T17:00" }),
            );
        }
    }
    for &(a, b, kind) in links {
        let (a, b) = (a % n + 1, b % n + 1);
        let kind = ["fs", "ss", "ff", "sf"][usize::from(kind % 4)];
        try_op(
            &mut s,
            json!({ "op": "link.add", "pred": a, "succ": b, "kind": kind, "lag": { "text": "1j" } }),
        );
    }
    for &(t, r, units) in assigns {
        try_op(
            &mut s,
            json!({ "op": "assignment.add", "task": t % n + 1, "resource": u32::from(r % 2) + 1,
                    "units": { "value": 2_500 + i64::from(units % 8) * 2_500 } }),
        );
    }
    for (i, t) in specs.iter().enumerate() {
        if t.manual {
            try_op(
                &mut s,
                json!({ "op": "task.update", "uid": i + 1, "mode": "manual" }),
            );
        }
        if t.inactive {
            try_op(
                &mut s,
                json!({ "op": "task.update", "uid": i + 1, "active": false }),
            );
        }
    }
    s
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn p52_copy_paste_round_trip(
        specs in proptest::collection::vec(spec(), 1..12),
        links in proptest::collection::vec((any::<usize>(), any::<usize>(), any::<u8>()), 0..12),
        assigns in proptest::collection::vec((any::<usize>(), any::<u8>(), any::<u16>()), 0..6),
        picks in proptest::collection::vec(any::<usize>(), 1..5),
    ) {
        let mut s = build(&specs, &links, &assigns);
        let n = s.project().tasks.len();
        prop_assume!(n > 0);
        let selected: Vec<TaskUid> = picks
            .iter()
            .filter_map(|p| s.project().tasks.at(p % n).map(plannercraft_core::Task::uid))
            .collect();
        let clip = RowClip::capture(&s, &selected).unwrap();
        let (batch, report) = paste_batch(&s, &clip, &Placement::End, &PasteOptions::default()).unwrap();
        prop_assert_eq!(report.tasks as usize, clip.tasks.len());
        prop_assert_eq!(report.assignments_dropped, 0);
        let before: Vec<TaskUid> = s.project().tasks.iter().map(plannercraft_core::Task::uid).collect();
        let outcome = s.apply(&batch, ApplyMode::Commit);
        prop_assert!(outcome.is_ok(), "collage refusé : {:?}\n{:?}", outcome.err(), batch);
        let created: Vec<TaskUid> = s
            .project()
            .tasks
            .iter()
            .map(plannercraft_core::Task::uid)
            .filter(|u| !before.contains(u))
            .collect();
        prop_assert_eq!(created.len(), clip.tasks.len());
        let again = RowClip::capture(&s, &created).unwrap();
        prop_assert_eq!(again.tasks.len(), clip.tasks.len());
        for (i, (a, b)) in clip.tasks.iter().zip(&again.tasks).enumerate() {
            prop_assert_eq!(a.rel_level, b.rel_level);
            // Récapitulative manuelle : début et fin manuels recollés, la
            // durée (stockée et manuelle) suit (R-254 : temps ouvré entre
            // les deux, non estimé ; le moteur calcule celle d'une
            // récapitulative, R-054).
            let summary = clip.tasks.get(i + 1).is_some_and(|n| n.rel_level > a.rel_level);
            let mut expected = a.fields.clone();
            if summary && a.fields.manual {
                expected.duration = b.fields.duration;
                if let (Some((s0, f0, _)), Some((_, _, d1))) = (a.fields.manual_values, b.fields.manual_values) {
                    expected.manual_values = Some((s0, f0, d1));
                }
            }
            prop_assert_eq!(&expected, &b.fields);
        }
        prop_assert_eq!(&again.links, &clip.links);
        prop_assert_eq!(again.links_dropped, 0);
        prop_assert_eq!(&again.assignments, &clip.assignments);
    }
}
