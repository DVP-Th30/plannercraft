//! Calendriers (R-128) : base → `IsBaseCalendar 1`, `BaseCalendarUID -1`,
//! sept `WeekDay` ; dérivé → ses seuls jours non hérités ; exceptions
//! (forme `Exceptions`, jamais l'ancienne) ; semaines datées.

use plannercraft_core::calendar::{
    Calendar, CalendarException, CalendarKind, DayRule, DaySetting, Recurrence, WorkWeek,
};
use plannercraft_core::{DayRange, Moment, Subject, TimeOfDay, Weekday};

use super::emit::Emitter;
use super::{NO_EXTRAS, OrderProfile, entity, max_len, plan, unscoped};
use crate::codes;
use crate::scalar::{format_bool, format_datetime, format_time};
use crate::schema::Parent;

/// Écrit un calendrier.
pub(crate) fn write(em: &mut Emitter, c: &Calendar, profile: OrderProfile) {
    em.subject = Some(Subject::Calendar(c.uid()));
    let planned = plan(em, "Calendar", Parent::Calendar, &c.extras, &[], profile);
    let top = unscoped(&planned);
    entity(
        em,
        "Calendar",
        Parent::Calendar,
        profile,
        &top,
        |em, name| match name {
            "UID" => em.leaf(name, &c.uid().get().to_string()),
            "GUID" => em.opt_leaf(name, c.guid.map(|g| g.to_string())),
            "Name" => em.text(name, &c.name, max_len(Parent::Calendar, name)),
            "IsBaseCalendar" => em.leaf(name, format_bool(c.is_base())),
            "BaseCalendarUID" => match c.kind() {
                CalendarKind::Base { .. } => em.leaf(name, "-1"),
                CalendarKind::Derived { base, .. } => em.leaf(name, &base.get().to_string()),
            },
            "WeekDays" => week_days(em, c),
            "Exceptions" if !c.exceptions().is_empty() => {
                em.open(name);
                for e in c.exceptions() {
                    exception(em, e, profile);
                }
                em.close();
            }
            "WorkWeeks" if !c.work_weeks().is_empty() => {
                em.open(name);
                for w in c.work_weeks() {
                    work_week(em, w, profile);
                }
                em.close();
            }
            _ => {}
        },
    );
    em.subject = None;
}

/// Semaine par défaut : sept jours pour une base, les jours non hérités pour
/// un dérivé (aucun : `WeekDays` omis).
fn week_days(em: &mut Emitter, c: &Calendar) {
    let rules: Vec<(Weekday, DayRule)> = match c.kind() {
        CalendarKind::Base { week } => Weekday::ALL
            .iter()
            .map(|d| (*d, DayRule::from(week.get(*d).clone())))
            .collect(),
        CalendarKind::Derived { week, .. } => Weekday::ALL
            .iter()
            .filter(|d| !matches!(week.get(**d), DayRule::Inherit))
            .map(|d| (*d, week.get(*d).clone()))
            .collect(),
    };
    if rules.is_empty() {
        return;
    }
    em.open("WeekDays");
    for (d, rule) in &rules {
        em.open("WeekDay");
        day(em, *d, rule, true);
        em.close();
    }
    em.close();
}

/// `DayType`, `DayWorking`, et `WorkingTimes` si `with_times` et ouvré.
fn day(em: &mut Emitter, d: Weekday, rule: &DayRule, with_times: bool) {
    em.leaf("DayType", &codes::day_type_code(d).to_string());
    match rule {
        DayRule::Working(hours) => {
            em.leaf("DayWorking", "1");
            if with_times {
                working_times(em, hours.ranges().iter().map(|r| (r.from(), r.to())));
            }
        }
        DayRule::NonWorking | DayRule::Inherit => em.leaf("DayWorking", "0"),
    }
}

/// `WorkingTimes` : plages `FromTime`/`ToTime`, 24:00 écrit `00:00:00`
/// (R-077).
fn working_times(em: &mut Emitter, ranges: impl Iterator<Item = (TimeOfDay, TimeOfDay)>) {
    em.open("WorkingTimes");
    for (from, to) in ranges {
        em.open("WorkingTime");
        em.leaf("FromTime", &format_time(from, true));
        em.leaf("ToTime", &format_time(to, true));
        em.close();
    }
    em.close();
}

/// `TimePeriod` d'une période de jours : du premier jour 00:00:00 au
/// dernier jour 23:59:00 (forme de MS Project, O19 ; relu par R-080).
fn time_period(em: &mut Emitter, period: DayRange) {
    em.open("TimePeriod");
    let from = Moment::at(period.first(), TimeOfDay::MIDNIGHT).unwrap_or(Moment::MIN);
    let last = TimeOfDay::hm(23, 59).unwrap_or(TimeOfDay::MIDNIGHT);
    let to = Moment::at(period.last(), last).unwrap_or(Moment::MAX);
    em.leaf("FromDate", &format_datetime(from));
    em.leaf("ToDate", &format_datetime(to));
    em.close();
}

/// Une exception (R-128).
fn exception(em: &mut Emitter, e: &CalendarException, profile: OrderProfile) {
    entity(
        em,
        "Exception",
        Parent::Exception,
        profile,
        NO_EXTRAS,
        |em, name| match name {
            "EnteredByOccurrences" => em.leaf(name, format_bool(e.occurrences.is_some())),
            "TimePeriod" => time_period(em, e.period),
            "Occurrences" => {
                if let Some(n) = e.occurrences {
                    em.leaf(name, &n.to_string());
                }
            }
            "Name" => em.text(name, &e.name, max_len(Parent::Exception, name)),
            "Type" => em.leaf(name, &codes::exception_type_code(&e.recurrence).to_string()),
            "Period" => {
                let every = match e.recurrence {
                    // D15 : une période s'écrit quotidienne de pas 1.
                    Recurrence::Period => Some(1),
                    Recurrence::Daily { every }
                    | Recurrence::Weekly { every, .. }
                    | Recurrence::MonthlyByDay { every, .. }
                    | Recurrence::MonthlyByPosition { every, .. }
                    | Recurrence::ByDayCount { every }
                    | Recurrence::ByWeekdayCount { every } => Some(every),
                    Recurrence::YearlyByDay { .. } | Recurrence::YearlyByPosition { .. } => None,
                };
                if let Some(every) = every {
                    em.leaf(name, &every.to_string());
                }
            }
            "DaysOfWeek" => {
                if let Recurrence::Weekly { days, .. } = &e.recurrence {
                    em.leaf(name, &codes::days_of_week_mask(days).to_string());
                }
            }
            "MonthItem" => {
                if let Recurrence::MonthlyByPosition { item, .. }
                | Recurrence::YearlyByPosition { item, .. } = e.recurrence
                {
                    em.leaf(name, &codes::month_item_code(item).to_string());
                }
            }
            "MonthPosition" => {
                if let Recurrence::MonthlyByPosition { position, .. }
                | Recurrence::YearlyByPosition { position, .. } = e.recurrence
                {
                    em.leaf(name, &codes::month_position_code(position).to_string());
                }
            }
            "Month" => {
                if let Recurrence::YearlyByDay { month, .. } | Recurrence::YearlyByPosition { month, .. } =
                    e.recurrence
                {
                    em.leaf(name, &codes::month_code(month).to_string());
                }
            }
            "MonthDay" => {
                if let Recurrence::YearlyByDay { day, .. } | Recurrence::MonthlyByDay { day, .. } =
                    e.recurrence
                {
                    em.leaf(name, &day.to_string());
                }
            }
            "DayWorking" => em.leaf(name, format_bool(matches!(e.setting, DaySetting::Working(_)))),
            "WorkingTimes" => {
                if let DaySetting::Working(hours) = &e.setting {
                    working_times(em, hours.ranges().iter().map(|r| (r.from(), r.to())));
                }
            }
            _ => {}
        },
    );
}

/// Une semaine datée : profil observé, forme lue depuis J2 (`WeekDays`,
/// `WeekDay` avec plages, R-082) ; profil strict, forme pj15 (`WeekDay`
/// direct, `DayType` et `DayWorking` seulement : les plages d'un jour ouvré
/// sont perdues, relues 08:00-12:00 et 13:00-17:00).
fn work_week(em: &mut Emitter, w: &WorkWeek, profile: OrderProfile) {
    let days: Vec<(Weekday, &DayRule)> = Weekday::ALL
        .iter()
        .map(|d| (*d, w.days.get(*d)))
        .filter(|(_, r)| !matches!(r, DayRule::Inherit))
        .collect();
    entity(
        em,
        "WorkWeek",
        Parent::WorkWeek,
        profile,
        NO_EXTRAS,
        |em, name| match name {
            "TimePeriod" => time_period(em, w.period),
            "Name" => em.text(name, &w.name, max_len(Parent::WorkWeek, name)),
            "WeekDays" => {
                if !days.is_empty() {
                    em.open("WeekDays");
                    for (d, rule) in &days {
                        em.open("WeekDay");
                        day(em, *d, rule, true);
                        em.close();
                    }
                    em.close();
                }
            }
            "WeekDay" => {
                for (d, rule) in &days {
                    em.open("WeekDay");
                    day(em, *d, rule, false);
                    em.close();
                }
            }
            _ => {}
        },
    );
}
