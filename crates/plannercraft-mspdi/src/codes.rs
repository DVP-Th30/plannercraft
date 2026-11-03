//! Tables des codes numériques MSPDI → types du cœur (ADR-0008 : aucun code
//! MSPDI dans `plannercraft-core`).
//!
//! Source : schéma `mspdi_pj15.xsd` (Project 2013 SDK) et pages
//! learn.microsoft.com « XML Schema for the Tasks / Calendars / Project
//! Element » (S1 à S4). Une fonction par table ; un code hors liste rend
//! `None`, que l'appelant signale (`mspdi.unknown_code`) avant d'appliquer la
//! valeur par défaut.

use plannercraft_core::calendar::{MonthItem, MonthPosition, Recurrence};
use plannercraft_core::{
    DurationUnit, LinkType, Month, RateUnit, TaskType, TimeBasis, Week, Weekday, WorkContour,
};

/// Écart entre un code de durée et sa variante estimée (« ? ») : 32.
const ESTIMATED_OFFSET: i64 = 32;

/// `PredecessorLink/Type` : 0 FF, 1 FS, 2 SF, 3 SS (S4).
#[must_use]
pub const fn link_type(code: i64) -> Option<LinkType> {
    match code {
        0 => Some(LinkType::FinishToFinish),
        1 => Some(LinkType::FinishToStart),
        2 => Some(LinkType::StartToFinish),
        3 => Some(LinkType::StartToStart),
        _ => None,
    }
}

/// Format d'une durée ou d'un décalage (`DurationFormat`, `LagFormat`, S3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationCode {
    /// Durée dans une unité de temps.
    Time {
        /// Unité d'affichage.
        unit: DurationUnit,
        /// Ouvrée ou écoulée.
        basis: TimeBasis,
        /// Variante estimée (« ? »).
        estimated: bool,
    },
    /// Pourcentage (`%`, `e%`) : n'a de sens que pour un décalage.
    Percent {
        /// Ouvré (`%`) ou écoulé (`e%`).
        basis: TimeBasis,
        /// Variante estimée.
        estimated: bool,
    },
    /// Format nul (21, 53) : unité par défaut du projet.
    Null {
        /// Variante estimée.
        estimated: bool,
    },
}

/// `DurationFormat` / `LagFormat` : 3 m, 4 em, 5 h, 6 eh, 7 j, 8 ej, 9 sem,
/// 10 esem, 11 mois, 12 emois, 19 %, 20 e%, 21 nul ; + 32 pour la variante
/// estimée (35 à 44, 51 à 53).
#[must_use]
pub fn duration_format(code: i64) -> Option<DurationCode> {
    let (base, estimated) = if (35..=53).contains(&code) {
        (code.saturating_sub(ESTIMATED_OFFSET), true)
    } else {
        (code, false)
    };
    let time = |unit, basis| {
        Some(DurationCode::Time {
            unit,
            basis,
            estimated,
        })
    };
    match base {
        3 => time(DurationUnit::Minute, TimeBasis::Working),
        4 => time(DurationUnit::Minute, TimeBasis::Elapsed),
        5 => time(DurationUnit::Hour, TimeBasis::Working),
        6 => time(DurationUnit::Hour, TimeBasis::Elapsed),
        7 => time(DurationUnit::Day, TimeBasis::Working),
        8 => time(DurationUnit::Day, TimeBasis::Elapsed),
        9 => time(DurationUnit::Week, TimeBasis::Working),
        10 => time(DurationUnit::Week, TimeBasis::Elapsed),
        11 => time(DurationUnit::Month, TimeBasis::Working),
        12 => time(DurationUnit::Month, TimeBasis::Elapsed),
        19 => Some(DurationCode::Percent {
            basis: TimeBasis::Working,
            estimated,
        }),
        20 => Some(DurationCode::Percent {
            basis: TimeBasis::Elapsed,
            estimated,
        }),
        21 => Some(DurationCode::Null { estimated }),
        _ => None,
    }
}

/// R-198 : `LevelingDelayFormat` (codes de [`duration_format`], S3) →
/// (unité, écoulé). Seuls les formats de temps conviennent (7 = j, 8 = ej,
/// variantes estimées comprises, l'estimation n'étant pas gardée) ; un
/// pourcentage ou le format nul rendent `None`.
#[must_use]
pub fn leveling_delay_format(code: i64) -> Option<(DurationUnit, bool)> {
    match duration_format(code)? {
        DurationCode::Time { unit, basis, .. } => Some((unit, basis == TimeBasis::Elapsed)),
        DurationCode::Percent { .. } | DurationCode::Null { .. } => None,
    }
}

/// Type de contrainte, sans sa date : défini dans le cœur (ADR-0010), les
/// codes `ConstraintType` (0 à 7) restent ici (ADR-0008).
pub use plannercraft_core::ConstraintKind;

/// `ConstraintType` : 0 ASAP, 1 ALAP, 2 MSO, 3 MFO, 4 SNET, 5 SNLT, 6 FNET,
/// 7 FNLT.
#[must_use]
pub const fn constraint_type(code: i64) -> Option<ConstraintKind> {
    match code {
        0 => Some(ConstraintKind::AsSoonAsPossible),
        1 => Some(ConstraintKind::AsLateAsPossible),
        2 => Some(ConstraintKind::MustStartOn),
        3 => Some(ConstraintKind::MustFinishOn),
        4 => Some(ConstraintKind::StartNoEarlierThan),
        5 => Some(ConstraintKind::StartNoLaterThan),
        6 => Some(ConstraintKind::FinishNoEarlierThan),
        7 => Some(ConstraintKind::FinishNoLaterThan),
        _ => None,
    }
}

/// `Type` d'une tâche et `DefaultTaskType` : 0 unités fixes, 1 durée fixe,
/// 2 travail fixe.
#[must_use]
pub const fn task_type(code: i64) -> Option<TaskType> {
    match code {
        0 => Some(TaskType::FixedUnits),
        1 => Some(TaskType::FixedDuration),
        2 => Some(TaskType::FixedWork),
        _ => None,
    }
}

/// `WeekDay/DayType` : un jour de semaine, ou l'ancienne forme d'exception.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayType {
    /// Jour de la semaine par défaut.
    Weekday(Weekday),
    /// 0 : exception datée, ancienne forme (R-081).
    LegacyException,
}

/// `DayType` : 0 ancienne exception, 1 dimanche … 7 samedi (S1).
#[must_use]
pub const fn day_type(code: i64) -> Option<DayType> {
    match code {
        0 => Some(DayType::LegacyException),
        1 => Some(DayType::Weekday(Weekday::Sunday)),
        2 => Some(DayType::Weekday(Weekday::Monday)),
        3 => Some(DayType::Weekday(Weekday::Tuesday)),
        4 => Some(DayType::Weekday(Weekday::Wednesday)),
        5 => Some(DayType::Weekday(Weekday::Thursday)),
        6 => Some(DayType::Weekday(Weekday::Friday)),
        7 => Some(DayType::Weekday(Weekday::Saturday)),
        _ => None,
    }
}

/// `WeekStartDay` : 0 dimanche … 6 samedi (S2).
#[must_use]
pub const fn week_start_day(code: i64) -> Option<Weekday> {
    match code {
        0 => Some(Weekday::Sunday),
        1 => Some(Weekday::Monday),
        2 => Some(Weekday::Tuesday),
        3 => Some(Weekday::Wednesday),
        4 => Some(Weekday::Thursday),
        5 => Some(Weekday::Friday),
        6 => Some(Weekday::Saturday),
        _ => None,
    }
}

/// Type d'une exception de calendrier (`Exception/Type`, 1 à 9, S1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExceptionType {
    /// 1 : tous les `Period` jours.
    Daily,
    /// 2 : chaque année, le `MonthDay` du mois `Month`.
    YearlyByDay,
    /// 3 : chaque année, le n-ième `MonthItem` du mois `Month`.
    YearlyByPosition,
    /// 4 : tous les `Period` mois, le `MonthDay`.
    MonthlyByDay,
    /// 5 : tous les `Period` mois, le n-ième `MonthItem`.
    MonthlyByPosition,
    /// 6 : toutes les `Period` semaines, les jours de `DaysOfWeek`.
    Weekly,
    /// 7 : sémantique non documentée.
    ByDayCount,
    /// 8 : sémantique non documentée.
    ByWeekdayCount,
    /// 9 : chaque jour de la période.
    Period,
}

/// `Exception/Type` : 1 à 9.
#[must_use]
pub const fn exception_type(code: i64) -> Option<ExceptionType> {
    match code {
        1 => Some(ExceptionType::Daily),
        2 => Some(ExceptionType::YearlyByDay),
        3 => Some(ExceptionType::YearlyByPosition),
        4 => Some(ExceptionType::MonthlyByDay),
        5 => Some(ExceptionType::MonthlyByPosition),
        6 => Some(ExceptionType::Weekly),
        7 => Some(ExceptionType::ByDayCount),
        8 => Some(ExceptionType::ByWeekdayCount),
        9 => Some(ExceptionType::Period),
        _ => None,
    }
}

/// `Month` : 0 janvier … 11 décembre.
#[must_use]
pub const fn month(code: i64) -> Option<Month> {
    match code {
        0 => Some(Month::January),
        1 => Some(Month::February),
        2 => Some(Month::March),
        3 => Some(Month::April),
        4 => Some(Month::May),
        5 => Some(Month::June),
        6 => Some(Month::July),
        7 => Some(Month::August),
        8 => Some(Month::September),
        9 => Some(Month::October),
        10 => Some(Month::November),
        11 => Some(Month::December),
        _ => None,
    }
}

/// `MonthItem` : 0 jour, 1 jour ouvré, 2 jour de week-end, 3 dimanche …
/// 9 samedi.
#[must_use]
pub const fn month_item(code: i64) -> Option<MonthItem> {
    match code {
        0 => Some(MonthItem::Day),
        1 => Some(MonthItem::Weekday),
        2 => Some(MonthItem::WeekendDay),
        3 => Some(MonthItem::On(Weekday::Sunday)),
        4 => Some(MonthItem::On(Weekday::Monday)),
        5 => Some(MonthItem::On(Weekday::Tuesday)),
        6 => Some(MonthItem::On(Weekday::Wednesday)),
        7 => Some(MonthItem::On(Weekday::Thursday)),
        8 => Some(MonthItem::On(Weekday::Friday)),
        9 => Some(MonthItem::On(Weekday::Saturday)),
        _ => None,
    }
}

/// `MonthPosition` : 0 premier, 1 deuxième, 2 troisième, 3 quatrième,
/// 4 dernier.
#[must_use]
pub const fn month_position(code: i64) -> Option<MonthPosition> {
    match code {
        0 => Some(MonthPosition::First),
        1 => Some(MonthPosition::Second),
        2 => Some(MonthPosition::Third),
        3 => Some(MonthPosition::Fourth),
        4 => Some(MonthPosition::Last),
        _ => None,
    }
}

/// `DaysOfWeek` : masque de bits, 1 = dimanche, 2 = lundi … 64 = samedi.
/// `None` pour un masque négatif ou portant un bit au-delà de 64 (> 127).
#[must_use]
pub const fn days_of_week(mask: i64) -> Option<Week<bool>> {
    if mask < 0 || mask > 127 {
        return None;
    }
    Some(Week {
        sunday: mask & 1 != 0,
        monday: mask & 2 != 0,
        tuesday: mask & 4 != 0,
        wednesday: mask & 8 != 0,
        thursday: mask & 16 != 0,
        friday: mask & 32 != 0,
        saturday: mask & 64 != 0,
    })
}

/// `WorkFormat` : 1 m, 2 h, 3 j, 4 sem, 5 mois.
#[must_use]
pub const fn work_format(code: i64) -> Option<DurationUnit> {
    match code {
        1 => Some(DurationUnit::Minute),
        2 => Some(DurationUnit::Hour),
        3 => Some(DurationUnit::Day),
        4 => Some(DurationUnit::Week),
        5 => Some(DurationUnit::Month),
        _ => None,
    }
}

/// Type de ressource MSPDI (`Resource/Type`, R-124) : 0 matérielle,
/// 1 travail ; 2 (coût) n'est pas dans pj15 mais se lit (le drapeau
/// `IsCostResource` l'emporte, voir `resource.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceTypeCode {
    /// 0 : matérielle.
    Material,
    /// 1 : travail.
    Work,
    /// 2 : coût.
    Cost,
}

/// `Resource/Type` : 0, 1, 2.
#[must_use]
pub const fn resource_type(code: i64) -> Option<ResourceTypeCode> {
    match code {
        0 => Some(ResourceTypeCode::Material),
        1 => Some(ResourceTypeCode::Work),
        2 => Some(ResourceTypeCode::Cost),
        _ => None,
    }
}

/// `StandardRateFormat`, `OvertimeRateFormat` (R-124) : unité d'affichage
/// du taux — 1 minute, 2 heure, 3 jour, 4 semaine, 5 mois, 7 an,
/// 8 matériel (unité de la ressource matérielle).
#[must_use]
pub const fn rate_format(code: i64) -> Option<RateUnit> {
    match code {
        1 => Some(RateUnit::Minute),
        2 => Some(RateUnit::Hour),
        3 => Some(RateUnit::Day),
        4 => Some(RateUnit::Week),
        5 => Some(RateUnit::Month),
        7 => Some(RateUnit::Year),
        8 => Some(RateUnit::MaterialUnit),
        _ => None,
    }
}

/// `WorkContour` (R-125) : 0 uniforme, 1 croissant, 2 décroissant, 3 deux
/// pics, 4 pic au début, 5 pic à la fin, 6 cloche, 7 tortue, 8 modifié.
#[must_use]
pub const fn work_contour(code: i64) -> Option<WorkContour> {
    match code {
        0 => Some(WorkContour::Flat),
        1 => Some(WorkContour::BackLoaded),
        2 => Some(WorkContour::FrontLoaded),
        3 => Some(WorkContour::DoublePeak),
        4 => Some(WorkContour::EarlyPeak),
        5 => Some(WorkContour::LatePeak),
        6 => Some(WorkContour::Bell),
        7 => Some(WorkContour::Turtle),
        8 => Some(WorkContour::Contoured),
        _ => None,
    }
}

// --- Tables inverses (écriture, R-120) -------------------------------------

/// `DurationFormat` / `LagFormat` d'une durée : code de (unité, base),
/// + 32 si estimée (inverse de [`duration_format`]).
#[must_use]
pub const fn duration_format_code(unit: DurationUnit, basis: TimeBasis, estimated: bool) -> i64 {
    let base: i64 = match (unit, basis) {
        (DurationUnit::Minute, TimeBasis::Working) => 3,
        (DurationUnit::Minute, TimeBasis::Elapsed) => 4,
        (DurationUnit::Hour, TimeBasis::Working) => 5,
        (DurationUnit::Hour, TimeBasis::Elapsed) => 6,
        (DurationUnit::Day, TimeBasis::Working) => 7,
        (DurationUnit::Day, TimeBasis::Elapsed) => 8,
        (DurationUnit::Week, TimeBasis::Working) => 9,
        (DurationUnit::Week, TimeBasis::Elapsed) => 10,
        (DurationUnit::Month, TimeBasis::Working) => 11,
        (DurationUnit::Month, TimeBasis::Elapsed) => 12,
    };
    if estimated {
        base.saturating_add(ESTIMATED_OFFSET)
    } else {
        base
    }
}

/// R-198 : `LevelingDelayFormat` d'un retard de nivellement (inverse de
/// [`leveling_delay_format`], jamais estimé).
#[must_use]
pub const fn leveling_delay_format_code(unit: DurationUnit, elapsed: bool) -> i64 {
    let basis = if elapsed {
        TimeBasis::Elapsed
    } else {
        TimeBasis::Working
    };
    duration_format_code(unit, basis, false)
}

/// `LagFormat` d'un décalage en pourcentage : 19 (`%`), 20 (`e%`).
#[must_use]
pub const fn percent_lag_format_code(basis: TimeBasis) -> i64 {
    match basis {
        TimeBasis::Working => 19,
        TimeBasis::Elapsed => 20,
    }
}

/// `PredecessorLink/Type` (inverse de [`link_type`]).
#[must_use]
pub const fn link_type_code(kind: LinkType) -> i64 {
    match kind {
        LinkType::FinishToFinish => 0,
        LinkType::FinishToStart => 1,
        LinkType::StartToFinish => 2,
        LinkType::StartToStart => 3,
    }
}

/// `ConstraintType` (inverse de [`constraint_type`]).
#[must_use]
pub const fn constraint_code(kind: ConstraintKind) -> i64 {
    match kind {
        ConstraintKind::AsSoonAsPossible => 0,
        ConstraintKind::AsLateAsPossible => 1,
        ConstraintKind::MustStartOn => 2,
        ConstraintKind::MustFinishOn => 3,
        ConstraintKind::StartNoEarlierThan => 4,
        ConstraintKind::StartNoLaterThan => 5,
        ConstraintKind::FinishNoEarlierThan => 6,
        ConstraintKind::FinishNoLaterThan => 7,
    }
}

/// `Type` d'une tâche, `DefaultTaskType` (inverse de [`task_type`]).
#[must_use]
pub const fn task_type_code(t: TaskType) -> i64 {
    match t {
        TaskType::FixedUnits => 0,
        TaskType::FixedDuration => 1,
        TaskType::FixedWork => 2,
    }
}

/// `DayType` d'un jour de la semaine : 1 dimanche … 7 samedi.
#[must_use]
pub const fn day_type_code(d: Weekday) -> i64 {
    match d {
        Weekday::Sunday => 1,
        Weekday::Monday => 2,
        Weekday::Tuesday => 3,
        Weekday::Wednesday => 4,
        Weekday::Thursday => 5,
        Weekday::Friday => 6,
        Weekday::Saturday => 7,
    }
}

/// `WeekStartDay` : 0 dimanche … 6 samedi.
#[must_use]
pub const fn week_start_code(d: Weekday) -> i64 {
    match d {
        Weekday::Sunday => 0,
        Weekday::Monday => 1,
        Weekday::Tuesday => 2,
        Weekday::Wednesday => 3,
        Weekday::Thursday => 4,
        Weekday::Friday => 5,
        Weekday::Saturday => 6,
    }
}

/// `WorkFormat` : 1 m, 2 h, 3 j, 4 sem, 5 mois.
#[must_use]
pub const fn work_format_code(unit: DurationUnit) -> i64 {
    match unit {
        DurationUnit::Minute => 1,
        DurationUnit::Hour => 2,
        DurationUnit::Day => 3,
        DurationUnit::Week => 4,
        DurationUnit::Month => 5,
    }
}

/// `Month` : 0 janvier … 11 décembre.
#[must_use]
pub const fn month_code(m: Month) -> i64 {
    match m {
        Month::January => 0,
        Month::February => 1,
        Month::March => 2,
        Month::April => 3,
        Month::May => 4,
        Month::June => 5,
        Month::July => 6,
        Month::August => 7,
        Month::September => 8,
        Month::October => 9,
        Month::November => 10,
        Month::December => 11,
    }
}

/// `MonthItem` (inverse de [`month_item`]).
#[must_use]
pub const fn month_item_code(item: MonthItem) -> i64 {
    match item {
        MonthItem::Day => 0,
        MonthItem::Weekday => 1,
        MonthItem::WeekendDay => 2,
        MonthItem::On(d) => match d {
            Weekday::Sunday => 3,
            Weekday::Monday => 4,
            Weekday::Tuesday => 5,
            Weekday::Wednesday => 6,
            Weekday::Thursday => 7,
            Weekday::Friday => 8,
            Weekday::Saturday => 9,
        },
    }
}

/// `MonthPosition` (inverse de [`month_position`]).
#[must_use]
pub const fn month_position_code(p: MonthPosition) -> i64 {
    match p {
        MonthPosition::First => 0,
        MonthPosition::Second => 1,
        MonthPosition::Third => 2,
        MonthPosition::Fourth => 3,
        MonthPosition::Last => 4,
    }
}

/// `DaysOfWeek` : masque, 1 = dimanche … 64 = samedi (inverse de
/// [`days_of_week`]).
#[must_use]
pub const fn days_of_week_mask(w: &Week<bool>) -> i64 {
    let mut mask = 0;
    if w.sunday {
        mask |= 1;
    }
    if w.monday {
        mask |= 2;
    }
    if w.tuesday {
        mask |= 4;
    }
    if w.wednesday {
        mask |= 8;
    }
    if w.thursday {
        mask |= 16;
    }
    if w.friday {
        mask |= 32;
    }
    if w.saturday {
        mask |= 64;
    }
    mask
}

/// `StandardRateFormat` / `OvertimeRateFormat` (inverse de
/// [`rate_format`]).
#[must_use]
pub const fn rate_format_code(unit: RateUnit) -> i64 {
    match unit {
        RateUnit::Minute => 1,
        RateUnit::Hour => 2,
        RateUnit::Day => 3,
        RateUnit::Week => 4,
        RateUnit::Month => 5,
        RateUnit::Year => 7,
        RateUnit::MaterialUnit => 8,
    }
}

/// `WorkContour` (inverse de [`work_contour`]).
#[must_use]
pub const fn work_contour_code(c: WorkContour) -> i64 {
    match c {
        WorkContour::Flat => 0,
        WorkContour::BackLoaded => 1,
        WorkContour::FrontLoaded => 2,
        WorkContour::DoublePeak => 3,
        WorkContour::EarlyPeak => 4,
        WorkContour::LatePeak => 5,
        WorkContour::Bell => 6,
        WorkContour::Turtle => 7,
        WorkContour::Contoured => 8,
    }
}

/// `Exception/Type` d'une récurrence ; `Period` (chaque jour de la
/// période) s'écrit `1` (quotidienne de pas 1, forme de MS Project pour un
/// férié, D15 ; relue `Daily { every: 1 }`, mêmes jours).
#[must_use]
pub const fn exception_type_code(r: &Recurrence) -> i64 {
    match r {
        Recurrence::Period | Recurrence::Daily { .. } => 1,
        Recurrence::YearlyByDay { .. } => 2,
        Recurrence::YearlyByPosition { .. } => 3,
        Recurrence::MonthlyByDay { .. } => 4,
        Recurrence::MonthlyByPosition { .. } => 5,
        Recurrence::Weekly { .. } => 6,
        Recurrence::ByDayCount { .. } => 7,
        Recurrence::ByWeekdayCount { .. } => 8,
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{
        ConstraintKind, DayType, DurationCode, ExceptionType, constraint_type, day_type, days_of_week,
        duration_format, exception_type, leveling_delay_format, leveling_delay_format_code, link_type, month,
        month_item, month_position, task_type, week_start_day, work_format,
    };
    use plannercraft_core::calendar::{MonthItem, MonthPosition};
    use plannercraft_core::{DurationUnit, LinkType, Month, TaskType, TimeBasis, Weekday};

    /// Valeurs voisines de chaque table, hors liste : aucune ne doit passer.
    const OUT: [i64; 6] = [-1, 64, 100, i64::MIN, i64::MAX, 1_000];

    #[test]
    fn link_types() {
        assert_eq!(link_type(0), Some(LinkType::FinishToFinish));
        assert_eq!(link_type(1), Some(LinkType::FinishToStart));
        assert_eq!(link_type(2), Some(LinkType::StartToFinish));
        assert_eq!(link_type(3), Some(LinkType::StartToStart));
        assert_eq!(link_type(4), None);
        assert!(OUT.iter().all(|c| link_type(*c).is_none()));
    }

    #[test]
    fn r198_leveling_delay_formats() {
        assert_eq!(leveling_delay_format(7), Some((DurationUnit::Day, false)));
        assert_eq!(leveling_delay_format(8), Some((DurationUnit::Day, true)));
        assert_eq!(leveling_delay_format(39), Some((DurationUnit::Day, false)));
        assert_eq!(leveling_delay_format(5), Some((DurationUnit::Hour, false)));
        assert_eq!(leveling_delay_format(19), None);
        assert_eq!(leveling_delay_format(21), None);
        assert!(OUT.iter().all(|c| leveling_delay_format(*c).is_none()));
        for code in (3..=12).chain(35..=44) {
            let (unit, elapsed) = leveling_delay_format(code).unwrap();
            let back = leveling_delay_format_code(unit, elapsed);
            assert_eq!(leveling_delay_format(back), Some((unit, elapsed)));
            assert!(back <= 12, "{code} → {back}");
        }
    }

    #[test]
    fn duration_formats_exhaustive() {
        use DurationUnit::{Day, Hour, Minute, Month as Mo, Week};
        use TimeBasis::{Elapsed as E, Working as W};
        let time = [
            (3, Minute, W),
            (4, Minute, E),
            (5, Hour, W),
            (6, Hour, E),
            (7, Day, W),
            (8, Day, E),
            (9, Week, W),
            (10, Week, E),
            (11, Mo, W),
            (12, Mo, E),
        ];
        for (code, unit, basis) in time {
            for (c, estimated) in [(code, false), (code + 32, true)] {
                assert_eq!(
                    duration_format(c),
                    Some(DurationCode::Time {
                        unit,
                        basis,
                        estimated
                    }),
                    "{c}"
                );
            }
        }
        assert_eq!(
            duration_format(19),
            Some(DurationCode::Percent {
                basis: W,
                estimated: false
            })
        );
        assert_eq!(
            duration_format(52),
            Some(DurationCode::Percent {
                basis: E,
                estimated: true
            })
        );
        assert_eq!(duration_format(21), Some(DurationCode::Null { estimated: false }));
        assert_eq!(duration_format(53), Some(DurationCode::Null { estimated: true }));
        let valid: Vec<i64> = (3..=12).chain(19..=21).chain(35..=44).chain(51..=53).collect();
        for c in -5..=80 {
            assert_eq!(duration_format(c).is_some(), valid.contains(&c), "{c}");
        }
        assert!(OUT.iter().all(|c| duration_format(*c).is_none()));
    }

    #[test]
    fn constraint_and_task_types() {
        let kinds = [
            ConstraintKind::AsSoonAsPossible,
            ConstraintKind::AsLateAsPossible,
            ConstraintKind::MustStartOn,
            ConstraintKind::MustFinishOn,
            ConstraintKind::StartNoEarlierThan,
            ConstraintKind::StartNoLaterThan,
            ConstraintKind::FinishNoEarlierThan,
            ConstraintKind::FinishNoLaterThan,
        ];
        for (code, kind) in (0_i64..).zip(kinds) {
            assert_eq!(constraint_type(code), Some(kind));
        }
        assert_eq!(constraint_type(8), None);
        assert_eq!(task_type(0), Some(TaskType::FixedUnits));
        assert_eq!(task_type(1), Some(TaskType::FixedDuration));
        assert_eq!(task_type(2), Some(TaskType::FixedWork));
        assert_eq!(task_type(3), None);
        assert!(
            OUT.iter()
                .all(|c| constraint_type(*c).is_none() && task_type(*c).is_none())
        );
    }

    #[test]
    fn days_and_weeks() {
        assert_eq!(day_type(0), Some(DayType::LegacyException));
        for (code, d) in (1_i64..).zip(Weekday::ALL) {
            assert_eq!(day_type(code), Some(DayType::Weekday(d)));
        }
        assert_eq!(day_type(8), None);
        for (code, d) in (0_i64..).zip(Weekday::ALL) {
            assert_eq!(week_start_day(code), Some(d));
        }
        assert_eq!(week_start_day(7), None);
        let w = days_of_week(1 | 4 | 64).unwrap();
        assert!(w.sunday && w.tuesday && w.saturday);
        assert!(!w.monday && !w.wednesday && !w.thursday && !w.friday);
        assert_eq!(days_of_week(0).unwrap(), plannercraft_core::Week::default());
        assert!(days_of_week(127).is_some());
        assert_eq!(days_of_week(128), None);
        assert_eq!(days_of_week(-1), None);
        assert!(
            OUT.iter()
                .all(|c| day_type(*c).is_none() && week_start_day(*c).is_none())
        );
    }

    #[test]
    fn exceptions_and_months() {
        let types = [
            ExceptionType::Daily,
            ExceptionType::YearlyByDay,
            ExceptionType::YearlyByPosition,
            ExceptionType::MonthlyByDay,
            ExceptionType::MonthlyByPosition,
            ExceptionType::Weekly,
            ExceptionType::ByDayCount,
            ExceptionType::ByWeekdayCount,
            ExceptionType::Period,
        ];
        for (code, t) in (1_i64..).zip(types) {
            assert_eq!(exception_type(code), Some(t));
        }
        assert_eq!(exception_type(0), None);
        assert_eq!(exception_type(10), None);
        for (code, mo) in (0_i64..).zip(Month::ALL) {
            assert_eq!(month(code), Some(mo));
        }
        assert_eq!(month(12), None);
        assert_eq!(month_item(0), Some(MonthItem::Day));
        assert_eq!(month_item(1), Some(MonthItem::Weekday));
        assert_eq!(month_item(2), Some(MonthItem::WeekendDay));
        for (code, d) in (3_i64..).zip(Weekday::ALL) {
            assert_eq!(month_item(code), Some(MonthItem::On(d)));
        }
        assert_eq!(month_item(10), None);
        let positions = [
            MonthPosition::First,
            MonthPosition::Second,
            MonthPosition::Third,
            MonthPosition::Fourth,
            MonthPosition::Last,
        ];
        for (code, p) in (0_i64..).zip(positions) {
            assert_eq!(month_position(code), Some(p));
        }
        assert_eq!(month_position(5), None);
        assert!(OUT.iter().all(|c| {
            exception_type(*c).is_none()
                && month(*c).is_none()
                && month_item(*c).is_none()
                && month_position(*c).is_none()
        }));
    }

    #[test]
    fn work_formats() {
        let units = [
            DurationUnit::Minute,
            DurationUnit::Hour,
            DurationUnit::Day,
            DurationUnit::Week,
            DurationUnit::Month,
        ];
        for (code, u) in (1_i64..).zip(units) {
            assert_eq!(work_format(code), Some(u));
        }
        assert_eq!(work_format(0), None);
        assert_eq!(work_format(6), None);
        assert!(OUT.iter().all(|c| work_format(*c).is_none()));
    }

    #[test]
    fn r088_resource_rate_and_contour_codes() {
        use super::{ResourceTypeCode, rate_format, resource_type, work_contour};
        use plannercraft_core::{RateUnit, WorkContour};
        assert_eq!(resource_type(0), Some(ResourceTypeCode::Material));
        assert_eq!(resource_type(1), Some(ResourceTypeCode::Work));
        assert_eq!(resource_type(2), Some(ResourceTypeCode::Cost));
        assert_eq!(resource_type(3), None);
        let units = [
            (1, RateUnit::Minute),
            (2, RateUnit::Hour),
            (3, RateUnit::Day),
            (4, RateUnit::Week),
            (5, RateUnit::Month),
            (7, RateUnit::Year),
            (8, RateUnit::MaterialUnit),
        ];
        for (code, unit) in units {
            assert_eq!(rate_format(code), Some(unit));
        }
        assert_eq!(rate_format(6), None);
        assert_eq!(rate_format(0), None);
        assert_eq!(work_contour(0), Some(WorkContour::Flat));
        assert_eq!(work_contour(8), Some(WorkContour::Contoured));
        assert_eq!(work_contour(9), None);
        assert!(
            OUT.iter().all(|c| resource_type(*c).is_none()
                && rate_format(*c).is_none()
                && work_contour(*c).is_none())
        );
    }

    #[test]
    fn r120_inverse_tables_round_trip() {
        use super::{
            constraint_code, day_type_code, days_of_week_mask, duration_format_code, link_type_code,
            month_code, month_item_code, month_position_code, percent_lag_format_code, task_type_code,
            week_start_code, work_format_code,
        };
        use DurationUnit::{Day, Hour, Minute, Month as Mo, Week};
        use TimeBasis::{Elapsed as E, Working as W};
        for unit in [Minute, Hour, Day, Week, Mo] {
            for basis in [W, E] {
                for estimated in [false, true] {
                    let code = duration_format_code(unit, basis, estimated);
                    assert_eq!(
                        duration_format(code),
                        Some(DurationCode::Time {
                            unit,
                            basis,
                            estimated
                        })
                    );
                }
            }
            assert_eq!(work_format(work_format_code(unit)), Some(unit));
        }
        assert_eq!(duration_format_code(Day, W, true), 39);
        for basis in [W, E] {
            assert!(matches!(
                duration_format(percent_lag_format_code(basis)),
                Some(DurationCode::Percent { basis: b, .. }) if b == basis
            ));
        }
        for code in 0..4 {
            assert_eq!(link_type_code(link_type(code).unwrap()), code);
        }
        for code in 0..8 {
            assert_eq!(constraint_code(constraint_type(code).unwrap()), code);
        }
        for code in 0..3 {
            assert_eq!(task_type_code(task_type(code).unwrap()), code);
        }
        for d in Weekday::ALL {
            assert_eq!(day_type(day_type_code(d)), Some(DayType::Weekday(d)));
            assert_eq!(week_start_day(week_start_code(d)), Some(d));
        }
        for m in Month::ALL {
            assert_eq!(month(month_code(m)), Some(m));
        }
        for code in 0..10 {
            assert_eq!(month_item_code(month_item(code).unwrap()), code);
        }
        for code in 0..5 {
            assert_eq!(month_position_code(month_position(code).unwrap()), code);
        }
        for mask in 0..128 {
            assert_eq!(days_of_week_mask(&days_of_week(mask).unwrap()), mask);
        }
    }

    #[test]
    fn r128_exception_types() {
        use super::exception_type_code;
        use plannercraft_core::calendar::Recurrence;
        assert_eq!(exception_type_code(&Recurrence::Period), 1);
        assert_eq!(exception_type_code(&Recurrence::Daily { every: 3 }), 1);
        assert_eq!(exception_type_code(&Recurrence::ByWeekdayCount { every: 1 }), 8);
        assert_eq!(
            exception_type(exception_type_code(&Recurrence::YearlyByDay {
                month: Month::March,
                day: 4
            })),
            Some(ExceptionType::YearlyByDay)
        );
    }

    #[test]
    fn r124_r125_inverse_codes() {
        use super::{rate_format, rate_format_code, work_contour, work_contour_code};
        for code in [1, 2, 3, 4, 5, 7, 8] {
            assert_eq!(rate_format_code(rate_format(code).unwrap()), code);
        }
        for code in 0..9 {
            assert_eq!(work_contour_code(work_contour(code).unwrap()), code);
        }
    }
}
