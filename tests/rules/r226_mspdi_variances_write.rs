//! R-226 — MSPDI : écarts écrits (minutes × 1000) avec le moteur, relus
//! tels quels avec les valeurs relues.

use plannercraft_core::{
    Assignment, AssignmentUid, BaselineNumber, Duration, DurationUnit, Minutes, Project, Resource,
    ResourceUid, Task, TaskBaseline, TaskUid, TimeBasis, Work, schedule,
};
use plannercraft_mspdi::write::{Computed, WriteOptions, write};
use plannercraft_mspdi::{ReadOptions, read};
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

fn entity(xml: &str, element: &str, uid: u32) -> String {
    let marker = format!("<{element}><UID>{uid}</UID>");
    let start = xml.find(&marker).unwrap();
    let rest = xml.get(start..).unwrap();
    let end = rest.find(&format!("</{element}>")).unwrap();
    rest.get(..end).unwrap().to_owned()
}

/// T1 1 j (R1, 8 h), sans ligne de base.
fn project() -> Project {
    let mut p = new_project("2026-10-19T08:00");
    p.summary_task = Some(Task::new(TaskUid::PROJECT_SUMMARY, "P".to_owned(), &p.options));
    let mut t = task(&p, 1, 480);
    t.work = work(480);
    p.tasks.push(t).unwrap();
    p.resources
        .push(Resource::new(ResourceUid::new(1), "R1".to_owned()))
        .unwrap();
    let mut a = Assignment::new(AssignmentUid::new(1), TaskUid::new(1), Some(ResourceUid::new(1)));
    a.work = work(480);
    p.assignments.push(a).unwrap();
    p
}

fn by_engine(p: &Project) -> String {
    let s = schedule(p);
    compact(&text(&write(p, Computed::Schedule(&s), &WriteOptions::default())))
}

#[test]
fn r226_no_baseline_work_variance() {
    // O43 : sans ligne de base, début et fin 0, travail = W × 1000.
    let xml = by_engine(&project());
    let t = entity(&xml, "Task", 1);
    assert!(t.contains("<StartVariance>0</StartVariance>"), "{t}");
    assert!(t.contains("<FinishVariance>0</FinishVariance>"), "{t}");
    assert!(t.contains("<WorkVariance>480000.00</WorkVariance>"), "{t}");
    let r = entity(&xml, "Resource", 1);
    assert!(r.contains("<WorkVariance>480000.00</WorkVariance>"), "{r}");
    let a = entity(&xml, "Assignment", 1);
    assert!(a.contains("<WorkVariance>480000.00</WorkVariance>"), "{a}");
    assert!(entity(&xml, "Task", 0).contains("<WorkVariance>480000.00</WorkVariance>"));
}

#[test]
fn r226_task_variances_written() {
    // Ligne 0 : début jeu 15/10 13:00, fin ven 16/10 13:00, travail 6 h ;
    // T1 planifiée lun 19/10 08:00-17:00, 8 h. Écart de début : jeu 13:00 →
    // lun 08:00 = 4 h + 8 h = +720 ⇒ `<StartVariance>720000</StartVariance>` ;
    // travail +120 ⇒ `120000.00`.
    let mut p = project();
    p.tasks.get_mut(TaskUid::new(1)).unwrap().baselines.insert(
        BaselineNumber::new(0).unwrap(),
        TaskBaseline {
            start: Some(moment("2026-10-15T13:00")),
            finish: Some(moment("2026-10-16T13:00")),
            work: Some(work(360)),
            ..TaskBaseline::default()
        },
    );
    let xml = by_engine(&p);
    let t = entity(&xml, "Task", 1);
    assert!(t.contains("<StartVariance>720000</StartVariance>"), "{t}");
    // Fin : lun 17:00 − ven 13:00 = 4 h + 8 h = 720 min ouvrées.
    assert!(t.contains("<FinishVariance>720000</FinishVariance>"), "{t}");
    assert!(t.contains("<WorkVariance>120000.00</WorkVariance>"), "{t}");
    // Valeurs relues : réécrites telles quelles ; sans valeurs calculées :
    // omises.
    let s = schedule(&p);
    let first = reread(&write(&p, Computed::Schedule(&s), &WriteOptions::default()).xml);
    let rec = &first.recorded.tasks[&TaskUid::new(1)];
    assert_eq!(rec.start_variance, Some(720_000));
    let again = reread(&write(&p, Computed::Recorded(&first.recorded), &WriteOptions::default()).xml);
    assert_eq!(again.recorded, first.recorded);
    let omitted = compact(&text(&write(&p, Computed::Omit, &WriteOptions::default())));
    assert!(!omitted.contains("Variance>"));
}

#[test]
fn r226_assignment_dates_variance_omitted() {
    // Écarts de dates d'affectation : unité absente du schéma, omis avec le
    // moteur ; relus et réécrits tels quels avec les valeurs relues.
    let xml = by_engine(&project());
    let a = entity(&xml, "Assignment", 1);
    assert!(
        !a.contains("<StartVariance>") && !a.contains("<FinishVariance>"),
        "{a}"
    );
    let doc = read(
        b"<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID>\
          <Calendars><Calendar><UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar></Calendar></Calendars>\
          <Tasks><Task><UID>1</UID><ID>1</ID><Name>T</Name><Duration>PT8H0M0S</Duration></Task></Tasks>\
          <Resources><Resource><UID>1</UID><ID>1</ID><Name>R</Name><WorkVariance>0.00</WorkVariance></Resource></Resources>\
          <Assignments><Assignment><UID>7</UID><TaskUID>1</TaskUID><ResourceUID>1</ResourceUID>\
          <StartVariance>4800</StartVariance><FinishVariance>0</FinishVariance><WorkVariance>12.5</WorkVariance></Assignment></Assignments></Project>",
        &ReadOptions::new(moment("2026-01-01T00:00")),
    )
    .unwrap();
    let r = doc.recorded.assignments[&AssignmentUid::new(7)];
    assert_eq!((r.start_variance, r.finish_variance), (Some(4_800), Some(0)));
    let xml = compact(&text(&write(
        &doc.project,
        Computed::Recorded(&doc.recorded),
        &WriteOptions::default(),
    )));
    let a = entity(&xml, "Assignment", 7);
    assert!(a.contains("<StartVariance>4800</StartVariance>"), "{a}");
    assert!(a.contains("<WorkVariance>12.50</WorkVariance>"), "{a}");
    assert!(entity(&xml, "Resource", 1).contains("<WorkVariance>0.00</WorkVariance>"));
}

#[test]
fn r226_corpus_variances_equal_recorded() {
    // Les cinq fichiers (O42, O43) : écarts écrits par le moteur égaux à
    // ceux du fichier, pour chaque tâche, ressource et affectation dont le
    // moteur redonne les valeurs (A4 UID 1 : `10800000.00` ; R1 de A4 :
    // `31440000.00`).
    let mut seen = 0;
    for name in [
        "cogito-advanced-sample-msp2010.xml",
        "practicemap.xml",
        "shopmetrics.xml",
        "takeout-system-zh.xml",
        "projinteg.xml",
    ] {
        let Ok(bytes) = std::fs::read(tests_path(&format!("corpus/files/{name}"))) else {
            continue;
        };
        seen += 1;
        let doc = read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
        let s = schedule(&doc.project);
        let out = write(&doc.project, Computed::Schedule(&s), &WriteOptions::default());
        let back = reread(&out.xml).recorded;
        let mut differ = Vec::new();
        for (uid, rec) in &doc.recorded.tasks {
            let w = &back.tasks[uid];
            let key =
                |r: &plannercraft_mspdi::RecordedTask| (r.start_variance, r.finish_variance, r.work_variance);
            if key(w) != key(rec) {
                differ.push(format!("tâche {} : {:?} ≠ {:?}", uid.get(), key(w), key(rec)));
            }
        }
        for (uid, rec) in &doc.recorded.resources {
            let w = &back.resources[uid];
            if uid.get() != 0 && w.work_variance != rec.work_variance {
                differ.push(format!("ressource {}", uid.get()));
            }
        }
        for (uid, rec) in &doc.recorded.assignments {
            let w = &back.assignments[uid];
            if w.work_variance != rec.work_variance {
                differ.push(format!("affectation {}", uid.get()));
            }
        }
        assert_eq!(differ, Vec::<String>::new(), "{name}");
        if name == "shopmetrics.xml" {
            let xml = compact(&text(&out));
            assert!(entity(&xml, "Task", 1).contains("<WorkVariance>10800000.00</WorkVariance>"));
            assert!(entity(&xml, "Resource", 1).contains("<WorkVariance>31440000.00</WorkVariance>"));
        }
    }
    eprintln!("R-226 : {seen} fichiers du corpus comparés");
}
