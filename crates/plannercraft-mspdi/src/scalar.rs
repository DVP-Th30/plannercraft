//! Valeurs scalaires de MSPDI : booléens, entiers, dates, heures, durées
//! ISO 8601 et dixièmes de minute (R-070, R-072, R-073, R-075, R-076).
//!
//! Chaque fonction est pure et totale : une écriture invalide rend `None` ou
//! une erreur typée, jamais une panique. L'appelant transforme l'échec en
//! diagnostic d'import et applique la valeur par défaut (correspondance
//! indulgente, ADR-0009). Aucun flottant : les décimales des durées sont lues
//! en entiers, puis arrondies par la règle unique R-001.

use chrono::NaiveDate;
use plannercraft_core::{Decimal, Minutes, Moment, TimeOfDay, round_div};

/// Booléen MSPDI (`xsd:boolean`) : `1`/`true` ou `0`/`false`, espaces de tête
/// et de fin ignorés (R-076). Toute autre écriture rend `None`.
#[must_use]
pub fn parse_bool(s: &str) -> Option<bool> {
    match s.trim() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

/// Entier signé (`xsd:int`, `xsd:integer`) sur 64 bits, espaces ignorés.
/// Dépassement ou écriture invalide : `None`.
#[must_use]
pub fn parse_int(s: &str) -> Option<i64> {
    s.trim().parse().ok()
}

/// Curseur sur les octets d'une valeur (lecture sans indexation paniquante).
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    const fn new(s: &'a str) -> Self {
        Self {
            bytes: s.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    /// Consomme `b` s'il est le prochain octet.
    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    /// Exactement `n` chiffres décimaux.
    fn fixed_digits(&mut self, n: usize) -> Option<u32> {
        let mut v: u32 = 0;
        for _ in 0..n {
            let d = self.peek().filter(u8::is_ascii_digit)?;
            v = v.checked_mul(10)?.checked_add(u32::from(d.checked_sub(b'0')?))?;
            self.bump();
        }
        Some(v)
    }

    /// Une suite non vide de chiffres, en `i128` ; `None` si vide. Le
    /// booléen vaut vrai si la valeur déborde (les chiffres sont consommés).
    fn digits_i128(&mut self) -> Option<(i128, bool)> {
        let mut v: i128 = 0;
        let mut count: usize = 0;
        let mut overflow = false;
        while let Some(d) = self.peek().filter(u8::is_ascii_digit) {
            match v
                .checked_mul(10)
                .and_then(|x| x.checked_add(i128::from(d.saturating_sub(b'0'))))
            {
                Some(x) => v = x,
                None => overflow = true,
            }
            count = count.saturating_add(1);
            self.bump();
        }
        (count > 0).then_some((v, overflow))
    }
}

/// Partie décimale lue : valeur ramenée à 9 chiffres, et vrai si des
/// chiffres non nuls au-delà du neuvième ont été ignorés.
fn fraction_nanos(c: &mut Cursor<'_>) -> Option<(i128, bool)> {
    let mut value: i128 = 0;
    let mut count: u32 = 0;
    let mut dropped = false;
    while let Some(d) = c.peek().filter(u8::is_ascii_digit) {
        let digit = i128::from(d.checked_sub(b'0')?);
        if count < 9 {
            value = value.checked_mul(10)?.checked_add(digit)?;
            count = count.saturating_add(1);
        } else if digit != 0 {
            dropped = true;
        }
        c.bump();
    }
    if count == 0 && !dropped {
        return None;
    }
    let pad = 10_i128.checked_pow(9_u32.saturating_sub(count))?;
    Some((value.checked_mul(pad)?, dropped))
}

/// Instant lu dans un `xsd:dateTime` MSPDI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedMoment {
    /// Instant, à la minute (secondes tronquées, R-004).
    pub moment: Moment,
    /// Vrai si des secondes ou fractions non nulles ont été tronquées.
    pub rounded: bool,
    /// Vrai si un suffixe de fuseau (`Z`, `±hh:mm`) a été ignoré.
    pub zone_ignored: bool,
}

/// Échec de lecture d'une date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DateTimeError {
    /// Écriture invalide.
    #[error("date invalide")]
    Syntax,
    /// Date valide hors du domaine 1984-2149 (R-003).
    #[error("date hors du domaine 1984-2149")]
    OutOfDomain,
}

/// Heure `hh:mm[:ss[.fff]]` : minutes depuis minuit (0 à 1 440) et
/// troncature des secondes. `24:00` n'est admis que sans secondes.
fn clock(c: &mut Cursor<'_>) -> Option<(u16, bool)> {
    let h = c.fixed_digits(2)?;
    if !c.eat(b':') {
        return None;
    }
    let m = c.fixed_digits(2)?;
    let mut rounded = false;
    if c.eat(b':') {
        let s = c.fixed_digits(2)?;
        if s >= 60 {
            return None;
        }
        rounded = s != 0;
        if c.eat(b'.') {
            let (nanos, dropped) = fraction_nanos(c)?;
            rounded = rounded || nanos != 0 || dropped;
        }
    }
    if m >= 60 || h > 24 || (h == 24 && (m != 0 || rounded)) {
        return None;
    }
    let minutes = u16::try_from(h.checked_mul(60)?.checked_add(m)?).ok()?;
    Some((minutes, rounded))
}

/// R-075 : `AAAA-MM-JJThh:mm[:ss[.fff]]`, sans fuseau. Un suffixe `Z` ou
/// `±hh:mm` est ignoré et signalé (`zone_ignored`) ; les secondes sont
/// tronquées (R-004) ; `24:00:00` désigne le lendemain à 00:00 (XSD).
///
/// # Errors
///
/// [`DateTimeError::Syntax`] pour une écriture invalide,
/// [`DateTimeError::OutOfDomain`] hors de 1984-2149.
pub fn parse_datetime(s: &str) -> Result<ParsedMoment, DateTimeError> {
    let mut c = Cursor::new(s.trim());
    let date = (|| {
        let y = c.fixed_digits(4)?;
        if !c.eat(b'-') {
            return None;
        }
        let mo = c.fixed_digits(2)?;
        if !c.eat(b'-') {
            return None;
        }
        let d = c.fixed_digits(2)?;
        if !c.eat(b'T') {
            return None;
        }
        let (minutes, rounded) = clock(&mut c)?;
        let zone_ignored = zone_suffix(&mut c)?;
        c.at_end().then_some(())?;
        let date = NaiveDate::from_ymd_opt(i32::try_from(y).ok()?, mo, d)?;
        Some((date, minutes, rounded, zone_ignored))
    })();
    let (date, minutes, rounded, zone_ignored) = date.ok_or(DateTimeError::Syntax)?;
    // 24:00 : minuit du lendemain (le 31/12/2149 à 24:00 est `Moment::MAX`).
    let (date, minutes) = if minutes == 1_440 {
        (date.succ_opt().ok_or(DateTimeError::OutOfDomain)?, 0)
    } else {
        (date, minutes)
    };
    let naive = date
        .and_hms_opt(u32::from(minutes / 60), u32::from(minutes % 60), 0)
        .ok_or(DateTimeError::Syntax)?;
    let (moment, _) = Moment::from_naive_floor(naive).map_err(|_| DateTimeError::OutOfDomain)?;
    Ok(ParsedMoment {
        moment,
        rounded,
        zone_ignored,
    })
}

/// Suffixe de fuseau : rien (`false`), `Z` ou `±hh:mm` (`true`) ; `None` si
/// l'écriture est invalide.
fn zone_suffix(c: &mut Cursor<'_>) -> Option<bool> {
    if c.at_end() {
        return Some(false);
    }
    if c.eat(b'Z') {
        return Some(true);
    }
    if c.eat(b'+') || c.eat(b'-') {
        let h = c.fixed_digits(2)?;
        if !c.eat(b':') {
            return None;
        }
        let m = c.fixed_digits(2)?;
        return (h <= 14 && m < 60).then_some(true);
    }
    None
}

/// R-077 : heure `hh:mm[:ss]` d'une plage (`FromTime`, `ToTime`). Rend l'heure
/// et la troncature des secondes ; `None` si l'écriture est invalide.
/// `00:00:00` reste minuit : c'est l'appelant qui lit une fin à 00:00 comme
/// 24:00.
#[must_use]
pub fn parse_time(s: &str) -> Option<(TimeOfDay, bool)> {
    let mut c = Cursor::new(s.trim());
    let (minutes, rounded) = clock(&mut c)?;
    if !c.at_end() {
        return None;
    }
    TimeOfDay::new(minutes).ok().map(|t| (t, rounded))
}

/// Durée ISO 8601 lue (R-070), en minutes signées.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IsoDuration {
    /// Minutes, arrondies par R-001 ; négatives pour `-P…`.
    pub minutes: i64,
    /// Vrai si des secondes (ou fractions) ont été arrondies à la minute.
    pub rounded: bool,
}

/// Échec de lecture d'une durée ISO 8601.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IsoDurationError {
    /// Écriture invalide.
    #[error("durée ISO 8601 invalide")]
    Syntax,
    /// Années ou mois non nuls : longueur variable, refusée.
    #[error("durée en années ou en mois refusée")]
    YearsOrMonths,
    /// Valeur trop grande.
    #[error("durée hors limites")]
    Overflow,
}

/// Nanosecondes dans une minute.
const NANOS_PER_MINUTE: i128 = 60_000_000_000;
/// Nanosecondes dans une seconde.
const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// R-070 : durée `[-]PnDTnHnMnS` (`xsd:duration`) en minutes. Un jour vaut
/// 24 heures ; des décimales sont admises dans la dernière composante
/// seulement ; les secondes sont arrondies à la minute par R-001 (`rounded`).
/// Années et mois non nuls refusés (longueur variable) ; semaines (`W`) hors
/// `xsd:duration`, refusées.
///
/// Exemples : `PT80H0M0S` → 4 800 ; `PT8H30M0S` → 510 ; `PT0H0M30S` → 1 ;
/// `PT1.5H` → 90 ; `P2D` → 2 880 ; `-PT8H` → −480.
///
/// # Errors
///
/// [`IsoDurationError`].
pub fn parse_iso_duration(s: &str) -> Result<IsoDuration, IsoDurationError> {
    let signed = iso_duration_nanos(s)?;
    let minutes = round_div(signed, NANOS_PER_MINUTE).ok_or(IsoDurationError::Overflow)?;
    let rounded = signed.checked_rem(NANOS_PER_MINUTE).is_some_and(|r| r != 0);
    Ok(IsoDuration { minutes, rounded })
}

/// Nanosecondes dans une minute, pour l'arrondi d'une somme de durées lues
/// par [`iso_duration_nanos`].
pub(crate) const ISO_NANOS_PER_MINUTE: i128 = NANOS_PER_MINUTE;

/// Durée `xsd:duration` en nanosecondes exactes (même syntaxe que
/// [`parse_iso_duration`], sans arrondi à la minute ; un chiffre au-delà de
/// la nanoseconde ajoute 1 ns). Sert à sommer des valeurs fractionnaires
/// (`TimephasedData`, O30) avant un seul arrondi (R-001, R-199).
pub(crate) fn iso_duration_nanos(s: &str) -> Result<i128, IsoDurationError> {
    let mut c = Cursor::new(s.trim());
    let negative = c.eat(b'-');
    if !negative {
        let _ = c.eat(b'+');
    }
    if !c.eat(b'P') {
        return Err(IsoDurationError::Syntax);
    }
    // Composantes dans l'ordre ISO : Y, M, D, puis T, H, M, S.
    // Rang du dernier désignateur lu (0 = aucun) pour imposer l'ordre.
    let mut last_rank: u8 = 0;
    let mut in_time = false;
    let mut components: u32 = 0;
    let mut time_components: u32 = 0;
    let mut saw_fraction = false;
    let mut total: i128 = 0;
    let mut years_or_months = false;
    loop {
        if c.at_end() {
            break;
        }
        if c.eat(b'T') {
            if in_time {
                return Err(IsoDurationError::Syntax);
            }
            in_time = true;
            continue;
        }
        if saw_fraction {
            // Une décimale n'est admise que dans la dernière composante.
            return Err(IsoDurationError::Syntax);
        }
        let (int, int_overflow) = c.digits_i128().ok_or(IsoDurationError::Syntax)?;
        let frac = if c.eat(b'.') {
            saw_fraction = true;
            Some(fraction_nanos(&mut c).ok_or(IsoDurationError::Syntax)?)
        } else {
            None
        };
        let designator = c.peek().ok_or(IsoDurationError::Syntax)?;
        c.bump();
        let (rank, seconds): (u8, i128) = match (in_time, designator) {
            (false, b'Y') => (1, 0),
            (false, b'M') => (2, 0),
            (false, b'D') => (3, 86_400),
            (true, b'H') => (4, 3_600),
            (true, b'M') => (5, 60),
            (true, b'S') => (6, 1),
            _ => return Err(IsoDurationError::Syntax),
        };
        if rank <= last_rank {
            return Err(IsoDurationError::Syntax);
        }
        last_rank = rank;
        components = components.saturating_add(1);
        if in_time {
            time_components = time_components.saturating_add(1);
        }
        let (frac_nanos, dropped) = frac.unwrap_or((0, false));
        if int_overflow {
            return Err(IsoDurationError::Overflow);
        }
        if rank <= 2 {
            if int != 0 || frac_nanos != 0 || dropped {
                years_or_months = true;
            }
            continue;
        }
        let unit = seconds
            .checked_mul(NANOS_PER_SECOND)
            .ok_or(IsoDurationError::Overflow)?;
        let part = int
            .checked_mul(unit)
            .and_then(|v| v.checked_add(frac_nanos.checked_mul(seconds)?))
            .ok_or(IsoDurationError::Overflow)?;
        total = total.checked_add(part).ok_or(IsoDurationError::Overflow)?;
        if dropped {
            // Chiffres au-delà de la nanoseconde : la valeur n'est plus exacte,
            // l'arrondi est signalé.
            total = total.checked_add(1).ok_or(IsoDurationError::Overflow)?;
        }
    }
    if components == 0 || (in_time && time_components == 0) {
        return Err(IsoDurationError::Syntax);
    }
    if years_or_months {
        return Err(IsoDurationError::YearsOrMonths);
    }
    if negative {
        total.checked_neg().ok_or(IsoDurationError::Overflow)
    } else {
        Ok(total)
    }
}

/// R-072 : `LinkLag` (dixièmes de minute) → minutes, `round_div(lag, 10)`
/// (R-001). Le booléen vaut vrai si un reste a été arrondi. Exemples :
/// 4 800 → 480 ; −4 800 → −480 ; 25 → 3 (arrondi) ; −25 → −3.
#[must_use]
pub fn tenths_to_minutes(tenths: i64) -> (Minutes, bool) {
    // |tenths / 10| tient toujours dans un i64 : le repli ne sert jamais.
    let minutes = round_div(i128::from(tenths), 10).unwrap_or(0);
    (
        Minutes::new(minutes),
        tenths.checked_rem(10).is_some_and(|r| r != 0),
    )
}

/// R-073 (hypothèse pure) : un décalage en pourcentage est écrit
/// `LinkLag` = pourcentage × 10, donc la valeur en 1/10 000 vaut
/// `LinkLag × 10`. Exemple : 500 → 5 000 (50 %). `None` en cas de
/// débordement.
#[must_use]
pub const fn percent_lag_from_link_lag(link_lag: i64) -> Option<i64> {
    link_lag.checked_mul(10)
}

/// R-120 : date `AAAA-MM-JJThh:mm:00`, sans fuseau (inverse de R-075).
/// Calcul direct du calendrier civil (l'écriture de 5 000 tâches en formate
/// plus de 50 000) ; repli sur l'affichage de `Moment` en cas d'échec.
#[must_use]
pub fn format_datetime(m: Moment) -> String {
    civil(m.minutes()).map_or_else(
        || format!("{m}:00"),
        |(y, mo, d, h, mi)| format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:00"),
    )
}

/// Minutes depuis le 01/01/1984 → (année, mois, jour, heure, minute), par
/// l'algorithme des jours civils (ère de 400 ans, mars en premier mois).
pub(crate) fn civil(minutes: i64) -> Option<(i64, i64, i64, i64, i64)> {
    let days = minutes.checked_div_euclid(1_440)?;
    let in_day = minutes.checked_rem_euclid(1_440)?;
    // 01/01/1984 = jour 5 113 depuis le 01/01/1970 ; 719 468 : 01/03/0000.
    let z = days.checked_add(5_113 + 719_468)?;
    let era = z.checked_div_euclid(146_097)?;
    let doe = z.checked_sub(era.checked_mul(146_097)?)?;
    let yoe = doe
        .checked_sub(doe.checked_div(1_460)?)?
        .checked_add(doe.checked_div(36_524)?)?
        .checked_sub(doe.checked_div(146_096)?)?
        .checked_div(365)?;
    let doy = doe.checked_sub(
        yoe.checked_mul(365)?
            .checked_add(yoe.checked_div(4)?)?
            .checked_sub(yoe.checked_div(100)?)?,
    )?;
    let mp = doy.checked_mul(5)?.checked_add(2)?.checked_div(153)?;
    let d = doy
        .checked_sub(mp.checked_mul(153)?.checked_add(2)?.checked_div(5)?)?
        .checked_add(1)?;
    let mo = if mp < 10 {
        mp.checked_add(3)?
    } else {
        mp.checked_sub(9)?
    };
    let y = yoe
        .checked_add(era.checked_mul(400)?)?
        .checked_add(i64::from(mo <= 2))?;
    Some((y, mo, d, in_day.checked_div(60)?, in_day.checked_rem(60)?))
}

/// R-120 : heure `hh:mm:00`. 24:00 s'écrit `00:00:00` pour une fin de
/// plage (`end_of_range`, R-077) et `24:00:00` sinon (heure par défaut du
/// projet).
#[must_use]
pub fn format_time(t: TimeOfDay, end_of_range: bool) -> String {
    let minutes = t.minutes();
    if minutes >= 1_440 {
        return if end_of_range { "00:00:00" } else { "24:00:00" }.to_owned();
    }
    format!("{:02}:{:02}:00", minutes / 60, minutes % 60)
}

/// R-120 : durée ISO 8601 `PT{h}H{m}M0S` (inverse de R-070), `-PT…` si
/// négative. Exemple : 2 400 → `PT40H0M0S`.
#[must_use]
pub fn format_iso_duration(minutes: i64) -> String {
    let sign = if minutes < 0 { "-" } else { "" };
    let abs = minutes.unsigned_abs();
    format!("{sign}PT{}H{}M0S", abs / 60, abs % 60)
}

/// R-120 : booléen `0` / `1`.
#[must_use]
pub const fn format_bool(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

/// R-120 : décimal au point, sans zéro final (`1.50` → `1.5`, `2.00` →
/// `2`).
#[must_use]
pub fn format_decimal(d: Decimal) -> String {
    d.normalized().to_string()
}

/// Décimal lu (`xsd:decimal`, `xsd:float` sans exposant) : valeur exacte au
/// plus à 6 décimales, et vrai si des décimales au-delà de la sixième ont
/// été arrondies (R-001).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedDecimal {
    /// Valeur.
    pub value: Decimal,
    /// Vrai si la valeur a été arrondie à 6 décimales.
    pub rounded: bool,
}

/// R-088 : nombre décimal `[+-]chiffres[.chiffres]` (espaces de tête et de
/// fin ignorés ; `.5` et `5.` admis), sans flottant. Au-delà de 6
/// décimales, arrondi par R-001 (`rounded`). Exposant, valeur vide ou trop
/// grande pour un `i64` : `None`. Exemples : `1.00` → 1 ; `4.15875` →
/// 4,15875 ; `0.5` → 0,5 ; `-3` → −3 ; `0.1234567` → 0,123457 (arrondi).
#[must_use]
pub fn parse_decimal(s: &str) -> Option<ParsedDecimal> {
    let t = s.trim();
    let (negative, body) = match t.as_bytes().first() {
        Some(b'-') => (true, t.get(1..)?),
        Some(b'+') => (false, t.get(1..)?),
        _ => (false, t),
    };
    let (int, frac) = body.split_once('.').unwrap_or((body, ""));
    if (int.is_empty() && frac.is_empty()) || !int.bytes().chain(frac.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits = |d: &str| -> Option<i128> {
        d.bytes().try_fold(0_i128, |acc, b| {
            acc.checked_mul(10)?.checked_add(i128::from(b.checked_sub(b'0')?))
        })
    };
    let max = usize::from(Decimal::MAX_SCALE);
    let (kept, dropped) = if frac.len() > max {
        (frac.get(..max)?, frac.get(max..)?)
    } else {
        (frac, "")
    };
    let scale = u8::try_from(kept.len()).ok()?;
    let mut mantissa = digits(int)?
        .checked_mul(10_i128.checked_pow(u32::from(scale))?)?
        .checked_add(digits(kept)?)?;
    let mut rounded = false;
    if !dropped.is_empty() {
        // Arrondi à 6 décimales : (mantisse × 10^k + reste) / 10^k, R-001.
        let k = u32::try_from(dropped.len()).ok()?;
        let rest = digits(dropped)?;
        rounded = rest != 0;
        let den = 10_i128.checked_pow(k)?;
        let num = mantissa.checked_mul(den)?.checked_add(rest)?;
        mantissa = i128::from(round_div(num, den)?);
    }
    if negative {
        mantissa = mantissa.checked_neg()?;
    }
    let value = Decimal::new(i64::try_from(mantissa).ok()?, scale).ok()?;
    Some(ParsedDecimal { value, rounded })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{
        DateTimeError, IsoDuration, IsoDurationError, format_bool, format_datetime, format_decimal,
        format_iso_duration, format_time, parse_bool, parse_datetime, parse_decimal, parse_int,
        parse_iso_duration, parse_time, percent_lag_from_link_lag, tenths_to_minutes,
    };
    use plannercraft_core::{Minutes, Moment, TimeOfDay};

    fn m(s: &str) -> Moment {
        s.parse().unwrap()
    }

    #[allow(
        clippy::unnecessary_wraps,
        reason = "aide de test : se compare directement au résultat de parse_iso_duration"
    )]
    fn iso(minutes: i64, rounded: bool) -> Result<IsoDuration, IsoDurationError> {
        Ok(IsoDuration { minutes, rounded })
    }

    #[test]
    fn booleans_and_integers() {
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool(" true "), Some(true));
        assert_eq!(parse_bool("0"), Some(false));
        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("2"), None);
        assert_eq!(parse_bool("TRUE"), None);
        assert_eq!(parse_bool(""), None);
        assert_eq!(parse_int(" -65533 "), Some(-65_533));
        assert_eq!(parse_int("+7"), Some(7));
        assert_eq!(parse_int("1.5"), None);
        assert_eq!(parse_int("99999999999999999999"), None);
        assert_eq!(parse_int(""), None);
    }

    #[test]
    fn iso_durations() {
        assert_eq!(parse_iso_duration("PT80H0M0S"), iso(4_800, false));
        assert_eq!(parse_iso_duration("PT0H0M0S"), iso(0, false));
        assert_eq!(parse_iso_duration("PT8H30M0S"), iso(510, false));
        assert_eq!(parse_iso_duration("PT1.5H"), iso(90, false));
        assert_eq!(parse_iso_duration("P2D"), iso(2_880, false));
        assert_eq!(parse_iso_duration("P1DT1H"), iso(1_500, false));
        // 30 s = 0,5 min → 1 (moitié loin de zéro), arrondi signalé.
        assert_eq!(parse_iso_duration("PT0H0M30S"), iso(1, true));
        assert_eq!(parse_iso_duration("PT0H0M29S"), iso(0, true));
        assert_eq!(parse_iso_duration("PT0H0M0.5S"), iso(0, true));
        assert_eq!(parse_iso_duration("-PT8H"), iso(-480, false));
        assert_eq!(parse_iso_duration("-PT0H0M30S"), iso(-1, true));
        // Années et mois : refusés s'ils sont non nuls.
        assert_eq!(parse_iso_duration("P1M"), Err(IsoDurationError::YearsOrMonths));
        assert_eq!(parse_iso_duration("P1Y"), Err(IsoDurationError::YearsOrMonths));
        assert_eq!(parse_iso_duration("P0Y0M0DT8H0M0S"), iso(480, false));
        // Écritures invalides.
        for bad in [
            "",
            "P",
            "PT",
            "8H",
            "PT8",
            "PT8X",
            "PT1.5H30M",
            "PT8H8H",
            "PT30M8H",
            "P1W",
            "PTT8H",
            "P.5D",
            "P1D T1H",
        ] {
            assert_eq!(parse_iso_duration(bad), Err(IsoDurationError::Syntax), "{bad}");
        }
        // 10^14 h tient en minutes ; 10^40 h déborde.
        assert_eq!(
            parse_iso_duration("PT99999999999999H"),
            iso(5_999_999_999_999_940, false)
        );
        assert_eq!(
            parse_iso_duration("PT9999999999999999999999999999999999999999H"),
            Err(IsoDurationError::Overflow)
        );
    }

    #[test]
    fn datetimes() {
        let p = parse_datetime("2009-11-09T08:00:00").unwrap();
        assert_eq!(p.moment, m("2009-11-09T08:00"));
        assert!(!p.rounded && !p.zone_ignored);
        let p = parse_datetime("2026-08-07T18:18:20").unwrap();
        assert_eq!(p.moment, m("2026-08-07T18:18"));
        assert!(p.rounded);
        let p = parse_datetime("2026-08-07T18:18").unwrap();
        assert_eq!(p.moment, m("2026-08-07T18:18"));
        let p = parse_datetime("2026-08-07T18:18:00.000").unwrap();
        assert!(!p.rounded);
        let p = parse_datetime("2026-08-07T18:18:00Z").unwrap();
        assert!(p.zone_ignored);
        let p = parse_datetime("2026-08-07T18:18:00+02:00").unwrap();
        assert_eq!(p.moment, m("2026-08-07T18:18"));
        assert!(p.zone_ignored);
        // 24:00:00 = lendemain 00:00.
        assert_eq!(
            parse_datetime("2026-10-19T24:00:00").unwrap().moment,
            m("2026-10-20T00:00")
        );
        assert_eq!(
            parse_datetime("1983-12-31T08:00:00"),
            Err(DateTimeError::OutOfDomain)
        );
        assert_eq!(parse_datetime("2150-01-01T00:00:00").unwrap().moment, Moment::MAX);
        assert_eq!(
            parse_datetime("2150-01-01T00:01:00"),
            Err(DateTimeError::OutOfDomain)
        );
        for bad in [
            "",
            "2026-10-19",
            "2026-13-01T08:00:00",
            "2026-02-30T08:00:00",
            "2026-10-19T25:00:00",
            "2026-10-19T24:00:01",
            "2026-10-19T08:60:00",
            "2026-10-19 08:00:00",
            "26-10-19T08:00:00",
            "2026-10-19T08:00:00+2",
            "2026-10-19T08:00:00garbage",
        ] {
            assert_eq!(parse_datetime(bad), Err(DateTimeError::Syntax), "{bad}");
        }
    }

    #[test]
    fn times_of_day() {
        assert_eq!(
            parse_time("08:00:00"),
            Some((TimeOfDay::hm(8, 0).unwrap(), false))
        );
        assert_eq!(parse_time("00:00:00"), Some((TimeOfDay::MIDNIGHT, false)));
        assert_eq!(parse_time("24:00:00"), Some((TimeOfDay::END_OF_DAY, false)));
        assert_eq!(parse_time("08:00:30"), Some((TimeOfDay::hm(8, 0).unwrap(), true)));
        assert_eq!(parse_time("8:00:00"), None);
        assert_eq!(parse_time("08:00:00Z"), None);
    }

    #[test]
    fn tenths_and_percent_lags() {
        assert_eq!(tenths_to_minutes(4_800), (Minutes::new(480), false));
        assert_eq!(tenths_to_minutes(-4_800), (Minutes::new(-480), false));
        assert_eq!(tenths_to_minutes(25), (Minutes::new(3), true));
        assert_eq!(tenths_to_minutes(-25), (Minutes::new(-3), true));
        assert_eq!(tenths_to_minutes(i64::MIN).0.get(), -922_337_203_685_477_581);
        assert_eq!(percent_lag_from_link_lag(500), Some(5_000));
        assert_eq!(percent_lag_from_link_lag(-250), Some(-2_500));
        assert_eq!(percent_lag_from_link_lag(i64::MAX), None);
    }

    #[test]
    fn r088_decimals() {
        let d = |t: &str| parse_decimal(t).map(|p| (p.value.normalized().to_string(), p.rounded));
        assert_eq!(d("1.00"), Some(("1".to_owned(), false)));
        assert_eq!(d(" 4.15875 "), Some(("4.15875".to_owned(), false)));
        assert_eq!(d("0.5"), Some(("0.5".to_owned(), false)));
        assert_eq!(d(".5"), Some(("0.5".to_owned(), false)));
        assert_eq!(d("5."), Some(("5".to_owned(), false)));
        assert_eq!(d("-3"), Some(("-3".to_owned(), false)));
        assert_eq!(d("+2.25"), Some(("2.25".to_owned(), false)));
        assert_eq!(d("0.1234567"), Some(("0.123457".to_owned(), true)));
        assert_eq!(d("-0.0000005"), Some(("-0.000001".to_owned(), true)));
        assert_eq!(d("1.0000000"), Some(("1".to_owned(), false)));
        for bad in ["", ".", "-", "1e5", "1,5", "abc", "1.2.3", "99999999999999999999"] {
            assert_eq!(parse_decimal(bad), None, "{bad}");
        }
    }

    #[test]
    fn r120_value_forms_round_trip() {
        let at = m("2026-10-19T08:00");
        assert_eq!(format_datetime(at), "2026-10-19T08:00:00");
        assert_eq!(parse_datetime(&format_datetime(at)).unwrap().moment, at);
        assert_eq!(format_datetime(Moment::MAX), "2150-01-01T00:00:00");
        assert_eq!(
            parse_datetime(&format_datetime(Moment::MAX)).unwrap().moment,
            Moment::MAX
        );
        assert_eq!(format_time(TimeOfDay::hm(8, 5).unwrap(), true), "08:05:00");
        assert_eq!(format_time(TimeOfDay::END_OF_DAY, true), "00:00:00");
        assert_eq!(format_time(TimeOfDay::END_OF_DAY, false), "24:00:00");
        assert_eq!(parse_time("24:00:00").unwrap().0, TimeOfDay::END_OF_DAY);
        for minutes in [0, 1, 59, 60, 480, 2_400, 87_308_640, -480, -1] {
            let text = format_iso_duration(minutes);
            assert_eq!(parse_iso_duration(&text).unwrap().minutes, minutes, "{text}");
        }
        assert_eq!(format_iso_duration(2_400), "PT40H0M0S");
        assert_eq!(format_iso_duration(-90), "-PT1H30M0S");
        assert_eq!(format_bool(true), "1");
        assert_eq!(format_bool(false), "0");
        let d = |m: i64, s: u8| plannercraft_core::Decimal::new(m, s).unwrap();
        assert_eq!(format_decimal(d(150, 2)), "1.5");
        assert_eq!(format_decimal(d(200, 2)), "2");
        assert_eq!(format_decimal(d(-415_875, 5)), "-4.15875");
        assert_eq!(format_decimal(d(5, 1)), "0.5");
    }

    #[test]
    fn r120_fast_dates_match_the_calendar() {
        // Tout le domaine, un jour sur 7 et une minute variable : même texte
        // que l'affichage de `Moment` (chrono).
        let mut minutes = 0_i64;
        while minutes <= Moment::MAX.minutes() {
            let at = Moment::from_minutes(minutes).unwrap();
            assert_eq!(format_datetime(at), format!("{at}:00"), "{minutes}");
            minutes += 7 * 1_440 + 37;
        }
        assert_eq!(format_datetime(Moment::MAX), "2150-01-01T00:00:00");
        assert_eq!(format_datetime(Moment::MIN), "1984-01-01T00:00:00");
        assert_eq!(format_datetime(m("2024-02-29T23:59")), "2024-02-29T23:59:00");
        assert_eq!(format_datetime(m("2100-03-01T00:00")), "2100-03-01T00:00:00");
    }
}
