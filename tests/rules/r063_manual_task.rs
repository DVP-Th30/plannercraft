//! R-063 — Tâche manuelle (feuille) : dates saisies conservées sans
//! recalage ; une seule saisie : l'autre par `P.add` avec la durée saisie,
//! sinon la durée de la tâche ; deux saisies : durée utilisée = temps entre
//! elles ; fin < début : `task.manual_dates_inverted`, fin := début ; ni
//! liens entrants ni porte ne la déplacent (`link.manual_not_honored`) ; elle
//! pilote ses successeurs ; dates au plus tard, marges et critique calculées
//! (informatives) ; dans les agrégats et la fin du projet ; sans date :
//! `ManualUndated`. Source : S8 ; observé O9 ; reste : hypothèse.

use plannercraft_core::{Diagnostic, Driver, Exclusion, Minutes, TaskOutcome};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

#[test]
fn r063_manual_dates_drive_successors() {
    // M manuelle mer 21/10 08:00 – jeu 22/10 17:00 ; B FS de M 1 j →
    // ven 23/10 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("M", 480)
        .manual("M", Some("2026-10-21T08:00"), Some("2026-10-22T17:00"))
        .task("B", 480)
        .fs("M", "B")
        .run();
    let m = run.at("M");
    assert!(m.manual);
    assert_eq!(m.driver, Driver::Manual);
    assert_eq!(m.early.start, moment("2026-10-21T08:00"));
    assert_eq!(m.early.finish, moment("2026-10-22T17:00"));
    // Durée utilisée : temps ouvré entre les deux dates, 960.
    assert_eq!(m.duration, Minutes::new(960));
    assert_eq!(run.at("B").early.start, moment("2026-10-23T08:00"));
    assert_eq!(run.schedule.project_finish, moment("2026-10-23T17:00"));
}

#[test]
fn r063_links_do_not_move_a_manual_task() {
    // A 2 j FS → M manuelle lun 08:00 – 17:00 : M inchangée, avertissement.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("M", 480)
        .manual("M", Some("2026-10-19T08:00"), Some("2026-10-19T17:00"))
        .fs("A", "M")
        .run();
    assert_eq!(run.at("M").early.start, moment("2026-10-19T08:00"));
    assert!(
        run.schedule
            .diagnostics
            .contains(&Diagnostic::LinkManualNotHonored {
                predecessor: run.uid("A"),
                successor: run.uid("M"),
            })
    );
    // Dates au plus tard informatives : M n'a pas de successeur, LF = fin du
    // projet (mar 17:00), LS = mar 08:00 → TF = 480.
    assert_eq!(run.at("M").total_slack, Minutes::new(480));
}

#[test]
fn r063_single_entered_date() {
    // M manuelle, début mer 08:00 seul, 2 j → fin jeu 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("M", 960)
        .manual("M", Some("2026-10-21T08:00"), None)
        .run();
    assert_eq!(run.at("M").early.finish, moment("2026-10-22T17:00"));
    // Fin jeu 17:00 seule, 2 j → début add(jeu 17:00, −960) = mer 08:00.
    let run = Net::new("2026-10-19T08:00")
        .task("M", 960)
        .manual("M", None, Some("2026-10-22T17:00"))
        .run();
    assert_eq!(run.at("M").early.start, moment("2026-10-21T08:00"));
}

#[test]
fn r063_dates_not_realigned() {
    // Début saisi sam 24/10 10:00, fin lun 26/10 12:00 : conservés tels quels.
    let run = Net::new("2026-10-19T08:00")
        .task("M", 480)
        .manual("M", Some("2026-10-24T10:00"), Some("2026-10-26T12:00"))
        .run();
    assert_eq!(run.at("M").early.start, moment("2026-10-24T10:00"));
    assert_eq!(run.at("M").duration, Minutes::new(240));
}

#[test]
fn r063_inverted_dates() {
    // Fin avant début : erreur, fin ramenée au début.
    let run = Net::new("2026-10-19T08:00")
        .task("M", 480)
        .manual("M", Some("2026-10-21T08:00"), Some("2026-10-20T08:00"))
        .run();
    assert_eq!(run.at("M").early.finish, moment("2026-10-21T08:00"));
    assert!(run.codes().contains(&"task.manual_dates_inverted"));
}

#[test]
fn r063_undated_manual_task_excluded() {
    // Manuelle sans aucune date : exclue ; son lien vers B est ignoré.
    let run = Net::new("2026-10-19T08:00")
        .task("M", 480)
        .manual("M", None, None)
        .task("B", 480)
        .fs("M", "B")
        .run();
    assert_eq!(
        *run.outcome("M"),
        TaskOutcome::Excluded {
            reason: Exclusion::ManualUndated,
            display: None
        }
    );
    assert_eq!(run.at("B").early.start, moment("2026-10-19T08:00"));
    assert!(run.codes().contains(&"task.manual_undated"));
    assert!(run.codes().contains(&"link.excluded_endpoint"));
}
