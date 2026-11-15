//! Calendriers (`TBkndCal`, R-287).
//!
//! Observé (`docs/formats/mpp.md` § 10) : un enregistrement fixe de 12
//! octets par calendrier (UID, calendrier de base) ; le nom (clé
//! `0x0D400001`) et la semaine (clé `0x0D400008`) en valeurs variables.
//! La semaine : 7 jours de 60 octets, dimanche d'abord ; puis, à l'octet
//! 420, le nombre d'exceptions (`u32`) et les exceptions à la suite.

use std::collections::BTreeSet;

use plannercraft_core::calendar::model::{
    Calendar, CalendarException, CalendarKind, DayHours, DayRule, DaySetting, Recurrence, TimeRange,
};
use plannercraft_core::{CalendarUid, DayRange, TimeOfDay, Week, Weekday};

use plannercraft_mspdi::codes;

use crate::bytes::{le_i32, le_u16, le_u32, utf16_z};
use crate::diag::{MppCode, Sink};
use crate::layout::{self, CALENDAR_LEN, calendar_var};
use crate::tables::{FixedItem, VarValues};
use crate::time;

/// Longueur d'un jour.
const DAY_LEN: usize = 60;
/// Plages au plus par jour.
const MAX_RANGES: usize = 5;
/// Décalage du nombre d'exceptions.
const EXCEPTIONS_AT: usize = 420;
/// Partie fixe d'une exception (en-tête 12, jour 60, fin 20).
const EXCEPTION_FIXED: usize = 92;
/// Exceptions lues au plus par calendrier.
const MAX_EXCEPTIONS: usize = 5_000;

/// Jours dans l'ordre des enregistrements.
const DAYS: [Weekday; 7] = [
    Weekday::Sunday,
    Weekday::Monday,
    Weekday::Tuesday,
    Weekday::Wednesday,
    Weekday::Thursday,
    Weekday::Friday,
    Weekday::Saturday,
];

/// Calendriers lus.
#[derive(Debug, Default)]
pub struct Calendars {
    /// Calendriers de base, dans l'ordre des enregistrements.
    pub base: Vec<Calendar>,
    /// Calendriers dérivés (calendriers de ressource, R-296), dans l'ordre
    /// des enregistrements.
    pub derived: Vec<Calendar>,
}

/// Jour d'un enregistrement de 60 octets : `None` = jour par défaut (code
/// 1 à l'octet 0) ; sinon le réglage lu (code 0, nombre de plages à
/// l'octet 2, débuts `u16` à l'octet 8, durées `u32` à l'octet 20, en
/// dixièmes de minute).
fn day_record(rec: &[u8], uid: u32, sink: &mut Sink) -> Option<DaySetting> {
    if le_u16(rec, 0) == Some(1) {
        return None;
    }
    let count = usize::from(le_u16(rec, 2).unwrap_or(0));
    if count > MAX_RANGES {
        sink.push(
            MppCode::CalendarRangeInvalid,
            Some(uid),
            Some(format!("{count} plages")),
        );
    }
    let mut ranges = Vec::new();
    for k in 0..count.min(MAX_RANGES) {
        let start = le_u16(rec, 8_usize.saturating_add(k.saturating_mul(2)));
        let dur = le_u32(rec, 20_usize.saturating_add(k.saturating_mul(4)));
        let (Some(start), Some(dur)) = (start, dur) else {
            continue;
        };
        let from = u32::from(start).checked_div(10).unwrap_or(0);
        let tenth = dur.checked_div(10).unwrap_or(0);
        let to = from.saturating_add(tenth);
        let range = u16::try_from(from)
            .ok()
            .zip(u16::try_from(to).ok())
            .and_then(|(f, t)| Some((TimeOfDay::new(f).ok()?, TimeOfDay::new(t).ok()?)))
            .and_then(|(f, t)| TimeRange::new(f, t).ok());
        match range {
            Some(r) => ranges.push(r),
            None => sink.push(
                MppCode::CalendarRangeInvalid,
                Some(uid),
                Some(format!("{from}+{tenth}")),
            ),
        }
    }
    if ranges.is_empty() {
        return Some(DaySetting::NonWorking);
    }
    if let Ok(h) = DayHours::new(ranges) {
        Some(DaySetting::Working(h))
    } else {
        sink.push(
            MppCode::CalendarRangeInvalid,
            Some(uid),
            Some("plages en chevauchement".to_owned()),
        );
        Some(DaySetting::NonWorking)
    }
}

/// Semaine du calendrier Standard (repli quand le fichier n'a pas de
/// semaine par défaut).
fn standard_week() -> Week<DaySetting> {
    if let CalendarKind::Base { week } =
        plannercraft_core::calendar::standard(CalendarUid::new(1), String::new()).kind()
    {
        week.clone()
    } else {
        Week::from_fn(|_| DaySetting::NonWorking)
    }
}

/// Semaine par défaut du projet (propriété `0x02401388` : 7 jours de 60
/// octets, comme ceux d'un calendrier) : elle donne les jours « par
/// défaut » (code 1) des calendriers de base (observé,
/// `docs/formats/mpp.md` § 10). Absente ou illisible : semaine Standard.
#[must_use]
pub fn default_week(prop: Option<&[u8]>, sink: &mut Sink) -> Week<DaySetting> {
    let mut week = standard_week();
    let Some(blob) = prop else { return week };
    for (k, d) in DAYS.iter().enumerate() {
        let start = k.saturating_mul(DAY_LEN);
        let Some(rec) = start.checked_add(DAY_LEN).and_then(|e| blob.get(start..e)) else {
            break;
        };
        if let Some(setting) = day_record(rec, 0, sink) {
            *week.get_mut(*d) = setting;
        }
    }
    week
}

/// Récurrence d'une exception (R-287), lue à la fin de l'en-tête de
/// l'exception : type (`u32` à l'octet 72), paramètres aux octets 76 à 79 ;
/// nombre d'occurrences (`u32` à l'octet 4) quand l'exception a été saisie
/// par occurrences (`u32` à l'octet 8 égal à 1). Codes des mois, rangs,
/// jours et masques de jours : ceux de MSPDI (observé,
/// `docs/formats/mpp.md` § 10). `None` : type inconnu ou paramètre hors
/// bornes.
fn recurrence(head: &[u8]) -> Option<(Recurrence, Option<u32>)> {
    let kind = le_u32(head, 72)?;
    let b76 = i64::from(*head.get(76)?);
    let b77 = i64::from(*head.get(77)?);
    let w76 = le_u16(head, 76)?;
    let w78 = le_u16(head, 78)?;
    let rec = match kind {
        1 => Recurrence::Period,
        7 => Recurrence::Daily { every: w76 },
        6 => Recurrence::Weekly {
            every: w78,
            days: codes::days_of_week(b76)?,
        },
        5 => Recurrence::MonthlyByPosition {
            every: w78,
            position: codes::month_position(b76)?,
            item: codes::month_item(b77)?,
        },
        4 => Recurrence::MonthlyByDay {
            every: w78,
            day: u8::try_from(b76).ok()?,
        },
        3 => Recurrence::YearlyByPosition {
            month: codes::month(b76)?,
            position: codes::month_position(b77)?,
            item: codes::month_item(i64::from(w78))?,
        },
        2 => Recurrence::YearlyByDay {
            month: codes::month(b76)?,
            day: u8::try_from(b77).ok()?,
        },
        _ => return None,
    };
    rec.validate().ok()?;
    let occurrences = (kind != 1 && le_u32(head, 8) == Some(1))
        .then(|| le_u32(head, 4))
        .flatten();
    Some((rec, occurrences))
}

/// Exceptions à la suite de la semaine.
fn exceptions(blob: &[u8], uid: u32, cal: &mut Calendar, sink: &mut Sink) {
    let count = le_u32(blob, EXCEPTIONS_AT)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let mut pos = EXCEPTIONS_AT.saturating_add(4);
    for _ in 0..count.min(MAX_EXCEPTIONS) {
        let Some(head) = pos
            .checked_add(EXCEPTION_FIXED)
            .and_then(|end| blob.get(pos..end))
        else {
            sink.push(
                MppCode::CalendarExceptionInvalid,
                Some(uid),
                Some("exception tronquée".to_owned()),
            );
            return;
        };
        let name_len = le_u32(head, 88)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0);
        let name_start = pos.saturating_add(EXCEPTION_FIXED);
        let Some(name_end) = name_start.checked_add(name_len) else {
            return;
        };
        let name = blob
            .get(name_start..name_end)
            .map(|b| utf16_z(b, 255))
            .unwrap_or_default();
        // Enregistrement suivant aligné sur 4 octets depuis le début du bloc.
        pos = name_end.saturating_add(3) & !3_usize;
        let (Some(first), Some(last)) = (
            le_u16(head, 0).and_then(time::day),
            le_u16(head, 2).and_then(time::day),
        ) else {
            sink.push(MppCode::CalendarExceptionInvalid, Some(uid), Some(name));
            continue;
        };
        let Ok(period) = DayRange::new(first, last) else {
            sink.push(MppCode::CalendarExceptionInvalid, Some(uid), Some(name));
            continue;
        };
        let setting = head
            .get(12..72)
            .and_then(|d| day_record(d, uid, sink))
            .unwrap_or(DaySetting::NonWorking);
        let Some((recurrence, occurrences)) = recurrence(head) else {
            sink.push(MppCode::CalendarExceptionUnknownType, Some(uid), Some(name));
            continue;
        };
        let exception = CalendarException {
            name: name.clone(),
            period,
            setting,
            recurrence,
            occurrences,
        };
        if cal.add_exception(exception).is_err() {
            sink.push(MppCode::CalendarExceptionInvalid, Some(uid), Some(name));
        }
    }
}

/// Lit les calendriers de base et les calendriers dérivés (calendriers de
/// ressource, R-296 : jour au code 1 = hérité de la base).
#[must_use]
pub fn read(
    items: &[FixedItem<'_>],
    var: &VarValues<'_>,
    lay: crate::Layout,
    defaults: &Week<DaySetting>,
    sink: &mut Sink,
) -> Calendars {
    let cl = layout::calendar(lay);
    let mut out = Calendars::default();
    let mut seen = BTreeSet::new();
    for item in items {
        if item.deleted() || item.short() {
            continue;
        }
        let Some(rec) = item.data.get(..CALENDAR_LEN) else {
            continue;
        };
        let (Some(uid), Some(base)) = (le_u32(rec, cl.uid), le_i32(rec, cl.base)) else {
            continue;
        };
        if !seen.insert(uid) {
            sink.push(MppCode::DuplicateUid, Some(uid), Some("calendrier".to_owned()));
            continue;
        }
        let fields = var.get(&uid);
        let name = fields
            .and_then(|f| f.get(&calendar_var::NAME))
            .map(|b| utf16_z(b, 255))
            .unwrap_or_default();
        let blob = fields.and_then(|f| f.get(&calendar_var::DATA));
        if base > 0 {
            let mut cal =
                Calendar::new_derived(CalendarUid::new(uid), name, CalendarUid::new(base.unsigned_abs()));
            if let Some(blob) = blob {
                for (k, d) in DAYS.iter().enumerate() {
                    let start = k.saturating_mul(DAY_LEN);
                    let Some(rec) = start.checked_add(DAY_LEN).and_then(|e| blob.get(start..e)) else {
                        break;
                    };
                    let rule = match day_record(rec, uid, sink) {
                        None => DayRule::Inherit,
                        Some(DaySetting::NonWorking) => DayRule::NonWorking,
                        Some(DaySetting::Working(h)) => DayRule::Working(h),
                    };
                    if cal.set_day(*d, rule).is_err() {
                        sink.push(MppCode::CalendarRangeInvalid, Some(uid), None);
                    }
                }
                exceptions(blob, uid, &mut cal, sink);
            }
            out.derived.push(cal);
            continue;
        }
        let mut week = defaults.clone();
        if let Some(blob) = blob {
            for (k, d) in DAYS.iter().enumerate() {
                let start = k.saturating_mul(DAY_LEN);
                let Some(rec) = start.checked_add(DAY_LEN).and_then(|e| blob.get(start..e)) else {
                    break;
                };
                if let Some(setting) = day_record(rec, uid, sink) {
                    *week.get_mut(*d) = setting;
                }
            }
        }
        let mut cal = Calendar::new_base(CalendarUid::new(uid), name, week);
        if let Some(blob) = blob {
            exceptions(blob, uid, &mut cal, sink);
        }
        out.base.push(cal);
    }
    out
}
