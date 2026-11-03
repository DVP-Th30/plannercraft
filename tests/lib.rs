//! Outils communs aux harnais de tests transverses de PlannerCraft.
//!
//! Ce paquet (`plannercraft-tests`, non publié) regroupe trois cibles de test :
//! - `rules` (`tests/rules/`) : un test par règle de calcul `R-xxx` ;
//! - `oracle` (`tests/oracle/`) : comparaison avec des fichiers recalculés par
//!   MS Project, quand une licence sera disponible (ADR-0004) ;
//! - `corpus` (`tests/corpus/`) : fichiers MSPDI publics, recalculés et
//!   comparés aux valeurs enregistrées par MS Project ; le harnais vit dans
//!   [`corpus`] (manifeste, comparaison, verdict, rapport).
//!
//! Le module [`net`] construit de petits réseaux nommés pour les tests du
//! moteur (`Net`, `Run`) ; [`generate`] tire de grands réseaux déterministes
//! pour la mesure (`tests/perf`, `tests/benches`).

#![allow(
    clippy::panic,
    reason = "paquet de test : une aide qui reçoit une donnée de test invalide doit faire échouer le test"
)]

use std::path::{Path, PathBuf};

pub mod corpus;
pub mod daily;
pub mod generate;
pub mod mspdi;
pub mod net;
pub mod resource_edit;
pub mod session;
pub mod task_edit;

/// Dossier `tests/` du dépôt, quel que soit le répertoire courant.
#[must_use]
pub fn tests_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Chemin d'un fichier ou dossier relatif à `tests/`.
#[must_use]
pub fn tests_path(relative: &str) -> PathBuf {
    tests_dir().join(relative)
}

/// Instant depuis `"2026-10-19T08:00"` ; panique sur une écriture invalide
/// (aide de test).
///
/// # Panics
///
/// Si le texte n'est pas un instant valide du domaine.
#[must_use]
pub fn moment(s: &str) -> plannercraft_core::Moment {
    match s.parse() {
        Ok(m) => m,
        Err(e) => panic!("instant de test invalide « {s} » : {e}"),
    }
}

/// Jour depuis `"2026-12-25"` ; panique sur une écriture invalide (aide de test).
///
/// # Panics
///
/// Si le texte n'est pas un jour valide du domaine.
#[must_use]
pub fn day(s: &str) -> plannercraft_core::Day {
    match s.parse() {
        Ok(d) => d,
        Err(e) => panic!("jour de test invalide « {s} » : {e}"),
    }
}

/// Heure depuis `"08:00"` ; panique sur une écriture invalide (aide de test).
///
/// # Panics
///
/// Si le texte n'est pas une heure valide.
#[must_use]
pub fn time(s: &str) -> plannercraft_core::TimeOfDay {
    match s.parse() {
        Ok(t) => t,
        Err(e) => panic!("heure de test invalide « {s} » : {e}"),
    }
}

/// Plages d'un jour depuis `[("08:00", "12:00"), ("13:00", "17:00")]`.
///
/// # Panics
///
/// Si une plage ou l'ensemble des plages est invalide.
#[must_use]
pub fn hours(ranges: &[(&str, &str)]) -> plannercraft_core::calendar::DayHours {
    let ranges = ranges
        .iter()
        .map(
            |(a, b)| match plannercraft_core::calendar::TimeRange::new(time(a), time(b)) {
                Ok(r) => r,
                Err(e) => panic!("plage de test invalide {a}-{b} : {e}"),
            },
        )
        .collect();
    match plannercraft_core::calendar::DayHours::new(ranges) {
        Ok(h) => h,
        Err(e) => panic!("plages de test invalides : {e}"),
    }
}

/// Jour ouvré avec ces plages.
///
/// # Panics
///
/// Voir [`hours`].
#[must_use]
pub fn working(ranges: &[(&str, &str)]) -> plannercraft_core::calendar::DaySetting {
    plannercraft_core::calendar::DaySetting::Working(hours(ranges))
}

/// Intervalle de jours `[from, to]`.
///
/// # Panics
///
/// Si un jour est invalide ou si `from > to`.
#[must_use]
pub fn days(from: &str, to: &str) -> plannercraft_core::DayRange {
    match plannercraft_core::DayRange::new(day(from), day(to)) {
        Ok(r) => r,
        Err(e) => panic!("intervalle de test invalide {from} → {to} : {e}"),
    }
}

/// Exception non récurrente (`Period`) sur `[from, to]`.
#[must_use]
pub fn exception(
    from: &str,
    to: &str,
    setting: plannercraft_core::calendar::DaySetting,
) -> plannercraft_core::calendar::CalendarException {
    plannercraft_core::calendar::CalendarException {
        name: format!("{from} → {to}"),
        period: days(from, to),
        setting,
        recurrence: plannercraft_core::calendar::Recurrence::Period,
        occurrences: None,
    }
}

/// Collection de calendriers.
///
/// # Panics
///
/// Si deux calendriers ont le même UID.
#[must_use]
pub fn calendars(
    list: Vec<plannercraft_core::calendar::Calendar>,
) -> plannercraft_core::Arena<plannercraft_core::CalendarUid, plannercraft_core::calendar::Calendar> {
    match plannercraft_core::Arena::try_from(list) {
        Ok(a) => a,
        Err(e) => panic!("calendriers de test invalides : {e}"),
    }
}

/// Calendrier Standard, UID 1.
#[must_use]
pub fn standard() -> plannercraft_core::calendar::Calendar {
    plannercraft_core::calendar::standard(plannercraft_core::CalendarUid::new(1), "Standard".to_owned())
}

/// Minutes ouvrées du jour `d` du calendrier `uid`, par la résolution froide
/// (`resolve_day`).
///
/// # Panics
///
/// Si la résolution échoue.
#[must_use]
pub fn day_minutes(
    calendars: &plannercraft_core::Arena<
        plannercraft_core::CalendarUid,
        plannercraft_core::calendar::Calendar,
    >,
    uid: u32,
    d: &str,
) -> i64 {
    match plannercraft_core::calendar::resolve_day(
        calendars,
        plannercraft_core::CalendarUid::new(uid),
        day(d),
    ) {
        Ok(r) => r.hours.total().get(),
        Err(e) => panic!("résolution du {d} impossible : {e}"),
    }
}

/// Calendrier `uid` compilé.
///
/// # Panics
///
/// Si la compilation échoue.
#[must_use]
pub fn compiled(
    calendars: &plannercraft_core::Arena<
        plannercraft_core::CalendarUid,
        plannercraft_core::calendar::Calendar,
    >,
    uid: u32,
) -> plannercraft_core::calendar::CompiledCalendar {
    match plannercraft_core::calendar::CompiledCalendar::compile(
        calendars,
        plannercraft_core::CalendarUid::new(uid),
    ) {
        Ok(c) => c,
        Err(e) => panic!("compilation du calendrier {uid} impossible : {e}"),
    }
}

/// Calendrier compilé depuis un seul calendrier.
///
/// # Panics
///
/// Si la compilation échoue.
#[must_use]
pub fn compile_one(
    c: plannercraft_core::calendar::Calendar,
) -> plannercraft_core::calendar::CompiledCalendar {
    let uid = c.uid().get();
    compiled(&calendars(vec![c]), uid)
}

/// Minutes signées.
#[must_use]
pub const fn mins(v: i64) -> plannercraft_core::Minutes {
    plannercraft_core::Minutes::new(v)
}
