//! R-099 — Option « les tâches respectent toujours leurs dates de
//! contrainte » (`ProjectOptions::honor_constraints`, vraie par défaut,
//! R-032) : active, une contrainte dure l'emporte sur les liens (serrage,
//! `constraint.override`) ; inactive, les liens l'emportent et
//! `constraint.not_honored` signale la contrainte non tenue — c'est le seul
//! cas où ce diagnostic apparaît. Sources : S8, S24, S34, Q13 ; O15.

use plannercraft_core::{Constraint, Diagnostic, Minutes, ProjectOptions};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

/// P 2 j FS → A 1 j de contrainte `c`.
fn run(c: Constraint, honor: bool) -> plannercraft_tests::net::Run {
    Net::new("2026-10-19T08:00")
        .task("P", 960)
        .task("A", 480)
        .fs("P", "A")
        .constraint("A", c)
        .honor(honor)
        .run()
}

fn codes(r: &plannercraft_tests::net::Run) -> Vec<&'static str> {
    r.schedule.diagnostics.iter().map(Diagnostic::code).collect()
}

#[test]
fn r099_option_is_on_by_default() {
    assert!(ProjectOptions::default().honor_constraints);
}

#[test]
fn r099_snlt_and_fnlt_in_both_modes() {
    // SNLT mar 20/10 08:00 et FNLT mar 20/10 17:00 (A 1 j) : même ancrage
    // A = mar 08:00 – 17:00. Active : A sur l'ancrage, override ; inactive :
    // A mer (le lien), not_honored. Marge de P −480 dans les deux cas.
    for c in [
        Constraint::StartNoLaterThan {
            date: moment("2026-10-20T08:00"),
        },
        Constraint::FinishNoLaterThan {
            date: moment("2026-10-20T17:00"),
        },
    ] {
        let on = run(c, true);
        assert_eq!(on.at("A").early.start, moment("2026-10-20T08:00"), "{c:?}");
        assert_eq!(codes(&on), ["constraint.override"], "{c:?}");
        assert_eq!(on.at("P").total_slack, Minutes::new(-480), "{c:?}");
        let off = run(c, false);
        assert_eq!(off.at("A").early.start, moment("2026-10-21T08:00"), "{c:?}");
        assert_eq!(codes(&off), ["constraint.not_honored"], "{c:?}");
        assert_eq!(off.at("P").total_slack, Minutes::new(-480), "{c:?}");
        assert_eq!(off.at("A").total_slack, Minutes::new(-480), "{c:?}");
    }
}

#[test]
fn r099_floors_do_not_depend_on_the_option() {
    // SNET et FNET (planchers) : mêmes dates, aucun diagnostic, option
    // active ou non.
    for c in [
        Constraint::StartNoEarlierThan {
            date: moment("2026-10-22T08:00"),
        },
        Constraint::FinishNoEarlierThan {
            date: moment("2026-10-22T17:00"),
        },
    ] {
        let on = run(c, true);
        let off = run(c, false);
        assert_eq!(on.at("A").early.start, moment("2026-10-22T08:00"), "{c:?}");
        assert_eq!(on.schedule, off.schedule, "{c:?}");
        assert_eq!(on.codes(), [""; 0], "{c:?}");
    }
}

#[test]
fn r099_mso_and_mfo_in_both_modes() {
    // MSO mar 20/10 08:00 et MFO mar 20/10 17:00 : épingle (option active)
    // ou fenêtre (inactive), mêmes marges de P (−480) qu'avec SNLT.
    for c in [
        Constraint::MustStartOn {
            date: moment("2026-10-20T08:00"),
        },
        Constraint::MustFinishOn {
            date: moment("2026-10-20T17:00"),
        },
    ] {
        let on = run(c, true);
        assert_eq!(on.at("A").early.start, moment("2026-10-20T08:00"), "{c:?}");
        assert_eq!(on.at("A").early, on.at("A").late, "{c:?}");
        assert_eq!(codes(&on), ["constraint.override"], "{c:?}");
        let off = run(c, false);
        assert_eq!(off.at("A").early.start, moment("2026-10-21T08:00"), "{c:?}");
        assert_eq!(codes(&off), ["constraint.not_honored"], "{c:?}");
        for r in [&on, &off] {
            assert_eq!(r.at("P").total_slack, Minutes::new(-480), "{c:?}");
        }
    }
}
