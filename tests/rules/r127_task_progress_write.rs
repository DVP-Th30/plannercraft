//! R-127 — Travail et avancement d'une tâche à l'écriture.

use plannercraft_core::{Minutes, Percent, Task, TaskUid, Work};
use plannercraft_mspdi::write::WriteCode;
use plannercraft_tests::mspdi::{compact, new_project, reread, text, written};

#[test]
fn r127_percentages_are_whole_and_work_is_iso() {
    let mut proj = new_project("2026-10-19T08:00");
    let opts = proj.options;
    for (uid, pct) in [(1, 3_333), (2, 6_650), (3, 10_000)] {
        let mut t = Task::new(TaskUid::new(uid), format!("T{uid}"), &opts);
        t.progress.percent_complete = Percent::new(pct).unwrap();
        t.work = Work::new(Minutes::new(960)).unwrap();
        t.progress.actual_work = Work::new(Minutes::new(90)).unwrap();
        t.progress.remaining_work = Work::new(Minutes::new(870)).unwrap();
        t.progress.actual_duration = Minutes::new(240);
        t.progress.remaining_duration = Minutes::new(240);
        proj.tasks.push(t).unwrap();
    }
    let out = written(&proj);
    let xml = compact(&text(&out));
    // 3 333 → 33 (arrondi signalé) ; 6 650 → 67 (moitié loin de zéro) ;
    // 10 000 → 100.
    for v in ["33", "67", "100"] {
        assert!(
            xml.contains(&format!("<PercentComplete>{v}</PercentComplete>")),
            "{v}"
        );
    }
    let rounded: Vec<&str> = out
        .diagnostics
        .iter()
        .filter(|d| d.code == WriteCode::Rounded)
        .map(|d| d.element.as_str())
        .collect();
    assert_eq!(
        rounded,
        ["Tasks/Task/PercentComplete", "Tasks/Task/PercentComplete"]
    );
    assert_eq!(out.diagnostics[0].code.code(), "mspdi.write.rounded");
    assert!(xml.contains("<Work>PT16H0M0S</Work>"));
    assert!(xml.contains("<ActualWork>PT1H30M0S</ActualWork>"));
    assert!(xml.contains("<RemainingWork>PT14H30M0S</RemainingWork>"));
    assert!(xml.contains("<ActualDuration>PT4H0M0S</ActualDuration>"));
    let back = reread(&out.xml).project;
    let t = back.tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(t.progress.percent_complete, Percent::new(3_300).unwrap());
    assert_eq!(t.progress.actual_work.minutes(), Minutes::new(90));
    assert_eq!(t.work.minutes(), Minutes::new(960));
}

#[test]
fn r127_remaining_of_a_task_without_progress_is_duration_minus_actual() {
    // Tâche de 5 j non commencée importée d'un CSV (avancement par défaut) :
    // MS Project écrit toujours reste = durée − réel ; PlannerCraft aussi, au lieu
    // de 0. Le travail restant suit le travail. Relu : valeurs gardées,
    // image décrite par `normalize` (R-133).
    use plannercraft_automation::csv::{CsvImportOptions, import};
    use plannercraft_core::ScheduleAnchor;
    use plannercraft_mspdi::write::WriteOptions;
    use plannercraft_tests::moment;
    let options = CsvImportOptions::new(
        ScheduleAnchor::FromStart(moment("2026-10-19T08:00")),
        moment("2026-10-17T08:00"),
    );
    let csv = "Nom;Durée\nA;5j\n";
    let mut proj = import(csv.as_bytes(), &options).unwrap().project;
    let opts = proj.options;
    let mut w = Task::new(TaskUid::new(2), "B".to_owned(), &opts);
    w.work = Work::new(Minutes::new(960)).unwrap();
    proj.tasks.push(w).unwrap();
    let out = written(&proj);
    let xml = compact(&text(&out));
    assert!(xml.contains("<Name>A</Name>"));
    assert!(
        xml.contains("<RemainingDuration>PT40H0M0S</RemainingDuration>"),
        "{xml}"
    );
    assert!(xml.contains("<RemainingWork>PT16H0M0S</RemainingWork>"));
    assert!(xml.contains("<ActualDuration>PT0H0M0S</ActualDuration>"));
    let back = reread(&out.xml).project;
    let a = back.tasks.get(TaskUid::new(1)).unwrap();
    assert_eq!(a.progress.remaining_duration, Minutes::new(2_400));
    assert_eq!(
        back,
        plannercraft_mspdi::normalize(&proj, &WriteOptions::default())
    );
    // Avancement saisi : valeurs écrites telles quelles (premier test).
}
