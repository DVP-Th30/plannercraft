//! P49 (J7, lot L8, R-240 à R-242) : sur des feuilles automatiques sans
//! lien, contrainte ni ressource (calendrier Standard, début lun
//! 19/10/2026 08:00), `bar_move(n)` puis `bar_move(−n)` rend le début
//! d'origine, `bar_resize(n)` puis `bar_resize(−n)` la durée d'origine ;
//! l'aperçu (`gesture::preview`) donne le début et la fin calculés après
//! application ; chaque geste appliqué fait une seule entrée d'annulation
//! (1 024 cas). Écarts recalés vers l'avant jusqu'à ce que la cible tombe
//! un jour ouvré (une cible chômée est recalée par le moteur, R-090 :
//! l'aller-retour ne rend alors pas l'origine, par construction) ; pas de
//! `prop_assume` : avec `PROPTEST_CASES=10000` (CI), les rejets dépassaient
//! le plafond global de proptest. Déplacements vers l'avant seulement (rien
//! ne passe avant le début du projet, R-036).

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_automation::gesture::{self, GestureKind};
use plannercraft_automation::{AnchorSpec, ApplyMode, Batch, NewProject, Session};
use plannercraft_core::calendar::CalendarSet;
use plannercraft_core::{Minutes, Moment, TaskUid, Weekday};
use proptest::prelude::*;
use serde_json::json;

fn now() -> Moment {
    "2026-10-19T08:00".parse().unwrap()
}

/// Session neuve avec une feuille par durée (texte « Nh » ou « Nj »).
fn session(durations: &[String]) -> Session {
    let init = NewProject {
        name: "P49".to_owned(),
        anchor: AnchorSpec::Start(now()),
        lang: None,
    };
    let mut s = Session::new_project(init, now()).unwrap();
    let ops: Vec<_> = durations
        .iter()
        .enumerate()
        .map(|(i, d)| json!({ "op": "task.add", "name": format!("T{i}"), "duration": { "text": d } }))
        .collect();
    let batch = Batch::from_json(json!({ "format_version": 1, "lang": "fr", "ops": ops })).unwrap();
    s.apply(&batch, ApplyMode::Commit).unwrap();
    s
}

fn dates(s: &Session, uid: TaskUid) -> (Moment, Moment) {
    let t = s.schedule().scheduled(uid).unwrap();
    (t.start, t.finish)
}

fn duration(s: &Session, uid: TaskUid) -> Minutes {
    s.project().tasks.get(uid).unwrap().duration.minutes()
}

fn working_day(t: Moment) -> bool {
    !matches!(t.weekday(), Weekday::Saturday | Weekday::Sunday)
}

fn plus_days(t: Moment, n: i64) -> Moment {
    t.checked_add(Minutes::new(n * 1_440)).unwrap()
}

/// Applique `b` et vérifie qu'il fait une seule entrée d'annulation.
fn commit(s: &mut Session, b: &Batch) {
    let before = s.history().entries.len();
    s.apply(b, ApplyMode::Commit).unwrap();
    assert_eq!(s.history().entries.len(), before + 1);
}

/// Premier écart `≥ n` qui satisfait `ok` (recalage vers l'avant au lieu
/// d'un rejet : la boucle finit, une cible ouvrée est au plus 2 jours plus
/// loin et `target > start` devient vrai pour un écart assez grand).
fn next_working_offset(mut n: i64, ok: impl Fn(i64) -> bool) -> i64 {
    while !ok(n) {
        n += 1;
    }
    n
}

/// Durée en texte : heures (1 à 60 h) ou jours (1 à 10 j).
fn duration_text() -> impl Strategy<Value = String> {
    prop_oneof![
        (1_u32..=60).prop_map(|h| format!("{h}h")),
        (1_u32..=10).prop_map(|d| format!("{d}j")),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_024))]

    #[test]
    fn p49_move_round_trip(
        durations in prop::collection::vec(duration_text(), 1..5),
        pick in 0_usize..8,
        n in 1_i64..=40,
    ) {
        let mut s = session(&durations);
        let uid = TaskUid::new(u32::try_from(pick % durations.len()).unwrap() + 1);
        let (start, _) = dates(&s, uid);
        let n = next_working_offset(n, |n| working_day(plus_days(start, n)));
        let set = CalendarSet::compile(s.project()).0;
        let p = gesture::preview(&s, &set, uid, GestureKind::Move, n).unwrap();
        let b = gesture::bar_move(&s, uid, n).unwrap().unwrap();
        commit(&mut s, &b);
        prop_assert_eq!((p.start, p.finish), dates(&s, uid));
        let back = gesture::bar_move(&s, uid, -n).unwrap().unwrap();
        commit(&mut s, &back);
        prop_assert_eq!(dates(&s, uid).0, start);
    }

    #[test]
    fn p49_resize_round_trip(
        durations in prop::collection::vec(duration_text(), 1..5),
        pick in 0_usize..8,
        n in -12_i64..=20,
    ) {
        let mut s = session(&durations);
        let uid = TaskUid::new(u32::try_from(pick % durations.len()).unwrap() + 1);
        let (start, finish) = dates(&s, uid);
        let n = next_working_offset(n, |n| {
            let target = plus_days(finish, n);
            n != 0 && target > start && working_day(target)
        });
        let target = plus_days(finish, n);
        let original = duration(&s, uid);
        let set = CalendarSet::compile(s.project()).0;
        let p = gesture::preview(&s, &set, uid, GestureKind::Resize, n).unwrap();
        let b = gesture::bar_resize(&s, &set, uid, n).unwrap().unwrap();
        commit(&mut s, &b);
        prop_assert_eq!((p.start, p.finish), dates(&s, uid));
        prop_assert_eq!(p.duration, Some(duration(&s, uid)));
        prop_assert_eq!(dates(&s, uid).1, target);
        let set = CalendarSet::compile(s.project()).0;
        let b = gesture::bar_resize(&s, &set, uid, -n).unwrap().unwrap();
        commit(&mut s, &b);
        prop_assert_eq!(duration(&s, uid), original);
        prop_assert_eq!(dates(&s, uid), (start, finish));
    }
}
