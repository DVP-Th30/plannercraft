//! Format des commandes (ADR-0012 D6, lot L3) : lecture d'un lot (version,
//! champs inconnus), formes JSON des durées et des décalages, messages
//! d'erreur.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]

use plannercraft_automation::command::params::{ApplyBatchParams, TaskListParams};
use plannercraft_automation::command::{DurationJson, DurationUnitJson, LagJson, parse_params};
use plannercraft_automation::error::{CommandErrorKind, MAX_MESSAGE_CHARS, codes};
use plannercraft_automation::text::{Language, TextContext, TextError};
use plannercraft_automation::{Batch, CommandError};
use plannercraft_core::{
    Diagnostic, Duration, DurationUnit, Lag, LagValue, Minutes, ProjectOptions, SignedPercent, TaskUid,
    TimeBasis,
};
use proptest::prelude::*;

fn ctx(lang: Language) -> TextContext {
    TextContext::for_options(&ProjectOptions::default(), lang)
}

fn dur(minutes: Option<i64>, unit: Option<DurationUnitJson>, text: Option<&str>) -> DurationJson {
    DurationJson {
        minutes,
        unit,
        text: text.map(str::to_owned),
        elapsed: None,
        estimated: None,
    }
}

#[test]
fn batch_unknown_field_refused() {
    for json in [
        r#"{"format_version":1,"ops":[],"extra":1}"#,
        r#"{"format_version":1,"ops":[{"op":"task.add","name":"A","color":"red"}]}"#,
        r#"{"format_version":1,"ops":[{"op":"task.add","name":"A","duration":{"text":"5j","days":5}}]}"#,
        r#"{"format_version":1,"ops":[{"op":"task.frobnicate","uid":1}]}"#,
    ] {
        let err = Batch::from_json_str(json).unwrap_err();
        assert_eq!(err.code, codes::PARAM_INVALID, "{json}");
        assert!(matches!(err.kind, CommandErrorKind::InvalidParam(_)));
    }
    // Pas du JSON.
    assert_eq!(Batch::from_json_str("{").unwrap_err().code, codes::PARAM_INVALID);
    // Paramètres d'un outil : même refus.
    let err = parse_params::<TaskListParams>(serde_json::json!({ "critical": true, "x": 1 })).unwrap_err();
    assert_eq!(err.code, codes::PARAM_INVALID);
    let ok = parse_params::<TaskListParams>(serde_json::json!({ "critical": true, "limit": 10 })).unwrap();
    assert!(ok.critical && ok.limit == Some(10));
}

#[test]
fn batch_version_2_refused() {
    // Version contrôlée avant les opérations : une opération inconnue d'un
    // format futur ne masque pas le vrai motif.
    let err = Batch::from_json_str(r#"{"format_version":2,"ops":[{"op":"task.teleport"}]}"#).unwrap_err();
    assert_eq!(err.code, codes::BATCH_UNSUPPORTED_VERSION);
    assert_eq!(err.kind, CommandErrorKind::UnsupportedVersion(2));
    assert_eq!(err.field.as_deref(), Some("format_version"));
    let err = Batch::from_json_str(r#"{"format_version":0,"ops":[]}"#).unwrap_err();
    assert_eq!(err.code, codes::BATCH_UNSUPPORTED_VERSION);
    // Version absente ou non entière.
    for json in [r#"{"ops":[]}"#, r#"{"format_version":"1","ops":[]}"#] {
        let err = Batch::from_json_str(json).unwrap_err();
        assert_eq!(err.code, codes::PARAM_INVALID);
        assert_eq!(err.field.as_deref(), Some("format_version"));
    }
    let batch = Batch::from_json_str(r#"{"format_version":1,"ops":[]}"#).unwrap();
    assert_eq!(batch.format_version, Batch::FORMAT_VERSION);
    // `apply_batch` : version facultative, 1 par défaut.
    let p: ApplyBatchParams = parse_params(serde_json::json!({ "ops": [] })).unwrap();
    assert_eq!(p.into_batch().format_version, 1);
}

#[test]
fn duration_json_text_minutes_agree() {
    // Exemple du cahier : {"text":"5d"} en anglais = 2 400 min en jours.
    let en = ctx(Language::En);
    let d = dur(None, None, Some("5d")).to_duration(&en).unwrap();
    assert_eq!((d.minutes().get(), d.unit()), (2_400, DurationUnit::Day));
    assert!(!d.is_estimated() && d.basis() == TimeBasis::Working);
    // Le français se lit aussi dans un lot anglais (R-140).
    assert_eq!(dur(None, None, Some("5j")).to_duration(&en).unwrap(), d);
    // `minutes` seul : unité par défaut du projet (jours).
    assert_eq!(dur(Some(2_400), None, None).to_duration(&en).unwrap(), d);
    // Les deux, concordants (unité donnée ou non).
    assert_eq!(dur(Some(2_400), None, Some("5j")).to_duration(&en).unwrap(), d);
    assert_eq!(
        dur(Some(2_400), Some(DurationUnitJson::Day), Some("5j"))
            .to_duration(&en)
            .unwrap(),
        d
    );
    // `minutes` + unité : 2 400 min affichées en semaines.
    let w = dur(Some(2_400), Some(DurationUnitJson::Week), None)
        .to_duration(&en)
        .unwrap();
    assert_eq!((w.minutes().get(), w.unit()), (2_400, DurationUnit::Week));
    // Drapeaux : écoulée, estimée.
    let e = DurationJson {
        elapsed: Some(true),
        estimated: Some(true),
        ..dur(Some(7_200), None, None)
    };
    let e = e.to_duration(&en).unwrap();
    assert_eq!(e.basis(), TimeBasis::Elapsed);
    assert!(e.is_estimated());
    assert_eq!(dur(None, None, Some("5ed?")).to_duration(&en).unwrap(), e);
    // Sortie : la forme complète du cahier.
    let fr = ctx(Language::Fr);
    let out = serde_json::to_value(DurationJson::from_duration(&d, &fr)).unwrap();
    assert_eq!(
        out,
        serde_json::json!({"minutes":2400,"unit":"day","text":"5j","elapsed":false,"estimated":false})
    );
}

#[test]
fn duration_json_mismatch_refused() {
    let fr = ctx(Language::Fr);
    // Exemple du cahier : {"text":"5j","minutes":2000} → param.mismatch.
    let err = dur(Some(2_000), None, Some("5j")).to_duration(&fr).unwrap_err();
    assert_eq!(err.code, codes::PARAM_MISMATCH);
    assert_eq!(err.kind, CommandErrorKind::ParamMismatch);
    // Unité contraire.
    let err = dur(None, Some(DurationUnitJson::Hour), Some("5j"))
        .to_duration(&fr)
        .unwrap_err();
    assert_eq!(err.code, codes::PARAM_MISMATCH);
    // Drapeau contraire.
    let d = DurationJson {
        estimated: Some(true),
        ..dur(None, None, Some("5j"))
    };
    assert_eq!(d.to_duration(&fr).unwrap_err().code, codes::PARAM_MISMATCH);
    // Ni texte ni minutes ; minutes négatives ; texte illisible.
    assert_eq!(
        dur(None, Some(DurationUnitJson::Day), None)
            .to_duration(&fr)
            .unwrap_err()
            .code,
        codes::PARAM_INVALID
    );
    assert_eq!(
        dur(Some(-1), None, None).to_duration(&fr).unwrap_err().code,
        codes::PARAM_INVALID
    );
    assert_eq!(
        dur(Some(Duration::MAX_MINUTES + 1), None, None)
            .to_duration(&fr)
            .unwrap_err()
            .code,
        codes::PARAM_INVALID
    );
    let err = dur(None, None, Some("5 parsecs")).to_duration(&fr).unwrap_err();
    assert_eq!(err.code, "text.unknown_unit");
    assert!(matches!(
        err.kind,
        CommandErrorKind::Text(TextError::UnknownUnit { .. })
    ));
}

#[test]
fn lag_json_forms() {
    let fr = ctx(Language::Fr);
    let lag = |minutes, percent, unit, text: Option<&str>| LagJson {
        minutes,
        percent,
        unit,
        text: text.map(str::to_owned),
        elapsed: None,
    };
    // « +2j » = 960 min en jours ; « -50% » = −5 000 (1/10 000).
    let two_days = lag(None, None, None, Some("+2j")).to_lag(&fr).unwrap();
    assert_eq!(
        two_days.value,
        LagValue::Time {
            minutes: Minutes::new(960),
            unit: DurationUnit::Day
        }
    );
    assert_eq!(lag(Some(960), None, None, None).to_lag(&fr).unwrap(), two_days);
    assert_eq!(
        lag(Some(960), None, Some(DurationUnitJson::Day), Some("2j"))
            .to_lag(&fr)
            .unwrap(),
        two_days
    );
    let half = lag(None, Some(-5_000), None, None).to_lag(&fr).unwrap();
    assert_eq!(
        half.value,
        LagValue::Percent {
            percent: SignedPercent::new(-5_000).unwrap()
        }
    );
    assert_eq!(lag(None, None, None, Some("-50%")).to_lag(&fr).unwrap(), half);
    // Refus.
    let code = |l: LagJson| l.to_lag(&fr).unwrap_err().code;
    assert_eq!(
        code(lag(Some(960), Some(5_000), None, None)),
        codes::PARAM_INVALID
    );
    assert_eq!(code(lag(None, None, None, None)), codes::PARAM_INVALID);
    assert_eq!(code(lag(None, Some(2_000_000), None, None)), codes::PARAM_INVALID);
    assert_eq!(
        code(lag(None, Some(5_000), Some(DurationUnitJson::Day), None)),
        codes::PARAM_MISMATCH
    );
    assert_eq!(
        code(lag(Some(480), None, None, Some("+2j"))),
        codes::PARAM_MISMATCH
    );
    assert_eq!(
        code(lag(None, Some(5_000), None, Some("+2j"))),
        codes::PARAM_MISMATCH
    );
    assert_eq!(
        code(lag(Some(960), None, None, Some("50%"))),
        codes::PARAM_MISMATCH
    );
    // Sortie relue à l'identique.
    for l in [two_days, half, Lag::ZERO] {
        assert_eq!(LagJson::from_lag(&l, &fr).to_lag(&fr).unwrap(), l);
    }
}

fn unit_strategy() -> impl Strategy<Value = DurationUnit> {
    prop_oneof![
        Just(DurationUnit::Minute),
        Just(DurationUnit::Hour),
        Just(DurationUnit::Day),
        Just(DurationUnit::Week),
        Just(DurationUnit::Month),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Une sortie est une entrée valide : `parse(view(d)) == d`, en
    /// français et en anglais, champs complets ou texte seul.
    #[test]
    fn duration_json_output_is_valid_input(
        minutes in 0..=Duration::MAX_MINUTES,
        unit in unit_strategy(),
        elapsed: bool,
        estimated: bool,
        english: bool,
    ) {
        let lang = if english { Language::En } else { Language::Fr };
        let c = ctx(lang);
        let basis = if elapsed { TimeBasis::Elapsed } else { TimeBasis::Working };
        let d = Duration::new(Minutes::new(minutes), unit, basis, estimated).unwrap();
        let view = DurationJson::from_duration(&d, &c);
        prop_assert_eq!(view.to_duration(&c).unwrap(), d);
        // Relue après un passage par le JSON.
        let again: DurationJson = serde_json::from_value(serde_json::to_value(&view).unwrap()).unwrap();
        prop_assert_eq!(again.to_duration(&c).unwrap(), d);
        // Texte seul, et minutes seules.
        prop_assert_eq!(dur(None, None, view.text.as_deref()).to_duration(&c).unwrap(), d);
        let numeric = DurationJson { text: None, ..view };
        prop_assert_eq!(numeric.to_duration(&c).unwrap(), d);
    }

    /// Même propriété pour un décalage (durée signée ou pourcentage).
    #[test]
    fn lag_json_output_is_valid_input(
        minutes in -Duration::MAX_MINUTES..=Duration::MAX_MINUTES,
        percent in -SignedPercent::LIMIT..=SignedPercent::LIMIT,
        use_percent: bool,
        unit in unit_strategy(),
        elapsed: bool,
        english: bool,
    ) {
        let lang = if english { Language::En } else { Language::Fr };
        let c = ctx(lang);
        let basis = if elapsed { TimeBasis::Elapsed } else { TimeBasis::Working };
        let value = if use_percent {
            LagValue::Percent { percent: SignedPercent::new(percent).unwrap() }
        } else {
            LagValue::Time { minutes: Minutes::new(minutes), unit }
        };
        let l = Lag::new(basis, value).unwrap();
        prop_assert_eq!(LagJson::from_lag(&l, &c).to_lag(&c).unwrap(), l);
    }
}

#[test]
fn error_messages_fr_en_bounded() {
    let samples = [
        CommandError::new(
            codes::TASK_NOT_FOUND,
            CommandErrorKind::UnknownTask(TaskUid::new(42)),
        ),
        CommandError::new(
            codes::BATCH_UNKNOWN_LABEL,
            CommandErrorKind::UnknownLabel("$x".to_owned()),
        )
        .with_op_index(3)
        .with_field("pred"),
        CommandError::text(TextError::UnknownUnit { unit: "z".to_owned() }).with_field("duration"),
        CommandError::new(
            codes::LIMIT_EXCEEDED,
            CommandErrorKind::Limit {
                what: "ops",
                max: 10_000,
            },
        ),
        CommandError::not_yet(5),
        CommandError::new(codes::PARAM_MISMATCH, CommandErrorKind::ParamMismatch),
    ];
    for e in &samples {
        let fr = e.message(Language::Fr);
        let en = e.message(Language::En);
        assert!(!fr.is_empty() && !en.is_empty());
        assert_ne!(fr, en, "{}", e.code);
    }
    assert_eq!(samples[0].message(Language::Fr), "aucune tâche d'UID 42");
    assert_eq!(samples[0].message(Language::En), "no task with UID 42");
    assert!(
        samples[1]
            .message(Language::Fr)
            .ends_with("(opération 3, champ « pred »)")
    );
    assert!(
        samples[1]
            .message(Language::En)
            .ends_with("(operation 3, field \u{201c}pred\u{201d})")
    );
    // Message borné (C-MCP-03) même pour une liste énorme de diagnostics.
    let many: Vec<Diagnostic> = (1..=10_000)
        .map(|i| Diagnostic::LinkCycle {
            tasks: vec![TaskUid::new(i), TaskUid::new(i + 1)],
        })
        .collect();
    let e = CommandError::new(
        codes::BATCH_WOULD_CREATE_ERRORS,
        CommandErrorKind::WouldCreateErrors(many),
    );
    for lang in [Language::Fr, Language::En] {
        let m = e.message(lang);
        assert!(m.chars().count() <= MAX_MESSAGE_CHARS);
        assert!(m.contains("10000") && m.contains("link.cycle"));
    }
}
