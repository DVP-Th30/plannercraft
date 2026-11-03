//! P24 (R-133), étendue en P43 (J6 : avancement, reprises, options de la
//! date d'état, lignes de base générées ; R-214, R-215, R-218, R-226) :
//! aller-retour MSPDI sur des projets générés.
//!
//! Pour tout projet sans erreur de `Project::validate` :
//! `read(write(p)).project == normalize(p)` dans les profils observé et
//! strict ; avec les valeurs du moteur, `normalize(schedule_image(p, s))`
//! (R-199 : travail des récapitulatives, durée des feuilles affectées,
//! répartitions périmées) ; `normalize` est idempotente ; `write` est
//! déterministe.
//! 256 cas, 2 048 sous `PROPTEST_CASES` (CI).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

#[path = "support/generate.rs"]
mod generate;

use plannercraft_core::{Severity, schedule};
use plannercraft_mspdi::write::{Computed, OrderProfile, WriteOptions, schedule_image, write};
use plannercraft_mspdi::{ReadOptions, normalize, read};
use proptest::prelude::*;

fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .map_or(256, |n| n.min(2_048))
}

fn opts() -> ReadOptions {
    ReadOptions::new("2026-01-01T00:00".parse().unwrap())
}

/// Égalité, avec la première différence des formes `Debug` en cas d'échec.
fn same<T: PartialEq + std::fmt::Debug>(a: &T, b: &T, what: &str) {
    if a == b {
        return;
    }
    let (x, y) = (format!("{a:?}"), format!("{b:?}"));
    let at = x.bytes().zip(y.bytes()).position(|(p, q)| p != q).unwrap_or(0);
    let from = at.saturating_sub(300);
    let cut = |s: &str| s.get(from..(at + 300).min(s.len())).unwrap_or("").to_owned();
    panic!("{what}\n  relu    : …{}…\n  attendu : …{}…", cut(&x), cut(&y));
}

/// Vérifie P24 et P43 pour une graine.
fn check(seed: u64) {
    let p = generate::project(seed);
    if p.validate()
        .finish()
        .iter()
        .any(|d| d.severity() == Severity::Error)
    {
        return;
    }
    let sched = schedule(&p);
    for (profile, computed) in [
        (OrderProfile::Observed, Computed::Schedule(&sched)),
        (OrderProfile::Strict, Computed::Omit),
    ] {
        let options = WriteOptions {
            profile,
            ..WriteOptions::default()
        };
        let out = write(&p, computed, &options);
        assert_eq!(
            write(&p, computed, &options).xml,
            out.xml,
            "{seed} : écriture non déterministe"
        );
        let back = read(&out.xml, &opts()).unwrap_or_else(|e| panic!("{seed} {profile:?} : {e}"));
        let image = match computed {
            Computed::Schedule(s) => schedule_image(&p, s),
            _ => p.clone(),
        };
        let expected = normalize(&image, &options);
        same(
            &back.project,
            &expected,
            &format!("{seed} {profile:?} : image de l'aller-retour"),
        );
        same(
            &normalize(&expected, &options),
            &expected,
            &format!("{seed} {profile:?} : normalize non idempotente"),
        );
    }
}

#[test]
fn p43_progress_round_trip_fixed_seeds() {
    // Graines dont le projet a une feuille en cours avec reprise.
    let mut n = 0;
    for seed in 0..400 {
        let p = generate::project(seed);
        if p.tasks
            .iter()
            .any(|t| t.progress.resume.is_some() && t.progress.actual_finish.is_none())
        {
            check(seed);
            n += 1;
        }
    }
    assert!(n > 20, "{n}");
}

#[test]
fn p24_fixed_seeds() {
    for seed in 0..64 {
        check(seed);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]
    #[test]
    fn p24_round_trip(seed in any::<u64>()) {
        check(seed);
    }
}
