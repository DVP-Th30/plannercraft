//! R-094 — Ne pas finir avant le (FNET) : plancher de fin `f*` (R-090) en
//! passe avant ; s'il pilote, `early = A` (fin = `f*`) ; aucun effet en
//! passe arrière depuis le début. Sources : S27 (« EF=CD »), S28, S30, O14,
//! O17.

use plannercraft_core::calendar::{Calendar, DaySetting};
use plannercraft_core::{CalendarUid, Constraint, ConstraintKind, Driver, Minutes, Week, Weekday};
use plannercraft_tests::net::Net;
use plannercraft_tests::{moment, working};

fn fnet(d: &str) -> Constraint {
    Constraint::FinishNoEarlierThan { date: moment(d) }
}

#[test]
fn r094_floor_on_the_finish() {
    // A 1 j FS → B 1 j FNET jeu 22/10 17:00 → B jeu 08:00 – 17:00, TF(B) 0 ;
    // A lun, au plus tard mer 08:00 – 17:00 : TF(A) = FS(A) = 960.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .constraint("B", fnet("2026-10-22T17:00"))
        .run();
    let b = run.at("B");
    assert_eq!(b.early.start, moment("2026-10-22T08:00"));
    assert_eq!(b.early.finish, moment("2026-10-22T17:00"));
    assert_eq!(
        b.driver,
        Driver::Constraint {
            kind: ConstraintKind::FinishNoEarlierThan
        }
    );
    assert_eq!(b.total_slack, Minutes::ZERO);
    assert_eq!(run.at("A").total_slack, Minutes::new(960));
    assert_eq!(run.at("A").free_slack, Minutes::new(960));
}

#[test]
fn r094_floor_before_the_project_start_has_no_effect() {
    // A 2 j FNET mar 20/10 12:00 : A = ven 16/10 13:00 – mar 12:00 ; le
    // placement depuis le début du projet finit mar 17:00 > f* → A lun
    // 08:00 – mar 17:00, pilote le début du projet.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 960)
        .constraint("A", fnet("2026-10-20T12:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-19T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-20T17:00"));
    assert_eq!(run.at("A").driver, Driver::ProjectStart);
}

#[test]
fn r094_finish_at_a_range_start() {
    // A 1 j FNET lun 26/10 08:00 → ven 23/10 08:00 – lun 26/10 08:00 ; B FS
    // → lun 26/10 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .task("B", 480)
        .fs("A", "B")
        .constraint("A", fnet("2026-10-26T08:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-26T08:00"));
    assert_eq!(run.at("B").early.start, moment("2026-10-26T08:00"));
    assert_eq!(run.at("B").early.finish, moment("2026-10-26T17:00"));
}

#[test]
fn r094_a7_calendar() {
    // O14 (A7 UID 15) : calendrier ven 18:00-20:00, 21:00-23:00, sam
    // 09:00-13:00, 14:00-18:00. A 8 h FNET ven 18/10/2024 18:00 ; Z 8 h FNET
    // ven 20/12/2024 18:00 fixe la fin du projet (ven 20/12 18:00).
    // A : ES sam 12/10 09:00, EF ven 18/10 18:00 ; au plus tard depuis la fin
    // du projet (R-030 : instant ouvré précédant ven 20/12 18:00 = sam 14/12
    // 18:00) : LS sam 14/12 09:00, LF sam 14/12 18:00 ; TF = FS = 9 semaines
    // de 12 h = 6 480.
    let cal = Calendar::new_base(
        CalendarUid::new(5),
        "A7".to_owned(),
        Week::from_fn(|d| match d {
            Weekday::Friday => working(&[("18:00", "20:00"), ("21:00", "23:00")]),
            Weekday::Saturday => working(&[("09:00", "13:00"), ("14:00", "18:00")]),
            _ => DaySetting::NonWorking,
        }),
    );
    let run = Net::new("2024-10-04T08:00")
        .project(|p| {
            p.calendars.push(cal).unwrap();
            p.calendar = CalendarUid::new(5);
        })
        .task("A", 480)
        .task("Z", 480)
        .constraint("A", fnet("2024-10-18T18:00"))
        .constraint("Z", fnet("2024-12-20T18:00"))
        .run();
    assert_eq!(run.schedule.project_finish, moment("2024-12-20T18:00"));
    let a = run.at("A");
    assert_eq!(a.early.start, moment("2024-10-12T09:00"));
    assert_eq!(a.early.finish, moment("2024-10-18T18:00"));
    assert_eq!(a.late.start, moment("2024-12-14T09:00"));
    assert_eq!(a.late.finish, moment("2024-12-14T18:00"));
    assert_eq!(a.total_slack, Minutes::new(6_480));
    assert_eq!(a.free_slack, Minutes::new(6_480));
}
