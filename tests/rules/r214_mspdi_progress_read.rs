//! R-214 — MSPDI lecture de l'avancement (J6) : reprise d'une feuille en
//! cours dans le modèle, options de la date d'état dans `ProgressOptions`,
//! restes relus remplacés par la dérivation (`mspdi.progress_inconsistent`).

use plannercraft_core::{AssignmentUid, Minutes, Percent, ProgressOptions, TaskUid, Work};
use plannercraft_mspdi::write::{Computed, WriteOptions, write};
use plannercraft_mspdi::{ImportCode, MspdiDocument, ReadOptions, read};
use plannercraft_tests::mspdi::{compact, text};
use plannercraft_tests::{moment, tests_path};

/// Document : options `options` du projet, tâches `tasks` (éléments
/// `Task` complets), une ressource UID 1 et les affectations `assignments`.
fn doc(options: &str, tasks: &str, assignments: &str) -> MspdiDocument {
    let xml = format!(
        "<Project><StartDate>2026-10-19T08:00:00</StartDate><CalendarUID>1</CalendarUID>{options}\
         <Calendars><Calendar><UID>1</UID><Name>Standard</Name><IsBaseCalendar>1</IsBaseCalendar></Calendar></Calendars>\
         <Tasks>{tasks}</Tasks>\
         <Resources><Resource><UID>1</UID><ID>1</ID><Name>R</Name></Resource></Resources>\
         <Assignments>{assignments}</Assignments></Project>"
    );
    read(xml.as_bytes(), &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap()
}

/// Tâche d'UID `uid` au niveau `level`, 2 j (960 min), avec les éléments
/// `body`.
fn task_at(uid: u32, level: u16, body: &str) -> String {
    format!(
        "<Task><UID>{uid}</UID><ID>{uid}</ID><Name>T{uid}</Name><OutlineLevel>{level}</OutlineLevel>\
         <Duration>PT16H0M0S</Duration>{body}</Task>"
    )
}

/// Feuille de premier niveau.
fn leaf(uid: u32, body: &str) -> String {
    task_at(uid, 1, body)
}

fn inconsistent(d: &MspdiDocument) -> Vec<u32> {
    d.diagnostics
        .iter()
        .filter_map(|x| match x.code {
            ImportCode::ProgressInconsistent { count } => Some(count),
            _ => None,
        })
        .collect()
}

#[test]
fn r214_resume_read_in_progress_only() {
    // UID 1 en cours (début réel, 25 %) : `Resume` lu dans le modèle.
    // UID 2 achevée, UID 3 non commencée, UID 4 récapitulative de UID 5 :
    // `Resume` relu seulement dans `RecordedSchedule` (valeur calculée).
    let resume = "<Resume>2026-10-20T13:00:00</Resume><Stop>2026-10-19T12:00:00</Stop>";
    let tasks = [
        leaf(
            1,
            &format!(
                "<PercentComplete>25</PercentComplete><ActualStart>2026-10-19T08:00:00</ActualStart>{resume}"
            ),
        ),
        leaf(
            2,
            &format!(
                "<ActualStart>2026-10-19T08:00:00</ActualStart><ActualFinish>2026-10-20T17:00:00</ActualFinish>{resume}"
            ),
        ),
        leaf(3, resume),
        task_at(4, 1, &format!("<PercentComplete>50</PercentComplete>{resume}")),
        task_at(5, 2, ""),
    ]
    .concat();
    let d = doc("", &tasks, "");
    let r = |uid: u32| d.project.tasks.get(TaskUid::new(uid)).unwrap().progress.resume;
    assert_eq!(r(1), Some(moment("2026-10-20T13:00")));
    assert_eq!((r(2), r(3), r(4), r(5)), (None, None, None, None));
    for uid in [1, 2, 3, 4] {
        let rec = &d.recorded.tasks[&TaskUid::new(uid)];
        assert_eq!(rec.resume, Some(moment("2026-10-20T13:00")), "{uid}");
        assert_eq!(rec.stop, Some(moment("2026-10-19T12:00")), "{uid}");
    }
    // Plus en `Extras` (classe P → M? et C).
    for t in &d.project.tasks {
        assert!(t.extras.0.iter().all(|e| e.key != "Resume" && e.key != "Stop"));
    }
    // Sans valeurs calculées, seule la reprise de UID 1 est réécrite.
    let xml = compact(&text(&write(
        &d.project,
        Computed::Omit,
        &WriteOptions::default(),
    )));
    assert_eq!(xml.matches("<Resume>2026-10-20T13:00:00</Resume>").count(), 1);
    assert!(!xml.contains("<Stop>"));
    // Valeurs relues : réécrites telles quelles.
    let xml = compact(&text(&write(
        &d.project,
        Computed::Recorded(&d.recorded),
        &WriteOptions::default(),
    )));
    assert_eq!(xml.matches("<Resume>2026-10-20T13:00:00</Resume>").count(), 4);
    assert_eq!(xml.matches("<Stop>2026-10-19T12:00:00</Stop>").count(), 4);
}

#[test]
fn r214_options_in_model() {
    // O36 : options lues dans `ProgressOptions` (P → M), réécrites.
    let d = doc(
        "<SplitsInProgressTasks>0</SplitsInProgressTasks><TaskUpdatesResource>1</TaskUpdatesResource>\
         <MoveCompletedEndsBack>0</MoveCompletedEndsBack><MoveRemainingStartsBack>0</MoveRemainingStartsBack>\
         <MoveRemainingStartsForward>0</MoveRemainingStartsForward><MoveCompletedEndsForward>0</MoveCompletedEndsForward>",
        &leaf(1, ""),
        "",
    );
    let o = d.project.options.progress;
    assert!(!o.split_in_progress);
    assert_eq!(
        ProgressOptions {
            split_in_progress: true,
            ..o
        },
        ProgressOptions::default()
    );
    assert!(d.project.extras.0.iter().all(|e| !e.key.contains("Splits")));
    // Absentes : défauts d'un projet neuf.
    assert_eq!(
        doc("", &leaf(1, ""), "").project.options.progress,
        ProgressOptions::default()
    );
    let xml = compact(&text(&write(
        &d.project,
        Computed::Omit,
        &WriteOptions::default(),
    )));
    assert!(xml.contains("<SplitsInProgressTasks>0</SplitsInProgressTasks>"));
    assert!(xml.contains("<TaskUpdatesResource>1</TaskUpdatesResource>"));
    assert!(xml.contains("<MoveCompletedEndsForward>0</MoveCompletedEndsForward>"));
}

#[test]
fn r214_inconsistent_remaining_replaced() {
    // UID 1 : 100 % sans fin réelle, `RemainingDuration` = `Duration` ⇒
    // reste 0 (la durée réelle effective vaut D). UID 2 : 50 %, restes
    // cohérents (8 h, travail 16 − 4 = 12 h). Affectation 7 : W 16 h, réel
    // 4 h, reste relu 10 h ⇒ 12 h. Une info, `count` 2.
    let tasks = [
        leaf(
            1,
            "<PercentComplete>100</PercentComplete><RemainingDuration>PT16H0M0S</RemainingDuration>",
        ),
        leaf(
            2,
            "<PercentComplete>50</PercentComplete><RemainingDuration>PT8H0M0S</RemainingDuration>\
             <Work>PT16H0M0S</Work><ActualWork>PT4H0M0S</ActualWork><RemainingWork>PT12H0M0S</RemainingWork>",
        ),
    ]
    .concat();
    let d = doc(
        "",
        &tasks,
        "<Assignment><UID>7</UID><TaskUID>2</TaskUID><ResourceUID>1</ResourceUID><Work>PT16H0M0S</Work>\
         <ActualWork>PT4H0M0S</ActualWork><RemainingWork>PT10H0M0S</RemainingWork></Assignment>",
    );
    let t = |uid: u32| d.project.tasks.get(TaskUid::new(uid)).unwrap().progress;
    assert_eq!(t(1).remaining_duration, Minutes::ZERO);
    // Le % relu reste stocké ; la durée réelle effective l'emporte (R-200).
    assert_eq!(t(1).percent_complete, Percent::FULL);
    assert_eq!(t(2).remaining_duration, Minutes::new(480));
    assert_eq!(t(2).remaining_work, Work::new(Minutes::new(720)).unwrap());
    let a = d.project.assignments.get(AssignmentUid::new(7)).unwrap();
    assert_eq!(a.remaining_work, Work::new(Minutes::new(720)).unwrap());
    assert_eq!(inconsistent(&d), [2]);
    let x = d
        .diagnostics
        .iter()
        .find(|x| matches!(x.code, ImportCode::ProgressInconsistent { .. }))
        .unwrap();
    assert_eq!(x.code.code(), "mspdi.progress_inconsistent");
    assert_eq!(x.element, "Tasks/Task/RemainingDuration");
    assert_eq!(x.severity, plannercraft_core::Severity::Info);
    // Cohérent : aucun signalement.
    let consistent = doc(
        "",
        &leaf(1, "<RemainingDuration>PT16H0M0S</RemainingDuration>"),
        "",
    );
    assert_eq!(inconsistent(&consistent), Vec::<u32>::new());
}

#[test]
fn r214_a7_no_inconsistency() {
    // A7 (O37 à O40) : restes enregistrés = dérivation, aucun signalement ;
    // options lues (`SplitsInProgressTasks` 0) ; aucune tâche en cours,
    // donc aucune reprise dans le modèle.
    let Ok(bytes) = std::fs::read(tests_path("corpus/files/projinteg.xml")) else {
        return;
    };
    let d = read(&bytes, &ReadOptions::new(moment("2026-01-01T00:00"))).unwrap();
    assert_eq!(inconsistent(&d), Vec::<u32>::new());
    assert!(!d.project.options.progress.split_in_progress);
    assert!(d.project.tasks.iter().all(|t| t.progress.resume.is_none()));
    assert_eq!(
        d.recorded.tasks[&TaskUid::new(2)].stop,
        Some(moment("2024-10-05T13:00"))
    );
}
