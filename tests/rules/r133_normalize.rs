//! R-133 — Image d'un projet par l'aller-retour MSPDI (`normalize`).

use plannercraft_core::calendar::{CalendarException, DaySetting, Recurrence};
use plannercraft_core::{
    CalendarUid, DayRange, ExtraEntry, Extras, Lag, LagValue, Link, LinkType, OutlineLevel, Percent,
    SignedPercent, Task, TaskUid, TimeBasis,
};
use plannercraft_mspdi::normalize;
use plannercraft_mspdi::write::{Computed, OrderProfile, WriteOptions, write};
use plannercraft_tests::day;
use plannercraft_tests::mspdi::{new_project, reread};

fn round_trip(proj: &plannercraft_core::Project, options: &WriteOptions) -> plannercraft_core::Project {
    reread(&write(proj, Computed::Omit, options).xml).project
}

#[test]
fn r133_percentages_and_percent_lags() {
    // % achevé 3 333 → 3 300 ; décalage −25,05 % → −25,1 % (pas de 0,1 %).
    let mut proj = new_project("2026-10-19T08:00");
    let opts = proj.options;
    let mut a = Task::new(TaskUid::new(1), "A".to_owned(), &opts);
    a.progress.percent_complete = Percent::new(3_333).unwrap();
    proj.tasks.push(a).unwrap();
    let mut b = Task::new(TaskUid::new(2), "B".to_owned(), &opts);
    b.predecessors = vec![Link {
        predecessor: TaskUid::new(1),
        kind: LinkType::StartToStart,
        lag: Lag::new(
            TimeBasis::Working,
            LagValue::Percent {
                percent: SignedPercent::new(-2_505).unwrap(),
            },
        )
        .unwrap(),
    }];
    proj.tasks.push(b).unwrap();
    let options = WriteOptions::default();
    let norm = normalize(&proj, &options);
    let t = |p: &plannercraft_core::Project, uid: u32| p.tasks.get(TaskUid::new(uid)).unwrap().clone();
    assert_eq!(
        t(&norm, 1).progress.percent_complete,
        Percent::new(3_300).unwrap()
    );
    assert_eq!(
        t(&norm, 2).predecessors[0].lag.value,
        LagValue::Percent {
            percent: SignedPercent::new(-2_510).unwrap()
        }
    );
    assert_eq!(round_trip(&proj, &options), norm);
    assert_eq!(normalize(&norm, &options), norm);
}

#[test]
fn r133_summary_duration_strings_period_and_extras() {
    let mut proj = new_project("2026-10-19T08:00");
    let opts = proj.options;
    let mut s = Task::new(TaskUid::new(1), "S".to_owned(), &opts);
    s.notes = "a\u{1}b".to_owned();
    proj.tasks.push(s).unwrap();
    let mut a = Task::new(TaskUid::new(2), "é".repeat(600), &opts);
    a.outline_level = OutlineLevel::new(2).unwrap();
    a.extras = Extras(vec![
        ExtraEntry {
            origin: "plannercraft".to_owned(),
            key: "Contact".to_owned(),
            payload: "<Contact>x</Contact>".to_owned(),
        },
        ExtraEntry {
            origin: "mspdi".to_owned(),
            key: "Duration>Zzz".to_owned(),
            payload: "<Zzz/>".to_owned(),
        },
    ]);
    proj.tasks.push(a).unwrap();
    proj.calendars
        .get_mut(CalendarUid::new(1))
        .unwrap()
        .add_exception(CalendarException {
            name: "Noël".to_owned(),
            period: DayRange::single(day("2026-12-25")),
            setting: DaySetting::NonWorking,
            recurrence: Recurrence::Period,
            occurrences: None,
        })
        .unwrap();
    let options = WriteOptions::default();
    let norm = normalize(&proj, &options);
    let t = |uid: u32| norm.tasks.get(TaskUid::new(uid)).unwrap();
    // Récapitulative automatique : durée calculée, minutes à 0 (R-071).
    assert_eq!(t(1).duration.minutes().get(), 0);
    assert_eq!(t(1).notes, "a\u{FFFD}b");
    assert_eq!(t(2).name.chars().count(), 512);
    // Origine étrangère retirée ; ancre sur `Duration` (M?, absent selon le
    // rôle) recalculée sur l'élément réécrit précédent (`Priority`).
    let keys: Vec<&str> = t(2).extras.0.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, ["Priority>Zzz"]);
    assert_eq!(
        norm.calendars.get(CalendarUid::new(1)).unwrap().exceptions()[0].recurrence,
        Recurrence::Daily { every: 1 }
    );
    // `CurrencyCode` ajouté (R-130).
    assert!(
        norm.extras
            .0
            .iter()
            .any(|e| e.key == "CurrencyCode" && e.payload == "<CurrencyCode>EUR</CurrencyCode>")
    );
    assert_eq!(round_trip(&proj, &options), norm);
    assert_eq!(normalize(&norm, &options), norm);
    // Profil strict : hors schéma retirés, GUID absents.
    let strict = WriteOptions {
        profile: OrderProfile::Strict,
        ..WriteOptions::default()
    };
    let ns = normalize(&proj, &strict);
    assert!(ns.tasks.get(TaskUid::new(2)).unwrap().extras.is_empty());
    assert_eq!(round_trip(&proj, &strict), ns);
}
