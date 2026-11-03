//! R-215 — MSPDI écriture de l'avancement avec le moteur (J6) : avancement
//! calculé des feuilles, des récapitulatives et de l'UID 0, `Stop`/`Resume`,
//! réels des affectations, répartitions de réel gardées ou jetées.

use plannercraft_core::{
    Assignment, AssignmentUid, Duration, DurationUnit, ExtraEntry, Extras, Minutes, OutlineLevel, Percent,
    Project, Resource, ResourceUid, Task, TaskUid, TimeBasis, Work, schedule,
};
use plannercraft_mspdi::write::{Computed, WriteCode, WriteOptions, Written, schedule_image, write};
use plannercraft_mspdi::{ReadOptions, normalize, read};
use plannercraft_tests::mspdi::{compact, new_project, reread, text};
use plannercraft_tests::{moment, tests_path};

fn task(p: &Project, uid: u32, minutes: i64) -> Task {
    let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &p.options);
    t.duration = Duration::new(
        Minutes::new(minutes),
        DurationUnit::Day,
        TimeBasis::Working,
        false,
    )
    .unwrap();
    t
}

fn work(m: i64) -> Work {
    Work::new(Minutes::new(m)).unwrap()
}

/// Écrit avec le moteur ; rend le texte compact et la sortie.
fn by_engine(p: &Project) -> (String, Written) {
    let s = schedule(p);
    let out = write(p, Computed::Schedule(&s), &WriteOptions::default());
    (compact(&text(&out)), out)
}

/// Contenu de l'élément `element` d'UID `uid` (texte compact).
fn entity(xml: &str, element: &str, uid: u32) -> String {
    let marker = format!("<{element}><UID>{uid}</UID>");
    let start = xml.find(&marker).unwrap();
    let rest = xml.get(start..).unwrap();
    let end = rest.find(&format!("</{element}>")).unwrap();
    rest.get(..end).unwrap().to_owned()
}

/// Aller-retour avec le moteur : la relecture égale l'image (P43).
fn assert_round_trip(p: &Project) {
    let s = schedule(p);
    let out = write(p, Computed::Schedule(&s), &WriteOptions::default());
    let back = reread(&out.xml);
    assert_eq!(
        back.project,
        normalize(&schedule_image(p, &s), &WriteOptions::default())
    );
}

/// T 2 j depuis lun 19/10/2026 08:00, commencée lun 08:00, 1 j réel (R-204).
fn in_progress() -> Project {
    let mut p = new_project("2026-10-19T08:00");
    let mut t = task(&p, 1, 960);
    t.progress.actual_start = Some(moment("2026-10-19T08:00"));
    t.progress.actual_duration = Minutes::new(480);
    p.tasks.push(t).unwrap();
    p
}

#[test]
fn r215_stop_resume_written() {
    // En cours : `Stop` = fin de la partie réelle (lun 17:00) ; `Resume`
    // omis sans reprise saisie (relecture finale J6, J6-F1 : un `Resume`
    // relu est une reprise saisie, R-214 ; la reprise calculée n'est pas
    // écrite).
    let p = in_progress();
    let (xml, _) = by_engine(&p);
    let t = entity(&xml, "Task", 1);
    assert!(t.contains("<Stop>2026-10-19T17:00:00</Stop>"), "{t}");
    assert!(!t.contains("<Resume>"), "{t}");
    assert!(t.contains("<PercentComplete>50</PercentComplete>"), "{t}");
    assert!(t.contains("<ActualDuration>PT8H0M0S</ActualDuration>"), "{t}");
    assert!(
        t.contains("<RemainingDuration>PT8H0M0S</RemainingDuration>"),
        "{t}"
    );
    let back = reread(&write(&p, Computed::Schedule(&schedule(&p)), &WriteOptions::default()).xml);
    let r = back.project.tasks.get(TaskUid::new(1)).unwrap().progress;
    assert_eq!(r.resume, None);
    assert_round_trip(&p);
    // Reprise saisie mar 13:00 : écrite telle quelle, relue telle quelle.
    let mut p = in_progress();
    p.tasks.get_mut(TaskUid::new(1)).unwrap().progress.resume = Some(moment("2026-10-20T13:00"));
    let (xml, _) = by_engine(&p);
    let t = entity(&xml, "Task", 1);
    assert!(
        t.contains("<Stop>2026-10-19T17:00:00</Stop><Resume>2026-10-20T13:00:00</Resume>"),
        "{t}"
    );
    assert_round_trip(&p);
    // Achevée : `Stop` = `Resume` = fin réelle ; non commencée : omis.
    let mut p = new_project("2026-10-19T08:00");
    let mut done = task(&p, 1, 480);
    done.progress.actual_start = Some(moment("2026-10-19T08:00"));
    done.progress.actual_finish = Some(moment("2026-10-19T17:00"));
    p.tasks.push(done).unwrap();
    p.tasks.push(task(&p, 2, 480)).unwrap();
    let (xml, _) = by_engine(&p);
    let t = entity(&xml, "Task", 1);
    assert!(
        t.contains("<Stop>2026-10-19T17:00:00</Stop><Resume>2026-10-19T17:00:00</Resume>"),
        "{t}"
    );
    assert!(t.contains("<PercentComplete>100</PercentComplete>"), "{t}");
    let n = entity(&xml, "Task", 2);
    assert!(!n.contains("<Stop>") && !n.contains("<Resume>") && !n.contains("<ActualStart>"));
    assert_round_trip(&p);
}

#[test]
fn r215_implicit_start_and_derived_percent_written() {
    // 25 % sans début réel ni durée réelle : début réel implicite (début au
    // plus tôt, lun 08:00), durée réelle effective 240, % dérivé 25.
    let mut p = new_project("2026-10-19T08:00");
    let mut t = task(&p, 1, 960);
    t.progress.percent_complete = Percent::new(2_500).unwrap();
    p.tasks.push(t).unwrap();
    let (xml, _) = by_engine(&p);
    let t = entity(&xml, "Task", 1);
    assert!(
        t.contains("<ActualStart>2026-10-19T08:00:00</ActualStart>"),
        "{t}"
    );
    assert!(t.contains("<ActualDuration>PT4H0M0S</ActualDuration>"), "{t}");
    assert!(t.contains("<PercentComplete>25</PercentComplete>"), "{t}");
    assert!(
        t.contains("<RemainingDuration>PT12H0M0S</RemainingDuration>"),
        "{t}"
    );
    assert_round_trip(&p);
}

#[test]
fn r215_summary_progress_from_engine() {
    // S (récapitulative) de A 1 j achevée et B 1 j non commencée, en
    // parallèle (S dure 1 j) ; UID 0. Agrégats de R-210 : 50 %, durée
    // réelle 480 × 480 / 960 = 240, reste 240, début réel lun 08:00, pas de
    // fin réelle, `Stop` = `Resume` = lun 17:00.
    let mut p = new_project("2026-10-19T08:00");
    p.summary_task = Some(Task::new(TaskUid::PROJECT_SUMMARY, "P".to_owned(), &p.options));
    let mut summary = task(&p, 1, 0);
    summary.outline_level = OutlineLevel::new(1).unwrap();
    p.tasks.push(summary).unwrap();
    let mut done = task(&p, 2, 480);
    done.outline_level = OutlineLevel::new(2).unwrap();
    done.progress.actual_start = Some(moment("2026-10-19T08:00"));
    done.progress.actual_finish = Some(moment("2026-10-19T17:00"));
    p.tasks.push(done).unwrap();
    let mut todo = task(&p, 3, 480);
    todo.outline_level = OutlineLevel::new(2).unwrap();
    p.tasks.push(todo).unwrap();
    let (xml, _) = by_engine(&p);
    for uid in [0, 1] {
        let t = entity(&xml, "Task", uid);
        assert!(t.contains("<PercentComplete>50</PercentComplete>"), "{uid} {t}");
        assert!(
            t.contains("<ActualStart>2026-10-19T08:00:00</ActualStart>"),
            "{t}"
        );
        assert!(!t.contains("<ActualFinish>"), "{t}");
        assert!(t.contains("<ActualDuration>PT4H0M0S</ActualDuration>"), "{t}");
        assert!(
            t.contains("<RemainingDuration>PT4H0M0S</RemainingDuration>"),
            "{t}"
        );
        assert!(
            t.contains("<Stop>2026-10-19T17:00:00</Stop><Resume>2026-10-19T17:00:00</Resume>"),
            "{t}"
        );
    }
    assert_round_trip(&p);
    // La récapitulative ne reçoit pas de reprise saisie.
    let back = reread(&write(&p, Computed::Schedule(&schedule(&p)), &WriteOptions::default()).xml);
    assert_eq!(
        back.project.tasks.get(TaskUid::new(1)).unwrap().progress.resume,
        None
    );
}

/// `in_progress` avec R1 à 100 % sur T (16 h, 4 h réelles) ; `rows` :
/// lignes de réel de l'affectation.
fn with_assignment(rows: &[(u8, &str)]) -> Project {
    let mut p = in_progress();
    p.resources
        .push(Resource::new(ResourceUid::new(1), "R1".to_owned()))
        .unwrap();
    let mut a = Assignment::new(AssignmentUid::new(1), TaskUid::new(1), Some(ResourceUid::new(1)));
    a.work = work(960);
    a.actual_work = work(240);
    a.extras = Extras(
        rows.iter()
            .map(|(kind, value)| ExtraEntry {
                origin: "mspdi".to_owned(),
                key: "TimephasedData".to_owned(),
                payload: format!(
                    "<TimephasedData><Type>{kind}</Type><UID>1</UID><Start>2026-10-19T08:00:00</Start>\
                     <Finish>2026-10-19T17:00:00</Finish><Unit>2</Unit><Value>{value}</Value></TimephasedData>"
                ),
            })
            .collect(),
    );
    p.assignments.push(a).unwrap();
    p.tasks.get_mut(TaskUid::new(1)).unwrap().work = work(960);
    p
}

#[test]
fn r215_assignment_actuals_written() {
    // Affectation d'une tâche en cours : `ActualStart` = début de son
    // placement, pas d'`ActualFinish`, `Stop` = fin de sa partie réelle,
    // `Resume` = début de son reste ; `RemainingWork` = W − réel = 12 h.
    let p = with_assignment(&[]);
    let s = schedule(&p);
    let placed = s.assignments[&AssignmentUid::new(1)];
    let actual = placed.actual.unwrap();
    let (xml, _) = by_engine(&p);
    let a = entity(&xml, "Assignment", 1);
    let date = |m: plannercraft_core::Moment| plannercraft_mspdi::scalar::format_datetime(m);
    assert!(
        a.contains(&format!("<ActualStart>{}</ActualStart>", date(placed.span.start))),
        "{a}"
    );
    assert!(!a.contains("<ActualFinish>"), "{a}");
    assert!(
        a.contains(&format!("<Stop>{}</Stop>", date(actual.finish))),
        "{a}"
    );
    let resume = placed.remaining_start.unwrap_or(actual.finish);
    assert!(a.contains(&format!("<Resume>{}</Resume>", date(resume))), "{a}");
    assert!(a.contains("<RemainingWork>PT12H0M0S</RemainingWork>"), "{a}");
    // Relu : valeurs calculées, dans `RecordedSchedule` seulement.
    let back = reread(&write(&p, Computed::Schedule(&s), &WriteOptions::default()).xml);
    let r = back.recorded.assignments[&AssignmentUid::new(1)];
    assert_eq!(r.actual_start, Some(placed.span.start));
    assert_eq!(r.stop, Some(actual.finish));
    assert_eq!(r.actual_finish, None);
    // Sans valeurs calculées : rien.
    let omitted = compact(&text(&write(&p, Computed::Omit, &WriteOptions::default())));
    let a = entity(&omitted, "Assignment", 1);
    assert!(!a.contains("<ActualStart>") && !a.contains("<Stop>"), "{a}");
}

#[test]
fn r215_timephased_actuals_kept_or_dropped() {
    // Somme des lignes de type 2 = travail réel (4 h) : lignes de réel
    // gardées ; travail réel changé (6 h) : lignes 2 et 3 jetées,
    // `mspdi.write.timephased_dropped { count: 2 }`.
    let rows = [(2, "PT4H0M0S"), (3, "PT0H0M0S")];
    let p = with_assignment(&rows);
    let (xml, out) = by_engine(&p);
    assert_eq!(
        entity(&xml, "Assignment", 1).matches("<TimephasedData>").count(),
        2
    );
    assert!(
        out.diagnostics
            .iter()
            .all(|d| !matches!(d.code, WriteCode::TimephasedDropped { .. }))
    );
    assert_round_trip(&p);
    let mut p = with_assignment(&rows);
    p.assignments.get_mut(AssignmentUid::new(1)).unwrap().actual_work = work(360);
    let (xml, out) = by_engine(&p);
    assert_eq!(
        entity(&xml, "Assignment", 1).matches("<TimephasedData>").count(),
        0
    );
    let dropped: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| matches!(d.code, WriteCode::TimephasedDropped { .. }))
        .collect();
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].code, WriteCode::TimephasedDropped { count: 2 });
    assert_round_trip(&p);
    // Sans valeurs calculées, réécrites telles quelles.
    let omitted = compact(&text(&write(&p, Computed::Omit, &WriteOptions::default())));
    assert_eq!(
        entity(&omitted, "Assignment", 1)
            .matches("<TimephasedData>")
            .count(),
        2
    );
}

#[test]
fn r215_a7_written_by_engine() {
    // A7 (O37 à O40) écrit avec le moteur : UID 2 `Stop` sam 13:00 ; UID 0
    // durée réelle 773 min (relue 772 : PT12H52M42S tronqué, R-004) ;
    // aucune répartition jetée ; relu = `normalize(schedule_image)`.
    let Ok(bytes) = std::fs::read(tests_path("corpus/files/projinteg.xml")) else {
        return;
    };
    let doc = read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    let s = schedule(&doc.project);
    let out = write(&doc.project, Computed::Schedule(&s), &WriteOptions::default());
    let xml = compact(&text(&out));
    let t2 = entity(&xml, "Task", 2);
    assert!(
        t2.contains("<Stop>2024-10-05T13:00:00</Stop><Resume>2024-10-05T13:00:00</Resume>"),
        "{t2}"
    );
    let t0 = entity(&xml, "Task", 0);
    assert!(t0.contains("<ActualDuration>PT12H53M0S</ActualDuration>"), "{t0}");
    assert!(t0.contains("<PercentComplete>10</PercentComplete>"), "{t0}");
    assert!(
        out.diagnostics
            .iter()
            .all(|d| !matches!(d.code, WriteCode::TimephasedDropped { .. })),
        "{:?}",
        out.diagnostics
    );
    let back = read(&out.xml, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    assert_eq!(
        back.project,
        normalize(&schedule_image(&doc.project, &s), &WriteOptions::default())
    );
    // `Stop`, `Resume` et réels écrits par le moteur égaux à ceux du fichier,
    // sauf les trois affectations de travail nul de la manuelle UID 4 (47 à
    // 49 : datées à sa fin réelle par MS Project, à son début par le
    // placement du moteur, R-183, hors lot).
    let mut differ = Vec::new();
    for (uid, rec) in &doc.recorded.tasks {
        let w = &back.recorded.tasks[uid];
        if (w.stop, w.resume) != (rec.stop, rec.resume) {
            differ.push(format!("tâche {}", uid.get()));
        }
    }
    for (uid, rec) in &doc.recorded.assignments {
        let w = &back.recorded.assignments[uid];
        let key =
            |r: &plannercraft_mspdi::RecordedAssignment| (r.actual_start, r.actual_finish, r.stop, r.resume);
        if key(w) != key(rec) {
            differ.push(format!("affectation {}", uid.get()));
        }
    }
    assert_eq!(differ, ["affectation 47", "affectation 48", "affectation 49"]);
}

/// Relecture finale J6 (J6-F1) : P 2 j FS → T 3 j commencée lun 08:00, 1 j
/// réel, reprise tirée par le lien (mer 08:00). Après l'aller-retour, puis
/// P raccourcie à 1 j, le reste de T avance comme dans le projet d'origine
/// (mar 08:00 ; mar 13:00 avec une reprise saisie mar 13:00) : la reprise
/// calculée n'est pas relue comme saisie.
#[test]
fn r215_resume_round_trip_keeps_plan_behaviour() {
    for entered in [None, Some(moment("2026-10-20T13:00"))] {
        let mut p = new_project("2026-10-19T08:00");
        p.tasks.push(task(&p, 1, 960)).unwrap();
        let mut t = task(&p, 2, 1_440);
        t.progress.actual_start = Some(moment("2026-10-19T08:00"));
        t.progress.actual_duration = Minutes::new(480);
        t.progress.resume = entered;
        t.predecessors.push(plannercraft_core::Link {
            predecessor: TaskUid::new(1),
            kind: plannercraft_core::LinkType::FinishToStart,
            lag: plannercraft_core::Lag::ZERO,
        });
        p.tasks.push(t).unwrap();
        let resume_of = |p: &Project| {
            schedule(p)
                .scheduled(TaskUid::new(2))
                .and_then(|g| g.progress)
                .map(|g| g.resume)
        };
        assert_eq!(resume_of(&p), Some(moment("2026-10-21T08:00")));
        let s = schedule(&p);
        let out = write(&p, Computed::Schedule(&s), &WriteOptions::default());
        let mut back = reread(&out.xml).project;
        assert_eq!(resume_of(&back), Some(moment("2026-10-21T08:00")));
        let one_day = Duration::new(Minutes::new(480), DurationUnit::Day, TimeBasis::Working, false).unwrap();
        p.tasks.get_mut(TaskUid::new(1)).unwrap().duration = one_day;
        back.tasks.get_mut(TaskUid::new(1)).unwrap().duration = one_day;
        let expected = entered.unwrap_or(moment("2026-10-20T08:00"));
        assert_eq!(resume_of(&p), Some(expected), "{entered:?}");
        assert_eq!(resume_of(&back), Some(expected), "{entered:?}");
    }
}

/// Relecture finale J6 (J6-F2) : travail réel et restes d'une
/// récapitulative et de l'UID 0 écrits depuis le moteur, jamais les
/// valeurs stockées (ici périmées). S1 {A 2 j, R1 W 16 h dont 8 h réelles,
/// 50 % ; B 1 j, R1 W 8 h} : `Work` 24 h, `ActualWork` 8 h, `RemainingWork`
/// 16 h ; S2 {C 1 j non commencée, R1 W 8 h} : `ActualWork` 0,
/// `RemainingDuration` = durée calculée (8 h), `RemainingWork` = travail
/// calculé (8 h).
#[test]
fn r215_summary_work_written() {
    let mut p = new_project("2026-10-19T08:00");
    p.summary_task = Some(Task::new(TaskUid::PROJECT_SUMMARY, "P".to_owned(), &p.options));
    p.resources
        .push(Resource::new(ResourceUid::new(1), "R1".to_owned()))
        .unwrap();
    let stale = |t: &mut Task| {
        t.progress.actual_work = work(999);
        t.progress.remaining_work = Work::ZERO;
        t.progress.remaining_duration = Minutes::ZERO;
    };
    let level = |t: &mut Task, l: u16| t.outline_level = OutlineLevel::new(l).unwrap();
    let mut s1 = task(&p, 1, 0);
    stale(&mut s1);
    p.tasks.push(s1).unwrap();
    let mut leaf_a = task(&p, 2, 960);
    level(&mut leaf_a, 2);
    leaf_a.work = work(960);
    leaf_a.progress.actual_start = Some(moment("2026-10-19T08:00"));
    leaf_a.progress.actual_duration = Minutes::new(480);
    leaf_a.progress.actual_work = work(480);
    p.tasks.push(leaf_a).unwrap();
    let mut leaf_b = task(&p, 3, 480);
    level(&mut leaf_b, 2);
    leaf_b.work = work(480);
    p.tasks.push(leaf_b).unwrap();
    let mut s2 = task(&p, 4, 0);
    stale(&mut s2);
    p.tasks.push(s2).unwrap();
    let mut leaf_c = task(&p, 5, 480);
    level(&mut leaf_c, 2);
    leaf_c.work = work(480);
    p.tasks.push(leaf_c).unwrap();
    for (uid, task, w, aw) in [(1, 2, 960, 480), (2, 3, 480, 0), (3, 5, 480, 0)] {
        let mut asg = Assignment::new(
            AssignmentUid::new(uid),
            TaskUid::new(task),
            Some(ResourceUid::new(1)),
        );
        asg.work = work(w);
        asg.actual_work = work(aw);
        p.assignments.push(asg).unwrap();
    }
    let (xml, _) = by_engine(&p);
    let s1 = entity(&xml, "Task", 1);
    assert!(s1.contains("<Work>PT24H0M0S</Work>"), "{s1}");
    assert!(s1.contains("<ActualWork>PT8H0M0S</ActualWork>"), "{s1}");
    assert!(s1.contains("<RemainingWork>PT16H0M0S</RemainingWork>"), "{s1}");
    let s2 = entity(&xml, "Task", 4);
    assert!(s2.contains("<ActualWork>PT0H0M0S</ActualWork>"), "{s2}");
    assert!(
        s2.contains("<RemainingDuration>PT8H0M0S</RemainingDuration>"),
        "{s2}"
    );
    assert!(s2.contains("<RemainingWork>PT8H0M0S</RemainingWork>"), "{s2}");
    let p0 = entity(&xml, "Task", 0);
    assert!(p0.contains("<Work>PT32H0M0S</Work>"), "{p0}");
    assert!(p0.contains("<ActualWork>PT8H0M0S</ActualWork>"), "{p0}");
    assert!(p0.contains("<RemainingWork>PT24H0M0S</RemainingWork>"), "{p0}");
    assert_round_trip(&p);
}
