//! R-211 — Travail réel : le % de la tâche met à jour le travail réel de
//! chaque affectation (`TaskUpdatesResource`), travail réel saisi réparti au
//! prorata (plus grand reste, puis UID croissant), reste du travail dérivé.
//! Calendrier Standard, T 2 j (960 min) lun 19/10/2026 08:00 – mar 17:00 ;
//! R1 100 % (W 960), R2 50 % (W 480).

use crate::r201_percent_input::{MON, at, calendar, ctx, part, pct, task, work, write};
use plannercraft_core::progress::edit::{EditError, Input, apply, remaining_work_entered};
use plannercraft_core::progress::remaining_work;
use plannercraft_core::{Assignment, AssignmentUid, Percent, TaskUid, Work};

const TUE: &str = "2026-10-20T17:00";

#[test]
fn r211_prorata_and_largest_remainder() {
    let cal = calendar();
    let c = ctx(&cal, MON, TUE);
    let parts = [part(2, 480), part(1, 960)];

    // 50 % : AW = 50 % de W = 480 et 240 ; % travail = 720 × 10 000 /
    // 1 440 = 5 000 ; reste du travail 720.
    let e = apply(&task(960), &parts, &[Input::Percent(pct(5_000))], &c).unwrap();
    assert_eq!(e.parts[0].uid, AssignmentUid::new(1));
    assert_eq!(e.parts[0].actual_work, work(480));
    assert_eq!(e.parts[1].actual_work, work(240));
    assert_eq!(e.progress.percent_work_complete, pct(5_000));
    assert_eq!(e.progress.actual_work, work(720));
    assert_eq!(e.progress.remaining_work, work(720));

    // Travail réel 540 sur la tâche : 540 × 960 / 1 440 = 360, 540 × 480 /
    // 1 440 = 180 ; R-211 amendée (2026-10-20) : saisi seul, il fixe la
    // durée réelle = 960 × 540 / 1 440 = 360 (% 3 750), début réel
    // implicite lun 08:00.
    let e = apply(&task(960), &parts, &[Input::ActualWork(work(540))], &c).unwrap();
    assert_eq!(e.parts[0].actual_work, work(360));
    assert_eq!(e.parts[1].actual_work, work(180));
    assert_eq!(e.progress.percent_work_complete, pct(3_750));
    assert_eq!(e.progress.actual_duration, plannercraft_core::Minutes::new(360));
    assert_eq!(e.progress.percent_complete, pct(3_750));
    assert_eq!(e.progress.actual_start, Some(at(MON)));
    // % travail 25 % saisi seul : AW 240 et 120, durée réelle 240.
    let e = apply(&task(960), &parts, &[Input::PercentWork(pct(2_500))], &c).unwrap();
    assert_eq!(e.progress.actual_duration, plannercraft_core::Minutes::new(240));
    // Avec un champ de durée, la durée réelle saisie l'emporte.
    let e = apply(
        &task(960),
        &parts,
        &[
            Input::ActualWork(work(540)),
            Input::ActualDuration(plannercraft_core::Minutes::new(480)),
        ],
        &c,
    )
    .unwrap();
    assert_eq!(e.progress.actual_duration, plannercraft_core::Minutes::new(480));

    // 7 min sur trois W égaux : 2 chacune, la minute restante au plus petit
    // UID ⇒ 3, 2, 2.
    let three = [part(3, 60), part(1, 60), part(2, 60)];
    let e = apply(&task(960), &three, &[Input::ActualWork(work(7))], &c).unwrap();
    let aw: Vec<Work> = e.parts.iter().map(|p| p.actual_work).collect();
    assert_eq!(aw, [work(3), work(2), work(2)]);

    // % travail 100 % : travail achevé ⇒ tâche achevée (S52), fin réelle =
    // fin planifiée mar 17:00.
    let e = apply(&task(960), &parts, &[Input::PercentWork(Percent::FULL)], &c).unwrap();
    assert_eq!(e.progress.actual_finish, Some(at(TUE)));
    assert_eq!(e.progress.percent_complete, Percent::FULL);
    assert_eq!(e.progress.remaining_work, Work::ZERO);

    // Travail réel au-delà de W (1 800 > 1 440) : 1 200 et 600, W = AW.
    let e = apply(&task(960), &parts, &[Input::ActualWork(work(1_800))], &c).unwrap();
    assert_eq!(e.parts[0].work, work(1_200));
    assert_eq!(e.parts[1].work, work(600));
    assert_eq!(e.progress.actual_finish, Some(at(TUE)));

    // Sans affectation de travail : refusé.
    assert_eq!(
        apply(&task(960), &[], &[Input::ActualWork(work(60))], &c),
        Err(EditError::Conflict("actual_work", "assignments"))
    );
    // Travail non achevé sur une tâche achevée : refusé.
    let mut t = task(960);
    let done = apply(&t, &parts, &[Input::Percent(Percent::FULL)], &c).unwrap();
    write(&mut t, &done);
    assert_eq!(
        apply(&t, &done.parts, &[Input::PercentWork(pct(5_000))], &c),
        Err(EditError::Conflict("percent_work_complete", "actual_finish"))
    );
}

#[test]
fn r211_remaining_work_changes_work() {
    // Reste du travail dérivé : W 960, AW 480 ⇒ 480 (progress::remaining_work).
    let mut a = Assignment::new(AssignmentUid::new(1), TaskUid::new(1), None);
    a.work = work(960);
    a.actual_work = work(480);
    assert_eq!(remaining_work(&a), work(480));

    // Reste saisi 720 sur cette affectation : W = AW + reste = 1 200 ; le
    // travail réel ne bouge pas (R-189 « travail » suit, côté commande).
    let p = plannercraft_core::progress::edit::Part {
        actual_work: work(480),
        ..part(1, 960)
    };
    let q = remaining_work_entered(p, work(720)).unwrap();
    assert_eq!(q.work, work(1_200));
    assert_eq!(q.actual_work, work(480));
    a.work = q.work;
    assert_eq!(remaining_work(&a), work(720));
}

/// R-211 amendée (arbitrage du 2026-10-20), cas relevé par le lot L10 : T 3
/// j, R 100 % (W 24 h), travail réel 16 h saisi seul ⇒ durée réelle 2 j,
/// `Stop` mar 17:00, reprise mer 08:00, fin mer 17:00 (D = 3 j) ; la partie
/// réelle de l'affectation (16 h, lun – mar) finit à `Stop` et son reste
/// (8 h) part de la reprise, sans chevauchement.
#[test]
fn r211_actual_work_alone_sets_actual_duration() {
    use plannercraft_tests::resource_edit::with_tasks;
    use plannercraft_tests::task_edit::apply as apply_ops;
    use serde_json::json;

    let mut s = with_tasks(&[("T", "3j")]);
    apply_ops(
        &mut s,
        json!([
            { "op": "resource.add", "name": "R" },
            { "op": "assignment.add", "task": 1, "resource": 1 },
            { "op": "progress.update", "task": 1, "actual_work": { "text": "16h" } }
        ]),
    );
    let t = s.project().tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(t.progress.actual_duration, plannercraft_core::Minutes::new(960));
    let Some(plannercraft_core::TaskOutcome::Scheduled(ts)) = s.schedule().get(TaskUid::new(1)) else {
        panic!("T non planifiée");
    };
    let g = ts.progress.unwrap();
    assert_eq!(g.stop, crate::r201_percent_input::at("2026-10-20T17:00"));
    assert_eq!(g.resume, crate::r201_percent_input::at("2026-10-21T08:00"));
    assert_eq!(ts.finish, crate::r201_percent_input::at("2026-10-21T17:00"));
    let a = s.schedule().assignments.values().next().unwrap();
    let actual = a.actual.unwrap();
    assert_eq!(actual.finish, g.stop);
    assert_eq!(a.remaining_start, Some(g.resume));
}
