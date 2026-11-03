//! R-212 — % physique : stocké, borné à [0, 10 000], sans effet sur les
//! dates, la durée ni les autres % (S56).

use crate::r201_percent_input::{FRI, MON, calendar, ctx, part, pct, task, work, write};
use plannercraft_core::Percent;
use plannercraft_core::progress::edit::{Input, apply};

#[test]
fn r212_physical_no_effect() {
    let cal = calendar();
    let c = ctx(&cal, MON, FRI);
    let mut t = task(2_400);
    let parts = [part(1, 2_400)];
    let e = apply(&t, &parts, &[Input::Percent(pct(4_000))], &c).unwrap();
    write(&mut t, &e);
    // 75 % physique : seul `physical_percent_complete` change.
    let p = apply(&t, &e.parts, &[Input::Physical(pct(7_500))], &c).unwrap();
    assert_eq!(p.progress.physical_percent_complete, pct(7_500));
    let mut expected = e.progress;
    expected.physical_percent_complete = pct(7_500);
    assert_eq!(p.progress, expected);
    assert_eq!(p.duration, e.duration);
    assert_eq!(p.parts, e.parts);
    assert_eq!(p.parts[0].actual_work, work(960));
    // Hors de [0, 10 000] : non représentable.
    assert!(Percent::new(10_001).is_err());
}
