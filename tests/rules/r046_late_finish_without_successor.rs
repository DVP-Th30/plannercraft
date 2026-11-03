//! R-046 — Fin au plus tard sans successeur : plafond commun = fin du
//! projet ; une tâche sans successeur effectif a LF = fin du projet.
//! Sources : S22, Q9 ; observé O2 (A2 UID 2).

use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r046_final_task_late_finish_is_project_finish() {
    // N1 : D 1 j seul (lun) ; fin du projet mer 21/10 17:00 → D LF mer 17:00,
    // LS mer 08:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .task("D", 480)
        .fs("A", "C")
        .fs("B", "C")
        .run();
    assert_eq!(run.at("D").late.finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("D").late.start, moment("2026-10-21T08:00"));
    // C, tâche finale du chemin le plus long : LF = EF.
    assert_eq!(run.at("C").late, run.at("C").early);
}
