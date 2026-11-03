//! R-031 — Jalon (durée nulle). Passe avant : t si t termine une minute
//! ouvrée ou se trouve dans une plage (`previous_working_moment(t) == t`),
//! sinon l'instant ouvré suivant. Passe arrière, symétrique : t si
//! `next_working_moment(t) == t`, sinon l'instant ouvré précédent.
//! Hypothèse concurrente (Q3) : aucun recalage ; le corpus tranchera.
//! Calendrier Standard.

use plannercraft_core::calendar::CompiledCalendar;
use plannercraft_core::{Minutes, Moment, TimeScale};
use plannercraft_tests::{compile_one, moment, standard};

fn cal() -> CompiledCalendar {
    compile_one(standard())
}

fn forward(c: &CompiledCalendar, t: &str) -> Moment {
    let s = TimeScale::Working(c)
        .forward_span(moment(t), Minutes::ZERO)
        .unwrap();
    assert_eq!(s.start, s.finish, "un jalon commence et finit au même instant");
    s.start
}

fn backward(c: &CompiledCalendar, t: &str) -> Moment {
    let s = TimeScale::Working(c)
        .backward_span(moment(t), Minutes::ZERO)
        .unwrap();
    assert_eq!(s.start, s.finish, "un jalon commence et finit au même instant");
    s.start
}

#[test]
fn r031_forward_milestone() {
    let c = cal();
    // Ven 23/10 17:00 termine une minute ouvrée : le jalon reste ven 17:00,
    // et non lun 08:00 (un jalon lié en FS s'affiche à la fin de son
    // prédécesseur).
    assert_eq!(forward(&c, "2026-10-23T17:00"), moment("2026-10-23T17:00"));
    // Lun 08:00 : début de plage, la minute [08:00, 08:01) est ouvrée mais
    // [07:59, 08:00) ne l'est pas → instant ouvré suivant = 08:00.
    assert_eq!(forward(&c, "2026-10-19T08:00"), moment("2026-10-19T08:00"));
    // Lun 12:00 termine la plage du matin : reste 12:00.
    assert_eq!(forward(&c, "2026-10-19T12:00"), moment("2026-10-19T12:00"));
    // Lun 12:30, dans la pause : 13:00.
    assert_eq!(forward(&c, "2026-10-19T12:30"), moment("2026-10-19T13:00"));
    // Sam 24/10 10:00 : lun 26/10 08:00.
    assert_eq!(forward(&c, "2026-10-24T10:00"), moment("2026-10-26T08:00"));
}

#[test]
fn r031_backward_milestone() {
    let c = cal();
    // Sam 10:00 → ven 23/10 17:00.
    assert_eq!(backward(&c, "2026-10-24T10:00"), moment("2026-10-23T17:00"));
    // Lun 08:00 commence une plage : reste lun 08:00.
    assert_eq!(backward(&c, "2026-10-19T08:00"), moment("2026-10-19T08:00"));
    // Lun 12:30, dans la pause : 12:00.
    assert_eq!(backward(&c, "2026-10-19T12:30"), moment("2026-10-19T12:00"));
}
