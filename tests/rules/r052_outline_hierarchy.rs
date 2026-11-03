//! R-052 — Hiérarchie : parent = dernière tâche non vide précédente de niveau
//! inférieur ; récapitulative = suivie d'une tâche non vide de niveau
//! supérieur ; lignes vides ignorées ; drapeau MSPDI `Summary` non lu.
//! Source : S4 ; hypothèse.

use plannercraft_core::{Driver, TaskOutcome, TaskRole};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r052_parents_and_summaries() {
    // ID 1 (niv. 1), 2 (niv. 2), 3 (vide), 4 (niv. 2), 5 (niv. 3), 6 (niv. 1).
    // Un lien FS de 6 vers 1 borne toutes les sous-tâches de 1 (porte) :
    // 2 et 4 sont enfants de 1, 5 est enfant de 4.
    let run = Net::new("2026-10-19T08:00")
        .task("T1", 480)
        .task("T2", 480)
        .level("T2", 2)
        .null_row("T3")
        .task("T4", 480)
        .level("T4", 2)
        .task("T5", 480)
        .level("T5", 3)
        .task("T6", 480)
        .fs("T6", "T1")
        .run();
    assert_eq!(run.at("T1").role, TaskRole::Summary);
    assert_eq!(run.at("T4").role, TaskRole::Summary);
    for leaf in ["T2", "T5", "T6"] {
        assert_eq!(run.at(leaf).role, TaskRole::Leaf, "{leaf}");
    }
    assert!(matches!(run.outcome("T3"), TaskOutcome::Excluded { .. }));
    // T6 lun ; les sous-tâches de T1 commencent mar 20/10 08:00.
    assert_eq!(run.at("T2").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("T5").early.start, moment("2026-10-20T08:00"));
    // Pilote de T2 : la porte de T1 ; de T5 : la porte de son parent T4.
    assert_eq!(
        run.at("T2").driver,
        Driver::SummaryGate {
            summary: run.uid("T1")
        }
    );
    assert_eq!(
        run.at("T5").driver,
        Driver::SummaryGate {
            summary: run.uid("T4")
        }
    );
    // T6 est de premier niveau : sans lien entrant, lun 08:00.
    assert_eq!(run.at("T6").early.start, moment("2026-10-19T08:00"));
}

#[test]
fn r052_level_jump_tolerated() {
    // Niveau 1 puis niveau 3 : le parent de la seconde est la première.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 480)
        .task("A", 960)
        .level("A", 3)
        .run();
    assert_eq!(run.at("S").role, TaskRole::Summary);
    assert_eq!(run.at("S").early, run.at("A").early);
}
