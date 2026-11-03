//! R-062 — Tâche inactive : `Excluded { Inactive }` (descendants :
//! `InactiveAncestor`) ; placée par ses prédécesseurs actifs, ses
//! prédécesseurs inactifs et la porte de son parent (`display`) ; ses liens
//! vers une tâche active sont ignorés (`link.inactive`), sans pont ; hors
//! agrégats, fin du projet, passe arrière et marge libre de ses
//! prédécesseurs. Source : S25 ; affichage et absence de pont : hypothèse.

use plannercraft_core::{Diagnostic, Exclusion, Minutes, Span, TaskOutcome};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn span(a: &str, b: &str) -> Span {
    Span {
        start: moment(a),
        finish: moment(b),
    }
}

#[test]
fn r062_inactive_task_has_no_influence() {
    // A 2 j → I (inactive, 1 j) → B 1 j.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("I", 480)
        .task("B", 480)
        .fs("A", "I")
        .fs("I", "B")
        .inactive("I")
        .run();
    // I placée pour l'affichage : mer 08:00 – 17:00.
    assert_eq!(
        *run.outcome("I"),
        TaskOutcome::Excluded {
            reason: Exclusion::Inactive,
            display: Some(span("2026-10-21T08:00", "2026-10-21T17:00")),
        }
    );
    // B ne dépend de rien : lun 08:00 – 17:00 ; fin du projet mar 17:00.
    assert_eq!(run.at("B").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.schedule.project_finish, moment("2026-10-20T17:00"));
    // A n'a aucun successeur actif : LF = fin du projet, TF 0 ; TF(B) = 480.
    assert_eq!(run.at("A").total_slack, Minutes::ZERO);
    assert_eq!(run.at("A").free_slack, Minutes::ZERO);
    assert_eq!(run.at("B").total_slack, Minutes::new(480));
    assert!(run.schedule.diagnostics.contains(&Diagnostic::LinkInactive {
        predecessor: run.uid("I"),
        successor: run.uid("B"),
    }));
    // Une tâche inactive n'est pas dans l'ordre des tâches planifiées.
    assert!(!run.schedule.order.contains(&run.uid("I")));
}

#[test]
fn r062_inactive_summary_and_descendants() {
    // S inactive {A 1 j} ; T 1 j active : S et A placées pour l'affichage.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 480)
        .task("A", 480)
        .level("A", 2)
        .task("T", 480)
        .inactive("S")
        .run();
    let display = Some(span("2026-10-19T08:00", "2026-10-19T17:00"));
    assert_eq!(
        *run.outcome("S"),
        TaskOutcome::Excluded {
            reason: Exclusion::Inactive,
            display,
        }
    );
    assert_eq!(
        *run.outcome("A"),
        TaskOutcome::Excluded {
            reason: Exclusion::InactiveAncestor,
            display,
        }
    );
}

#[test]
fn r062_inactive_excluded_from_project_finish() {
    // I inactive de 5 j, A 1 j : fin du projet lun 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("I", 2_400)
        .inactive("I")
        .run();
    assert_eq!(run.schedule.project_finish, moment("2026-10-19T17:00"));
}

#[test]
fn r062_links_inside_an_inactive_summary_place_the_display() {
    // S inactive {A 1 j, B 1 j FS de A} ; T 1 j active. Le lien A → B relie
    // deux tâches inactives : gardé pour l'affichage, B affichée mar 08:00 –
    // 17:00 (et non lun comme A), sans `link.inactive`. S affiche l'agrégat
    // lun 08:00 – mar 17:00. Rien ne touche T ni la fin du projet.
    let run = Net::new("2026-10-19T08:00")
        .task("S", 0)
        .task("A", 480)
        .level("A", 2)
        .task("B", 480)
        .level("B", 2)
        .task("T", 480)
        .fs("A", "B")
        .inactive("S")
        .run();
    assert_eq!(
        *run.outcome("B"),
        TaskOutcome::Excluded {
            reason: Exclusion::InactiveAncestor,
            display: Some(span("2026-10-20T08:00", "2026-10-20T17:00")),
        }
    );
    assert_eq!(
        *run.outcome("S"),
        TaskOutcome::Excluded {
            reason: Exclusion::Inactive,
            display: Some(span("2026-10-19T08:00", "2026-10-20T17:00")),
        }
    );
    assert!(!run.codes().contains(&"link.inactive"), "{:?}", run.codes());
    assert_eq!(run.schedule.project_finish, moment("2026-10-19T17:00"));
    assert_eq!(run.at("T").total_slack, Minutes::ZERO);
}
