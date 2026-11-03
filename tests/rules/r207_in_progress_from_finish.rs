//! R-207 — Planification à rebours avec avancement. Achevée : R-203
//! inchangée. En cours : reste placé au plus tôt depuis `Resume`
//! (`max(Stop, reprise saisie)`, sans borne), dates au plus tard = au plus
//! tôt, `LateDriver::Actual`, `progress.from_finish_in_progress`
//! (avertissement). Ses prédécesseurs sont plafonnés par sa reprise.
//! Hypothèse.

use plannercraft_core::{Diagnostic, Driver, LateDriver, Minutes, Severity, Span};
use plannercraft_tests::moment;

use crate::r204_in_progress_leaf::started;

#[test]
fn r207_from_finish_in_progress() {
    // Fin du projet ven 23/10 17:00, T 3 j commencée lun à 3 333 ⇒ T lun
    // 08:00 – mer 17:00, TF 0, avertissement.
    let run = started(3_333).from_finish("2026-10-23T17:00").run();
    let t = run.at("T");
    let span = Span {
        start: moment("2026-10-19T08:00"),
        finish: moment("2026-10-21T17:00"),
    };
    assert_eq!((t.start, t.finish), (span.start, span.finish));
    assert_eq!(t.early, span);
    assert_eq!(t.late, span);
    assert_eq!(t.driver, Driver::Actual);
    assert_eq!(t.late_driver, LateDriver::Actual);
    assert_eq!(t.total_slack, Minutes::ZERO);
    let p = t.progress.unwrap();
    assert_eq!(
        (p.stop, p.resume),
        (moment("2026-10-19T17:00"), moment("2026-10-20T08:00"))
    );
    let warn = Diagnostic::ProgressFromFinishInProgress { task: run.uid("T") };
    assert!(run.schedule.diagnostics.contains(&warn));
    assert_eq!(warn.severity(), Severity::Warning);
    // Prédécesseur P 1 j FS → T : plafonné par la reprise (mar 08:00), P
    // lun 08:00 – 17:00 ; successeur U 1 j : placé sous l'ancre (ven).
    let run = started(3_333)
        .from_finish("2026-10-23T17:00")
        .task("P", 480)
        .task("U", 480)
        .fs("P", "T")
        .fs("T", "U")
        .run();
    assert_eq!(run.at("P").finish, moment("2026-10-19T17:00"));
    assert_eq!(run.at("T").finish, moment("2026-10-21T17:00"));
    assert_eq!(run.at("U").start, moment("2026-10-23T08:00"));
    assert_eq!(
        plannercraft_core::schedule(&started(3_333).from_finish("2026-10-23T17:00").build()),
        plannercraft_core::schedule::schedule_reference(
            &started(3_333).from_finish("2026-10-23T17:00").build()
        )
    );
}
