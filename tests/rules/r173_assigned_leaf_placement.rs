//! R-173 — Placement d'une feuille affectée : chaque affectation de travail
//! est placée sur son calendrier effectif (R-171) depuis le début au plus tôt
//! du réseau, décalée de son délai (R-177) ; début de la tâche = premier
//! instant où une ressource affectée peut travailler ; fin = la plus tardive
//! des fins d'affectation ; durée calculée = temps ouvré de l'union des
//! intervalles travaillés (et non l'enveloppe). Source : S45, S14 (« total
//! span of active working time »), S8 ; O23, O26.

use plannercraft_core::{LinkType, Minutes, TaskSchedule, Work};
use plannercraft_tests::moment;
use plannercraft_tests::net::{Net, lag_w};

use crate::r171_assignment_effective_calendar::{assign, assign_full, placed, resource};

#[test]
fn r173_union_not_envelope() {
    // R1 1 j, R2 1 j avec délai 2 j ⇒ R1 lun, R2 mer ; tâche lun 08:00 –
    // mer 17:00, durée 960 (union) et non 1 440 (enveloppe).
    let net = Net::new("2026-10-19T08:00").task("A", 480);
    let net = resource(resource(net, 1, None), 2, None);
    let net = assign(net, 1, "A", 1, 480);
    let run = assign_full(net, 2, "A", Some(2), (480, 10_000, 960)).run();
    let a = run.at("A");
    assert_eq!(a.start, moment("2026-10-19T08:00"));
    assert_eq!(a.finish, moment("2026-10-21T17:00"));
    assert_eq!(a.duration, Minutes::new(960));
    assert_eq!(placed(&run, 1).span.finish, moment("2026-10-19T17:00"));
    assert_eq!(placed(&run, 2).span.start, moment("2026-10-21T08:00"));
    assert_eq!(placed(&run, 2).span.finish, moment("2026-10-21T17:00"));
}

#[test]
fn r173_two_finishes() {
    // R1 8 h décalée d'un jour, R2 8 h (A6 UID 7, O26, où le décalage est
    // un retard de nivellement) ⇒ R2 lun, R1 mar ; tâche lun 08:00 – mar
    // 17:00, durée 960 ; le successeur part de la fin la plus tardive.
    let net = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B");
    let net = resource(resource(net, 1, None), 2, None);
    let net = assign_full(net, 1, "A", Some(1), (480, 10_000, 480));
    let run = assign(net, 2, "A", 2, 480).run();
    assert_eq!(run.at("A").start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("A").duration, Minutes::new(960));
    assert_eq!(run.at("B").start, moment("2026-10-21T08:00"));
    assert_eq!(placed(&run, 1).span.start, moment("2026-10-20T08:00"));
}

/// Champs de date et de marge d'une tâche planifiée (travail, R-184,
/// retiré).
fn dates(s: &TaskSchedule) -> (TaskSchedule, Minutes) {
    let mut c = *s;
    c.work = Work::ZERO;
    c.driver = plannercraft_core::Driver::ProjectStart;
    c.late_driver = plannercraft_core::LateDriver::ProjectFinish;
    (c, s.duration)
}

#[test]
fn r173_single_assignment_equals_j4() {
    // Une affectation, ressource sans calendrier : 20 h à 50 % ⇒ d_a =
    // 2 400 ; mêmes dates, marges et durée que la tâche de 2 400 min sans
    // affectation (P34), lien à décalage compris.
    let build = |minutes: i64| {
        Net::new("2026-10-19T08:00")
            .task("P", 480)
            .task("A", minutes)
            .task("Z", 4_800)
            .link("P", "A", LinkType::FinishToStart, lag_w(120))
            .with_task("A", |t| t.deadline = Some(moment("2026-10-30T17:00")))
    };
    let oracle = build(2_400).run();
    let net = resource(build(480), 1, None);
    let run = assign_full(net, 1, "A", Some(1), (1_200, 5_000, 0)).run();
    assert_eq!(dates(run.at("A")), dates(oracle.at("A")));
    assert_eq!(placed(&run, 1).span, run.at("A").early);
    assert_eq!(placed(&run, 1).late, Some(run.at("A").late));
}

/// Session : tâche A de 3 j, R1 et R2 à 100 % et 24 h chacune, R2 en congé
/// le mar 20/10 (calendrier propre) ⇒ R1 lun 19 – mer 21, R2 lun 19 – jeu
/// 22/10 17:00 ; union lun – jeu = 4 j (1 920 min), et non 3 j (le plus
/// grand des `d_a`).
fn two_parts_session() -> plannercraft_automation::Session {
    use plannercraft_tests::resource_edit::{resource, with_tasks};
    use plannercraft_tests::task_edit::apply;
    use serde_json::json;
    let mut s = with_tasks(&[("A", "3j")]);
    apply(
        &mut s,
        json!([
            { "op": "resource.add", "name": "R1" },
            { "op": "resource.add", "name": "R2" }
        ]),
    );
    let cal = resource(&s, 2).calendar.unwrap().get();
    apply(
        &mut s,
        json!([
            { "op": "calendar.set_exception", "cal": cal, "from": "2026-10-20", "to": "2026-10-20" },
            { "op": "assignment.add", "task": 1, "resource": 1, "work": { "text": "24h" } },
            { "op": "assignment.add", "task": 1, "resource": 2, "work": { "text": "24h" } }
        ]),
    );
    s
}

/// Enregistre la session en MSPDI et la rouvre.
fn reopened(s: &plannercraft_automation::Session) -> plannercraft_automation::Session {
    use plannercraft_automation::text::Language;
    let bytes = s
        .save_bytes(&plannercraft_mspdi::write::WriteOptions::default())
        .xml;
    let origin = plannercraft_automation::DocOrigin {
        name: "p.xml".to_owned(),
        path: None,
    };
    plannercraft_automation::Session::open_bytes(&bytes, origin, moment("2026-10-19T08:00"), Language::Fr)
        .unwrap()
        .0
}

#[test]
fn r173_session_duration_is_union_and_reread_identical() {
    use plannercraft_tests::resource_edit::{duration, units_work};
    let s = two_parts_session();
    assert_eq!(
        (units_work(&s, 1, 1), units_work(&s, 1, 2)),
        ((10_000, 1_440), (10_000, 1_440))
    );
    let a = s.schedule().get(plannercraft_core::TaskUid::new(1)).unwrap();
    let plannercraft_core::TaskOutcome::Scheduled(a) = a else {
        panic!("A non planifiée");
    };
    assert_eq!(a.finish, moment("2026-10-22T17:00"));
    assert_eq!(a.duration, Minutes::new(1_920));
    // La durée du modèle suit l'union (S14), comme la durée écrite (R-199) :
    // la réouverture redonne le même modèle de tâche.
    assert_eq!(duration(&s, 1), 1_920);
    let r = reopened(&s);
    assert_eq!(duration(&r, 1), 1_920);
    assert_eq!(
        r.project()
            .tasks
            .get(plannercraft_core::TaskUid::new(1))
            .map(|t| t.duration),
        s.project()
            .tasks
            .get(plannercraft_core::TaskUid::new(1))
            .map(|t| t.duration)
    );
    assert_eq!(
        r.schedule().get(plannercraft_core::TaskUid::new(1)),
        s.schedule().get(plannercraft_core::TaskUid::new(1))
    );
}

#[test]
fn r173_open_syncs_recorded_duration() {
    // Fichier dont la durée enregistrée de A (3 j, le plus grand des `d_a`)
    // n'est pas l'union (4 j) : dès l'ouverture, la durée du modèle est
    // l'union, document non modifié ; un lot sans rapport (renommage) ne
    // change plus la durée de A.
    use plannercraft_tests::resource_edit::duration;
    use plannercraft_tests::task_edit::apply;
    use serde_json::json;
    let s = two_parts_session();
    let xml = String::from_utf8(
        s.save_bytes(&plannercraft_mspdi::write::WriteOptions::default())
            .xml,
    )
    .unwrap();
    let union = "<Duration>PT32H0M0S</Duration>";
    assert!(xml.contains(union), "{xml}");
    let legacy = xml.replacen(union, "<Duration>PT24H0M0S</Duration>", 1);
    let origin = plannercraft_automation::DocOrigin {
        name: "p.xml".to_owned(),
        path: None,
    };
    let (mut r, _) = plannercraft_automation::Session::open_bytes(
        legacy.as_bytes(),
        origin,
        moment("2026-10-19T08:00"),
        plannercraft_automation::text::Language::Fr,
    )
    .unwrap();
    assert_eq!(duration(&r, 1), 1_920);
    assert!(!r.is_dirty());
    let before = r
        .project()
        .tasks
        .get(plannercraft_core::TaskUid::new(1))
        .unwrap()
        .duration;
    apply(&mut r, json!([{ "op": "task.update", "uid": 1, "name": "A2" }]));
    let after = r
        .project()
        .tasks
        .get(plannercraft_core::TaskUid::new(1))
        .unwrap()
        .duration;
    assert_eq!(before, after);
}

#[test]
fn r173_session_duration_follows_later_edits() {
    // Les éditions suivantes partent de l'union, en session comme après la
    // réouverture : même résultat des deux côtés.
    use plannercraft_tests::resource_edit::{duration, units_work};
    use plannercraft_tests::task_edit::apply;
    use serde_json::json;
    let edits = [
        // Retrait non piloté de R2 : durée gardée (4 j) puis replacée sur
        // R1 seule (24 h ⇒ 3 j) : la durée suit, 1 440.
        (
            json!([{ "op": "task.update", "uid": 1, "effort_driven": false },
                   { "op": "assignment.remove", "task": 1, "resource": 2 }]),
            1_440,
        ),
        // Passage en durée fixe : rien n'est recalculé, la durée tenue est
        // l'union (4 j) ; chaque affectation dure alors 4 j (R-170).
        (
            json!([{ "op": "task.update", "uid": 1, "type": "fixed_duration" }]),
            1_920,
        ),
        // Ajout non piloté d'une troisième ressource : w = D × u avec D =
        // l'union (4 j ⇒ 32 h), comme MS Project, dont D est l'union.
        (
            json!([{ "op": "task.update", "uid": 1, "effort_driven": false },
                   { "op": "resource.add", "name": "R3" },
                   { "op": "assignment.add", "task": 1, "resource": 3 }]),
            1_920,
        ),
    ];
    for (k, (ops, expected)) in edits.into_iter().enumerate() {
        let mut s = two_parts_session();
        let mut r = reopened(&s);
        apply(&mut s, ops.clone());
        apply(&mut r, ops);
        assert_eq!(duration(&s, 1), expected, "édition {k}");
        assert_eq!(duration(&r, 1), duration(&s, 1), "édition {k}");
        // Reste des affectations dérivé à l'écriture (R-199) : relu égal au
        // travail, laissé tel quel en session ; le reste est identique.
        let mut relu = r.project().assignments.clone();
        for o in &s.project().assignments {
            if let Some(a) = relu.get_mut(plannercraft_core::HasUid::uid(o)) {
                a.remaining_work = o.remaining_work;
            }
        }
        assert_eq!(s.project().assignments, relu, "édition {k}");
        if k == 2 {
            assert_eq!(units_work(&s, 1, 3), (10_000, 1_920));
        }
    }
}

#[test]
fn r173_duration_sync_exclusions() {
    // Durée fixe et unités nulles : la durée du modèle est une entrée du
    // placement (S41, R-170, R-181), jamais remplacée par l'union.
    let base = || {
        let net = Net::new("2026-10-19T08:00").task("A", 480);
        let net = resource(resource(net, 1, None), 2, None);
        let net = assign(net, 1, "A", 1, 480);
        assign_full(net, 2, "A", Some(2), (480, 10_000, 960))
    };
    let sync = |net: Net| {
        let mut p = net.build();
        let s = plannercraft_core::schedule(&p);
        let changed = plannercraft_core::schedule::sync_assigned_durations(&mut p, &s);
        (changed, p)
    };
    // Unités fixes : union (960) reportée.
    let (changed, p) = sync(base());
    assert_eq!(changed, [plannercraft_core::TaskUid::new(1)]);
    assert_eq!(
        p.tasks.iter().next().unwrap().duration.minutes(),
        Minutes::new(960)
    );
    // Durée fixe : rien.
    let (changed, _) =
        sync(base().with_task("A", |t| t.task_type = plannercraft_core::TaskType::FixedDuration));
    assert_eq!(changed, Vec::<plannercraft_core::TaskUid>::new());
    // Une affectation à unités nulles : rien.
    let (changed, _) = sync(assign_full(
        resource(base(), 3, None),
        3,
        "A",
        Some(3),
        (480, 0, 0),
    ));
    assert_eq!(changed, Vec::<plannercraft_core::TaskUid>::new());
}

/// R1 sur le Standard 80 h (lun 19 – ven 30/10) ; R2 sur un calendrier de
/// week-end (sam-dim 08:00-12:00, 13:00-17:00) 16 h (sam 24 – dim 25/10).
fn weekend_parts() -> Net {
    use crate::r171_assignment_effective_calendar::base_calendar;
    use plannercraft_core::Weekday;
    let net = Net::new("2026-10-19T08:00").task("A", 4_800).project(|p| {
        p.calendars
            .push(base_calendar(30, |d| {
                matches!(d, Weekday::Saturday | Weekday::Sunday)
            }))
            .unwrap();
    });
    let net = resource(resource(net, 1, None), 2, Some(30));
    assign(assign(net, 1, "A", 1, 4_800), 2, "A", 2, 960)
}

#[test]
fn r173_measure_without_compiled_union() {
    // Union des deux calendriers non compilée (budget nul, R-172) : sur
    // [sam 24 08:00, dim 25 17:00), où les deux parts sont actives, la
    // mesure passe par le repli (plus grand des temps ouvrés : 16 h de R2),
    // jamais par l'échelle de la première part seule (0 h le week-end).
    // Durée = 40 h + 16 h + 40 h = 5 760, comme avec l'union compilée.
    let run = weekend_parts().run_with(plannercraft_core::Limits {
        combined_segments: 0,
        ..plannercraft_core::Limits::default()
    });
    assert!(
        run.codes().contains(&"calendar.too_many_segments"),
        "{:?}",
        run.codes()
    );
    assert_eq!(run.at("A").duration, Minutes::new(5_760));
    assert_eq!(weekend_parts().run().at("A").duration, Minutes::new(5_760));
}

#[test]
fn r173_too_many_parts_measured_as_longest() {
    // 1 001 affectations de 1 h décalées chacune d'une heure de plus
    // (plafond de la mesure exacte : 1 000 parts, règle 10) : durée = le
    // plus grand `d_a` (60), `assignment.too_many_parts` ; les dates ne
    // changent pas (fin = dernière part, 1 001 h ouvrées après le début).
    let mut net = Net::new("2026-10-19T08:00").task("A", 60);
    for k in 1..=1_001u32 {
        net = resource(net, k, None);
        net = assign_full(net, k, "A", Some(k), (60, 10_000, i64::from(k - 1) * 60));
    }
    let run = net.run();
    assert!(
        run.codes().contains(&"assignment.too_many_parts"),
        "{:?}",
        run.codes()
    );
    let a = run.at("A");
    assert_eq!(a.duration, Minutes::new(60));
    assert_eq!(a.start, moment("2026-10-19T08:00"));
}
