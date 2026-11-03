//! R-093 — Ne pas commencer avant le (SNET) : plancher `s*` (R-090) en
//! passe avant ; pilote seulement s'il déplace le début placé (R-044) ;
//! aucun effet en passe arrière depuis le début. Sources : S27 (« ES=CD »),
//! S28, S30.

use plannercraft_core::{Constraint, ConstraintKind, Driver, LinkType, Minutes};
use plannercraft_tests::moment;
use plannercraft_tests::net::Net;

fn snet(d: &str) -> Constraint {
    Constraint::StartNoEarlierThan { date: moment(d) }
}

#[test]
fn r093_floor_moves_the_task() {
    // A 2 j FS → B 1 j SNET mer 21/10 13:00 : borne du lien mar 17:00, placée
    // mer 08:00 < s* = mer 13:00 → B mer 13:00 – jeu 22/10 12:00, pilote
    // Constraint ; TF(B) 0 ; A au plus tard lun 13:00 – mer 12:00, TF(A) =
    // FS(A) = 240.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .task("B", 480)
        .fs("A", "B")
        .constraint("B", snet("2026-10-21T13:00"))
        .run();
    let b = run.at("B");
    assert_eq!(b.early.start, moment("2026-10-21T13:00"));
    assert_eq!(b.early.finish, moment("2026-10-22T12:00"));
    assert_eq!(
        b.driver,
        Driver::Constraint {
            kind: ConstraintKind::StartNoEarlierThan
        }
    );
    assert_eq!(b.total_slack, Minutes::ZERO);
    let a = run.at("A");
    assert_eq!(a.late.start, moment("2026-10-19T13:00"));
    assert_eq!(a.late.finish, moment("2026-10-21T12:00"));
    assert_eq!(a.total_slack, Minutes::new(240));
    assert_eq!(a.free_slack, Minutes::new(240));
    assert_eq!(run.codes(), [""; 0]);
}

#[test]
fn r093_link_wins_over_an_earlier_floor() {
    // P 3 j FS → A 1 j SNET mar 20/10 08:00 → A jeu 22/10 08:00, pilote le
    // lien.
    let run = Net::new("2026-10-19T08:00")
        .task("P", 1_440)
        .task("A", 480)
        .fs("P", "A")
        .constraint("A", snet("2026-10-20T08:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-22T08:00"));
    assert_eq!(
        run.at("A").driver,
        Driver::Link {
            predecessor: run.uid("P"),
            link: LinkType::FinishToStart
        }
    );
}

#[test]
fn r093_equal_placed_start_keeps_the_link_driver() {
    // A 1 j FS → B SNET mar 20/10 08:00 : borne du lien lun 17:00 (brute),
    // placée mar 08:00 = s* → B mar 08:00, pilote le lien (R-044 amendée).
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .constraint("B", snet("2026-10-20T08:00"))
        .run();
    assert_eq!(run.at("B").early.start, moment("2026-10-20T08:00"));
    assert_eq!(
        run.at("B").driver,
        Driver::Link {
            predecessor: run.uid("A"),
            link: LinkType::FinishToStart
        }
    );
}

#[test]
fn r093_no_effect_on_late_dates() {
    // A 1 j SNET mar 20/10 08:00 ; C 4 j seule (fin jeu 22/10 17:00) : A
    // mar ; au plus tard jeu 08:00 – 17:00 (le plancher ne plafonne rien),
    // TF 960.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("C", 1_920)
        .constraint("A", snet("2026-10-20T08:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-20T08:00"));
    assert_eq!(run.at("A").late.start, moment("2026-10-22T08:00"));
    assert_eq!(run.at("A").total_slack, Minutes::new(960));
}
