//! R-090 — Normalisation des dates de contrainte, sur l'échelle de
//! placement `P` de la tâche : date de début `A = P.forward_span(c, d)` ;
//! date de fin `f* = P.backward_span(c, 0).finish`, `A = (P.add(f*, −d),
//! f*)` ; temps écoulé : aucun recalage. Sources : S27 (« EF=CD »), S9, O14.
//! Hypothèse concurrente : fin = date brute même en temps chômé.

use plannercraft_core::calendar::{Calendar, DaySetting};
use plannercraft_core::{CalendarUid, Constraint, ConstraintKind, Driver, Week, Weekday};
use plannercraft_tests::net::Net;
use plannercraft_tests::{moment, working};

fn snet(d: &str) -> Constraint {
    Constraint::StartNoEarlierThan { date: moment(d) }
}

fn fnet(d: &str) -> Constraint {
    Constraint::FinishNoEarlierThan { date: moment(d) }
}

/// Calendrier écrit à la main sur le modèle du calendrier du projet de A7
/// (O14) : ven 18:00-20:00 et 21:00-23:00, sam 09:00-13:00 et 14:00-18:00.
fn a7_calendar() -> Calendar {
    Calendar::new_base(
        CalendarUid::new(5),
        "A7".to_owned(),
        Week::from_fn(|d| match d {
            Weekday::Friday => working(&[("18:00", "20:00"), ("21:00", "23:00")]),
            Weekday::Saturday => working(&[("09:00", "13:00"), ("14:00", "18:00")]),
            _ => DaySetting::NonWorking,
        }),
    )
}

/// Projet sur le calendrier « A7 », début ven 04/10/2024 08:00.
fn a7() -> Net {
    Net::new("2024-10-04T08:00").project(|p| {
        p.calendars.push(a7_calendar()).unwrap();
        p.calendar = CalendarUid::new(5);
    })
}

#[test]
fn r090_start_side_moves_forward() {
    // SNET sam 24/10 10:00, 1 j → lun 26/10 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .constraint("A", snet("2026-10-24T10:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-26T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-26T17:00"));
    // SNET mer 21/10 07:00 → mer 08:00 (S9 : un début saisi avant la plage
    // commence au début de la plage), et non 07:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .constraint("A", snet("2026-10-21T07:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-21T08:00"));
    assert_eq!(
        run.at("A").driver,
        Driver::Constraint {
            kind: ConstraintKind::StartNoEarlierThan
        }
    );
}

#[test]
fn r090_finish_side_keeps_a_range_start() {
    // FNET ven 23/10 17:00, 1 j → ven 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .constraint("A", fnet("2026-10-23T17:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-23T17:00"));
    // FNET lun 26/10 08:00 (début d'une plage) : f* = lun 08:00, gardé ;
    // A = ven 23/10 08:00 – lun 26/10 08:00. R-030 donnerait ven 17:00 :
    // cas qui discrimine.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .constraint("A", fnet("2026-10-26T08:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-26T08:00"));
    // FNET sam 24/10 10:00 (temps chômé) : instant ouvré précédent, ven
    // 17:00 → ven 08:00 – 17:00.
    let run = Net::new("2026-10-19T08:00")
        .task("A", 480)
        .constraint("A", fnet("2026-10-24T10:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-23T08:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-23T17:00"));
}

#[test]
fn r090_finish_side_on_the_a7_calendar() {
    // O14 : FNET ven 18/10/2024 18:00, 8 h : ven 18:00 commence la plage
    // 18:00-20:00, gardé ; 8 h en arrière : sam 12/10 14:00-18:00 et
    // 09:00-13:00 → sam 12/10 09:00 – ven 18/10 18:00.
    // 12 h, FNET ven 29/11 18:00 : sam 23/11 (8 h), ven 22/11 21:00-23:00
    // et 18:00-20:00 (4 h) → ven 22/11 18:00 – ven 29/11 18:00.
    let run = a7()
        .task("A", 480)
        .task("B", 720)
        .constraint("A", fnet("2024-10-18T18:00"))
        .constraint("B", fnet("2024-11-29T18:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2024-10-12T09:00"));
    assert_eq!(run.at("A").early.finish, moment("2024-10-18T18:00"));
    assert_eq!(run.at("B").early.start, moment("2024-11-22T18:00"));
    assert_eq!(run.at("B").early.finish, moment("2024-11-29T18:00"));
}

#[test]
fn r090_elapsed_duration_is_not_realigned() {
    // Durée écoulée 1 ej, FNET mer 21/10 10:00 → mar 20/10 10:00 – mer
    // 10:00, sans recalage.
    let run = Net::new("2026-10-19T08:00")
        .elapsed("A", 1_440)
        .constraint("A", fnet("2026-10-21T10:00"))
        .run();
    assert_eq!(run.at("A").early.start, moment("2026-10-20T10:00"));
    assert_eq!(run.at("A").early.finish, moment("2026-10-21T10:00"));
}
