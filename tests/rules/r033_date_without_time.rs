//! R-033 — Date saisie sans heure : un début reçoit l'heure de début par
//! défaut, une fin l'heure de fin par défaut. L'alignement sur le calendrier
//! se fait ensuite par le placement (R-030).

use plannercraft_core::{ProjectOptions, TimeOfDay};
use plannercraft_tests::{day, moment};

#[test]
fn r033_default_start_and_finish_times() {
    let o = ProjectOptions::default();
    // 2026-10-19 en début → 08:00 ; en fin → 17:00.
    assert_eq!(o.start_of(day("2026-10-19")), Ok(moment("2026-10-19T08:00")));
    assert_eq!(o.finish_of(day("2026-10-19")), Ok(moment("2026-10-19T17:00")));
}

#[test]
fn r033_custom_default_start_time() {
    // Début par défaut à 07:00 → 07:00 (le placement le recalera à 08:00 sur
    // le calendrier Standard, exemple de S9).
    let o = ProjectOptions {
        default_start_time: TimeOfDay::hm(7, 0).unwrap(),
        ..ProjectOptions::default()
    };
    assert_eq!(o.start_of(day("2026-10-19")), Ok(moment("2026-10-19T07:00")));
}

#[test]
fn r033_end_of_day_finish_is_next_midnight() {
    // Fin par défaut à 24:00 : le lendemain 00:00 ; le 31/12/2149 donne
    // l'instant maximal du domaine.
    let o = ProjectOptions {
        default_finish_time: TimeOfDay::END_OF_DAY,
        ..ProjectOptions::default()
    };
    assert_eq!(o.finish_of(day("2026-10-19")), Ok(moment("2026-10-20T00:00")));
    assert_eq!(o.finish_of(day("2149-12-31")), Ok(moment("2150-01-01T00:00")));
}

#[test]
fn r033_default_start_then_aligned_by_span() {
    use plannercraft_core::TimeScale;
    use plannercraft_tests::{compile_one, mins, standard};
    // Exemple de S9 : début par défaut 07:00 sur un calendrier à 08:00. La
    // date saisie devient 07:00, puis le placement d'une journée la recale :
    // 08:00 → 17:00.
    let o = ProjectOptions {
        default_start_time: TimeOfDay::hm(7, 0).unwrap(),
        ..ProjectOptions::default()
    };
    let from = o.start_of(day("2026-10-19")).unwrap();
    let c = compile_one(standard());
    let span = TimeScale::Working(&c).forward_span(from, mins(480)).unwrap();
    assert_eq!(
        (span.start, span.finish),
        (moment("2026-10-19T08:00"), moment("2026-10-19T17:00"))
    );
}
