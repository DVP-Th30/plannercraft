//! R-120 — Écriture : forme du document et des valeurs.

use plannercraft_core::{
    Duration, DurationUnit, Lag, LagValue, Link, LinkType, Minutes, SignedPercent, Task, TaskUid, TimeBasis,
};
use plannercraft_tests::mspdi::{compact, new_project, reread, text, written};

#[test]
fn r120_document_form() {
    let proj = new_project("2026-10-19T08:00");
    let out = written(&proj);
    assert!(!out.xml.starts_with(&[0xEF, 0xBB, 0xBF]), "pas de BOM");
    let xml = text(&out);
    assert!(xml.starts_with(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Project xmlns=\"http://schemas.microsoft.com/project\">\n"
    ));
    assert!(xml.ends_with("</Project>\n"));
    // Date sans fuseau, secondes à 00.
    assert!(xml.contains("<StartDate>2026-10-19T08:00:00</StartDate>"));
    assert!(xml.contains("<DefaultStartTime>08:00:00</DefaultStartTime>"));
    // Champ modélisé égal au défaut du schéma, écrit quand même : un
    // projet neuf n'est pas piloté par l'effort (R-032), le schéma dit vrai.
    assert!(xml.contains("<NewTasksEffortDriven>0</NewTasksEffortDriven>"));
    assert!(!reread(&out.xml).project.options.new_tasks_effort_driven);
    // Déterministe : deux appels, mêmes octets.
    assert_eq!(written(&proj).xml, out.xml);
}

#[test]
fn r120_durations_and_lags() {
    let mut proj = new_project("2026-10-19T08:00");
    let opts = proj.options;
    let mut a = Task::new(TaskUid::new(1), "A".to_owned(), &opts);
    // 2 400 min, jour, ouvrée, estimée.
    a.duration = Duration::new(Minutes::new(2_400), DurationUnit::Day, TimeBasis::Working, true).unwrap();
    proj.tasks.push(a).unwrap();
    let lag = |basis, value| Lag::new(basis, value).unwrap();
    let mut b = Task::new(TaskUid::new(2), "B".to_owned(), &opts);
    b.predecessors = vec![Link {
        predecessor: TaskUid::new(1),
        kind: LinkType::FinishToStart,
        lag: lag(
            TimeBasis::Working,
            LagValue::Time {
                minutes: Minutes::new(960),
                unit: DurationUnit::Day,
            },
        ),
    }];
    proj.tasks.push(b).unwrap();
    let mut c = Task::new(TaskUid::new(3), "C".to_owned(), &opts);
    c.predecessors = vec![
        Link {
            predecessor: TaskUid::new(1),
            kind: LinkType::StartToStart,
            lag: lag(
                TimeBasis::Working,
                LagValue::Percent {
                    percent: SignedPercent::new(-5_000).unwrap(),
                },
            ),
        },
        Link {
            predecessor: TaskUid::new(2),
            kind: LinkType::FinishToStart,
            lag: lag(
                TimeBasis::Elapsed,
                LagValue::Time {
                    minutes: Minutes::new(4_320),
                    unit: DurationUnit::Day,
                },
            ),
        },
    ];
    proj.tasks.push(c).unwrap();
    let out = written(&proj);
    let xml = compact(&text(&out));
    assert!(xml.contains("<Duration>PT40H0M0S</Duration><DurationFormat>39</DurationFormat>"));
    assert!(xml.contains("<Estimated>1</Estimated>"));
    // FS + 2 j : dixièmes de minute, format jour.
    assert!(xml.contains("<PredecessorUID>1</PredecessorUID><Type>1</Type><CrossProject>0</CrossProject><LinkLag>9600</LinkLag><LagFormat>7</LagFormat>"));
    // SS −50 % : pourcentage × 10, format %.
    assert!(xml.contains(
        "<Type>3</Type><CrossProject>0</CrossProject><LinkLag>-500</LinkLag><LagFormat>19</LagFormat>"
    ));
    // 3 ej : 43 200 dixièmes, format jour écoulé.
    assert!(xml.contains("<LinkLag>43200</LinkLag><LagFormat>8</LagFormat>"));
    // Relu identique.
    let back = reread(&out.xml).project;
    assert_eq!(
        back.tasks.get(TaskUid::new(3)).unwrap().predecessors,
        proj.tasks.get(TaskUid::new(3)).unwrap().predecessors
    );
    assert_eq!(
        back.tasks.get(TaskUid::new(1)).unwrap().duration,
        proj.tasks.get(TaskUid::new(1)).unwrap().duration
    );
    assert_eq!(out.diagnostics, Vec::new());
}

#[test]
fn r120_indentation_and_newlines() {
    use plannercraft_mspdi::write::{Computed, Indent, Newline, WriteOptions, write};
    let proj = new_project("2026-10-19T08:00");
    let crlf = write(
        &proj,
        Computed::Omit,
        &WriteOptions {
            newline: Newline::Crlf,
            indent: Indent::Spaces(2),
            ..WriteOptions::default()
        },
    );
    let xml = String::from_utf8(crlf.xml.clone()).unwrap();
    assert!(xml.contains("\r\n  <SaveVersion>14</SaveVersion>\r\n"));
    let none = write(
        &proj,
        Computed::Omit,
        &WriteOptions {
            indent: Indent::None,
            ..WriteOptions::default()
        },
    );
    let flat = String::from_utf8(none.xml.clone()).unwrap();
    assert_eq!(flat.lines().count(), 2);
    assert_eq!(reread(&crlf.xml).project, reread(&none.xml).project);
}
