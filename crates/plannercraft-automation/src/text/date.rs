//! Dates en texte (R-143) : « 19/10/2026 08:00 », « lun. 19/10/26 »,
//! « 10/19/2026 5:00 PM », « 2026-10-19T08:00 ».

use chrono::{Datelike, NaiveDate, Timelike};
use plannercraft_core::{Day, Moment, TimeOfDay};

use super::lex::{Cursor, bounded_chars, fold, is_space};
use super::{DateOrder, TextContext, TextError};

/// Rôle d'une date saisie : l'heure par défaut en dépend (R-033).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateRole {
    /// Date de début : heure de début par défaut.
    Start,
    /// Date de fin : heure de fin par défaut.
    Finish,
}

/// Noms de jours de la semaine admis (repliés), ignorés à la lecture.
const WEEKDAYS: &[&str] = &[
    "lun",
    "mar",
    "mer",
    "jeu",
    "ven",
    "sam",
    "dim",
    "lundi",
    "mardi",
    "mercredi",
    "jeudi",
    "vendredi",
    "samedi",
    "dimanche",
    "mon",
    "tue",
    "tues",
    "wed",
    "thu",
    "thur",
    "thurs",
    "fri",
    "sat",
    "sun",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

/// Lit au plus `max` chiffres (au moins un) ; rend la valeur et leur nombre.
fn digits(cur: &mut Cursor<'_>, max: usize) -> Result<(u32, usize), TextError> {
    let mut v: u32 = 0;
    let mut n = 0_usize;
    while n < max {
        let Some(d) = cur.peek().and_then(|c| c.to_digit(10)) else {
            break;
        };
        v = v
            .checked_mul(10)
            .and_then(|v| v.checked_add(d))
            .ok_or(TextError::BadDate)?;
        n = n.saturating_add(1);
        cur.bump();
    }
    if n == 0 {
        return Err(TextError::BadDate);
    }
    Ok((v, n))
}

/// Saute un nom de jour facultatif en tête (« lun. », « Mon »).
fn skip_weekday(cur: &mut Cursor<'_>) -> Result<(), TextError> {
    let mut word = Vec::new();
    while let Some(c) = cur.peek().filter(|c| c.is_alphabetic()) {
        word.push(c);
        cur.bump();
    }
    if word.is_empty() {
        return Ok(());
    }
    if !WEEKDAYS.contains(&fold(word.iter().copied()).as_str()) {
        return Err(TextError::BadDate);
    }
    if cur.peek() == Some('.') {
        cur.bump();
    }
    if cur.peek() == Some(',') {
        cur.bump();
    }
    cur.skip_spaces();
    Ok(())
}

/// Lit un séparateur de date (`/`, `.`, `-`), le même partout.
fn date_separator(cur: &mut Cursor<'_>, expected: Option<char>) -> Result<char, TextError> {
    match cur.peek() {
        Some(c) if matches!(c, '/' | '.' | '-') && expected.is_none_or(|e| e == c) => {
            cur.bump();
            Ok(c)
        }
        _ => Err(TextError::BadDate),
    }
}

/// Année sur 4 chiffres, ou sur 2 : 84 à 99 → 1984-1999, 00 à 83 →
/// 2000-2083 (domaine de 1984 à 2149, R-003).
fn full_year(value: u32, len: usize) -> Result<i32, TextError> {
    let year = match len {
        4 => value,
        2 if value >= 84 => value.checked_add(1_900).ok_or(TextError::BadDate)?,
        2 => value.checked_add(2_000).ok_or(TextError::BadDate)?,
        _ => return Err(TextError::BadDate),
    };
    i32::try_from(year).map_err(|_| TextError::BadDate)
}

/// Heure facultative : `hh:mm` ou `h:mm` (secondes `:ss` tolérées et
/// tronquées, R-004), puis `AM`/`PM` facultatif.
fn time_of_day(cur: &mut Cursor<'_>) -> Result<Option<TimeOfDay>, TextError> {
    cur.skip_spaces();
    if cur.at_end() {
        return Ok(None);
    }
    let (h, _) = digits(cur, 2)?;
    if cur.peek() != Some(':') {
        return Err(TextError::BadDate);
    }
    cur.bump();
    let (m, n) = digits(cur, 2)?;
    if n != 2 {
        return Err(TextError::BadDate);
    }
    if cur.peek() == Some(':') {
        cur.bump();
        let (sec, n) = digits(cur, 2)?;
        if n != 2 || sec >= 60 {
            return Err(TextError::BadDate);
        }
    }
    cur.skip_spaces();
    let suffix: String = fold(cur.rest().iter().copied())
        .chars()
        .filter(|c| *c != '.')
        .collect();
    let suffix = suffix.trim_matches(is_space);
    let hour = match suffix {
        "" => h,
        "am" | "pm" => {
            if !(1..=12).contains(&h) {
                return Err(TextError::BadDate);
            }
            let base = if h == 12 { 0 } else { h };
            if suffix == "pm" {
                base.saturating_add(12)
            } else {
                base
            }
        }
        _ => return Err(TextError::BadDate),
    };
    let hour = u16::try_from(hour).map_err(|_| TextError::BadDate)?;
    let minute = u16::try_from(m).map_err(|_| TextError::BadDate)?;
    TimeOfDay::hm(hour, minute)
        .map(Some)
        .map_err(|_| TextError::BadDate)
}

/// Jour depuis année, mois, jour : `BadDate` s'il n'existe pas,
/// `OutOfRange` hors du domaine (R-003).
fn day_of(year: i32, month: u32, day: u32) -> Result<Day, TextError> {
    let date = NaiveDate::from_ymd_opt(year, month, day).ok_or(TextError::BadDate)?;
    Day::from_naive(date).map_err(|_| TextError::OutOfRange)
}

/// R-143 : lit une date. Nom de jour abrégé ou complet facultatif, ignoré
/// (« lun. », « Mon ») ; JJ/MM/AAAA (contexte français) ou MM/JJ/AAAA
/// (anglais), séparateurs `/`, `.` ou `-` ; année sur 4 chiffres, ou 2 (84 à
/// 99 → 1984-1999, 00 à 83 → 2000-2083) ; ISO `AAAA-MM-JJ[Thh:mm]` toujours
/// admis ; heure `hh:mm` ou `h:mm AM/PM` facultative ; sans heure, l'heure
/// par défaut du rôle (R-033 ; 24:00 → le lendemain 00:00).
///
/// Exemples (français) : « 19/10/2026 08:00 » → lun 19/10/2026 08:00 ;
/// « lun. 19/10/26 » en fin → 17:00 ; (anglais) « 10/19/2026 5:00 PM » →
/// 17:00 ; « 31/02/2026 » → `text.bad_date`.
///
/// # Errors
///
/// [`TextError::Empty`], [`TextError::BadDate`] (forme illisible, jour
/// inexistant, heure invalide), [`TextError::OutOfRange`] (hors de 1984 à
/// 2149), [`TextError::TooLong`].
pub fn parse_date(s: &str, role: DateRole, ctx: &TextContext) -> Result<Moment, TextError> {
    let chars = bounded_chars(s)?;
    let mut cur = Cursor::new(&chars);
    cur.skip_spaces();
    if cur.at_end() {
        return Err(TextError::Empty);
    }
    skip_weekday(&mut cur)?;
    let (first, first_len) = digits(&mut cur, 4)?;
    let mut glued = false;
    let day = if first_len == 4 {
        // ISO : AAAA-MM-JJ, puis `T` ou une espace avant l'heure.
        if cur.peek() != Some('-') {
            return Err(TextError::BadDate);
        }
        cur.bump();
        let (month, _) = digits(&mut cur, 2)?;
        date_separator(&mut cur, Some('-'))?;
        let (d, _) = digits(&mut cur, 2)?;
        if cur.peek() == Some('T') && cur.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            cur.bump();
            glued = true;
        }
        day_of(full_year(first, 4)?, month, d)?
    } else {
        let sep = date_separator(&mut cur, None)?;
        let (second, _) = digits(&mut cur, 2)?;
        date_separator(&mut cur, Some(sep))?;
        let (y, y_len) = digits(&mut cur, 4)?;
        let (d, month) = match ctx.date_order {
            DateOrder::DayMonthYear => (first, second),
            DateOrder::MonthDayYear => (second, first),
        };
        day_of(full_year(y, y_len)?, month, d)?
    };
    if !glued && cur.peek().is_some_and(|c| !is_space(c)) {
        return Err(TextError::BadDate);
    }
    let time = time_of_day(&mut cur)?.unwrap_or(match role {
        DateRole::Start => ctx.default_start_time,
        DateRole::Finish => ctx.default_finish_time,
    });
    Moment::at(day, time).map_err(|_| TextError::OutOfRange)
}

/// R-143 : écrit une date `JJ/MM/AAAA hh:mm` (JJ/MM) ou `MM/DD/YYYY hh:mm`
/// (MM/JJ), heure sur 24 heures. La fin du domaine (2150-01-01 00:00, R-003)
/// s'écrit « 31/12/2149 24:00 », relisible.
#[must_use]
pub fn format_date(m: Moment, ctx: &TextContext) -> String {
    let (dt, hour, minute) = if m == Moment::MAX {
        let last = Moment::from_minutes(m.minutes().saturating_sub(1)).unwrap_or(m);
        (last.to_naive(), 24, 0)
    } else {
        let dt = m.to_naive();
        (dt, dt.hour(), dt.minute())
    };
    let (a, b) = match ctx.date_order {
        DateOrder::DayMonthYear => (dt.day(), dt.month()),
        DateOrder::MonthDayYear => (dt.month(), dt.day()),
    };
    format!("{a:02}/{b:02}/{:04} {hour:02}:{minute:02}", dt.year())
}
