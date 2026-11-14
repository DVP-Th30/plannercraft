//! Calendriers (`TBkndCal`, R-287).
//!
//! Observé (`docs/formats/mpp.md` § 10) : un enregistrement fixe de 12
//! octets par calendrier (UID, calendrier de base) ; le nom (clé
//! `0x0D400001`) et la semaine (clé `0x0D400008`) en valeurs variables.
//! La semaine : 7 jours de 60 octets, dimanche d'abord ; puis, à l'octet
//! 420, le nombre d'exceptions (`u32`) et les exceptions à la suite.

use std::collections::BTreeSet;

use plannercraft_core::calendar::model::{
    Calendar, CalendarException, CalendarKind, DayHours, DaySetting, Recurrence, TimeRange,
};
use plannercraft_core::{CalendarUid, DayRange, TimeOfDay, Week, Weekday};

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
const MAX_EXCEPTIONS: usize = 10_000;

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
    /// Calendriers dérivés (de ressource) non repris.
    pub derived: u32,
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

/// Semaine par défaut de MS Project (observé : un jour « par défaut » d'un
/// calendrier de base vaut le jour du calendrier Standard).
fn default_week() -> Week<DaySetting> {
    if let CalendarKind::Base { week } =
        plannercraft_core::calendar::standard(CalendarUid::new(1), String::new()).kind()
    {
        week.clone()
    } else {
        Week::from_fn(|_| DaySetting::NonWorking)
    }
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
        // Octet 8 non nul : exception périodique (format non établi) ; la
        // longueur des suivantes n'est plus connue.
        if le_u32(head, 8) != Some(0) {
            sink.push(MppCode::CalendarRecurringSkipped, Some(uid), None);
            return;
        }
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
        let exception = CalendarException {
            name: name.clone(),
            period,
            setting,
            recurrence: Recurrence::Period,
            occurrences: None,
        };
        if cal.add_exception(exception).is_err() {
            sink.push(MppCode::CalendarExceptionInvalid, Some(uid), Some(name));
        }
    }
}

/// Lit les calendriers de base ; les calendriers dérivés (de ressource)
/// sont comptés, pas repris.
#[must_use]
pub fn read(items: &[FixedItem<'_>], var: &VarValues, lay: crate::Layout, sink: &mut Sink) -> Calendars {
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
        if base > 0 {
            out.derived = out.derived.saturating_add(1);
            continue;
        }
        if !seen.insert(uid) {
            sink.push(MppCode::DuplicateUid, Some(uid), Some("calendrier".to_owned()));
            continue;
        }
        let fields = var.get(&uid);
        let name = fields
            .and_then(|f| f.get(&calendar_var::NAME))
            .map(|b| utf16_z(b, 255))
            .unwrap_or_default();
        let mut week = default_week();
        let blob = fields.and_then(|f| f.get(&calendar_var::DATA));
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
