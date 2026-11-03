//! P29 — Texte : `parse(format(x)) == x` pour les durées, les décalages,
//! les listes de prédécesseurs et les dates, en français et en anglais, avec des facteurs de projet quelconques ; une
//! chaîne arbitraire donne un `Result`, jamais une panique (règle 10).
//!
//! Nombre de cas fixé ici (2 048) : la variable `PROPTEST_CASES` de la CI
//! (10 000) vise les propriétés du moteur.

#![allow(
    clippy::unwrap_used,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_automation::text::{
    DateRole, Language, MAX_INPUT_CHARS, PredecessorRef, TextContext, TextError, format_date,
    format_duration, format_lag, format_predecessor, parse_date, parse_duration, parse_lag,
    parse_predecessors,
};
use plannercraft_core::{
    ConversionFactors, Duration, DurationUnit, Lag, LagValue, LinkType, Minutes, Moment, ProjectOptions,
    SignedPercent, TimeBasis,
};
use proptest::prelude::*;

fn factors() -> impl Strategy<Value = ConversionFactors> {
    prop_oneof![
        Just(ConversionFactors::default()),
        (1_u32..=1_440, 1_u32..=10_080, 1_u32..=31)
            .prop_map(|(d, w, m)| ConversionFactors::new(d, w, m).unwrap()),
    ]
}

fn unit() -> impl Strategy<Value = DurationUnit> {
    prop_oneof![
        Just(DurationUnit::Minute),
        Just(DurationUnit::Hour),
        Just(DurationUnit::Day),
        Just(DurationUnit::Week),
        Just(DurationUnit::Month),
    ]
}

fn basis() -> impl Strategy<Value = TimeBasis> {
    prop_oneof![Just(TimeBasis::Working), Just(TimeBasis::Elapsed)]
}

fn language() -> impl Strategy<Value = Language> {
    prop_oneof![Just(Language::Fr), Just(Language::En)]
}

fn minutes() -> impl Strategy<Value = i64> {
    prop_oneof![0_i64..=10_000, 0_i64..=Duration::MAX_MINUTES]
}

fn lag() -> impl Strategy<Value = Lag> {
    prop_oneof![
        Just(Lag::ZERO),
        (minutes(), any::<bool>(), unit(), basis()).prop_map(|(m, neg, u, b)| {
            let minutes = Minutes::new(if neg { -m } else { m });
            Lag::new(b, LagValue::Time { minutes, unit: u }).unwrap()
        }),
        (-1_000_000_i64..=1_000_000, basis()).prop_map(|(p, b)| {
            let percent = SignedPercent::new(p).unwrap();
            Lag::new(b, LagValue::Percent { percent }).unwrap()
        }),
    ]
}

fn link_type() -> impl Strategy<Value = LinkType> {
    prop_oneof![
        Just(LinkType::FinishToStart),
        Just(LinkType::StartToStart),
        Just(LinkType::FinishToFinish),
        Just(LinkType::StartToFinish),
    ]
}

fn predecessor() -> impl Strategy<Value = PredecessorRef> {
    (0_u32..=u32::MAX, link_type(), lag()).prop_map(|(id, kind, lag)| PredecessorRef { id, kind, lag })
}

/// Vrai si le nombre en tête du texte a une virgule suivie d'exactement
/// trois chiffres (« 5,025j ») : forme refusée en anglais (R-140).
fn comma_then_three_digits(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars.iter().position(|c| *c == ',').is_some_and(|i| {
        let digits = chars
            .iter()
            .skip(i + 1)
            .take_while(|c| c.is_ascii_digit())
            .count();
        digits == 3
    })
}

fn context(f: ConversionFactors, language: Language) -> TextContext {
    let options = ProjectOptions {
        factors: f,
        ..ProjectOptions::default()
    };
    TextContext::for_options(&options, language)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_048))]

    #[test]
    fn p29_duration_round_trip(
        f in factors(), lang in language(), m in minutes(), u in unit(), b in basis(),
        est in any::<bool>(),
    ) {
        let d = Duration::new(Minutes::new(m), u, b, est).unwrap();
        let ctx = context(f, lang);
        let text = format_duration(&d, &ctx);
        prop_assert_eq!(parse_duration(&text, &ctx), Ok(d), "{}", text);
        // La lecture accepte l'autre langue (marque et abréviations), sauf
        // une virgule suivie d'exactement trois chiffres lue en anglais :
        // séparateur de milliers anglais, refusé (R-140).
        let other_lang = match lang { Language::Fr => Language::En, Language::En => Language::Fr };
        let other = context(f, other_lang);
        if other_lang == Language::En && comma_then_three_digits(&text) {
            prop_assert!(
                matches!(parse_duration(&text, &other), Err(TextError::BadNumber { .. })),
                "{}", text
            );
        } else {
            prop_assert_eq!(parse_duration(&text, &other), Ok(d), "{}", text);
        }
    }

    #[test]
    fn p29_lag_round_trip(
        f in factors(), lang in language(), m in minutes(), neg in any::<bool>(), u in unit(),
        b in basis(), pct in -1_000_000_i64..=1_000_000, is_pct in any::<bool>(),
    ) {
        let value = if is_pct {
            LagValue::Percent { percent: SignedPercent::new(pct).unwrap() }
        } else {
            LagValue::Time { minutes: Minutes::new(if neg { -m } else { m }), unit: u }
        };
        let l = Lag::new(b, value).unwrap();
        let ctx = context(f, lang);
        let text = format_lag(&l, &ctx);
        prop_assert!(text.starts_with('+') || text.starts_with('-'), "{}", text);
        prop_assert_eq!(parse_lag(&text, &ctx), Ok(l), "{}", text);
    }

    #[test]
    fn p29_predecessors_round_trip(
        f in factors(), lang in language(), refs in prop::collection::vec(predecessor(), 0..6),
    ) {
        let ctx = context(f, lang);
        let text = refs
            .iter()
            .map(|r| format_predecessor(r, &ctx))
            .collect::<Vec<_>>()
            .join(&ctx.list_separator.to_string());
        prop_assert_eq!(parse_predecessors(&text, &ctx), (refs, vec![]), "{}", text);
    }

    #[test]
    fn p29_dates_round_trip(m in 0_i64..=Moment::MAX.minutes(), lang in language()) {
        let ctx = context(ConversionFactors::default(), lang);
        let m = Moment::from_minutes(m).unwrap();
        let text = format_date(m, &ctx);
        prop_assert_eq!(parse_date(&text, DateRole::Start, &ctx), Ok(m), "{}", &text);
        prop_assert_eq!(parse_date(&text, DateRole::Finish, &ctx), Ok(m), "{}", &text);
    }

    #[test]
    fn p29_arbitrary_text_never_panics(s in "\\PC{0,40}", lang in language()) {
        let ctx = context(ConversionFactors::default(), lang);
        let _ = parse_duration(&s, &ctx);
        let _ = parse_lag(&s, &ctx);
        let _ = parse_predecessors(&s, &ctx);
        let _ = parse_date(&s, DateRole::Start, &ctx);
    }

    #[test]
    fn p29_list_and_date_like_input_never_panics(
        s in "[ 0-9;,.:/\\-+%FSDfsdTjhAPMamp?]{0,30}", lang in language(),
    ) {
        let ctx = context(ConversionFactors::default(), lang);
        let (refs, _) = parse_predecessors(&s, &ctx);
        for r in refs {
            prop_assert_eq!(parse_predecessors(&format_predecessor(&r, &ctx), &ctx), (vec![r], vec![]));
        }
        if let Ok(m) = parse_date(&s, DateRole::Start, &ctx) {
            prop_assert_eq!(parse_date(&format_date(m, &ctx), DateRole::Start, &ctx), Ok(m));
        }
    }

    #[test]
    fn p29_text_like_input_never_panics(
        s in "[ +\\-0-9,.\u{a0}\u{202f}?%eéjdhmswoJDHMSWOÉ]{0,24}", lang in language(),
    ) {
        let ctx = context(ConversionFactors::default(), lang);
        if let Ok(d) = parse_duration(&s, &ctx) {
            // Ce qui est lu se réécrit et se relit à l'identique.
            prop_assert_eq!(parse_duration(&format_duration(&d, &ctx), &ctx), Ok(d));
        }
        if let Ok(l) = parse_lag(&s, &ctx) {
            prop_assert_eq!(parse_lag(&format_lag(&l, &ctx), &ctx), Ok(l));
        }
    }
}

#[test]
fn p29_long_input_is_refused() {
    let ctx = context(ConversionFactors::default(), Language::Fr);
    let long = "9".repeat(MAX_INPUT_CHARS + 1);
    assert_eq!(parse_duration(&long, &ctx), Err(TextError::TooLong));
    assert_eq!(parse_lag(&long, &ctx), Err(TextError::TooLong));
    assert_eq!(parse_date(&long, DateRole::Start, &ctx), Err(TextError::TooLong));
    assert_eq!(
        parse_predecessors(&long, &ctx),
        (vec![], vec![TextError::TooLong])
    );
}
