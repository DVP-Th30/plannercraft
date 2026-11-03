//! P62 (seconde passe UX, lot L8 ; R-265, R-266) : aller-retour MSPDI des
//! champs personnalisés sur des projets générés.
//!
//! Pour tout projet généré (P24) auquel on ajoute des champs personnalisés
//! (≤ 30 champs, alias, valeurs de chaque type aux bornes, textes à
//! caractères spéciaux), des définitions et des valeurs inconnues gardées
//! en `Extras` (Coût1, Texte2 à formule) : `read(write(p)).project ==
//! normalize(p)`, `normalize` idempotente, définitions et valeurs inconnues
//! réécrites à l'identique, deux écritures ⇒ mêmes octets. 256 cas, 2 048
//! sous `PROPTEST_CASES` (CI).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

#[path = "support/generate.rs"]
mod generate;

use plannercraft_core::model::custom::MAX_CUSTOM_NUMBER_ABS;
use plannercraft_core::{
    CustomFieldDef, CustomFieldId, CustomKind, CustomValue, ExtraEntry, Moment, Project, Severity,
};
use plannercraft_mspdi::write::{Computed, OrderProfile, WriteOptions, write};
use plannercraft_mspdi::{ReadOptions, normalize, read};
use proptest::prelude::*;

use generate::Rng;

fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .map_or(256, |n| n.min(2_048))
}

fn opts() -> ReadOptions {
    ReadOptions::new("2026-01-01T00:00".parse().unwrap())
}

/// Définition à formule de Texte2 et définition de Coût1 : étrangère et
/// inconnue, gardées en `Extras` octet pour octet.
const FORMULA_TEXT2: &str = "<ExtendedAttribute><FieldID>188743734</FieldID><FieldName>Text2</FieldName>\
     <Formula>[Name] &amp; &quot;x&quot;</Formula></ExtendedAttribute>";
const COST1: &str = "<ExtendedAttribute><FieldID>188743786</FieldID><FieldName>Cost1</FieldName>\
     <Alias>Budget</Alias></ExtendedAttribute>";

fn mspdi(key: &str, payload: String) -> ExtraEntry {
    ExtraEntry {
        origin: "mspdi".to_owned(),
        key: key.to_owned(),
        payload,
    }
}

/// Valeur personnalisée aux bornes, du type de `id`.
fn value(rng: &mut Rng, id: CustomFieldId) -> CustomValue {
    match id.kind() {
        CustomKind::Text => {
            if rng.chance(15) {
                CustomValue::Text("é".repeat(255))
            } else {
                CustomValue::Text(generate::string(rng).chars().take(255).collect())
            }
        }
        CustomKind::Number => CustomValue::Number(match rng.below(5) {
            0 => MAX_CUSTOM_NUMBER_ABS,
            1 => -MAX_CUSTOM_NUMBER_ABS,
            2 => 0,
            3 => rng.int(-1_000_000, 1_000_000),
            _ => 125_000,
        }),
        CustomKind::Date => CustomValue::Date(match rng.below(3) {
            0 => Moment::MIN,
            1 => Moment::MAX,
            _ => "2026-11-02T08:00".parse().unwrap(),
        }),
        CustomKind::Flag => CustomValue::Flag(rng.chance(70)),
    }
}

/// Projet généré (P24) avec champs personnalisés et `Extras` inconnus.
fn project(seed: u64) -> Project {
    let mut p = generate::project(seed);
    let mut rng = Rng::new(seed ^ 0x5eed_c057);
    let all: Vec<CustomFieldId> = CustomFieldId::all().collect();
    let n = rng.below(31);
    let fields: Vec<CustomFieldId> = (0..n).map(|_| *rng.pick(&all)).collect();
    for id in &fields {
        if rng.chance(40) {
            let alias = if rng.chance(10) {
                "a".repeat(60)
            } else {
                generate::string(&mut rng)
            };
            p.custom_fields.insert(*id, CustomFieldDef { alias: Some(alias) });
        }
    }
    let foreign_text2 = rng.chance(30);
    let mut defs = String::new();
    if foreign_text2 {
        defs.push_str(FORMULA_TEXT2);
    }
    if rng.chance(50) {
        defs.push_str(COST1);
    }
    if !defs.is_empty() {
        p.extras.0.push(mspdi(
            "ExtendedAttributes",
            format!("<ExtendedAttributes>{defs}</ExtendedAttributes>"),
        ));
        plannercraft_mspdi::extras::canonical_order(
            &mut p.extras,
            plannercraft_mspdi::schema::Parent::Project,
        );
    }
    let uids: Vec<_> = p.tasks.iter().map(plannercraft_core::Task::uid).collect();
    for uid in uids {
        let t = p.tasks.get_mut(uid).unwrap();
        for id in &fields {
            if rng.chance(50) {
                let v = value(&mut rng, *id);
                t.custom.insert(*id, v);
            }
        }
        if rng.chance(20) {
            t.extras.0.push(mspdi(
                "ExtendedAttribute",
                "<ExtendedAttribute><FieldID>188743786</FieldID><Value>12.5</Value></ExtendedAttribute>"
                    .to_owned(),
            ));
        }
        if foreign_text2 && rng.chance(20) {
            t.extras.0.push(mspdi(
                "ExtendedAttribute",
                "<ExtendedAttribute><FieldID>188743734</FieldID><Value>calc</Value></ExtendedAttribute>"
                    .to_owned(),
            ));
        }
        plannercraft_mspdi::extras::canonical_order(&mut t.extras, plannercraft_mspdi::schema::Parent::Task);
    }
    p
}

/// Vérifie P62 pour une graine ; vrai si le projet a été vérifié avec au
/// moins une valeur personnalisée relue dans le modèle.
fn check(seed: u64) -> bool {
    let p = project(seed);
    if p.validate()
        .finish()
        .iter()
        .any(|d| d.severity() == Severity::Error)
    {
        return false;
    }
    let mut modeled = false;
    for profile in [OrderProfile::Observed, OrderProfile::Strict] {
        let options = WriteOptions {
            profile,
            ..WriteOptions::default()
        };
        let out = write(&p, Computed::Omit, &options);
        assert_eq!(
            write(&p, Computed::Omit, &options).xml,
            out.xml,
            "{seed} : écriture non déterministe"
        );
        let back = read(&out.xml, &opts()).unwrap_or_else(|e| panic!("{seed} {profile:?} : {e}"));
        let expected = normalize(&p, &options);
        modeled |= back.project.tasks.iter().any(|t| !t.custom.is_empty());
        assert!(
            back.project == expected,
            "{seed} {profile:?} : image de l'aller-retour\n{:?}\n{:?}",
            back.project.custom_fields,
            expected.custom_fields
        );
        assert!(
            normalize(&expected, &options) == expected,
            "{seed} {profile:?} : normalize non idempotente"
        );
        // Définitions et valeurs inconnues réécrites à l'identique.
        let text = String::from_utf8(out.xml.clone()).unwrap();
        for unknown in [FORMULA_TEXT2, COST1] {
            let had = p.extras.0.iter().any(|e| e.payload.contains(unknown));
            assert_eq!(text.contains(unknown), had, "{seed} {profile:?}");
        }
    }
    modeled
}

#[test]
fn p62_custom_round_trip_fixed_seeds() {
    let checked = (0..64).filter(|seed| check(*seed)).count();
    assert!(checked >= 32, "{checked}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]
    #[test]
    fn p62_custom_roundtrip(seed in any::<u64>()) {
        let _ = check(seed);
    }
}
