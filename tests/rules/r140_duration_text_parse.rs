//! R-140 — Durée en texte (lecture) : nombre (séparateurs de milliers,
//! virgule ou point, 1 à 6 décimales), unité facultative en français ou en
//! anglais (casse et accents ignorés), `e`/`é` pour l'écoulé, `?` pour une
//! durée estimée. Options 480 / 2 400 / 20 sauf mention.

use plannercraft_automation::text::{Language, TextContext, TextError, parse_duration, parse_lag};
use plannercraft_core::{
    ConversionFactors, Duration, DurationUnit, LagValue, Minutes, ProjectOptions, TimeBasis,
};

fn ctx() -> TextContext {
    TextContext::for_options(&ProjectOptions::default(), Language::Fr)
}

fn dur(minutes: i64, unit: DurationUnit, basis: TimeBasis, estimated: bool) -> Duration {
    Duration::new(Minutes::new(minutes), unit, basis, estimated).unwrap()
}

fn work(minutes: i64, unit: DurationUnit) -> Duration {
    dur(minutes, unit, TimeBasis::Working, false)
}

#[test]
fn r140_days_in_french_and_english() {
    // 5 j × 480 = 2 400, unité jour, quelle que soit l'écriture.
    for s in ["5j", "5d", "5 jours", "5 Jours", " 5J ", "5 days", "5 jr", "5dy"] {
        assert_eq!(
            parse_duration(s, &ctx()),
            Ok(work(2_400, DurationUnit::Day)),
            "{s}"
        );
    }
    // 1,5 j = 720 : virgule ou point acceptés dans la même cellule.
    for s in ["1,5 j", "1.5d", "1,5j"] {
        assert_eq!(parse_duration(s, &ctx()), Ok(work(720, DurationUnit::Day)), "{s}");
    }
}

#[test]
fn r140_other_units() {
    // 3 h = 180 ; 2 sem = 2 × 2 400 = 4 800 ; 1 mois = 20 × 480 = 9 600 ;
    // 45 min = 45.
    assert_eq!(parse_duration("3h", &ctx()), Ok(work(180, DurationUnit::Hour)));
    assert_eq!(
        parse_duration("3 heures", &ctx()),
        Ok(work(180, DurationUnit::Hour))
    );
    for s in ["2s", "2 sem.", "2w", "2 semaines", "2 wk"] {
        assert_eq!(
            parse_duration(s, &ctx()),
            Ok(work(4_800, DurationUnit::Week)),
            "{s}"
        );
    }
    for s in ["1mo", "1 mois", "1ms", "1 month"] {
        assert_eq!(
            parse_duration(s, &ctx()),
            Ok(work(9_600, DurationUnit::Month)),
            "{s}"
        );
    }
    for s in ["45m", "45 min", "45mn", "45 minutes"] {
        assert_eq!(
            parse_duration(s, &ctx()),
            Ok(work(45, DurationUnit::Minute)),
            "{s}"
        );
    }
}

#[test]
fn r140_elapsed_units() {
    // 5 jours écoulés = 5 × 1 440 = 7 200 (R-007), préfixe e/é ou suffixe é.
    for s in ["5ej", "5éj", "5jé", "5ed", "5 ejours", "5 ÉJ"] {
        assert_eq!(
            parse_duration(s, &ctx()),
            Ok(dur(7_200, DurationUnit::Day, TimeBasis::Elapsed, false)),
            "{s}"
        );
    }
    // 1 emois = 30 jours = 43 200 (R-007) ; 2 eh = 120 ; 1 esem = 10 080.
    for s in ["1emo", "1ems", "1moé"] {
        assert_eq!(
            parse_duration(s, &ctx()),
            Ok(dur(43_200, DurationUnit::Month, TimeBasis::Elapsed, false)),
            "{s}"
        );
    }
    assert_eq!(
        parse_duration("2eh", &ctx()),
        Ok(dur(120, DurationUnit::Hour, TimeBasis::Elapsed, false))
    );
    assert_eq!(
        parse_duration("1es", &ctx()),
        Ok(dur(10_080, DurationUnit::Week, TimeBasis::Elapsed, false))
    );
}

#[test]
fn r140_estimated_and_default_unit() {
    // « 2j? » : 960, estimée (S14) ; espaces admises avant le `?`.
    for s in ["2j?", "2 j ?", "2j ? "] {
        assert_eq!(
            parse_duration(s, &ctx()),
            Ok(dur(960, DurationUnit::Day, TimeBasis::Working, true)),
            "{s}"
        );
    }
    // Sans unité : unité par défaut du projet (jour, S10) : 10 j = 4 800.
    assert_eq!(parse_duration("10", &ctx()), Ok(work(4_800, DurationUnit::Day)));
    assert_eq!(
        parse_duration("1?", &ctx()),
        Ok(dur(480, DurationUnit::Day, TimeBasis::Working, true))
    );
    // Unité par défaut « heure » : « 10 » = 600.
    let options = ProjectOptions {
        default_duration_unit: DurationUnit::Hour,
        ..ProjectOptions::default()
    };
    let hours = TextContext::for_options(&options, Language::En);
    assert_eq!(parse_duration("10", &hours), Ok(work(600, DurationUnit::Hour)));
}

#[test]
fn r140_thousands_and_rounding() {
    // « 1 000 h » avec une espace fine insécable (U+202F) : 1 000 × 60 = 60 000.
    assert_eq!(
        parse_duration("1\u{202f}000 h", &ctx()),
        Ok(work(60_000, DurationUnit::Hour))
    );
    assert_eq!(
        parse_duration("1\u{a0}000h", &ctx()),
        Ok(work(60_000, DurationUnit::Hour))
    );
    assert_eq!(
        parse_duration("1 000 h", &ctx()),
        Ok(work(60_000, DurationUnit::Hour))
    );
    // 0,01 j = 4,8 min → 5 (R-001) ; 0,33 j = 158,4 → 158.
    assert_eq!(parse_duration("0,01j", &ctx()), Ok(work(5, DurationUnit::Day)));
    assert_eq!(parse_duration("0.33d", &ctx()), Ok(work(158, DurationUnit::Day)));
    // 6 décimales au plus : 0,000001 j = 0,00048 min → 0.
    assert_eq!(
        parse_duration("0,000001j", &ctx()),
        Ok(work(0, DurationUnit::Day))
    );
}

#[test]
fn r140_project_factors() {
    // 450 min/jour : 2 j = 900 ; 1 sem reste MinutesPerWeek (2 400, R-006).
    let options = ProjectOptions {
        factors: ConversionFactors::new(450, 2_400, 20).unwrap(),
        ..ProjectOptions::default()
    };
    let c = TextContext::for_options(&options, Language::Fr);
    assert_eq!(parse_duration("2j", &c), Ok(work(900, DurationUnit::Day)));
    assert_eq!(parse_duration("1s", &c), Ok(work(2_400, DurationUnit::Week)));
    // 1 mois = 20 × 450 = 9 000.
    assert_eq!(parse_duration("1mo", &c), Ok(work(9_000, DurationUnit::Month)));
}

#[test]
fn r140_refusals() {
    assert_eq!(parse_duration("-2j", &ctx()), Err(TextError::Negative));
    assert_eq!(
        parse_duration("5x", &ctx()),
        Err(TextError::UnknownUnit { unit: "x".to_owned() })
    );
    assert_eq!(
        parse_duration("5j 3h", &ctx()),
        Err(TextError::UnknownUnit {
            unit: "j 3h".to_owned()
        })
    );
    assert_eq!(
        parse_duration("5j?x", &ctx()),
        Err(TextError::UnknownUnit {
            unit: "j?x".to_owned()
        })
    );
    // Un pourcentage n'est pas une durée.
    assert_eq!(
        parse_duration("50%", &ctx()),
        Err(TextError::UnknownUnit { unit: "%".to_owned() })
    );
    assert_eq!(parse_duration("", &ctx()), Err(TextError::Empty));
    assert_eq!(parse_duration("  \u{a0}", &ctx()), Err(TextError::Empty));
    assert_eq!(parse_duration("j", &ctx()), Err(TextError::BadNumber { at: 0 }));
    assert_eq!(
        parse_duration(" ,5j", &ctx()),
        Err(TextError::BadNumber { at: 1 })
    );
    assert_eq!(
        parse_duration("1,1234567j", &ctx()),
        Err(TextError::BadNumber { at: 8 })
    );
    // Au-delà de la durée maximale (87 308 640 min, R-008) : 200 000 j.
    assert_eq!(parse_duration("200000j", &ctx()), Err(TextError::OutOfRange));
    assert_eq!(
        parse_duration("99999999999999999999j", &ctx()),
        Err(TextError::OutOfRange)
    );
    assert_eq!(
        parse_duration(&"1".repeat(2_000), &ctx()),
        Err(TextError::TooLong)
    );
}

#[test]
fn r140_lags_are_signed() {
    // Décalage : signe admis ; « +2j » = 960 ; « -1j » = −480 ; « - 1,5 h » = −90.
    let time = |s: &str| match parse_lag(s, &ctx()).unwrap().value {
        LagValue::Time { minutes, unit } => (minutes.get(), unit),
        LagValue::Percent { .. } => panic!("{s} : pourcentage inattendu"),
    };
    assert_eq!(time("+2j"), (960, DurationUnit::Day));
    assert_eq!(time("2j"), (960, DurationUnit::Day));
    assert_eq!(time("-1j"), (-480, DurationUnit::Day));
    assert_eq!(time("- 1,5 h"), (-90, DurationUnit::Hour));
    let l = parse_lag("+3ej", &ctx()).unwrap();
    assert_eq!(l.basis, TimeBasis::Elapsed);
    assert_eq!(time("+3ej"), (4_320, DurationUnit::Day));
    // Pourcentage de la durée du prédécesseur, en 1/10 000 (R-002, R-010).
    let pct = |s: &str| {
        let l = parse_lag(s, &ctx()).unwrap();
        match l.value {
            LagValue::Percent { percent } => (percent.get(), l.basis),
            LagValue::Time { .. } => panic!("{s} : durée inattendue"),
        }
    };
    assert_eq!(pct("-50%"), (-5_000, TimeBasis::Working));
    assert_eq!(pct("+ 50 %"), (5_000, TimeBasis::Working));
    assert_eq!(pct("12,5e%"), (1_250, TimeBasis::Elapsed));
    assert_eq!(pct("10é%"), (1_000, TimeBasis::Elapsed));
    // Refus : `?` sur un décalage, pourcentage au-delà de ±10 000 %, durée
    // au-delà de 87 308 640 min.
    assert_eq!(
        parse_lag("2j?", &ctx()),
        Err(TextError::UnknownUnit {
            unit: "j?".to_owned()
        })
    );
    assert_eq!(parse_lag("10001%", &ctx()), Err(TextError::OutOfRange));
    assert_eq!(parse_lag("-200000j", &ctx()), Err(TextError::OutOfRange));
    assert_eq!(parse_lag("", &ctx()), Err(TextError::Empty));
}

#[test]
fn r140_english_comma_thousands_are_refused() {
    // « 1,000 hrs » en anglais : séparateur de milliers anglais, pas une
    // marque décimale ; refusé plutôt que lu 1 h sans diagnostic. En
    // français, la virgule reste décimale (1 h).
    let en = TextContext::for_options(&ProjectOptions::default(), Language::En);
    assert_eq!(
        parse_duration("1,000 hrs", &en),
        Err(TextError::BadNumber { at: 1 })
    );
    assert_eq!(parse_lag("+2,500h", &en), Err(TextError::BadNumber { at: 2 }));
    assert_eq!(
        parse_duration("1,000 h", &ctx()),
        Ok(work(60, DurationUnit::Hour))
    );
    assert_eq!(parse_duration("1,5 hrs", &en), Ok(work(90, DurationUnit::Hour)));
    assert_eq!(parse_duration("1.000 hrs", &en), Ok(work(60, DurationUnit::Hour)));
    assert_eq!(parse_duration("2 wks", &en), Ok(work(4_800, DurationUnit::Week)));
}
