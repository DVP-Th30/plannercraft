//! R-122 — Écriture : valeurs calculées et politique (`choose_computed`).

use plannercraft_core::{
    Assignment, AssignmentUid, CalendarUid, Duration, DurationUnit, ExtraEntry, Extras, LevelingDelay, Link,
    Minutes, Percent, Resource, ResourceUid, ScheduleAnchor, SchedulingMode, Task, TaskUid, TimeBasis,
    schedule,
};
use plannercraft_mspdi::write::{Computed, WriteCode, WriteOptions, choose_computed, write};
use plannercraft_tests::moment;
use plannercraft_tests::mspdi::{compact, new_project, reread, text};

fn days(n: i64) -> Duration {
    Duration::new(
        Minutes::new(n * 480),
        DurationUnit::Day,
        TimeBasis::Working,
        false,
    )
    .unwrap()
}

fn task(proj: &plannercraft_core::Project, uid: u32, d: i64) -> Task {
    let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &proj.options);
    t.duration = days(d);
    t
}

/// Contenu de l'élément `Task` d'UID `uid` (texte compact).
fn task_xml(xml: &str, uid: u32) -> String {
    let marker = format!("<Task><UID>{uid}</UID>");
    let start = xml.find(&marker).unwrap();
    let rest = xml.get(start..).unwrap();
    rest.get(..rest.find("</Task>").unwrap()).unwrap().to_owned()
}

#[test]
fn r122_schedule_values_are_written() {
    // A 2 j → B 1 j (FS), lun 19/10/2026 08:00 ; UID 0 présent.
    let mut proj = new_project("2026-10-19T08:00");
    proj.summary_task = Some(Task::new(TaskUid::PROJECT_SUMMARY, "P".to_owned(), &proj.options));
    let a = task(&proj, 1, 2);
    let mut b = task(&proj, 2, 1);
    b.predecessors = vec![Link::finish_to_start(TaskUid::new(1))];
    proj.tasks.push(a).unwrap();
    proj.tasks.push(b).unwrap();
    let sched = schedule(&proj);
    let (computed, diags) = choose_computed(&proj, &sched, None);
    assert!(matches!(computed, Computed::Schedule(_)));
    assert_eq!(diags, Vec::new());
    let out = write(&proj, computed, &WriteOptions::default());
    let xml = compact(&text(&out));
    let a = task_xml(&xml, 1);
    assert!(a.contains("<Start>2026-10-19T08:00:00</Start><Finish>2026-10-20T17:00:00</Finish>"));
    assert!(a.contains(
        "<EarlyStart>2026-10-19T08:00:00</EarlyStart><EarlyFinish>2026-10-20T17:00:00</EarlyFinish>"
    ));
    assert!(
        a.contains("<LateStart>2026-10-19T08:00:00</LateStart><LateFinish>2026-10-20T17:00:00</LateFinish>")
    );
    assert!(a.contains("<FreeSlack>0</FreeSlack><TotalSlack>0</TotalSlack><StartSlack>0</StartSlack><FinishSlack>0</FinishSlack>"));
    assert!(a.contains("<Critical>1</Critical>"));
    // UID 0 : durée calculée de la récapitulative du projet (3 j = 24 h).
    let p0 = task_xml(&xml, 0);
    assert!(p0.contains("<Duration>PT24H0M0S</Duration>"), "{p0}");
    assert!(xml.contains("<FinishDate>2026-10-21T17:00:00</FinishDate>"));
    // Relu : valeurs enregistrées = valeurs du moteur ; tâches égales à leur
    // image par l'aller-retour (la valeur calculée n'entre pas dans le
    // modèle, ADR-0004 ; seul le reste d'une tâche sans avancement est
    // dérivé, R-127).
    let back = reread(&out.xml);
    assert_eq!(
        back.project.tasks,
        plannercraft_mspdi::normalize(&proj, &WriteOptions::default()).tasks
    );
    assert_eq!(
        back.project
            .tasks
            .get(TaskUid::new(1))
            .unwrap()
            .progress
            .remaining_duration,
        plannercraft_core::Minutes::new(960)
    );
    let rec = &back.recorded.tasks[&TaskUid::new(1)];
    assert_eq!(rec.total_slack, Some(plannercraft_mspdi::Tenths(0)));
    assert_eq!(back.recorded.project_finish, Some(moment("2026-10-21T17:00")));
}

#[test]
fn r122_slack_in_tenths_inactive_and_manual() {
    // A 1 j et C 73 j seules : TF(A) = 72 j = 34 560 min → 345 600.
    let mut proj = new_project("2026-10-19T08:00");
    proj.tasks.push(task(&proj, 1, 1)).unwrap();
    proj.tasks.push(task(&proj, 2, 73)).unwrap();
    let mut inactive = task(&proj, 3, 1);
    inactive.active = false;
    proj.tasks.push(inactive).unwrap();
    let mut manual = task(&proj, 4, 1);
    manual.mode = SchedulingMode::Manual {
        start: None,
        finish: None,
        duration: None,
    };
    proj.tasks.push(manual).unwrap();
    let sched = schedule(&proj);
    let out = write(&proj, Computed::Schedule(&sched), &WriteOptions::default());
    let xml = compact(&text(&out));
    assert!(task_xml(&xml, 1).contains("<TotalSlack>345600</TotalSlack>"));
    // Inactive : dates d'affichage seulement.
    let i = task_xml(&xml, 3);
    assert!(i.contains("<Start>2026-10-19T08:00:00</Start>"), "{i}");
    assert!(!i.contains("<EarlyStart>") && !i.contains("<TotalSlack>") && !i.contains("<Critical>"));
    // Manuelle sans date : exclue, aucune valeur calculée.
    let m = task_xml(&xml, 4);
    assert!(
        !m.contains("<Start>") && !m.contains("<EarlyStart>") && !m.contains("<Critical>"),
        "{m}"
    );
}

#[test]
fn r122_policy_with_progress() {
    // Amendement J6 (R-215) : une tâche à 50 % n'est plus un manque, le
    // moteur calcule l'avancement ; valeurs du moteur, avec ou sans valeurs
    // relues, aucun diagnostic. Les valeurs relues restent réécrites telles
    // quelles avec `Computed::Recorded`.
    let mut proj = new_project("2026-10-19T08:00");
    let mut a = task(&proj, 1, 2);
    a.progress.percent_complete = Percent::new(5_000).unwrap();
    proj.tasks.push(a).unwrap();
    let sched = schedule(&proj);
    let (computed, diags) = choose_computed(&proj, &sched, None);
    assert!(matches!(computed, Computed::Schedule(_)));
    assert_eq!(diags, Vec::new());
    let xml = compact(&text(&write(&proj, computed, &WriteOptions::default())));
    assert!(xml.contains("<EarlyStart>") && xml.contains("<FinishDate>"));
    let first = reread(&write(&proj, Computed::Schedule(&sched), &WriteOptions::default()).xml);
    let (computed, diags) = choose_computed(&proj, &sched, Some(&first.recorded));
    assert!(matches!(computed, Computed::Schedule(_)));
    assert_eq!(diags, Vec::new());
    let again = reread(
        &write(
            &proj,
            Computed::Recorded(&first.recorded),
            &WriteOptions::default(),
        )
        .xml,
    );
    assert_eq!(again.recorded, first.recorded);
}

#[test]
fn r122_no_gap_left() {
    // R-122 amendée (J5, R-199 ; J6, R-215) : ressource dotée d'un
    // calendrier, retard de nivellement (modèle ou `Extras` hérité) et
    // avancement ne sont plus des manques : le moteur est toujours écrit.
    let mut proj = new_project("2026-10-19T08:00");
    proj.tasks.push(task(&proj, 1, 2)).unwrap();
    let mut r = Resource::new(ResourceUid::new(1), "R".to_owned());
    r.calendar = Some(CalendarUid::new(1));
    proj.resources.push(r).unwrap();
    let mut a = Assignment::new(AssignmentUid::new(1), TaskUid::new(1), Some(ResourceUid::new(1)));
    a.leveling_delay = Some(LevelingDelay {
        minutes: Minutes::new(480),
        unit: DurationUnit::Day,
        elapsed: false,
    });
    proj.assignments.push(a).unwrap();
    {
        let t = proj.tasks.get_mut(TaskUid::new(1)).unwrap();
        t.leveling_delay = Some(LevelingDelay {
            minutes: Minutes::new(4_320),
            unit: DurationUnit::Day,
            elapsed: true,
        });
        t.extras = Extras(vec![ExtraEntry {
            origin: "mspdi".to_owned(),
            key: "LevelingDelay".to_owned(),
            payload: "<LevelingDelay>4800</LevelingDelay>".to_owned(),
        }]);
        t.progress.actual_start = Some(moment("2026-10-19T08:00"));
    }
    let sched = schedule(&proj);
    let (computed, diags) = choose_computed(&proj, &sched, None);
    assert!(matches!(computed, Computed::Schedule(_)));
    assert_eq!(diags, Vec::new());
    let out = write(&proj, computed, &WriteOptions::default());
    assert!(
        out.diagnostics
            .iter()
            .all(|d| d.code.code() != "mspdi.write.schedule_incomplete"),
        "{:?}",
        out.diagnostics
    );
}

#[test]
fn r122_schedule_of_another_project_is_refused() {
    let mut proj = new_project("2026-10-19T08:00");
    proj.tasks.push(task(&proj, 1, 2)).unwrap();
    let other = schedule(&new_project("2026-10-19T08:00"));
    let out = write(&proj, Computed::Schedule(&other), &WriteOptions::default());
    assert_eq!(out.diagnostics[0].code, WriteCode::ScheduleMismatch);
    assert_eq!(out.diagnostics[0].code.code(), "mspdi.write.schedule_mismatch");
    assert!(!text(&out).contains("<EarlyStart>"));
}

#[test]
fn r122_schedule_from_finish_writes_the_computed_start() {
    let mut proj = new_project("2026-10-19T08:00");
    proj.anchor = ScheduleAnchor::FromFinish(moment("2026-10-23T17:00"));
    proj.tasks.push(task(&proj, 1, 1)).unwrap();
    let sched = schedule(&proj);
    let xml = compact(&text(&write(
        &proj,
        Computed::Schedule(&sched),
        &WriteOptions::default(),
    )));
    assert!(xml.contains("<ScheduleFromStart>0</ScheduleFromStart>"));
    assert!(xml.contains("<FinishDate>2026-10-23T17:00:00</FinishDate>"));
    // `StartDate` = début calculé du moteur (`Schedule::project_start`).
    assert!(xml.contains(&format!("<StartDate>{}:00</StartDate>", sched.project_start)));
}

#[test]
fn r122_start_slack_on_base_of_derived_project_calendar() {
    // R-122 amendée (dette R-113) : calendrier du projet dérivé (calendrier
    // de ressource, comme A4) d'une base lun-ven 09:00-13:00, 15:00-19:00 ;
    // `StartSlack`/`FinishSlack` mesurés sur la base, comme la marge du
    // moteur. A 1 j, C 3 j : TF(A) = 2 j = 960 → `StartSlack` 9 600.
    use crate::r113_derived_project_calendar::{DERIVED, net};
    use crate::r171_assignment_effective_calendar::{assign, resource};
    let proj = net().task("A", 480).task("C", 1_440).build();
    let sched = schedule(&proj);
    let (computed, diags) = choose_computed(&proj, &sched, None);
    assert!(matches!(computed, Computed::Schedule(_)));
    assert_eq!(diags, Vec::new());
    let xml = compact(&text(&write(&proj, computed, &WriteOptions::default())));
    let a = task_xml(&xml, 1);
    assert!(a.contains("<TotalSlack>9600</TotalSlack>"), "{a}");
    assert!(
        a.contains("<StartSlack>9600</StartSlack><FinishSlack>9600</FinishSlack>"),
        "{a}"
    );
    // A 4 h, C 1 j, affectées à une ressource du calendrier dérivé (J5 :
    // seule une ressource place sur le dérivé, R-171, R-175) : ES lun 08:00,
    // EF 12:00, LS 13:00, LF 17:00 (placement sur le dérivé). Sur la base :
    // début 240 (09:00-13:00), fin 180 (12:00-13:00, 15:00-17:00), TF = 180.
    // Sur le dérivé, l'écrivain aurait trouvé 240/240, dont le minimum ne
    // redonne pas la marge : valeurs omises avant l'amendement.
    let assigned = resource(net().task("A", 240).task("C", 480), 1, Some(DERIVED));
    let proj = assign(assign(assigned, 1, "A", 1, 240), 2, "C", 1, 480).build();
    let sched = schedule(&proj);
    assert_eq!(
        sched.scheduled(TaskUid::new(1)).unwrap().total_slack,
        Minutes::new(180)
    );
    let xml = compact(&text(&write(
        &proj,
        Computed::Schedule(&sched),
        &WriteOptions::default(),
    )));
    let a = task_xml(&xml, 1);
    assert!(a.contains("<TotalSlack>1800</TotalSlack>"), "{a}");
    assert!(
        a.contains("<StartSlack>2400</StartSlack><FinishSlack>1800</FinishSlack>"),
        "{a}"
    );
}
