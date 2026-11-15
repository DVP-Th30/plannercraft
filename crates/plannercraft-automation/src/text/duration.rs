//! Durées et décalages en texte (R-140, R-141).

use plannercraft_core::{
    ConversionFactors, Decimal, Duration, DurationError, DurationUnit, Lag, LagValue, Minutes, Resolution,
    SignedPercent, TimeBasis, round_div,
};

use super::lex::{Cursor, bounded_chars, fold, format_decimal, is_space};
use super::{Language, TextContext, TextError};

/// Résolution d'une saisie en texte qui entre dans le modèle (R-140) : la
/// minute entière pendant les lots B2-B3, comme avant l'ADR-0039 ; le
/// dixième au lot B4, quand le calcul prendra la fraction en charge. Une
/// saisie à la minute arrondit le décimal lu une seule fois, par R-001
/// (« 0,01j » → 5 min), jamais en passant par le dixième.
pub const ENTRY_RESOLUTION: Resolution = Resolution::Minute;

/// R-140 : unité ouvrée d'une écriture repliée (casse et accents ignorés,
/// points finaux retirés).
fn working_unit(folded: &str) -> Option<DurationUnit> {
    Some(match folded {
        "m" | "mn" | "min" | "mins" | "minute" | "minutes" => DurationUnit::Minute,
        "h" | "hr" | "hrs" | "heure" | "heures" | "hour" | "hours" => DurationUnit::Hour,
        "j" | "jr" | "jour" | "jours" | "d" | "dy" | "day" | "days" => DurationUnit::Day,
        "s" | "sem" | "semaine" | "semaines" | "w" | "wk" | "wks" | "week" | "weeks" => DurationUnit::Week,
        "mo" | "ms" | "mois" | "mon" | "mons" | "month" | "months" => DurationUnit::Month,
        _ => return None,
    })
}

/// R-140 : unité et base d'une écriture d'unité. Écoulé : préfixe `e` ou `é`
/// (« ej », « éj », « emo »), ou suffixe `é` (« jé », « moé ») ; une écriture
/// ouvrée reconnue l'emporte (« heure » n'est pas « heur » + `e`).
fn unit_of(token: &[char]) -> Option<(DurationUnit, TimeBasis)> {
    let folded = fold(token.iter().copied());
    let folded = folded.trim_end_matches('.');
    if let Some(u) = working_unit(folded) {
        return Some((u, TimeBasis::Working));
    }
    if let Some(u) = folded.strip_prefix('e').and_then(working_unit) {
        return Some((u, TimeBasis::Elapsed));
    }
    folded
        .strip_suffix('e')
        .and_then(working_unit)
        .map(|u| (u, TimeBasis::Elapsed))
}

/// Texte d'un jeton de caractères, espaces de bord retirées.
fn token_text(token: &[char]) -> String {
    token.iter().collect::<String>().trim_matches(is_space).to_owned()
}

/// Jeton d'unité après le nombre : tout jusqu'au `?` final facultatif, espaces
/// de bord retirées. Renvoie le jeton et l'estimation.
///
/// # Errors
///
/// [`TextError::UnknownUnit`] si du texte suit le `?`.
fn unit_token(cur: &mut Cursor<'_>) -> Result<(Vec<char>, bool), TextError> {
    cur.skip_spaces();
    let rest = cur.rest();
    let (token, estimated) = match rest.iter().position(|c| *c == '?') {
        Some(q) => {
            let after = rest.get(q.saturating_add(1)..).unwrap_or(&[]);
            if !after.iter().all(|c| is_space(*c)) {
                return Err(TextError::UnknownUnit {
                    unit: token_text(rest),
                });
            }
            (rest.get(..q).unwrap_or(&[]), true)
        }
        None => (rest, false),
    };
    let mut token: Vec<char> = token.to_vec();
    while token.last().is_some_and(|c| is_space(*c)) {
        token.pop();
    }
    Ok((token, estimated))
}

/// Applique le signe à un décimal lu sans signe.
fn signed(value: Decimal, negative: bool) -> Result<Decimal, TextError> {
    if !negative {
        return Ok(value);
    }
    let m = value.mantissa().checked_neg().ok_or(TextError::OutOfRange)?;
    Decimal::new(m, value.scale()).map_err(|_| TextError::OutOfRange)
}

/// Erreur du cœur → erreur du texte.
const fn from_duration_error(e: &DurationError) -> TextError {
    match e {
        DurationError::Negative => TextError::Negative,
        _ => TextError::OutOfRange,
    }
}

/// R-140 : lit une durée de tâche. Forme : espaces, nombre (séparateurs de
/// milliers admis, `,` ou `.` et 1 à 6 décimales), espaces, unité
/// facultative (sinon l'unité par défaut du contexte, S10), `?` facultatif
/// (durée estimée, S14). Minutes = R-006/R-007 arrondies par R-001.
///
/// Exemples (480 / 2 400 / 20) : « 5j », « 5d », « 5 jours » → 2 400 ;
/// « 1,5 j » → 720 ; « 5ej » → 7 200 écoulées ; « 2j? » → 960 estimée.
///
/// # Errors
///
/// [`TextError`] : vide, nombre illisible, unité inconnue, négatif, hors
/// bornes, trop long.
pub fn parse_duration(s: &str, ctx: &TextContext) -> Result<Duration, TextError> {
    parse_duration_at(s, ctx, ENTRY_RESOLUTION)
}

/// [`parse_duration`] arrondie par R-001 à la `resolution` donnée : au
/// dixième, « 0,01j » (480 min/j) donne 4,8 min et « 112,5m » 112,5 min
/// (R-140, ADR-0039).
///
/// # Errors
///
/// Comme [`parse_duration`].
pub fn parse_duration_at(s: &str, ctx: &TextContext, resolution: Resolution) -> Result<Duration, TextError> {
    let chars = bounded_chars(s)?;
    let mut cur = Cursor::new(&chars);
    cur.skip_spaces();
    if cur.at_end() {
        return Err(TextError::Empty);
    }
    let negative = cur.sign();
    let value = cur.decimal_in(ctx.language)?;
    let (token, estimated) = unit_token(&mut cur)?;
    let (unit, basis) = if token.is_empty() {
        (ctx.default_duration_unit, TimeBasis::Working)
    } else {
        unit_of(&token).ok_or_else(|| TextError::UnknownUnit {
            unit: token_text(&token),
        })?
    };
    if negative && value.mantissa() != 0 {
        return Err(TextError::Negative);
    }
    let minutes = ctx
        .factors
        .to_minutes_at(value, unit, basis, resolution)
        .map_err(|e| from_duration_error(&e))?;
    Duration::new(minutes, unit, basis, estimated).map_err(|e| from_duration_error(&e))
}

/// R-140, R-142 : lit un décalage de lien. Forme : signe facultatif (`+`,
/// `-`), espaces, nombre, puis une unité de durée (R-140, sans `?`) ou `%`
/// (pourcentage de la durée du prédécesseur), `e%` / `é%` (pourcentage
/// écoulé, S3).
///
/// Exemples : « +2j » → 960 ; « -1j » → −480 ; « -50% » → −5 000
/// (1/10 000) ; « +3ej » → 4 320 écoulées.
///
/// # Errors
///
/// [`TextError`] : vide, nombre illisible, unité inconnue, hors bornes
/// (R-008 : |décalage| ≤ 87 308 640 min ; R-002 : ±10 000 %), trop long.
pub fn parse_lag(s: &str, ctx: &TextContext) -> Result<Lag, TextError> {
    parse_lag_at(s, ctx, ENTRY_RESOLUTION)
}

/// [`parse_lag`] arrondi par R-001 à la `resolution` donnée (R-140).
///
/// # Errors
///
/// Comme [`parse_lag`].
pub fn parse_lag_at(s: &str, ctx: &TextContext, resolution: Resolution) -> Result<Lag, TextError> {
    let chars = bounded_chars(s)?;
    let mut cur = Cursor::new(&chars);
    cur.skip_spaces();
    if cur.at_end() {
        return Err(TextError::Empty);
    }
    let negative = cur.sign();
    let value = signed(cur.decimal_in(ctx.language)?, negative)?;
    let (token, estimated) = unit_token(&mut cur)?;
    if estimated {
        let mut unit = token_text(&token);
        unit.push('?');
        return Err(TextError::UnknownUnit { unit });
    }
    let folded = fold(token.iter().copied());
    let percent_basis = match folded.as_str() {
        "%" => Some(TimeBasis::Working),
        "e%" => Some(TimeBasis::Elapsed),
        _ => None,
    };
    if let Some(basis) = percent_basis {
        let percent = SignedPercent::from_percent(value).map_err(|_| TextError::OutOfRange)?;
        return Lag::new(basis, LagValue::Percent { percent }).map_err(|_| TextError::OutOfRange);
    }
    let (unit, basis) = if token.is_empty() {
        (ctx.default_duration_unit, TimeBasis::Working)
    } else {
        unit_of(&token).ok_or_else(|| TextError::UnknownUnit {
            unit: token_text(&token),
        })?
    };
    let minutes = ctx
        .factors
        .to_minutes_at(value, unit, basis, resolution)
        .map_err(|e| from_duration_error(&e))?;
    Lag::new(basis, LagValue::Time { minutes, unit }).map_err(|_| TextError::OutOfRange)
}

/// R-141 : abréviation compacte d'une unité (français `m h j s mo`, anglais
/// `m h d w mo`), préfixe `e` si écoulée.
fn abbreviation(unit: DurationUnit, basis: TimeBasis, language: Language) -> String {
    let base = match (unit, language) {
        (DurationUnit::Minute, _) => "m",
        (DurationUnit::Hour, _) => "h",
        (DurationUnit::Day, Language::Fr) => "j",
        (DurationUnit::Day, Language::En) => "d",
        (DurationUnit::Week, Language::Fr) => "s",
        (DurationUnit::Week, Language::En) => "w",
        (DurationUnit::Month, _) => "mo",
    };
    match basis {
        TimeBasis::Working => base.to_owned(),
        TimeBasis::Elapsed => format!("e{base}"),
    }
}

/// R-141 : valeur exacte de `minutes` (≥ 0) dans l'unité, relue par R-140
/// à la résolution de la valeur (minute entière : la minute, texte
/// inchangé depuis J3 ; fraction : le dixième, ADR-0039) : 2 décimales
/// (R-009) ; si la relecture ne redonne pas la valeur, 5 décimales ; sinon
/// 6 (au plus 44 640 min par unité : erreur ≤ 5·10⁻⁷ × 44 640 < 0,05 min,
/// la relecture au dixième redonne la valeur ; à la minute, 5 décimales
/// suffisent toujours : 5·10⁻⁶ × 44 640 < 0,5). `None` si un calcul
/// déborde (impossible dans les bornes du modèle).
fn exact_value(
    factors: &ConversionFactors,
    minutes: Minutes,
    unit: DurationUnit,
    basis: TimeBasis,
) -> Option<Decimal> {
    let resolution = if minutes.whole_minutes().is_some() {
        Resolution::Minute
    } else {
        Resolution::Tenth
    };
    let rereads = |v: Decimal| {
        factors
            .to_minutes_at(v, unit, basis, resolution)
            .is_ok_and(|m| m == minutes)
    };
    let two = factors.to_unit_value(minutes, unit, basis).ok()?;
    if rereads(two) {
        return Some(two);
    }
    let factor = i128::from(factors.minutes_per_unit(unit, basis).tenths());
    let tenths = i128::from(minutes.tenths());
    let five = Decimal::new(round_div(tenths.checked_mul(100_000)?, factor)?, 5).ok()?;
    if rereads(five) {
        return Some(five.normalized());
    }
    let six = Decimal::new(round_div(tenths.checked_mul(1_000_000)?, factor)?, 6).ok()?;
    Some(six.normalized())
}

/// Valeur et abréviation de `minutes` (≥ 0) ; repli exact en minutes si la
/// conversion échoue (règle 10 : jamais de panique, jamais de valeur fausse).
fn value_and_unit(ctx: &TextContext, minutes: Minutes, unit: DurationUnit, basis: TimeBasis) -> String {
    let mut s;
    if let Some(v) = exact_value(&ctx.factors, minutes, unit, basis) {
        s = format_decimal(v, ctx.decimal);
        s.push_str(&abbreviation(unit, basis, ctx.language));
    } else {
        s = Decimal::new(minutes.tenths(), 1).map_or_else(
            |_| minutes.whole_minutes_round().to_string(),
            |d| format_decimal(d.normalized(), ctx.decimal),
        );
        s.push_str(&abbreviation(DurationUnit::Minute, basis, ctx.language));
    }
    s
}

/// R-141 : écrit une durée dans son unité stockée, avec la marque décimale
/// du contexte, l'abréviation compacte de la langue, `e` si écoulée, `?` si
/// estimée. Relue par [`parse_duration`], elle redonne exactement la même
/// durée (P29).
///
/// Exemples (480 / 2 400 / 20) : 2 400 en jours → « 5j » ; 720 → « 1,5j »
/// (« 1.5d ») ; 7 min en jours → « 0,01458j » ; 7 200 en jours écoulés,
/// estimée → « 5ej? ».
#[must_use]
pub fn format_duration(d: &Duration, ctx: &TextContext) -> String {
    let mut s = value_and_unit(ctx, d.minutes(), d.unit(), d.basis());
    if d.is_estimated() {
        s.push('?');
    }
    s
}

/// R-141 : écrit un décalage, toujours signé (`+` pour zéro ou positif,
/// `-` pour une avance) ; durée par [`format_duration`] sans estimation ;
/// pourcentage avec au plus 2 décimales (pas de 1/10 000), `%` ou `e%`.
///
/// Exemples : −480 en jours → « -1j » ; −5 000 (1/10 000) → « -50% » ;
/// 4 320 en jours écoulés → « +3ej ».
#[must_use]
pub fn format_lag(l: &Lag, ctx: &TextContext) -> String {
    match l.value {
        LagValue::Time { minutes, unit } => {
            let (sign, abs) = if minutes.is_negative() {
                ('-', minutes.checked_neg().unwrap_or(Minutes::ZERO))
            } else {
                ('+', minutes)
            };
            let mut s = String::from(sign);
            s.push_str(&value_and_unit(ctx, abs, unit, l.basis));
            s
        }
        LagValue::Percent { percent } => {
            let v = percent.get();
            let sign = if v < 0 { '-' } else { '+' };
            let abs = v.checked_abs().unwrap_or(0);
            let mut s = String::from(sign);
            // 1/10 000 = centièmes de pour-cent : exact à 2 décimales.
            let value = Decimal::new(abs, 2).unwrap_or(Decimal::integer(0));
            s.push_str(&format_decimal(value, ctx.decimal));
            if l.basis == TimeBasis::Elapsed {
                s.push('e');
            }
            s.push('%');
            s
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use plannercraft_core::{DurationUnit, TimeBasis};

    use super::unit_of;

    fn unit(s: &str) -> Option<(DurationUnit, TimeBasis)> {
        unit_of(&s.chars().collect::<Vec<_>>())
    }

    #[test]
    fn r140_unit_spellings() {
        use DurationUnit::{Day, Hour, Minute, Month, Week};
        use TimeBasis::{Elapsed as E, Working as W};
        for (s, u) in [
            ("m", Minute),
            ("mn", Minute),
            ("Min", Minute),
            ("minutes", Minute),
            ("h", Hour),
            ("hrs", Hour),
            ("Heures", Hour),
            ("hour", Hour),
            ("j", Day),
            ("jr", Day),
            ("JOURS", Day),
            ("d", Day),
            ("dy", Day),
            ("days", Day),
            ("s", Week),
            ("sem", Week),
            ("sem.", Week),
            ("semaines", Week),
            ("w", Week),
            ("wk", Week),
            ("wks", Week),
            ("Wks.", Week),
            ("weeks", Week),
            ("mo", Month),
            ("ms", Month),
            ("mois", Month),
            ("mon", Month),
            ("mons", Month),
            ("months", Month),
        ] {
            assert_eq!(unit(s), Some((u, W)), "{s}");
        }
        for (s, u) in [
            ("ej", Day),
            ("éj", Day),
            ("ed", Day),
            ("eh", Hour),
            ("em", Minute),
            ("es", Week),
            ("ew", Week),
            ("emo", Month),
            ("ems", Month),
            ("ewks", Week),
            ("emons", Month),
            ("jé", Day),
            ("hé", Hour),
            ("sé", Week),
            ("moé", Month),
            ("Éjours", Day),
        ] {
            assert_eq!(unit(s), Some((u, E)), "{s}");
        }
        // Pas d'unité « année » dans le modèle : refusées comme inconnues.
        for s in ["x", "e", "é", "", "jj", "5", "%", "yr", "yrs", "y"] {
            assert_eq!(unit(s), None, "{s}");
        }
    }
}
