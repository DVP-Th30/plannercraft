//! R-195 — Commandes `resource.*` : UID par compteur propre jamais
//! réutilisé, calendrier propre dérivé de la base du calendrier du projet
//! (O34), calendrier partagé, refus de la suppression d'une ressource
//! affectée sans `cascade` et du changement de type d'une ressource
//! affectée. Repère : calendrier Standard (UID 1, calendrier du projet),
//! calendriers fournis UID 1 à 3, début lun 19/10/2026 08:00.

use plannercraft_automation::error::codes;
use plannercraft_automation::{CommandErrorKind, SideEffect};
use plannercraft_core::calendar::CalendarKind;
use plannercraft_core::{AssignmentUid, CalendarUid, Money, Rate, RateUnit, ResourceKind, ResourceUid};
use plannercraft_tests::resource_edit::{duration, fresh, resource, units_work, with_tasks};
use plannercraft_tests::task_edit::{apply, try_apply};
use serde_json::json;

#[test]
fn r195_add_creates_own_calendar() {
    // Cahier : `add "Maçon" as $m max_units 200%` ⇒ UID 1, calendrier UID 4
    // « Maçon » dérivé de Standard (UID 1).
    let mut s = fresh();
    let o = apply(
        &mut s,
        json!([{ "op": "resource.add", "name": "Maçon", "as": "$m", "max_units": { "text": "200%" } }]),
    );
    assert_eq!(o.created_resources, [ResourceUid::new(1)]);
    assert_eq!(o.resource_labels.get("$m"), Some(&ResourceUid::new(1)));
    assert_eq!(o.side_effects, [SideEffect::CalendarCreated(CalendarUid::new(4))]);
    let r = resource(&s, 1);
    assert_eq!(r.name, "Maçon");
    assert_eq!(r.kind, ResourceKind::Work);
    assert_eq!(r.max_units.get(), 20_000);
    assert_eq!(r.calendar, Some(CalendarUid::new(4)));
    let cal = s.project().calendars.get(CalendarUid::new(4)).unwrap();
    assert_eq!(cal.name, "Maçon");
    assert!(matches!(cal.kind(), CalendarKind::Derived { base, .. } if *base == CalendarUid::new(1)));
    // Défauts : travail, 100 %, calendrier propre ; matérielle et coût sans
    // calendrier ; « project » : aucun calendrier propre.
    let o = apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "Électricien" },
            { "op": "resource.add", "name": "Béton", "kind": "material", "material_label": "m³" },
            { "op": "resource.add", "name": "Location", "kind": "cost" },
            { "op": "resource.add", "name": "Grutier", "calendar": "project" }
        ]),
    );
    assert_eq!(o.side_effects, [SideEffect::CalendarCreated(CalendarUid::new(5))]);
    assert_eq!(resource(&s, 2).max_units.get(), 10_000);
    assert_eq!(resource(&s, 2).calendar, Some(CalendarUid::new(5)));
    assert_eq!(
        resource(&s, 3).kind,
        ResourceKind::Material {
            label: "m³".to_owned()
        }
    );
    assert_eq!(resource(&s, 3).calendar, None);
    assert_eq!(resource(&s, 4).kind, ResourceKind::Cost);
    assert_eq!(resource(&s, 5).calendar, None);
}

#[test]
fn r195_add_shared_calendar() {
    // Deux ressources sur le calendrier 4 créé pour la première : aucun
    // calendrier de plus.
    let mut s = fresh();
    apply(&mut s, json!([{ "op": "resource.add", "name": "R1" }]));
    let o = apply(
        &mut s,
        json!([{ "op": "resource.add", "name": "R2", "calendar": 4 }]),
    );
    assert_eq!(o.side_effects, []);
    assert_eq!(resource(&s, 2).calendar, Some(CalendarUid::new(4)));
    assert_eq!(s.project().calendars.len(), 4);
    // Calendrier inconnu ; calendrier, capacité ou étiquette contraires au
    // type ; nom de plus de 512 caractères.
    for (op, code, field) in [
        (
            json!({ "op": "resource.add", "name": "X", "calendar": 99 }),
            codes::CALENDAR_NOT_FOUND,
            "calendar",
        ),
        (
            json!({ "op": "resource.add", "name": "X", "kind": "material", "calendar": "own" }),
            codes::PARAM_INVALID,
            "calendar",
        ),
        (
            json!({ "op": "resource.add", "name": "X", "kind": "cost", "max_units": { "value": 5_000 } }),
            codes::PARAM_INVALID,
            "max_units",
        ),
        (
            json!({ "op": "resource.add", "name": "X", "material_label": "t" }),
            codes::PARAM_INVALID,
            "material_label",
        ),
        (
            json!({ "op": "resource.add", "name": "x".repeat(513) }),
            codes::PARAM_INVALID,
            "name",
        ),
        (
            json!({ "op": "resource.add", "name": "X", "max_units": { "value": 5_000, "text": "40%" } }),
            codes::PARAM_MISMATCH,
            "max_units",
        ),
    ] {
        let err = try_apply(&mut s, json!([op.clone()])).unwrap_err();
        assert_eq!((err.code, err.field.as_deref()), (code, Some(field)), "{op}");
    }
    // Nom vide accepté (comme une tâche, R-152).
    apply(&mut s, json!([{ "op": "resource.add", "name": "" }]));
    assert_eq!(resource(&s, 3).name, "");
}

#[test]
fn r195_uid_monotone_after_undo() {
    // R-151 appliquée aux ressources et aux calendriers : l'undo ne fait pas
    // reculer les compteurs.
    let mut s = fresh();
    apply(&mut s, json!([{ "op": "resource.add", "name": "A" }]));
    assert_eq!(resource(&s, 1).calendar, Some(CalendarUid::new(4)));
    s.undo().unwrap();
    assert!(s.project().resources.is_empty());
    assert!(s.project().calendars.get(CalendarUid::new(4)).is_none());
    let o = apply(&mut s, json!([{ "op": "resource.add", "name": "B" }]));
    assert_eq!(o.created_resources, [ResourceUid::new(2)]);
    assert_eq!(resource(&s, 2).calendar, Some(CalendarUid::new(5)));
    // Étiquettes : propres au lot, refusées en double, inconnues ailleurs.
    let err = try_apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "C", "as": "$c" },
            { "op": "resource.add", "name": "D", "as": "$c" }
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, codes::BATCH_DUPLICATE_LABEL);
    let err = try_apply(
        &mut s,
        json!([{ "op": "resource.update", "uid": "$c", "name": "E" }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::BATCH_UNKNOWN_LABEL, Some("uid"))
    );
    // Le refus n'a rien consommé de visible : la suivante est 3 ou plus,
    // jamais 2.
    let o = apply(&mut s, json!([{ "op": "resource.add", "name": "F" }]));
    assert!(o.created_resources[0].get() >= 3);
}

#[test]
fn r195_delete_requires_cascade() {
    // Cahier : `delete $m` avec 2 affectations sans `cascade` ⇒ refus.
    // Tâches A (4 j) et B (2 j), non pilotées ; Maçon affecté aux deux.
    let mut s = with_tasks(&[("A", "4j"), ("B", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "Maçon", "as": "$m" },
            { "op": "assignment.add", "task": 1, "resource": "$m" },
            { "op": "assignment.add", "task": 2, "resource": "$m" }
        ]),
    );
    let err = try_apply(&mut s, json!([{ "op": "resource.delete", "uid": 1 }])).unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::RESOURCE_HAS_ASSIGNMENTS, Some("cascade"))
    );
    assert_eq!(err.kind, CommandErrorKind::ResourceHasAssignments { count: 2 });
    // Avec `cascade` : affectations retirées (R-192 : durée gardée, travail
    // nul), ressource partie, calendrier propre gardé.
    let o = apply(
        &mut s,
        json!([{ "op": "resource.delete", "uid": 1, "cascade": true }]),
    );
    assert_eq!(
        o.side_effects,
        [
            SideEffect::AssignmentRemoved(AssignmentUid::new(1)),
            SideEffect::AssignmentRemoved(AssignmentUid::new(2)),
        ]
    );
    assert!(s.project().resources.is_empty() && s.project().assignments.is_empty());
    assert!(s.project().calendars.contains(CalendarUid::new(4)));
    assert_eq!((duration(&s, 1), duration(&s, 2)), (1_920, 960));
    // Sans affectation : supprimée sans `cascade`.
    apply(&mut s, json!([{ "op": "resource.add", "name": "X" }]));
    apply(&mut s, json!([{ "op": "resource.delete", "uid": 2 }]));
    let err = try_apply(&mut s, json!([{ "op": "resource.delete", "uid": 2 }])).unwrap_err();
    assert_eq!(err.code, codes::RESOURCE_NOT_FOUND);
}

#[test]
fn r195_delete_cascade_rebalances_effort_driven() {
    // R-191 par la cascade : unités fixes 2 j pilotée, R1 et R2 à 100 %
    // (16 h chacune) ; supprimer R2 ⇒ R1 32 h, 4 j (1 920 min).
    let mut s = with_tasks(&[("A", "4j")]);
    apply(
        &mut s,
        json!([
            { "op": "task.update", "uid": 1, "effort_driven": true },
            { "op": "resource.add", "name": "R1" },
            { "op": "resource.add", "name": "R2" },
            { "op": "assignment.add", "task": 1, "resource": 1 },
            { "op": "assignment.add", "task": 1, "resource": 2 }
        ]),
    );
    // 4 j avec R1 (32 h), + R2 pilotée ⇒ 2 j, 16 h chacune (R-190).
    assert_eq!(duration(&s, 1), 960);
    apply(
        &mut s,
        json!([{ "op": "resource.delete", "uid": 2, "cascade": true }]),
    );
    assert_eq!(units_work(&s, 1, 1), (10_000, 1_920));
    assert_eq!(duration(&s, 1), 1_920);
}

#[test]
fn r195_kind_change_refused() {
    // Ressource affectée : changer de type est refusé ; sans affectation,
    // accepté (une matérielle n'a ni calendrier ni capacité).
    let mut s = with_tasks(&[("A", "1j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "R1" },
            { "op": "resource.add", "name": "R2" },
            { "op": "assignment.add", "task": 1, "resource": 1 }
        ]),
    );
    let err = try_apply(
        &mut s,
        json!([{ "op": "resource.update", "uid": 1, "kind": "material" }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::RESOURCE_HAS_ASSIGNMENTS, Some("kind"))
    );
    // Même type reposé : rien n'est refusé.
    apply(
        &mut s,
        json!([{ "op": "resource.update", "uid": 1, "kind": "work" }]),
    );
    apply(
        &mut s,
        json!([{ "op": "resource.update", "uid": 2, "kind": "material", "material_label": "t" }]),
    );
    let r = resource(&s, 2);
    assert_eq!(
        r.kind,
        ResourceKind::Material {
            label: "t".to_owned()
        }
    );
    assert_eq!(r.calendar, None);
    let err = try_apply(
        &mut s,
        json!([{ "op": "resource.update", "uid": 2, "max_units": { "text": "50%" } }]),
    )
    .unwrap_err();
    assert_eq!(
        (err.code, err.field.as_deref()),
        (codes::PARAM_INVALID, Some("max_units"))
    );
}

#[test]
fn r195_update_fields() {
    // Champs `Patch` : posés, gardés ou effacés ; nom et capacité ne
    // s'effacent pas.
    let mut s = fresh();
    apply(
        &mut s,
        json!([{ "op": "resource.add", "name": "R", "initials": "R", "group": "G" }]),
    );
    apply(
        &mut s,
        json!([{ "op": "resource.update", "uid": 1, "name": "S", "max_units": { "value": 15_000 },
                 "calendar": 2, "group": null, "notes": "n", "inactive": true }]),
    );
    let r = resource(&s, 1);
    assert_eq!(
        (r.name.as_str(), r.initials.as_str(), r.group.as_str()),
        ("S", "R", "")
    );
    assert_eq!(
        (r.max_units.get(), r.calendar),
        (15_000, Some(CalendarUid::new(2)))
    );
    assert_eq!((r.notes.as_str(), r.is_inactive), ("n", true));
    apply(
        &mut s,
        json!([{ "op": "resource.update", "uid": 1, "calendar": null }]),
    );
    assert_eq!(resource(&s, 1).calendar, None);
    for (op, field) in [
        (json!({ "op": "resource.update", "uid": 1, "name": null }), "name"),
        (
            json!({ "op": "resource.update", "uid": 1, "max_units": null }),
            "max_units",
        ),
        (
            json!({ "op": "resource.update", "uid": 1, "calendar": 77 }),
            "calendar",
        ),
    ] {
        let err = try_apply(&mut s, json!([op.clone()])).unwrap_err();
        assert_eq!(err.field.as_deref(), Some(field), "{op}");
    }
}

#[test]
fn r195_rates_stored_not_computed() {
    // R-195 amendée (J7, O46, ADR-0013 D7) : taux stockés tels que saisis
    // (montant en centièmes, unité d'origine), jamais convertis ni
    // calculés ; écrits avec leur format (`StandardRateFormat` 3 : jour).
    let mut s = with_tasks(&[("T", "2j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "Maçon" },
            { "op": "resource.add", "name": "Béton", "kind": "material", "material_label": "m³" },
            { "op": "resource.add", "name": "Frais", "kind": "cost" },
            { "op": "assignment.add", "task": 1, "resource": 1 }
        ]),
    );
    let schedule = s.schedule().clone();
    let o = apply(
        &mut s,
        json!([
            { "op": "resource.update", "uid": 1, "standard_rate": { "amount": 12_000, "per": "day" },
              "overtime_rate": { "amount": 2_250 }, "cost_per_use": { "amount": 500 } },
            { "op": "resource.update", "uid": 2, "standard_rate": { "amount": 415_875 } }
        ]),
    );
    assert_eq!(o.diff.changed_total, 0);
    assert_eq!(s.schedule(), &schedule);
    let r = resource(&s, 1);
    assert_eq!(
        (r.standard_rate, r.overtime_rate, r.cost_per_use),
        (
            Rate {
                amount: Money::new(12_000).unwrap(),
                per: RateUnit::Day
            },
            Rate {
                amount: Money::new(2_250).unwrap(),
                per: RateUnit::Hour
            },
            Money::new(500).unwrap()
        )
    );
    assert_eq!(resource(&s, 2).standard_rate.per, RateUnit::MaterialUnit);
    // Écrit puis relu : mêmes taux, même format.
    let w = s.save_bytes(&plannercraft_mspdi::write::WriteOptions::default());
    assert!(String::from_utf8_lossy(&w.xml).contains("<StandardRateFormat>3</StandardRateFormat>"));
    let doc = plannercraft_tests::mspdi::reread(&w.xml);
    let back = doc.project.resources.get(ResourceUid::new(1)).unwrap();
    assert_eq!(
        (back.standard_rate, back.overtime_rate, back.cost_per_use),
        (r_rates(&s, 1))
    );
    // `null` : remis à 0.
    apply(
        &mut s,
        json!([{ "op": "resource.update", "uid": 1, "overtime_rate": null }]),
    );
    assert_eq!(resource(&s, 1).overtime_rate, Rate::default());
    // Refus : montant négatif, unité de matériau sur une ressource de
    // travail, unité de temps sur une matérielle, heures supplémentaires
    // d'une matérielle, taux d'une ressource de coût, `per` d'un coût par
    // utilisation.
    for (op, field) in [
        (
            json!({ "op": "resource.update", "uid": 1, "standard_rate": { "amount": -1 } }),
            "standard_rate",
        ),
        (
            json!({ "op": "resource.update", "uid": 1, "standard_rate": { "amount": 1, "per": "material_unit" } }),
            "standard_rate",
        ),
        (
            json!({ "op": "resource.update", "uid": 2, "standard_rate": { "amount": 1, "per": "hour" } }),
            "standard_rate",
        ),
        (
            json!({ "op": "resource.update", "uid": 2, "overtime_rate": { "amount": 1 } }),
            "overtime_rate",
        ),
        (
            json!({ "op": "resource.update", "uid": 3, "standard_rate": { "amount": 1 } }),
            "standard_rate",
        ),
        (
            json!({ "op": "resource.update", "uid": 1, "cost_per_use": { "amount": 1, "per": "day" } }),
            "cost_per_use",
        ),
    ] {
        let err = try_apply(&mut s, json!([op.clone()])).unwrap_err();
        assert_eq!(
            (err.code, err.field.as_deref()),
            (codes::PARAM_INVALID, Some(field)),
            "{op}"
        );
    }
}

fn r_rates(s: &plannercraft_automation::Session, uid: u32) -> (Rate, Rate, Money) {
    let r = resource(s, uid);
    (r.standard_rate, r.overtime_rate, r.cost_per_use)
}
