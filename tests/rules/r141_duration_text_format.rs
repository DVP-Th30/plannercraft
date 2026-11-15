//! R-141 — Durée et décalage en texte (écriture) : valeur dans l'unité
//! stockée à 2 décimales, 5 si la relecture ne redonne pas les minutes ;
//! marque décimale du contexte ; abréviation compacte (`m h j s mo`,
//! `m h d w mo`) ; `e` si écoulé ; `?` si estimé ; décalage toujours signé.
//! Options 480 / 2 400 / 20.

use plannercraft_automation::text::{
    Language, TextContext, format_duration, format_lag, parse_duration, parse_lag,
};
use plannercraft_core::{
    Duration, DurationUnit, Lag, LagValue, Minutes, ProjectOptions, SignedPercent, TimeBasis,
};

fn ctx(language: Language) -> TextContext {
    TextContext::for_options(&ProjectOptions::default(), language)
}

fn dur(minutes: i64, unit: DurationUnit, basis: TimeBasis, estimated: bool) -> Duration {
    Duration::new(Minutes::new(minutes), unit, basis, estimated).unwrap()
}

fn time_lag(minutes: i64, unit: DurationUnit, basis: TimeBasis) -> Lag {
    Lag::new(
        basis,
        LagValue::Time {
            minutes: Minutes::new(minutes),
            unit,
        },
    )
    .unwrap()
}

#[test]
fn r141_durations_fr_and_en() {
    let w = TimeBasis::Working;
    let cases = [
        // (minutes, unité, base, estimée, FR, EN)
        (2_400, DurationUnit::Day, w, false, "5j", "5d"),
        (720, DurationUnit::Day, w, false, "1,5j", "1.5d"),
        (180, DurationUnit::Hour, w, false, "3h", "3h"),
        (4_800, DurationUnit::Week, w, false, "2s", "2w"),
        (9_600, DurationUnit::Month, w, false, "1mo", "1mo"),
        (45, DurationUnit::Minute, w, false, "45m", "45m"),
        (480, DurationUnit::Day, w, true, "1j?", "1d?"),
        (0, DurationUnit::Day, w, false, "0j", "0d"),
        (7_200, DurationUnit::Day, TimeBasis::Elapsed, true, "5ej?", "5ed?"),
        (
            10_080,
            DurationUnit::Week,
            TimeBasis::Elapsed,
            false,
            "1es",
            "1ew",
        ),
        (
            43_200,
            DurationUnit::Month,
            TimeBasis::Elapsed,
            false,
            "1emo",
            "1emo",
        ),
    ];
    for (m, unit, basis, est, fr, en) in cases {
        let d = dur(m, unit, basis, est);
        assert_eq!(format_duration(&d, &ctx(Language::Fr)), fr);
        assert_eq!(format_duration(&d, &ctx(Language::En)), en);
        // Relu, il redonne la même durée (P29).
        assert_eq!(parse_duration(fr, &ctx(Language::Fr)), Ok(d));
        assert_eq!(parse_duration(en, &ctx(Language::En)), Ok(d));
    }
}

#[test]
fn r141_five_decimals_when_two_do_not_round_trip() {
    // 7 min en jours : 2 décimales donnent 0,01 j = 4,8 → 5 ≠ 7 ; 5 décimales :
    // 7 / 480 = 0,0145833… → 0,01458 ; 0,01458 × 480 = 6,9984 → 7.
    let d = dur(7, DurationUnit::Day, TimeBasis::Working, false);
    assert_eq!(format_duration(&d, &ctx(Language::Fr)), "0,01458j");
    assert_eq!(format_duration(&d, &ctx(Language::En)), "0.01458d");
    assert_eq!(parse_duration("0,01458j", &ctx(Language::Fr)), Ok(d));
    // 158 min en jours : 0,33 j se relit 158,4 au dixième (ADR-0039 ; avant,
    // relu à la minute, « 0,33j » suffisait) : 5 décimales, 0,32917 j =
    // 158,0016 → 158.
    let d = dur(158, DurationUnit::Day, TimeBasis::Working, false);
    assert_eq!(format_duration(&d, &ctx(Language::Fr)), "0,32917j");
    assert_eq!(parse_duration("0,32917j", &ctx(Language::Fr)), Ok(d));
    // 1 min en mois (9 600 min) : 0,00010 → 0,0001 ; 0,0001 × 9 600 = 0,96 → 1.
    let d = dur(1, DurationUnit::Month, TimeBasis::Working, false);
    assert_eq!(format_duration(&d, &ctx(Language::Fr)), "0,0001mo");
    assert_eq!(parse_duration("0,0001mo", &ctx(Language::Fr)), Ok(d));
}

#[test]
fn r141_lags() {
    let w = TimeBasis::Working;
    // −480 min en jours → « -1j » ; 960 → « +2j » ; 0 → « +0j ».
    assert_eq!(
        format_lag(&time_lag(-480, DurationUnit::Day, w), &ctx(Language::Fr)),
        "-1j"
    );
    assert_eq!(
        format_lag(&time_lag(960, DurationUnit::Day, w), &ctx(Language::En)),
        "+2d"
    );
    assert_eq!(format_lag(&Lag::ZERO, &ctx(Language::Fr)), "+0j");
    // 4 320 min en jours écoulés → « +3ej ».
    let l = time_lag(4_320, DurationUnit::Day, TimeBasis::Elapsed);
    assert_eq!(format_lag(&l, &ctx(Language::Fr)), "+3ej");
    assert_eq!(parse_lag("+3ej", &ctx(Language::Fr)), Ok(l));
    // −90 min en heures → « -1,5h ».
    let l = time_lag(-90, DurationUnit::Hour, w);
    assert_eq!(format_lag(&l, &ctx(Language::Fr)), "-1,5h");
    assert_eq!(parse_lag("-1,5h", &ctx(Language::Fr)), Ok(l));
    // −5 000 (1/10 000) → « -50% » ; 1 250 écoulé → « +12,5e% ».
    let pct = |v: i64, basis| {
        Lag::new(
            basis,
            LagValue::Percent {
                percent: SignedPercent::new(v).unwrap(),
            },
        )
        .unwrap()
    };
    assert_eq!(format_lag(&pct(-5_000, w), &ctx(Language::Fr)), "-50%");
    assert_eq!(
        format_lag(&pct(1_250, TimeBasis::Elapsed), &ctx(Language::Fr)),
        "+12,5e%"
    );
    assert_eq!(format_lag(&pct(1, w), &ctx(Language::En)), "+0.01%");
    for v in [-5_000, 1_250, 1, 0, 1_000_000] {
        for basis in [w, TimeBasis::Elapsed] {
            let l = pct(v, basis);
            assert_eq!(
                parse_lag(&format_lag(&l, &ctx(Language::Fr)), &ctx(Language::Fr)),
                Ok(l)
            );
        }
    }
}

/// ADR-0039 (lot B3) : une valeur au dixième s'écrit pour se relire au
/// dixième (2, puis 5, puis 6 décimales), minute entière comprise depuis le
/// lot B4 (la saisie se relit au dixième).
#[test]
fn r141_tenths() {
    use plannercraft_automation::text::parse_duration_at;
    use plannercraft_core::Resolution;
    let fr = ctx(Language::Fr);
    let d = |t: i64, unit| Duration::new(Minutes::from_tenths(t), unit, TimeBasis::Working, false).unwrap();
    for (t, unit, text) in [
        (1_125, DurationUnit::Minute, "112,5m"),
        (48, DurationUnit::Day, "0,01j"),
        (5, DurationUnit::Hour, "0,00833h"),
        (1, DurationUnit::Month, "0,00001mo"),
        (1, DurationUnit::Day, "0,00021j"),
        // Minutes entières : 7 min en jours garde son texte ; 61 min en
        // heures, « 1,02h » se relirait 61,2 min : « 1,01667h » (avant le
        // lot B4 : « 1,02h », relu à la minute).
        (70, DurationUnit::Day, "0,01458j"),
        (610, DurationUnit::Hour, "1,01667h"),
    ] {
        let duration = d(t, unit);
        assert_eq!(format_duration(&duration, &fr), text, "{t}");
        assert_eq!(
            parse_duration_at(text, &fr, Resolution::Tenth),
            Ok(duration),
            "{text}"
        );
        assert_eq!(parse_duration(text, &fr), Ok(duration), "{text}");
    }
    // Mois écoulé de 43 200 min : 5 décimales ne relisent pas le dixième,
    // 6 le font.
    let e = Duration::new(
        Minutes::from_tenths(1),
        DurationUnit::Month,
        TimeBasis::Elapsed,
        false,
    )
    .unwrap();
    let text = format_duration(&e, &fr);
    assert_eq!(text, "0,000002emo");
    assert_eq!(parse_duration_at(&text, &fr, Resolution::Tenth), Ok(e));
}
