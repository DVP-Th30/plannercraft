//! R-106 — Planification à rebours : passe arrière principale sous le
//! plafond commun `F` (ancre brute) ; toute tâche sans successeur effectif
//! finit à `F` recalé (`backward_span`) ; dates planifiées = dates au plus
//! tard ; tâche manuelle : dates saisies, prises comme dates au plus tard
//! (cause `Manual`), `link.manual_not_honored` pour chaque lien sortant
//! violé. Sources : S24, S33, S22 ; ADR-0009, ADR-0010.

use plannercraft_core::{Constraint, ConstraintKind, LateDriver, LinkType, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

/// F ven 23/10 17:00 ; A 1 j FS → B 2 j ; C 1 j ; toutes ALAP (défaut à
/// rebours, R-108).
fn r106_network(finish: &str) -> Net {
    Net::new("2026-10-19T08:00")
        .from_finish(finish)
        .task("A", 480)
        .task("B", 960)
        .task("C", 480)
        .fs("A", "B")
}

#[test]
fn r106_tasks_finish_at_the_anchor() {
    // B jeu 08:00 – ven 17:00 ; A mer 08:00 – 17:00 ; C ven 08:00 – 17:00.
    let run = r106_network("2026-10-23T17:00").run();
    for (name, start, finish) in [
        ("A", "2026-10-21T08:00", "2026-10-21T17:00"),
        ("B", "2026-10-22T08:00", "2026-10-23T17:00"),
        ("C", "2026-10-23T08:00", "2026-10-23T17:00"),
    ] {
        let t = run.at(name);
        assert_eq!(t.late.start, moment(start), "{name}");
        assert_eq!(t.late.finish, moment(finish), "{name}");
        // Dates planifiées = dates au plus tard.
        assert_eq!((t.start, t.finish), (t.late.start, t.late.finish), "{name}");
    }
    assert_eq!(run.at("B").late_driver, LateDriver::ProjectFinish);
    assert_eq!(run.at("C").late_driver, LateDriver::ProjectFinish);
    assert_eq!(
        run.at("A").late_driver,
        LateDriver::Successor {
            successor: run.uid("B"),
            link: LinkType::FinishToStart
        }
    );
    assert_eq!(run.schedule.project_finish, moment("2026-10-23T17:00"));
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r106_anchor_in_non_working_time() {
    // F sam 24/10 10:00 : `backward_span` recale la fin au ven 17:00 ;
    // mêmes dates ; la fin du projet reste l'ancre brute.
    let run = r106_network("2026-10-24T10:00").run();
    assert_eq!(run.at("B").late.finish, moment("2026-10-23T17:00"));
    assert_eq!(run.at("A").start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("C").start, moment("2026-10-23T08:00"));
    assert_eq!(run.schedule.project_finish, moment("2026-10-24T10:00"));
}

#[test]
fn r106_alap_is_the_default_from_finish() {
    // Les tâches ajoutées à rebours sont ALAP (R-108) : aucune épingle.
    let run = r106_network("2026-10-23T17:00").run();
    let project = r106_network("2026-10-23T17:00").build();
    for t in &project.tasks {
        assert_eq!(t.constraint, Constraint::AsLateAsPossible);
    }
    assert_eq!(run.at("C").total_slack, Minutes::new(960));
}

#[test]
fn r106_manual_task_keeps_its_dates_and_caps_its_predecessors() {
    // M manuelle mer 21/10 08:00 – jeu 22/10 17:00 ; X 1 j FS → M ; Y 1 j
    // FS de M ; F ven 23/10 17:00. Y ven ; M garde ses dates (au plus tard =
    // au plus tôt = saisies, cause `Manual`) ; X plafonné par le début de
    // M : mar 08:00 – 17:00. Aucun lien violé.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("X", 480)
        .task("M", 480)
        .task("Y", 480)
        .manual("M", Some("2026-10-21T08:00"), Some("2026-10-22T17:00"))
        .fs("X", "M")
        .fs("M", "Y")
        .run();
    let m = run.at("M");
    assert!(m.manual);
    assert_eq!(m.late.start, moment("2026-10-21T08:00"));
    assert_eq!(m.late.finish, moment("2026-10-22T17:00"));
    assert_eq!(m.early, m.late);
    assert_eq!(m.late_driver, LateDriver::Manual);
    assert_eq!(run.at("X").late.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("Y").late.start, moment("2026-10-23T08:00"));
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r106_manual_task_violating_a_successor() {
    // Même réseau, F jeu 22/10 17:00 : Y jeu 08:00 – 17:00 ; M finit jeu
    // 17:00, après le plafond jeu 08:00 de M → Y :
    // `link.manual_not_honored { M → Y }`. Début calculé mar 08:00 (X) ; Y
    // au plus tôt ven 08:00 (FS de M), TF(Y) = −480.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-22T17:00")
        .task("X", 480)
        .task("M", 480)
        .task("Y", 480)
        .manual("M", Some("2026-10-21T08:00"), Some("2026-10-22T17:00"))
        .fs("X", "M")
        .fs("M", "Y")
        .run();
    assert_eq!(run.codes(), ["link.manual_not_honored"]);
    assert_eq!(run.at("Y").late.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("Y").early.start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("Y").total_slack, Minutes::new(-480));
    assert_eq!(run.schedule.project_start, moment("2026-10-20T08:00"));
}

#[test]
fn r106_deadline_caps_late_dates() {
    // A 1 j, échéance mer 21/10 17:00, F ven 23/10 17:00 → A planifiée mer
    // 08:00 – 17:00 (plafond brut, R-102), cause `Deadline`.
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("A", 480)
        .deadline("A", "2026-10-21T17:00")
        .run();
    assert_eq!(run.at("A").start, moment("2026-10-21T08:00"));
    assert_eq!(run.at("A").late_driver, LateDriver::Deadline);
    assert_eq!(run.codes(), [""; 0]);
    // Une échéance dépassée par la date planifiée est signalée (MSO ven).
    let run = Net::new("2026-10-19T08:00")
        .from_finish("2026-10-23T17:00")
        .task("A", 480)
        .constraint(
            "A",
            Constraint::MustStartOn {
                date: moment("2026-10-23T08:00"),
            },
        )
        .deadline("A", "2026-10-22T17:00")
        .run();
    assert_eq!(run.at("A").start, moment("2026-10-23T08:00"));
    assert_eq!(
        run.at("A").late_driver,
        LateDriver::Constraint {
            kind: ConstraintKind::MustStartOn
        }
    );
    assert_eq!(run.codes(), ["task.deadline_missed"]);
}
