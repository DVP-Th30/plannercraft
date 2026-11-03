//! R-145 — CSV : une ligne = une tâche ; UID depuis « N° unique » s'il est
//! valide et unique ; niveaux et récapitulatives ; durée par R-140 ;
//! jalon ; contrainte par défaut selon le sens ; prédécesseurs par ID ou
//! par UID ; dates ignorées (écarts signalés) ou converties en contraintes ;
//! garde de formule. Projet depuis lun 19/10/2026 08:00, calendrier
//! Standard, 480 / 2 400 / 20.

use plannercraft_automation::csv::{CsvCode, CsvDates, CsvImport, CsvImportOptions, import};
use plannercraft_core::{
    Constraint, DurationUnit, LinkType, Minutes, ScheduleAnchor, Task, TaskUid, TimeBasis, schedule,
};
use plannercraft_tests::moment;

fn options() -> CsvImportOptions {
    CsvImportOptions::new(
        ScheduleAnchor::FromStart(moment("2026-10-19T08:00")),
        moment("2026-10-17T08:00"),
    )
}

fn read(csv: &str) -> CsvImport {
    import(csv.as_bytes(), &options()).unwrap()
}

fn task<'a>(i: &'a CsvImport, name: &str) -> &'a Task {
    i.project.tasks.iter().find(|t| t.name == name).unwrap()
}

fn codes(i: &CsvImport) -> Vec<(u64, &'static str, Option<String>)> {
    i.diagnostics
        .iter()
        .map(|d| (d.line, d.code.code(), d.value.clone()))
        .collect()
}

const HEADER: &str = "N°;Nom;Durée;Début;Fin;Prédécesseurs;Niveau hiérarchique;Jalon\n";

#[test]
fn r145_rows_to_tasks_and_schedule() {
    let csv = format!(
        "{HEADER}1;Phase;10j;;;;1;Non\n2;A;3j;;;;2;Non\n3;B;2j;;;2;2;Non\n4;Fin;;;;3;1;Oui\n5;X;2j;;;9;1;Non\n"
    );
    let i = read(&csv);
    // UID 1..n dans l'ordre (pas de colonne N° unique).
    let uids: Vec<u32> = i.project.tasks.iter().map(|t| t.uid().get()).collect();
    assert_eq!(uids, vec![1, 2, 3, 4, 5]);
    // Phase, récapitulative : durée lue ignorée, son unité gardée sur une
    // durée nulle (comme R-071) ; « 2h? » garderait heure et estimation.
    let phase = task(&i, "Phase").duration;
    assert!(phase.is_zero());
    assert_eq!(
        (phase.unit(), phase.basis(), phase.is_estimated()),
        (DurationUnit::Day, TimeBasis::Working, false)
    );
    // A : 3 j = 1 440 ; B : FS de A (ID 2) ; Fin : jalon sans durée → 0 ;
    // contrainte par défaut depuis le début : le plus tôt possible.
    assert_eq!(task(&i, "A").duration.minutes(), Minutes::new(1_440));
    assert_eq!(task(&i, "A").constraint, Constraint::AsSoonAsPossible);
    let b = task(&i, "B");
    assert_eq!(b.predecessors.len(), 1);
    assert_eq!(b.predecessors[0].predecessor, TaskUid::new(2));
    assert_eq!(b.predecessors[0].kind, LinkType::FinishToStart);
    let fin = task(&i, "Fin");
    assert!(fin.milestone);
    assert!(fin.duration.is_zero());
    assert_eq!(fin.predecessors[0].predecessor, TaskUid::new(3));
    // Diagnostics : durée de la récapitulative ignorée (ligne 2),
    // prédécesseur 9 inconnu (ligne 6).
    assert_eq!(
        codes(&i),
        vec![
            (2, "csv.summary_duration_ignored", Some("10j".to_owned())),
            (6, "csv.unknown_predecessor", Some("9".to_owned())),
        ]
    );
    // Calcul : A lun 08:00 – mer 21/10 17:00 ; B jeu 08:00 – ven 23/10
    // 17:00 ; jalon ven 17:00 ; Phase lun 08:00 – ven 17:00 (5 j = 2 400).
    let s = schedule(&i.project);
    let at = |uid| s.scheduled(TaskUid::new(uid)).unwrap();
    assert_eq!(
        (at(2).start, at(2).finish),
        (moment("2026-10-19T08:00"), moment("2026-10-21T17:00"))
    );
    assert_eq!(
        (at(3).start, at(3).finish),
        (moment("2026-10-22T08:00"), moment("2026-10-23T17:00"))
    );
    assert_eq!(at(4).start, moment("2026-10-23T17:00"));
    assert_eq!(
        (at(1).start, at(1).finish),
        (moment("2026-10-19T08:00"), moment("2026-10-23T17:00"))
    );
    assert_eq!(at(1).duration, Minutes::new(2_400));
}

#[test]
fn r145_unique_ids_null_rows_and_levels() {
    // N° unique 10, 10 (en double), « x », vide : 10 gardé, les autres
    // reçoivent 11, 12, 13 ; « 3 » seul : ligne vide (IsNull).
    let csv = "N°;N° unique;Nom;Niveau hiérarchique\n1;10;A;1\n2;10;B;3\n3\n4;x;C;0\n5;;D;\n";
    let i = read(csv);
    let rows: Vec<(u32, bool, u16)> = i
        .project
        .tasks
        .iter()
        .map(|t| (t.uid().get(), t.is_null, t.outline_level.get()))
        .collect();
    // B : niveau 3 après un niveau 1 → 2 (saut) ; C : niveau 0 → 1.
    assert_eq!(
        rows,
        vec![
            (10, false, 1),
            (11, false, 2),
            (12, true, 1),
            (13, false, 1),
            (14, false, 1)
        ]
    );
    assert_eq!(
        codes(&i),
        vec![
            (3, "csv.bad_value", Some("10".to_owned())),
            (3, "csv.outline_jump", Some("3".to_owned())),
            (5, "csv.bad_value", Some("x".to_owned())),
            (5, "csv.bad_value", Some("0".to_owned())),
        ]
    );
}

#[test]
fn r145_durations_milestones_percent() {
    let csv = "Nom;Durée;Jalon;% achevé\nA;;Non;\nB;2j?;yes;50%\nC;5x;peut-être;120\nD;1,5;VRAI;33,335\n";
    let imp = read(csv);
    // Durée vide : « 1j? » (Task::new, R-032).
    let a = task(&imp, "A");
    assert_eq!(
        (a.duration.minutes(), a.duration.is_estimated()),
        (Minutes::new(480), true)
    );
    let b = task(&imp, "B");
    assert!(b.milestone && b.duration.is_estimated());
    assert_eq!(b.progress.percent_complete.get(), 5_000);
    // Valeurs illisibles : valeur par défaut, diagnostic.
    let c = task(&imp, "C");
    assert_eq!((c.duration.minutes(), c.milestone), (Minutes::new(480), false));
    let d = task(&imp, "D");
    assert_eq!(
        (d.duration.minutes(), d.duration.unit()),
        (Minutes::new(720), DurationUnit::Day)
    );
    assert_eq!(d.duration.basis(), TimeBasis::Working);
    assert_eq!(d.progress.percent_complete.get(), 3_334);
    assert_eq!(
        codes(&imp),
        vec![
            (4, "csv.bad_value", Some("5x".to_owned())),
            (4, "csv.bad_value", Some("peut-être".to_owned())),
            (4, "csv.bad_value", Some("120".to_owned())),
        ]
    );
}

#[test]
fn r145_predecessor_diagnostics_and_uids() {
    // Par ID : sur soi, en double, externe, illisible.
    let csv = "N°;Nom;Prédécesseurs\n1;A;\n2;B;\"2;1;1FF;C:\\\\p.mpp\\\\1;1XY\"\n3;C;\"1DD+1j;2\"\n";
    let i = read(csv);
    let b = task(&i, "B");
    assert_eq!(b.predecessors.len(), 1);
    assert_eq!(
        (b.predecessors[0].predecessor, b.predecessors[0].kind),
        (TaskUid::new(1), LinkType::FinishToStart)
    );
    let c = task(&i, "C");
    assert_eq!(c.predecessors.len(), 2);
    assert_eq!(c.predecessors[0].kind, LinkType::StartToStart);
    // Un seul diagnostic par cellule (règle 10) : premier élément fautif
    // dans l'ordre de la cellule (« 2 », sur soi), puis le nombre des autres
    // (« 1FF » en double, le chemin externe, « 1XY » illisible).
    let got: Vec<(u64, &str, Option<&str>, u32)> = i
        .diagnostics
        .iter()
        .map(|d| (d.line, d.code.code(), d.value.as_deref(), d.others))
        .collect();
    assert_eq!(got, vec![(3, "csv.self_predecessor", Some("2"), 3)]);
    // Par UID (colonne « N° unique des prédécesseurs », anglais ici).
    let csv = "Unique ID,Name,Unique ID Predecessors\n7,A,\n9,B,7SS\n";
    let i = read(csv);
    let b = task(&i, "B");
    assert_eq!(
        (b.predecessors[0].predecessor, b.predecessors[0].kind),
        (TaskUid::new(7), LinkType::StartToStart)
    );
}

#[test]
fn r145_dates_ignored_or_constrained() {
    let csv = "Nom;Durée;Début;Fin\nA;1j;19/10/2026 08:00;19/10/2026 17:00\nB;1j;21/10/2026;\nC;1j;;23/10/2026\nD;1j;31/02/2026;\n";
    // Ignorées : écart signalé (information) pour B (mer 08:00 contre lun
    // 08:00) et C (ven 17:00 contre lun 17:00) ; A coïncide ; date illisible.
    let i = read(csv);
    assert_eq!(task(&i, "B").constraint, Constraint::AsSoonAsPossible);
    let mismatches: Vec<(u64, CsvCode)> = i
        .diagnostics
        .iter()
        .filter(|d| d.code.code() == "csv.date_mismatch")
        .map(|d| (d.line, d.code.clone()))
        .collect();
    assert_eq!(
        mismatches,
        vec![
            (
                3,
                CsvCode::DateMismatch {
                    csv: moment("2026-10-21T08:00"),
                    computed: Some(moment("2026-10-19T08:00"))
                }
            ),
            (
                4,
                CsvCode::DateMismatch {
                    csv: moment("2026-10-23T17:00"),
                    computed: Some(moment("2026-10-19T17:00"))
                }
            ),
        ]
    );
    assert!(
        i.diagnostics
            .iter()
            .any(|d| d.line == 5 && d.code == CsvCode::BadValue)
    );
    // Converties (S30) : Début → SNET, Fin → FNET, Début et Fin → SNET.
    let constrain = CsvImportOptions {
        dates: CsvDates::Constrain,
        ..options()
    };
    let i = import(csv.as_bytes(), &constrain).unwrap();
    assert_eq!(
        task(&i, "A").constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-10-19T08:00")
        }
    );
    assert_eq!(
        task(&i, "B").constraint,
        Constraint::StartNoEarlierThan {
            date: moment("2026-10-21T08:00")
        }
    );
    assert_eq!(
        task(&i, "C").constraint,
        Constraint::FinishNoEarlierThan {
            date: moment("2026-10-23T17:00")
        }
    );
    assert_eq!(task(&i, "D").constraint, Constraint::AsSoonAsPossible);
    assert!(i.diagnostics.iter().all(|d| d.code.code() != "csv.date_mismatch"));
    // B placée mer 21/10 08:00 par sa contrainte.
    let s = schedule(&i.project);
    assert_eq!(
        s.scheduled(task(&i, "B").uid()).unwrap().start,
        moment("2026-10-21T08:00")
    );
}

#[test]
fn r145_default_constraint_follows_the_anchor() {
    // À rebours, la contrainte par défaut est « le plus tard possible »
    // (R-108, `Constraint::default_for`).
    let back = CsvImportOptions {
        anchor: ScheduleAnchor::FromFinish(moment("2026-10-23T17:00")),
        ..options()
    };
    let i = import("Nom;Durée\nA;1j\n".as_bytes(), &back).unwrap();
    assert_eq!(task(&i, "A").constraint, Constraint::AsLateAsPossible);
}

#[test]
fn r145_formula_guard_and_control_chars() {
    // « '=SOMME(A1) » perd son apostrophe ; « 'abc » la garde ; un caractère
    // de contrôle devient U+FFFD (csv.invalid_char).
    let i = read("Nom;Notes\n'=SOMME(A1);'abc\nA\u{1}B;''x\n");
    let names: Vec<_> = i
        .project
        .tasks
        .iter()
        .map(|t| (t.name.clone(), t.notes.clone()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("=SOMME(A1)".to_owned(), "'abc".to_owned()),
            ("A\u{fffd}B".to_owned(), "'x".to_owned()),
        ]
    );
    assert_eq!(
        codes(&i),
        vec![(3, "csv.invalid_char", Some("A\u{fffd}B".to_owned()))]
    );
}

#[test]
fn r145_unique_predecessors_without_unique_id_column_are_ignored() {
    // Sans colonne « Unique ID », les UID sont réattribués 1..n : lire
    // « Unique ID Predecessors » relierait de mauvaises tâches. Colonne
    // ignorée, un seul diagnostic typé.
    let i = read("Name,Duration,Unique ID Predecessors\nA,1d,\nB,1d,1\nC,1d,2\n");
    assert!(i.project.tasks.iter().all(|t| t.predecessors.is_empty()));
    assert_eq!(codes(&i), vec![(3, "csv.unique_predecessors_without_uid", None)]);
    // La colonne Prédécesseurs (par ID) reste lue.
    let i = read("Name,Predecessors,Unique ID Predecessors\nA,,\nB,1,7\n");
    assert_eq!(task(&i, "B").predecessors.len(), 1);
    assert_eq!(codes(&i), vec![]);
}

#[test]
fn r145_one_diagnostic_per_predecessor_cell() {
    // 200 prédécesseurs inconnus dans une cellule : un diagnostic, le premier
    // élément et le nombre des 199 autres.
    let cell: Vec<String> = (1_000..1_200).map(|n| n.to_string()).collect();
    let csv = format!("Nom;Prédécesseurs\nA;\"{}\"\n", cell.join(";"));
    let i = read(&csv);
    let got: Vec<(&str, Option<&str>, u32)> = i
        .diagnostics
        .iter()
        .map(|d| (d.code.code(), d.value.as_deref(), d.others))
        .collect();
    assert_eq!(got, vec![("csv.unknown_predecessor", Some("1000"), 199)]);
}
