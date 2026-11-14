//! Valeurs des cellules (étape 5) : durées, dates, prédécesseurs, %
//! achevé, en français et en anglais, avec les variantes d'écriture vues
//! dans les exports MS Project (« jrs », « moiséc », « edays », « 1 jour? »,
//! « Lun 03/04/15 », « Mon 1/21/19 », « 21 janv. 2019 », « Jan 21 '19 »).
//!
//! Le texte est d'abord ramené à une écriture canonique, puis lu par les
//! analyseurs de `plannercraft-automation` (R-140 à R-143) : mêmes
//! durées, mêmes décalages, mêmes facteurs de conversion que la saisie.
//! Aucun flottant.

use chrono::{Datelike, NaiveDate, Weekday};
use plannercraft_automation::text::{Language, TextContext, parse_duration, parse_lag};
use plannercraft_core::{Duration, Lag, LinkType, TimeOfDay};
use serde::Serialize;

use crate::lines::fold;

/// Unité canonique d'une écriture d'unité repliée (points finaux
/// retirés), sans la base : `j`, `s`, `h`, `min`, `mo`.
fn base_unit(u: &str) -> Option<&'static str> {
    Some(match u {
        "j" | "jr" | "jrs" | "jour" | "jours" | "d" | "dy" | "dys" | "day" | "days" => "j",
        "s" | "sm" | "sms" | "sem" | "sems" | "semaine" | "semaines" | "w" | "wk" | "wks" | "week"
        | "weeks" => "s",
        "h" | "hr" | "hrs" | "heure" | "heures" | "hour" | "hours" => "h",
        "m" | "mn" | "min" | "mins" | "minute" | "minutes" => "min",
        "mo" | "mos" | "mois" | "ms" | "mon" | "mons" | "month" | "months" => "mo",
        _ => return None,
    })
}

/// Unité canonique et base écoulée d'une écriture d'unité : exacte, puis
/// suffixe « éc » / « é » (« moiséc », « jé »), puis préfixe « e »
/// (« edays », « emo »). Les mots « écoulé(e)(s) » / « elapsed » qui
/// suivent l'unité marquent aussi l'écoulé (traités par l'appelant).
///
/// « m » seul : mois dans un document français (« 3 m » lu sur un planning
/// du propriétaire couvre 60 jours ouvrés ; le français note les minutes
/// « mn » ou « min »), minutes ailleurs (« m » de MS Project en anglais).
fn unit_of(folded: &str, lang: Language) -> Option<(&'static str, bool)> {
    let u = folded.trim_end_matches('.');
    if lang == Language::Fr && u == "m" {
        return Some(("mo", false));
    }
    if let Some(b) = base_unit(u) {
        return Some((b, false));
    }
    for suffix in ["ec", "e"] {
        if let Some(b) = u
            .strip_suffix(suffix)
            .map(|s| s.trim_end_matches('.'))
            .and_then(base_unit)
        {
            return Some((b, true));
        }
    }
    u.strip_prefix('e').and_then(base_unit).map(|b| (b, true))
}

/// Durée ou décalage ramené à l'écriture canonique de R-140 : nombre tel
/// quel, unité canonique, « e » devant si écoulé, « ? » final si estimé.
/// `signed` : un signe `+`/`-` est admis en tête (décalage).
fn canonical_amount(text: &str, signed: bool, lang: Language) -> Option<String> {
    let folded = fold(text);
    let mut rest = folded.as_str().trim();
    let mut out = String::new();
    if signed && let Some(c) = rest.chars().next().filter(|c| matches!(c, '+' | '-')) {
        out.push(c);
        rest = rest.get(c.len_utf8()..).unwrap_or("").trim_start();
    }
    let estimated = rest.ends_with('?');
    let rest = rest.trim_end_matches('?').trim_end();
    let split = rest
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_digit() || matches!(c, ',' | '.' | ' ')))
        .map_or(rest.len(), |(i, _)| i);
    let number: String = rest
        .get(..split)
        .unwrap_or("")
        .chars()
        .filter(|c| *c != ' ')
        .collect();
    if number.is_empty() || !number.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    let unit_text = rest.get(split..).unwrap_or("").trim();
    if unit_text.starts_with('%') {
        // Décalage en pourcentage (« 50% », « 25 e% »).
        if !signed {
            return None;
        }
        out.push_str(&number);
        out.push_str(unit_text);
        return Some(out);
    }
    let mut words = unit_text.split_whitespace();
    let unit = match words.next() {
        None => None,
        Some(u) => Some(unit_of(u, lang)?),
    };
    let mut elapsed = unit.is_some_and(|(_, e)| e);
    for extra in words {
        let e = extra.trim_end_matches('.');
        if e.starts_with("ecoule") || e == "elapsed" || e == "ec" {
            elapsed = true;
        } else {
            return None;
        }
    }
    out.push_str(&number);
    if let Some((u, _)) = unit {
        if elapsed {
            out.push('e');
        }
        out.push_str(u);
    } else if elapsed {
        return None;
    }
    if estimated {
        out.push('?');
    }
    Some(out)
}

/// Contexte de lecture dans l'autre langue (marque décimale opposée).
fn other(ctx: &TextContext) -> TextContext {
    let language = match ctx.language {
        Language::Fr => Language::En,
        Language::En => Language::Fr,
    };
    TextContext {
        language,
        decimal: match language {
            Language::Fr => plannercraft_automation::text::DecimalMark::Comma,
            Language::En => plannercraft_automation::text::DecimalMark::Dot,
        },
        ..*ctx
    }
}

/// Lit une durée de cellule (« 22 jours », « 1 jour? », « 14,2 moiséc »,
/// « 5 edays », « 0 days »). Une valeur sans unité prend l'unité par défaut
/// du contexte (jours).
#[must_use]
pub fn duration(text: &str, ctx: &TextContext) -> Option<Duration> {
    let canonical = canonical_amount(text, false, ctx.language)?;
    parse_duration(&canonical, ctx)
        .or_else(|_| parse_duration(&canonical, &other(ctx)))
        .ok()
}

/// Lit un décalage (« +2 jours », « -1 sem », « +50% »).
#[must_use]
pub fn lag(text: &str, ctx: &TextContext) -> Option<Lag> {
    let canonical = canonical_amount(text, true, ctx.language)?;
    parse_lag(&canonical, ctx)
        .or_else(|_| parse_lag(&canonical, &other(ctx)))
        .ok()
}

/// Prédécesseur lu : ID, type, décalage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PredRef {
    /// ID du prédécesseur.
    pub id: u32,
    /// Type de lien.
    pub kind: LinkType,
    /// Décalage.
    pub lag: Lag,
}

/// Élément de prédécesseur refusé, et pourquoi.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PredError {
    /// Forme illisible.
    Unreadable,
    /// Prédécesseur d'un autre projet (chemin de fichier).
    External,
}

/// Type de lien d'un code replié : FD/FS, DD/SS, FF, DF/SF.
fn link_kind(code: &str) -> Option<LinkType> {
    Some(match code {
        "fd" | "fs" => LinkType::FinishToStart,
        "dd" | "ss" => LinkType::StartToStart,
        "ff" => LinkType::FinishToFinish,
        "df" | "sf" => LinkType::StartToFinish,
        _ => return None,
    })
}

/// Lit une liste de prédécesseurs (« 3FD+6 jours;5 », « 13,26FS-11 days »).
/// Séparateur : `;` s'il y en a un, sinon `,` (une virgule décimale d'un
/// décalage français reste lisible tant que la liste utilise `;`).
#[must_use]
pub fn predecessors(text: &str, ctx: &TextContext) -> Vec<Result<PredRef, (String, PredError)>> {
    let separator = if text.contains(';') { ';' } else { ',' };
    text.split(separator)
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(|element| predecessor(element, ctx).map_err(|e| (element.to_owned(), e)))
        .collect()
}

fn predecessor(element: &str, ctx: &TextContext) -> Result<PredRef, PredError> {
    if element.contains(['\\', '/', ':']) {
        return Err(PredError::External);
    }
    let folded = fold(element);
    let digits_end = folded
        .char_indices()
        .find(|(_, c)| !c.is_ascii_digit())
        .map_or(folded.len(), |(i, _)| i);
    let id: u32 = folded
        .get(..digits_end)
        .and_then(|d| d.parse().ok())
        .filter(|id| *id > 0)
        .ok_or(PredError::Unreadable)?;
    let rest = folded.get(digits_end..).unwrap_or("").trim_start();
    let letters_end = rest
        .char_indices()
        .find(|(_, c)| !c.is_ascii_alphabetic())
        .map_or(rest.len(), |(i, _)| i);
    let code = rest.get(..letters_end).unwrap_or("");
    let kind = if code.is_empty() {
        LinkType::FinishToStart
    } else {
        link_kind(code).ok_or(PredError::Unreadable)?
    };
    let lag_text = rest.get(letters_end..).unwrap_or("").trim();
    let lag = if lag_text.is_empty() {
        Lag::ZERO
    } else if lag_text.starts_with(['+', '-']) {
        lag(lag_text, ctx).ok_or(PredError::Unreadable)?
    } else {
        return Err(PredError::Unreadable);
    };
    Ok(PredRef { id, kind, lag })
}

/// Lit un pourcentage (« 50% », « 100 % »), en points de pourcentage
/// entiers de 0 à 100.
#[must_use]
pub fn percent(text: &str) -> Option<u8> {
    let t = text.trim().trim_end_matches('%').trim();
    let v: u8 = t.parse().ok()?;
    (v <= 100).then_some(v)
}

/// Ordre du jour et du mois d'une date numérique.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericOrder {
    /// JJ/MM/AA.
    DayMonth,
    /// MM/JJ/AA.
    MonthDay,
}

/// Date lue dans une cellule, avant le choix de l'ordre jour/mois.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateToken {
    /// Forme.
    pub form: DateForm,
    /// Heure lue, s'il y en a une.
    pub time: Option<TimeOfDay>,
    /// Jour de la semaine lu (« Lun », « Mon »), s'il y en a un.
    pub weekday: Option<Weekday>,
}

/// Forme d'une date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateForm {
    /// Date sans ambiguïté (mois en lettres, ISO).
    Exact(NaiveDate),
    /// Date numérique `a/b/année` : l'ordre se décide sur le document.
    Numeric {
        /// Premier nombre.
        a: u32,
        /// Second nombre.
        b: u32,
        /// Année (quatre chiffres).
        year: i32,
    },
}

impl DateToken {
    /// Jour selon un ordre (ignoré pour une date exacte) ; `None` si la
    /// date n'existe pas dans cet ordre.
    #[must_use]
    pub fn day(&self, order: NumericOrder) -> Option<NaiveDate> {
        match self.form {
            DateForm::Exact(d) => Some(d),
            DateForm::Numeric { a, b, year } => match order {
                NumericOrder::DayMonth => NaiveDate::from_ymd_opt(year, b, a),
                NumericOrder::MonthDay => NaiveDate::from_ymd_opt(year, a, b),
            },
        }
    }

    /// Vrai si la date se lit dans cet ordre et s'accorde avec le jour de
    /// la semaine lu (s'il y en a un).
    #[must_use]
    pub fn fits(&self, order: NumericOrder) -> bool {
        self.day(order)
            .is_some_and(|d| self.weekday.is_none_or(|w| d.weekday() == w))
    }

    /// Vrai si la date numérique se lit différemment selon l'ordre.
    #[must_use]
    pub fn is_ambiguous(&self) -> bool {
        match self.form {
            DateForm::Exact(_) => false,
            DateForm::Numeric { a, b, .. } => a != b && a <= 12 && b <= 12,
        }
    }
}

/// Jours de la semaine, repliés, en français et en anglais.
fn weekday(word: &str) -> Option<Weekday> {
    let w = word.trim_end_matches(['.', ',']);
    Some(match w {
        "lun" | "lundi" | "lu" | "mon" | "monday" | "mo" => Weekday::Mon,
        "mar" | "mardi" | "ma" | "tue" | "tues" | "tuesday" | "tu" => Weekday::Tue,
        "mer" | "mercredi" | "me" | "wed" | "wednesday" | "we" => Weekday::Wed,
        "jeu" | "jeudi" | "je" | "thu" | "thur" | "thurs" | "thursday" | "th" => Weekday::Thu,
        "ven" | "vendredi" | "ve" | "fri" | "friday" | "fr" => Weekday::Fri,
        "sam" | "samedi" | "sa" | "sat" | "saturday" => Weekday::Sat,
        "dim" | "dimanche" | "di" | "sun" | "sunday" | "su" => Weekday::Sun,
        _ => return None,
    })
}

/// Mois en lettres, repliés, en français et en anglais.
fn month(word: &str) -> Option<u32> {
    let w = word.trim_end_matches(['.', ',']);
    Some(match w {
        "jan" | "janv" | "janvier" | "january" => 1,
        "feb" | "fev" | "fevr" | "fevrier" | "february" => 2,
        "mar" | "mars" | "march" => 3,
        "apr" | "avr" | "avril" | "april" => 4,
        "may" | "mai" => 5,
        "jun" | "juin" | "june" => 6,
        "jul" | "juil" | "juillet" | "july" => 7,
        "aug" | "aou" | "aout" | "august" => 8,
        "sep" | "sept" | "septembre" | "september" => 9,
        "oct" | "octobre" | "october" => 10,
        "nov" | "novembre" | "november" => 11,
        "dec" | "decembre" | "december" => 12,
        _ => return None,
    })
}

/// Année sur 2 ou 4 chiffres (84 à 99 → 1984-1999, 00 à 83 → 2000-2083,
/// comme R-143).
fn full_year(text: &str) -> Option<i32> {
    let t = text.trim_start_matches(['\'', '\u{2019}']);
    if !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let v: i32 = t.parse().ok()?;
    match t.len() {
        4 => Some(v),
        2 if v >= 84 => v.checked_add(1900),
        2 => v.checked_add(2000),
        _ => None,
    }
}

/// Jeton d'une date : nombre (texte), mot, ou séparateur.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Num(String),
    Word(String),
    Sep(char),
}

fn tokens(folded: &str) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::new();
    // Une espace termine le nombre ou le mot en cours.
    let mut boundary = true;
    for c in folded.chars() {
        let last = if boundary { None } else { out.last_mut() };
        boundary = false;
        match (c, last) {
            (c, Some(Tok::Num(n))) if c.is_ascii_digit() => n.push(c),
            (c, _) if c.is_ascii_digit() => out.push(Tok::Num(c.to_string())),
            (c, Some(Tok::Word(w))) if c.is_alphabetic() || c == '.' => w.push(c),
            (c, _) if c.is_alphabetic() => out.push(Tok::Word(c.to_string())),
            ('\'' | '\u{2019}', _) => out.push(Tok::Sep('\'')),
            (c, _) if c.is_whitespace() => boundary = true,
            (c, _) => out.push(Tok::Sep(c)),
        }
    }
    out
}

/// Heure en fin de texte : `h:mm`, `hh:mm`, `hh:mm:ss`, `AM`/`PM`
/// facultatif ; rend l'heure et le texte qui précède.
fn split_time(folded: &str) -> Result<(Option<TimeOfDay>, &str), ()> {
    let Some(colon) = folded.rfind(':') else {
        return Ok((None, folded));
    };
    let before = folded.get(..colon).unwrap_or("");
    let hour_start = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map_or(colon, |(i, _)| i);
    let hours: u16 = before.get(hour_start..).and_then(|h| h.parse().ok()).ok_or(())?;
    let after = folded.get(colon.saturating_add(1)..).unwrap_or("");
    let minutes_text: String = after.chars().take_while(char::is_ascii_digit).collect();
    if minutes_text.len() != 2 {
        return Err(());
    }
    let minutes: u16 = minutes_text.parse().map_err(|_| ())?;
    let mut tail = after.get(2..).unwrap_or("").trim();
    if let Some(rest) = tail.strip_prefix(':') {
        tail = rest.trim_start_matches(|c: char| c.is_ascii_digit()).trim();
    }
    let suffix: String = tail.chars().filter(|c| *c != '.').collect();
    let hours = match suffix.as_str() {
        "" => hours,
        "am" | "pm" => {
            if !(1..=12).contains(&hours) {
                return Err(());
            }
            let base = if hours == 12 { 0 } else { hours };
            if suffix == "pm" {
                base.saturating_add(12)
            } else {
                base
            }
        }
        _ => return Err(()),
    };
    let time = TimeOfDay::hm(hours, minutes).map_err(|_| ())?;
    Ok((Some(time), before.get(..hour_start).unwrap_or("").trim_end()))
}

/// Lit une date de cellule. Formes : jour de la semaine facultatif, puis
/// `a/b/aa(aa)` (séparateurs `/`, `.`, `-`), `aaaa-mm-jj`, `jj mois aa(aa)`,
/// `mois jj[,] [']aa(aa)` ; heure facultative en fin.
#[must_use]
pub fn date(text: &str) -> Option<DateToken> {
    let folded = fold(text);
    let (time, rest) = split_time(&folded).ok()?;
    let toks = tokens(rest);
    // Jour de la semaine en tête, s'il est suivi d'autre chose.
    let (weekday, body): (Option<Weekday>, &[Tok]) = match toks.split_first() {
        Some((Tok::Word(w), tail)) if !tail.is_empty() => match weekday(w) {
            Some(d) if date_form(tail).is_some() => (Some(d), tail),
            _ => (None, toks.as_slice()),
        },
        _ => (None, toks.as_slice()),
    };
    let body: Vec<Tok> = body
        .iter()
        .filter(|t| !matches!(t, Tok::Sep(','))) // « Jan 21, 2019 »
        .cloned()
        .collect();
    let form = date_form(&body)?;
    Some(DateToken { form, time, weekday })
}

fn date_form(toks: &[Tok]) -> Option<DateForm> {
    let toks: Vec<&Tok> = toks.iter().filter(|t| !matches!(t, Tok::Sep(','))).collect();
    match toks.as_slice() {
        // aaaa-mm-jj
        [
            Tok::Num(y),
            Tok::Sep('-'),
            Tok::Num(m),
            Tok::Sep('-'),
            Tok::Num(d),
        ] if y.len() == 4 => {
            let date = NaiveDate::from_ymd_opt(full_year(y)?, m.parse().ok()?, d.parse().ok()?)?;
            Some(DateForm::Exact(date))
        }
        // a/b/aa
        [Tok::Num(a), Tok::Sep(s1), Tok::Num(b), Tok::Sep(s2), Tok::Num(y)]
            if s1 == s2 && matches!(s1, '/' | '.' | '-') && a.len() <= 2 && b.len() <= 2 =>
        {
            Some(DateForm::Numeric {
                a: a.parse().ok()?,
                b: b.parse().ok()?,
                year: full_year(y)?,
            })
        }
        // jj mois aa / jj-mois-aa
        [Tok::Num(d), rest @ ..] => {
            let rest: Vec<&&Tok> = rest
                .iter()
                .filter(|t| !matches!(t, Tok::Sep('-' | '/')))
                .collect();
            match rest.as_slice() {
                [Tok::Word(m), Tok::Num(y)] | [Tok::Word(m), Tok::Sep('\''), Tok::Num(y)] => {
                    let date = NaiveDate::from_ymd_opt(full_year(y)?, month(m)?, d.parse().ok()?)?;
                    Some(DateForm::Exact(date))
                }
                _ => None,
            }
        }
        // mois jj aa / mois jj 'aa
        [Tok::Word(m), Tok::Num(d), Tok::Num(y)]
        | [Tok::Word(m), Tok::Num(d), Tok::Sep('\''), Tok::Num(y)] => {
            let date = NaiveDate::from_ymd_opt(full_year(y)?, month(m)?, d.parse().ok()?)?;
            Some(DateForm::Exact(date))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use plannercraft_core::{DurationUnit, LagValue, ProjectOptions, TimeBasis};

    use super::*;

    fn ctx(lang: Language) -> TextContext {
        TextContext::for_options(&ProjectOptions::default(), lang)
    }

    fn minutes(text: &str, lang: Language) -> Option<(i64, DurationUnit, TimeBasis, bool)> {
        duration(text, &ctx(lang)).map(|d| (d.minutes().get(), d.unit(), d.basis(), d.is_estimated()))
    }

    #[test]
    fn duration_units_seen_in_exports() {
        use DurationUnit::{Day, Hour, Minute, Month, Week};
        use TimeBasis::{Elapsed, Working};
        let fr = Language::Fr;
        let en = Language::En;
        assert_eq!(minutes("22 jours", fr), Some((22 * 480, Day, Working, false)));
        assert_eq!(minutes("1 jour?", fr), Some((480, Day, Working, true)));
        assert_eq!(minutes("0 jour", fr), Some((0, Day, Working, false)));
        assert_eq!(minutes("3 jrs", fr), Some((3 * 480, Day, Working, false)));
        assert_eq!(minutes("2 jr", fr), Some((960, Day, Working, false)));
        assert_eq!(minutes("1 sem.", fr), Some((2400, Week, Working, false)));
        assert_eq!(minutes("2 mois", fr), Some((2 * 9600, Month, Working, false)));
        assert_eq!(minutes("1,5 j", fr), Some((720, Day, Working, false)));
        assert_eq!(minutes("8 h", fr), Some((480, Hour, Working, false)));
        assert_eq!(minutes("30 min", fr), Some((30, Minute, Working, false)));
        // « m » seul : mois en français (planning du propriétaire, lot P1b),
        // minutes en anglais.
        assert_eq!(minutes("3 m", fr), Some((3 * 9600, Month, Working, false)));
        assert_eq!(minutes("3 m", en), Some((3, Minute, Working, false)));
        assert_eq!(minutes("30 mn", fr), Some((30, Minute, Working, false)));
        assert_eq!(minutes("157 days", en), Some((157 * 480, Day, Working, false)));
        assert_eq!(minutes("1 day?", en), Some((480, Day, Working, true)));
        assert_eq!(minutes("1.5 wks", en), Some((3600, Week, Working, false)));
        assert_eq!(minutes("2 mons", en), Some((2 * 9600, Month, Working, false)));
        assert_eq!(minutes("4 hrs", en), Some((240, Hour, Working, false)));
        assert_eq!(minutes("5d", en), Some((2400, Day, Working, false)));
        // Écoulé : 24 h par jour (R-006).
        assert_eq!(minutes("5 edays", en), Some((5 * 1440, Day, Elapsed, false)));
        assert_eq!(
            minutes("3 jours écoulés", fr),
            Some((3 * 1440, Day, Elapsed, false))
        );
        assert!(minutes("14,2 moiséc", fr).is_some_and(|(_, u, b, _)| u == Month && b == Elapsed));
        // Sans unité : jours par défaut.
        assert_eq!(minutes("3", fr), Some((1440, Day, Working, false)));
        assert_eq!(minutes("abc", fr), None);
        assert_eq!(minutes("5 bananes", fr), None);
        assert_eq!(minutes("", fr), None);
    }

    #[test]
    fn predecessors_fr_and_en() {
        let fr = ctx(Language::Fr);
        let p = predecessors("3FD+6 jours;5;7DD", &fr);
        assert_eq!(p.len(), 3);
        let first = p[0].as_ref().unwrap();
        assert_eq!((first.id, first.kind), (3, LinkType::FinishToStart));
        assert_eq!(
            first.lag.value,
            LagValue::Time {
                minutes: plannercraft_core::Minutes::new(6 * 480),
                unit: DurationUnit::Day
            }
        );
        assert_eq!(p[2].as_ref().unwrap().kind, LinkType::StartToStart);
        let en = ctx(Language::En);
        let p = predecessors("13,26FS-11 days,34SS+2 wks", &en);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].as_ref().unwrap().id, 13);
        let second = p[1].as_ref().unwrap();
        assert_eq!(second.id, 26);
        assert_eq!(
            second.lag.value,
            LagValue::Time {
                minutes: plannercraft_core::Minutes::new(-11 * 480),
                unit: DurationUnit::Day
            }
        );
        assert_eq!(p[2].as_ref().unwrap().kind, LinkType::StartToStart);
        let p = predecessors("4DF;8FF-1 sem;12FD+50%", &fr);
        assert_eq!(p[0].as_ref().unwrap().kind, LinkType::StartToFinish);
        assert_eq!(p[1].as_ref().unwrap().kind, LinkType::FinishToFinish);
        assert!(matches!(
            p[2].as_ref().unwrap().lag.value,
            LagValue::Percent { .. }
        ));
        let bad = predecessors("C:\\autre.mpp\\12;abc", &fr);
        assert_eq!(bad[0].clone().unwrap_err().1, PredError::External);
        assert_eq!(bad[1].clone().unwrap_err().1, PredError::Unreadable);
    }

    #[test]
    fn dates_numeric_textual_and_weekdays() {
        let d = date("Ven 03/04/15").unwrap();
        assert_eq!(d.weekday, Some(Weekday::Fri));
        assert!(d.is_ambiguous());
        assert!(d.fits(NumericOrder::DayMonth));
        assert!(!d.fits(NumericOrder::MonthDay));
        let d = date("Mon 1/21/19").unwrap();
        assert!(d.fits(NumericOrder::MonthDay));
        assert!(!d.fits(NumericOrder::DayMonth));
        let d = date("lun. 21/01/2019 08:00").unwrap();
        assert_eq!(d.time, TimeOfDay::hm(8, 0).ok());
        assert_eq!(
            d.day(NumericOrder::DayMonth),
            NaiveDate::from_ymd_opt(2019, 1, 21)
        );
        let d = date("1/21/19 5:00 PM").unwrap();
        assert_eq!(d.time, TimeOfDay::hm(17, 0).ok());
        for (text, y, m, day) in [
            ("21 janv. 2019", 2019, 1, 21),
            ("21-Jan-19", 2019, 1, 21),
            ("Jan 21, 2019", 2019, 1, 21),
            ("Jan 21 '19", 2019, 1, 21),
            ("Mon January 21, 2019", 2019, 1, 21),
            ("mar. 5 mars 2019", 2019, 3, 5),
            ("2019-01-21", 2019, 1, 21),
            ("Mar 5, 19", 2019, 3, 5),
        ] {
            let d = date(text).unwrap();
            assert_eq!(
                d.day(NumericOrder::DayMonth),
                NaiveDate::from_ymd_opt(y, m, day),
                "{text}"
            );
            assert!(!d.is_ambiguous());
        }
        assert_eq!(date("NA"), None);
        assert_eq!(date("Mon 1/21"), None);
        assert_eq!(date("12 jours"), None);
        assert_eq!(
            date("31/02/2019").and_then(|d| d.day(NumericOrder::DayMonth)),
            None
        );
    }

    #[test]
    fn percent_values() {
        assert_eq!(percent("50%"), Some(50));
        assert_eq!(percent("100 %"), Some(100));
        assert_eq!(percent("120%"), None);
    }
}
