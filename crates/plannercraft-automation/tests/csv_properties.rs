//! P28 — CSV des tâches.
//! - Aller-retour : `import(export(p))` égale `p` sur la projection CSV
//!   (lignes vides, noms, notes, niveaux, jalons, durées des feuilles, liens)
//!   et `export(import(export(p))) == export(p)`, en français et en anglais.
//! - Robustesse : octets arbitraires, mutations d'un CSV valide et CSV
//!   synthétiques (guillemets, séparateurs, encodages) ne font jamais
//!   paniquer l'import (règle 10) ; le résultat est soit un projet valide,
//!   soit une erreur du fichier entier.
//!
//! Nombre de cas fixé ici : la variable `PROPTEST_CASES` de la CI (10 000)
//! vise les propriétés du moteur.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
)]

use plannercraft_automation::csv::{CsvExportOptions, CsvImportOptions, CsvLocale, export, import};
use plannercraft_core::{
    BuiltinCalendarNames, Duration, DurationUnit, Lag, LagValue, Link, LinkType, Minutes, Moment,
    OutlineLevel, Project, ProjectInit, ScheduleAnchor, SignedPercent, Task, TaskUid, TimeBasis,
};
use proptest::prelude::*;

fn options() -> CsvImportOptions {
    CsvImportOptions::new(
        ScheduleAnchor::FromStart("2026-10-19T08:00".parse::<Moment>().unwrap()),
        "2026-10-17T08:00".parse::<Moment>().unwrap(),
    )
}

const SAMPLE: &str = "N°;N° unique;Nom;Durée;Début;Fin;Prédécesseurs;Niveau hiérarchique;Jalon;Notes;% achevé\r\n\
1;1;Phase;10j;19/10/2026 08:00;23/10/2026 17:00;;1;Non;;0%\r\n\
2;2;\"A;b\";3j;;;;2;Non;\"l1\r\nl2\";50%\r\n\
3;3;B;2,5j?;;;\"2FD+1j;1DD\";2;Non;'=x;\r\n\
4;4;Fin;;;;3;1;Oui;;\r\n";

/// Import sans panique ; un projet importé est cohérent avec ses UID.
fn check(bytes: &[u8]) {
    if let Ok(i) = import(bytes, &options()) {
        for t in &i.project.tasks {
            assert!(t.uid().get() > 0);
            for l in &t.predecessors {
                assert!(i.project.tasks.contains(l.predecessor));
                assert_ne!(l.predecessor, t.uid());
            }
        }
        let _ = plannercraft_core::schedule(&i.project);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_024))]

    #[test]
    fn p28_arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        check(&bytes);
    }

    #[test]
    fn p28_mutated_sample_never_panics(
        edits in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0_u8..3), 1..8),
    ) {
        let mut bytes = SAMPLE.as_bytes().to_vec();
        for (at, byte, kind) in edits {
            if bytes.is_empty() {
                break;
            }
            let i = at.index(bytes.len());
            match kind {
                0 => bytes[i] = byte,
                1 => bytes.insert(i, byte),
                _ => {
                    bytes.remove(i);
                }
            }
        }
        check(&bytes);
    }

    #[test]
    fn p28_synthetic_csv_never_panics(
        cells in prop::collection::vec("[A-Za-z0-9 ;,\t\"'=+\\-@?%:/.éÉ\r\n]{0,12}", 1..40),
        sep in prop_oneof![Just(';'), Just(','), Just('\t')],
        utf16 in any::<bool>(),
    ) {
        let mut text = format!("Nom{sep}Durée{sep}Prédécesseurs{sep}Niveau hiérarchique{sep}N° unique\n");
        for (k, c) in cells.iter().enumerate() {
            text.push_str(c);
            text.push(if k % 5 == 4 { '\n' } else { sep });
        }
        if utf16 {
            let mut bytes = vec![0xFF, 0xFE];
            for u in text.encode_utf16() {
                bytes.extend(u.to_le_bytes());
            }
            check(&bytes);
        } else {
            check(text.as_bytes());
        }
    }
}

#[test]
fn p28_sample_imports() {
    let i = import(SAMPLE.as_bytes(), &options()).unwrap();
    assert_eq!(i.project.tasks.len(), 4);
    // Durée de la récapitulative ignorée (ligne 2) ; aucun autre problème que
    // des écarts de dates (information).
    let codes: Vec<_> = i.diagnostics.iter().map(|d| (d.line, d.code.code())).collect();
    assert_eq!(codes[0], (2, "csv.summary_duration_ignored"));
    assert!(
        codes[1..].iter().all(|(_, c)| *c == "csv.date_mismatch"),
        "{codes:?}"
    );
}

// --- P28 : aller-retour export → import (R-146 ↔ R-145) ---

/// Tâche générée : nom, notes, durée, niveau relatif, jalon, ligne vide,
/// prédécesseurs (positions antérieures ou postérieures, types, décalages).
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools, reason = "données générées de test")]
struct Gen {
    name: String,
    notes: String,
    minutes: i64,
    unit: DurationUnit,
    elapsed: bool,
    estimated: bool,
    deeper: bool,
    milestone: bool,
    null: bool,
    links: Vec<(usize, LinkType, Lag)>,
}

fn text() -> impl Strategy<Value = String> {
    // Séparateurs, guillemets, fins de ligne, garde de formule, accents ;
    // pas de caractère de contrôle (remplacé par U+FFFD à l'import).
    "[A-Za-z0-9 ;,\"'=+\\-@éÉ\t\r\n%?]{0,12}"
}

fn gen_task() -> impl Strategy<Value = Gen> {
    (
        (text(), text()),
        (0_i64..=50_000, unit(), any::<bool>(), any::<bool>()),
        (any::<bool>(), any::<bool>(), prop::bool::weighted(0.1)),
        prop::collection::vec((0_usize..64, link_type(), lag()), 0..3),
    )
        .prop_map(
            |((name, notes), (minutes, unit, elapsed, estimated), (deeper, milestone, null), links)| Gen {
                name,
                notes,
                minutes,
                unit,
                elapsed,
                estimated,
                deeper,
                milestone,
                null,
                links,
            },
        )
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

fn link_type() -> impl Strategy<Value = LinkType> {
    prop_oneof![
        Just(LinkType::FinishToStart),
        Just(LinkType::StartToStart),
        Just(LinkType::FinishToFinish),
        Just(LinkType::StartToFinish),
    ]
}

fn lag() -> impl Strategy<Value = Lag> {
    prop_oneof![
        Just(Lag::ZERO),
        (-20_000_i64..=20_000, unit(), any::<bool>()).prop_map(|(m, u, e)| {
            let basis = if e { TimeBasis::Elapsed } else { TimeBasis::Working };
            Lag::new(
                basis,
                LagValue::Time {
                    minutes: Minutes::new(m),
                    unit: u,
                },
            )
            .unwrap()
        }),
        (-20_000_i64..=20_000).prop_map(|p| {
            Lag::new(
                TimeBasis::Working,
                LagValue::Percent {
                    percent: SignedPercent::new(p).unwrap(),
                },
            )
            .unwrap()
        }),
    ]
}

/// Projet depuis les tâches générées : UID 1..n, niveaux valides (au plus un
/// cran de plus que la précédente), liens sans doublon ni auto-lien.
fn project(gens: &[Gen]) -> Project {
    let mut p = Project::new(ProjectInit {
        name: String::new(),
        anchor: ScheduleAnchor::FromStart("2026-10-19T08:00".parse::<Moment>().unwrap()),
        current_date: "2026-10-17T08:00".parse::<Moment>().unwrap(),
        calendar_names: BuiltinCalendarNames::default(),
    });
    let o = p.options;
    let mut level: u16 = 0;
    for (k, g) in gens.iter().enumerate() {
        let uid = TaskUid::new(u32::try_from(k).unwrap() + 1);
        let mut t = Task::new(uid, g.name.clone(), &o);
        if g.null {
            t.name = String::new();
            t.is_null = true;
            p.tasks.push(t).unwrap();
            continue;
        }
        level = if g.deeper || level == 0 { level + 1 } else { 1 };
        t.outline_level = OutlineLevel::new(level).unwrap();
        t.notes.clone_from(&g.notes);
        let basis = if g.elapsed {
            TimeBasis::Elapsed
        } else {
            TimeBasis::Working
        };
        t.duration = Duration::new(Minutes::new(g.minutes), g.unit, basis, g.estimated).unwrap();
        t.milestone = g.milestone;
        let mut links: Vec<Link> = Vec::new();
        for (to, kind, lag) in &g.links {
            let to = to % gens.len();
            let pred = TaskUid::new(u32::try_from(to).unwrap() + 1);
            if to != k && !links.iter().any(|l| l.predecessor == pred) {
                links.push(Link {
                    predecessor: pred,
                    kind: *kind,
                    lag: *lag,
                });
            }
        }
        t.predecessors = links;
        p.tasks.push(t).unwrap();
    }
    p
}

/// Projection CSV d'un projet : par position, ligne vide, nom, notes,
/// niveau, jalon, durée (feuilles seulement : celle d'une récapitulative
/// est calculée), liens (position du prédécesseur, type, décalage).
#[allow(clippy::type_complexity, reason = "projection de test")]
fn projection(
    p: &Project,
) -> Vec<(
    bool,
    String,
    String,
    u16,
    bool,
    Option<Duration>,
    Vec<(usize, LinkType, Lag)>,
)> {
    let tasks: Vec<&Task> = p.tasks.iter().collect();
    tasks
        .iter()
        .enumerate()
        .map(|(k, t)| {
            if t.is_null {
                return (true, String::new(), String::new(), 1, false, None, vec![]);
            }
            let summary = tasks[k + 1..]
                .iter()
                .find(|n| !n.is_null)
                .is_some_and(|n| n.outline_level > t.outline_level);
            let links = t
                .predecessors
                .iter()
                .map(|l| (p.tasks.position(l.predecessor).unwrap(), l.kind, l.lag))
                .collect();
            (
                false,
                t.name.clone(),
                t.notes.clone(),
                t.outline_level.get(),
                t.milestone,
                (!summary).then_some(t.duration),
                links,
            )
        })
        .collect()
}

fn round_trip(p: &Project, locale: CsvLocale) -> (Vec<u8>, Project) {
    let options = CsvExportOptions::new(locale);
    let bytes = export(p, &plannercraft_core::schedule(p), &options);
    let back = import(&bytes, &options_for(p)).unwrap();
    assert_eq!(back.detected.locale, locale);
    // Aucune perte signalée : seuls des informations (durées de
    // récapitulatives ignorées) peuvent apparaître, et l'avertissement de
    // dates jour/mois ambiguës (R-144 amendée en J4) : l'export écrit les
    // dates dans l'ordre de la langue de ses en-têtes, rien n'est perdu.
    for d in &back.diagnostics {
        if d.code == plannercraft_automation::csv::CsvCode::DateAmbiguous {
            continue;
        }
        assert_eq!(d.code.severity(), plannercraft_core::Severity::Info, "{d:?}");
    }
    (bytes, back.project)
}

fn options_for(p: &Project) -> CsvImportOptions {
    CsvImportOptions::new(p.anchor, p.current_date)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn p28_export_import_round_trip(
        gens in prop::collection::vec(gen_task(), 1..24),
        en in any::<bool>(),
    ) {
        let locale = if en { CsvLocale::En } else { CsvLocale::Fr };
        let p = project(&gens);
        let (bytes, back) = round_trip(&p, locale);
        prop_assert_eq!(projection(&back), projection(&p));
        // export(import(export(p))) == export(p).
        let again = export(&back, &plannercraft_core::schedule(&back), &CsvExportOptions::new(locale));
        prop_assert_eq!(again, bytes);
    }
}
