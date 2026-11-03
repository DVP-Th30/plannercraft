//! Import des calendriers (R-077 à R-082) : semaine par défaut, base ou
//! dérivé, exceptions (forme `Exceptions` et ancienne forme `DayType 0`),
//! semaines de travail datées.

use std::collections::BTreeMap;

use plannercraft_core::calendar::{
    Calendar, CalendarException, DayHours, DayRule, DaySetting, Recurrence, TimeRange, WorkWeek,
};
use plannercraft_core::{Arena, CalendarUid, DayRange, Moment, Subject, TimeOfDay, Week, Weekday};

use crate::codes::{self, DayType, ExceptionType};
use crate::diag::{ImportCode, Sink};
use crate::extras;
use crate::fields::Fields;
use crate::project::{code, guid};
use crate::schema::Parent;
use crate::xml::Element;

/// Chemin des éléments d'un calendrier.
const CAL: &str = "Calendars/Calendar";

/// Plages d'un jour ouvré sans `WorkingTimes` (R-078) : 08:00-12:00 et
/// 13:00-17:00.
const DEFAULT_HOURS: [(u16, u16); 2] = [(480, 720), (780, 1_020)];

/// En-tête d'un calendrier lu (premier passage).
struct Header<'e> {
    element: &'e Element<'e>,
    uid: CalendarUid,
    /// `IsBaseCalendar` ; absent : base si `BaseCalendarUID` est absent ou
    /// négatif (hypothèse), signalé.
    is_base: bool,
    is_base_missing: bool,
    base_uid: Option<(i64, u64)>,
}

/// Nature retenue d'un calendrier.
#[derive(Clone, Copy)]
enum Kind {
    Base,
    Derived(CalendarUid),
}

/// Importe les calendriers dans l'ordre du document ; `project_calendar` sert
/// de base aux dérivés sans `BaseCalendarUID`, le calendrier de base d'UID
/// minimal à ceux dont la base est négative (R-079 amendée).
pub(crate) fn import(
    elements: &[Element<'_>],
    project_calendar: CalendarUid,
    sink: &mut Sink<'_>,
) -> Arena<CalendarUid, Calendar> {
    // Premier passage : UID, doublons, nature déclarée.
    let mut headers: Vec<Header<'_>> = Vec::new();
    for element in elements {
        let f = Fields::new(&element.children, CAL, None, element.at);
        let Some(uid) = calendar_uid(&f, sink) else {
            continue;
        };
        if headers.iter().any(|h| h.uid == uid) {
            f.report(
                ImportCode::Duplicate,
                "UID",
                element.at,
                Some(&uid.get().to_string()),
                sink,
            );
            continue;
        }
        let mut f = f;
        f.set_subject(Some(Subject::Calendar(uid)));
        let declared = f.bool("IsBaseCalendar", sink);
        let base_uid = f.int("BaseCalendarUID", sink);
        headers.push(Header {
            element,
            uid,
            is_base: declared.unwrap_or_else(|| base_uid.is_none_or(|(b, _)| b < 0)),
            is_base_missing: declared.is_none(),
            base_uid,
        });
    }
    let declared_base: BTreeMap<CalendarUid, bool> = headers.iter().map(|h| (h.uid, h.is_base)).collect();

    let mut out = Arena::new();
    for h in &headers {
        let f = Fields::new(
            &h.element.children,
            CAL,
            Some(Subject::Calendar(h.uid)),
            h.element.at,
        );
        let kind = kind(h, &f, project_calendar, &declared_base, sink);
        let mut calendar = build(h.uid, kind, &f, sink);
        calendar.extras =
            plannercraft_core::Extras(extras::collect(&h.element.children, Parent::Calendar, None));
        extras::canonical_order(&mut calendar.extras, Parent::Calendar);
        // UID uniques (premier passage) : l'ajout ne peut pas échouer.
        let _ = out.push(calendar);
    }
    out
}

/// UID d'un calendrier ; absent, négatif ou illisible : calendrier ignoré.
fn calendar_uid(f: &Fields<'_>, sink: &mut Sink<'_>) -> Option<CalendarUid> {
    let Some((v, at)) = f.int("UID", sink) else {
        if f.scalar("UID", sink).is_none() {
            f.missing("UID", sink);
        }
        return None;
    };
    if let Ok(u) = u32::try_from(v) {
        Some(CalendarUid::new(u))
    } else {
        f.invalid("UID", at, &v.to_string(), sink);
        None
    }
}

/// R-079 : base ou dérivé.
fn kind(
    h: &Header<'_>,
    f: &Fields<'_>,
    project_calendar: CalendarUid,
    declared_base: &BTreeMap<CalendarUid, bool>,
    sink: &mut Sink<'_>,
) -> Kind {
    if h.is_base_missing && f.scalar("IsBaseCalendar", sink).is_none() {
        f.missing("IsBaseCalendar", sink);
    }
    if h.is_base {
        return Kind::Base;
    }
    match h.base_uid {
        Some((b, at)) if b >= 0 => {
            let Ok(base) = u32::try_from(b).map(CalendarUid::new) else {
                f.invalid("BaseCalendarUID", at, &b.to_string(), sink);
                return Kind::Base;
            };
            if !declared_base.contains_key(&base) {
                f.report(
                    ImportCode::DanglingReference,
                    "BaseCalendarUID",
                    at,
                    Some(&b.to_string()),
                    sink,
                );
            }
            Kind::Derived(base)
        }
        Some((b, at)) => {
            // R-079 amendée (O29) : base négative (−1, −65 533, −65 535) →
            // dérivé du calendrier de base d'UID minimal (autre que lui),
            // sinon base.
            f.report(
                ImportCode::DanglingReference,
                "BaseCalendarUID",
                at,
                Some(&b.to_string()),
                sink,
            );
            declared_base
                .iter()
                .find(|(uid, is_base)| **is_base && **uid != h.uid)
                .map_or(Kind::Base, |(uid, _)| Kind::Derived(*uid))
        }
        None => {
            // Base absente : dérivé du calendrier du projet s'il est de base
            // et différent, sinon base.
            f.report(
                ImportCode::DanglingReference,
                "BaseCalendarUID",
                f.at(),
                None,
                sink,
            );
            if project_calendar != h.uid && declared_base.get(&project_calendar).copied() == Some(true) {
                Kind::Derived(project_calendar)
            } else {
                Kind::Base
            }
        }
    }
}

/// Construit un calendrier.
fn build(uid: CalendarUid, kind: Kind, f: &Fields<'_>, sink: &mut Sink<'_>) -> Calendar {
    let name = f.raw("Name", sink).unwrap_or_default().to_owned();
    let mut days: Week<Option<DaySetting>> = Week::default();
    let mut legacy: Vec<DayEntry> = Vec::new();
    if let Some(week_days) = f.get("WeekDays", sink) {
        for wd in week_days.children.iter().filter(|c| c.name == "WeekDay") {
            let Some(entry) = day_entry(wd, &format!("{CAL}/WeekDays/WeekDay"), f.subject(), sink) else {
                continue;
            };
            match entry.day_type {
                DayType::Weekday(d) => {
                    let slot = days.get_mut(d);
                    if slot.is_some() {
                        sink.push(
                            ImportCode::Duplicate,
                            f.subject(),
                            entry.at,
                            &format!("{CAL}/WeekDays/WeekDay/DayType"),
                            Some(&weekday_code(d).to_string()),
                        );
                    } else {
                        *slot = Some(entry.setting.clone());
                    }
                }
                DayType::LegacyException => legacy.push(entry),
            }
        }
    }

    let mut calendar = match kind {
        Kind::Base => {
            let week = Week::from_fn(|d| {
                days.get(d).clone().unwrap_or_else(|| {
                    sink.push(
                        ImportCode::MissingRequired,
                        f.subject(),
                        f.at(),
                        &format!("{CAL}/WeekDays/WeekDay"),
                        Some(&weekday_code(d).to_string()),
                    );
                    DaySetting::NonWorking
                })
            });
            Calendar::new_base(uid, name, week)
        }
        Kind::Derived(base) => {
            let mut c = Calendar::new_derived(uid, name, base);
            for d in Weekday::ALL {
                if let Some(setting) = days.get(d).clone() {
                    // `set_day` ne refuse que `Inherit` sur une base.
                    let _ = c.set_day(d, DayRule::from(setting));
                }
            }
            c
        }
    };
    calendar.guid = guid(f, "GUID", sink);

    if f.has("Exceptions") {
        // R-081 : la forme `Exceptions` fait foi, l'ancienne est ignorée.
        if let Some(list) = f.get("Exceptions", sink) {
            for e in list.children.iter().filter(|c| c.name == "Exception") {
                if let Some(exception) = exception(e, f.subject(), sink) {
                    add_exception(&mut calendar, exception, e.at, f.subject(), sink);
                }
            }
        }
    } else {
        for entry in legacy {
            if let Some(period) = entry.period {
                let exception = CalendarException {
                    name: String::new(),
                    period,
                    setting: entry.setting,
                    recurrence: Recurrence::Period,
                    occurrences: None,
                };
                add_exception(&mut calendar, exception, entry.at, f.subject(), sink);
            }
        }
    }

    if let Some(weeks) = f.get("WorkWeeks", sink) {
        for w in weeks.children.iter().filter(|c| c.name == "WorkWeek") {
            if let Some(week) = work_week(w, f.subject(), sink)
                && calendar.add_work_week(week).is_err()
            {
                sink.push(
                    ImportCode::Inconsistent,
                    f.subject(),
                    w.at,
                    &format!("{CAL}/WorkWeeks/WorkWeek"),
                    None,
                );
            }
        }
    }
    calendar
}

/// Ajoute une exception ; chevauchement ou récurrence invalide : ignorée et
/// signalée.
fn add_exception(
    calendar: &mut Calendar,
    exception: CalendarException,
    at: u64,
    subject: Option<Subject>,
    sink: &mut Sink<'_>,
) {
    if calendar.add_exception(exception).is_err() {
        sink.push(
            ImportCode::Inconsistent,
            subject,
            at,
            &format!("{CAL}/Exceptions/Exception"),
            None,
        );
    }
}

/// Code `DayType` d'un jour (pour les diagnostics).
const fn weekday_code(d: Weekday) -> u8 {
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

/// Un `WeekDay` lu.
struct DayEntry {
    day_type: DayType,
    setting: DaySetting,
    /// Période (ancienne forme d'exception, `DayType 0`).
    period: Option<DayRange>,
    at: u64,
}

/// Lit un `WeekDay` (R-078) ; `DayType` absent ou inconnu : ignoré.
fn day_entry(e: &Element<'_>, path: &str, subject: Option<Subject>, sink: &mut Sink<'_>) -> Option<DayEntry> {
    let f = Fields::new(&e.children, path.to_owned(), subject, e.at);
    let Some(day_type) = code(&f, "DayType", codes::day_type, sink) else {
        if f.scalar("DayType", sink).is_none() {
            f.missing("DayType", sink);
        }
        return None;
    };
    let setting = day_setting(&f, sink);
    let period = match day_type {
        DayType::LegacyException => time_period(&f, sink),
        DayType::Weekday(_) => None,
    };
    Some(DayEntry {
        day_type,
        setting,
        period,
        at: e.at,
    })
}

/// Réglage d'un jour depuis `DayWorking` et `WorkingTimes` (R-077, R-078).
/// `DayWorking 1` sans plage : 08:00-12:00 et 13:00-17:00, signalé ;
/// `DayWorking` absent : ouvré si des plages sont données, sinon chômé.
fn day_setting(f: &Fields<'_>, sink: &mut Sink<'_>) -> DaySetting {
    let working = f.bool("DayWorking", sink);
    let ranges = f
        .get("WorkingTimes", sink)
        .map(|w| working_times(w, f, sink))
        .unwrap_or_default();
    match (working, ranges) {
        (Some(false), _) => DaySetting::NonWorking,
        (Some(true), r) if r.is_empty() => {
            f.report(ImportCode::InvalidValue, "WorkingTimes", f.at(), None, sink);
            hours(&DEFAULT_HOURS.map(|(a, b)| (minutes(a), minutes(b))))
        }
        (None, r) if r.is_empty() => DaySetting::NonWorking,
        (_, r) => hours(&r),
    }
}

/// Heure depuis des minutes connues valides (0 à 1 440).
fn minutes(m: u16) -> TimeOfDay {
    TimeOfDay::new(m).unwrap_or(TimeOfDay::MIDNIGHT)
}

/// Jour ouvré depuis des plages valides, triées, disjointes, au plus 5.
fn hours(ranges: &[(TimeOfDay, TimeOfDay)]) -> DaySetting {
    let ranges: Vec<TimeRange> = ranges
        .iter()
        .filter_map(|(a, b)| TimeRange::new(*a, *b).ok())
        .collect();
    DayHours::new(ranges).map_or(DaySetting::NonWorking, DaySetting::Working)
}

/// R-077 : plages `[FromTime, ToTime)` d'un `WorkingTimes`, `ToTime` 00:00
/// lu 24:00 ; vide ou inversée → ignorée ; triées ; chevauchements fusionnés
/// (signalé) ; au-delà de 5 plages, les contiguës fusionnées, puis les 5
/// premières gardées (signalé).
fn working_times(w: &Element<'_>, owner: &Fields<'_>, sink: &mut Sink<'_>) -> Vec<(TimeOfDay, TimeOfDay)> {
    let path = owner.path_of("WorkingTimes/WorkingTime");
    let mut out: Vec<(TimeOfDay, TimeOfDay)> = Vec::new();
    for wt in w.children.iter().filter(|c| c.name == "WorkingTime") {
        let f = Fields::new(&wt.children, path.clone(), owner.subject(), wt.at);
        let from = f.time("FromTime", sink);
        let to = f.time("ToTime", sink).map(|t| {
            if t == TimeOfDay::MIDNIGHT {
                TimeOfDay::END_OF_DAY
            } else {
                t
            }
        });
        match (from, to) {
            (Some(a), Some(b)) if a < b => out.push((a, b)),
            _ => {
                let value = format!(
                    "{}-{}",
                    f.raw("FromTime", sink).unwrap_or_default().trim(),
                    f.raw("ToTime", sink).unwrap_or_default().trim()
                );
                sink.push(
                    ImportCode::InvalidValue,
                    owner.subject(),
                    wt.at,
                    &path,
                    Some(&value),
                );
            }
        }
    }
    out.sort_unstable();
    let mut merged: Vec<(TimeOfDay, TimeOfDay)> = Vec::with_capacity(out.len());
    let mut overlapped = false;
    for (a, b) in out {
        match merged.last_mut() {
            Some(last) if a < last.1 => {
                overlapped = true;
                last.1 = last.1.max(b);
            }
            _ => merged.push((a, b)),
        }
    }
    if overlapped {
        sink.push(ImportCode::Inconsistent, owner.subject(), w.at, &path, None);
    }
    if merged.len() > DayHours::MAX_RANGES {
        let mut joined: Vec<(TimeOfDay, TimeOfDay)> = Vec::with_capacity(merged.len());
        for (a, b) in merged {
            match joined.last_mut() {
                Some(last) if a <= last.1 => last.1 = last.1.max(b),
                _ => joined.push((a, b)),
            }
        }
        merged = joined;
    }
    if merged.len() > DayHours::MAX_RANGES {
        sink.push(ImportCode::InvalidValue, owner.subject(), w.at, &path, None);
        merged.truncate(DayHours::MAX_RANGES);
    }
    merged
}

/// R-080 : `TimePeriod` → jours `[jour(FromDate), jour(ToDate)]`, un `ToDate`
/// à 00:00 strictement après `FromDate` comptant pour la veille. `FromDate`
/// absent : ignoré ; `ToDate` absent : un seul jour, signalé.
fn time_period(owner: &Fields<'_>, sink: &mut Sink<'_>) -> Option<DayRange> {
    let Some(tp) = owner.get("TimePeriod", sink) else {
        owner.missing("TimePeriod", sink);
        return None;
    };
    let f = Fields::new(&tp.children, owner.path_of("TimePeriod"), owner.subject(), tp.at);
    let Some(from) = f.datetime("FromDate", sink) else {
        if f.scalar("FromDate", sink).is_none() {
            f.missing("FromDate", sink);
        }
        return None;
    };
    let to = f.datetime("ToDate", sink).unwrap_or_else(|| {
        if f.scalar("ToDate", sink).is_none() {
            f.missing("ToDate", sink);
        }
        from
    });
    let last = last_day(from, to);
    let range = DayRange::new(from.day(), last).ok();
    if range.is_none() {
        f.report(ImportCode::InvalidValue, "ToDate", tp.at, None, sink);
    }
    range
}

/// Dernier jour couvert : celui de `to`, ou la veille si `to` est à 00:00
/// strictement après `from`.
fn last_day(from: Moment, to: Moment) -> plannercraft_core::Day {
    if to > from && to.time_of_day() == TimeOfDay::MIDNIGHT {
        to.day().checked_add_days(-1).unwrap_or_else(|_| to.day())
    } else {
        to.day()
    }
}

/// Pas d'une récurrence (`Period`) : absent → 1 ; < 1 ou trop grand →
/// signalé, 1.
fn every(f: &Fields<'_>, sink: &mut Sink<'_>) -> u16 {
    match f.int("Period", sink) {
        None => 1,
        Some((v, at)) => match u16::try_from(v) {
            Ok(p) if p >= 1 => p,
            _ => {
                f.invalid("Period", at, &v.to_string(), sink);
                1
            }
        },
    }
}

/// Valeur obligatoire d'une récurrence, traduite par `table`.
fn required<T>(
    f: &Fields<'_>,
    name: &str,
    table: impl Fn(i64) -> Option<T>,
    sink: &mut Sink<'_>,
) -> Option<T> {
    let v = code(f, name, table, sink);
    if v.is_none() && f.scalar(name, sink).is_none() {
        f.missing(name, sink);
    }
    v
}

/// Jour du mois, 1 à 31.
fn month_day(f: &Fields<'_>, sink: &mut Sink<'_>) -> Option<u8> {
    required(
        f,
        "MonthDay",
        |v| u8::try_from(v).ok().filter(|d| (1..=31).contains(d)),
        sink,
    )
}

/// R-080 : une exception de la forme `Exceptions/Exception`.
fn exception(e: &Element<'_>, subject: Option<Subject>, sink: &mut Sink<'_>) -> Option<CalendarException> {
    let f = Fields::new(&e.children, format!("{CAL}/Exceptions/Exception"), subject, e.at);
    let period = time_period(&f, sink)?;
    let kind = code(&f, "Type", codes::exception_type, sink);
    let kind = match kind {
        Some(k) => k,
        None if f.scalar("Type", sink).is_none() => {
            f.missing("Type", sink);
            ExceptionType::Period
        }
        None => return None,
    };
    let recurrence = match kind {
        ExceptionType::Period => Recurrence::Period,
        ExceptionType::Daily => Recurrence::Daily {
            every: every(&f, sink),
        },
        ExceptionType::YearlyByDay => Recurrence::YearlyByDay {
            month: required(&f, "Month", codes::month, sink)?,
            day: month_day(&f, sink)?,
        },
        ExceptionType::YearlyByPosition => Recurrence::YearlyByPosition {
            month: required(&f, "Month", codes::month, sink)?,
            position: required(&f, "MonthPosition", codes::month_position, sink)?,
            item: required(&f, "MonthItem", codes::month_item, sink)?,
        },
        ExceptionType::MonthlyByDay => Recurrence::MonthlyByDay {
            every: every(&f, sink),
            day: month_day(&f, sink)?,
        },
        ExceptionType::MonthlyByPosition => Recurrence::MonthlyByPosition {
            every: every(&f, sink),
            position: required(&f, "MonthPosition", codes::month_position, sink)?,
            item: required(&f, "MonthItem", codes::month_item, sink)?,
        },
        ExceptionType::Weekly => Recurrence::Weekly {
            every: every(&f, sink),
            days: required(&f, "DaysOfWeek", codes::days_of_week, sink)?,
        },
        ExceptionType::ByDayCount => Recurrence::ByDayCount {
            every: every(&f, sink),
        },
        ExceptionType::ByWeekdayCount => Recurrence::ByWeekdayCount {
            every: every(&f, sink),
        },
    };
    let occurrences = if f.bool("EnteredByOccurrences", sink) == Some(true) {
        f.int("Occurrences", sink).and_then(|(v, at)| {
            let n = u32::try_from(v).ok();
            if n.is_none() {
                f.invalid("Occurrences", at, &v.to_string(), sink);
            }
            n
        })
    } else {
        None
    };
    Some(CalendarException {
        name: f.raw("Name", sink).unwrap_or_default().to_owned(),
        period,
        setting: day_setting(&f, sink),
        recurrence,
        occurrences,
    })
}

/// R-082 : une semaine de travail datée ; jour absent → `Inherit`.
fn work_week(e: &Element<'_>, subject: Option<Subject>, sink: &mut Sink<'_>) -> Option<WorkWeek> {
    let path = format!("{CAL}/WorkWeeks/WorkWeek");
    let f = Fields::new(&e.children, path.clone(), subject, e.at);
    let period = time_period(&f, sink)?;
    let mut days: Week<DayRule> = Week::from_fn(|_| DayRule::Inherit);
    let mut seen: Week<bool> = Week::default();
    // Forme lue depuis J2 (`WeekDays/WeekDay`, plages comprises), puis forme
    // pj15 (`WeekDay` direct, sans plages ; écrite par le profil strict,
    // R-128).
    let listed = f
        .get("WeekDays", sink)
        .map(|list| list.children.as_slice())
        .unwrap_or_default();
    let direct = e.children.iter().filter(|c| c.name == "WeekDay");
    for wd in listed.iter().filter(|c| c.name == "WeekDay").chain(direct) {
        let Some(entry) = day_entry(wd, &format!("{path}/WeekDays/WeekDay"), subject, sink) else {
            continue;
        };
        match entry.day_type {
            DayType::Weekday(d) if !*seen.get(d) => {
                *seen.get_mut(d) = true;
                *days.get_mut(d) = DayRule::from(entry.setting);
            }
            DayType::Weekday(_) => {
                sink.push(
                    ImportCode::Duplicate,
                    subject,
                    entry.at,
                    &format!("{path}/WeekDays/WeekDay"),
                    None,
                );
            }
            DayType::LegacyException => {
                sink.push(
                    ImportCode::Inconsistent,
                    subject,
                    entry.at,
                    &format!("{path}/WeekDays/WeekDay/DayType"),
                    Some("0"),
                );
            }
        }
    }
    Some(WorkWeek {
        name: f.raw("Name", sink).unwrap_or_default().to_owned(),
        period,
        days,
    })
}
