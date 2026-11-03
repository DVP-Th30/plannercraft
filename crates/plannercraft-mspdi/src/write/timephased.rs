//! Répartition temporelle (`TimephasedData`) d'une affectation à
//! l'écriture (R-199) : les lignes de type 1 (travail restant planifié,
//! S37), seules à décrire le placement, sont gardées en `Extras` seulement
//! si ce placement est encore celui du moteur, jetées sinon (PlannerCraft n'en
//! produit jamais). Les autres types (réels 2-3, lignes de base 4-10 et
//! 16+) sont des faits saisis ou figés, indépendants du placement : gardés.
//! La lecture n'en garde de type 1 que pour un profil modifié à la main
//! (`WorkContour` 8, R-087).
//!
//! Référence du placement enregistré : les lignes de type 1 de
//! l'affectation, premier `Start`, dernier
//! `Finish` et somme des `Value` (nanosecondes exactes, un seul arrondi à
//! la minute, R-001). Mesuré sur le corpus avant le filtrage de la lecture
//! (R-087) : sur les 155 affectations des 5 fichiers qui ont des lignes de
//! type 1, premier `Start` et dernier `Finish` égalent `Start`/`Finish` de
//! l'affectation 155 fois, la somme arrondie égale `Work` 155 fois. La
//! référence est lue dans la charge elle-même, et non dans le
//! `RecordedSchedule` : celui-ci est jeté au premier commit d'une session
//! (R-162) et `Computed::Schedule` ne le porte pas.

use plannercraft_core::{Assignment, AssignmentSchedule, Moment};
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::extras::{ExtraKey, ORIGIN};
use crate::scalar::{ISO_NANOS_PER_MINUTE, iso_duration_nanos, parse_datetime, parse_int};

/// Nom de l'élément.
const TIMEPHASED: &str = "TimephasedData";

/// Type 1 : travail restant planifié de l'affectation (S37).
const SCHEDULED_WORK: i64 = 1;

/// Type 2 : travail réel de l'affectation (S37).
const ACTUAL_WORK: i64 = 2;

/// Type 3 : autre ligne de réel de l'affectation (S37 : types 2-3 réels).
const ACTUAL_OTHER: i64 = 3;

/// Vrai si l'`Extras` (`key`, `payload`) est une ligne de type 1 de
/// l'affectation elle-même (hors ligne de base : `Baseline#n/…` gardées).
pub(crate) fn is_scheduled_work(key: &str, payload: &str) -> bool {
    ExtraKey::parse(key).is_some_and(|k| is_scheduled_work_key(&k, payload))
}

/// Comme [`is_scheduled_work`], pour une clé déjà lue.
pub(crate) fn is_scheduled_work_key(key: &ExtraKey, payload: &str) -> bool {
    key.scope.is_none()
        && key.name == TIMEPHASED
        && Row::parse(payload).is_some_and(|r| r.kind == Some(SCHEDULED_WORK))
}

/// R-215 : ligne de réel (types 2-3) de l'affectation elle-même (hors
/// ligne de base).
pub(crate) fn is_actual_key(key: &ExtraKey, payload: &str) -> bool {
    key.scope.is_none()
        && key.name == TIMEPHASED
        && Row::parse(payload).is_some_and(|r| matches!(r.kind, Some(ACTUAL_WORK | ACTUAL_OTHER)))
}

/// Comme [`is_actual_key`], pour une clé texte.
pub(crate) fn is_actual(key: &str, payload: &str) -> bool {
    ExtraKey::parse(key).is_some_and(|k| is_actual_key(&k, payload))
}

/// R-215 : les lignes de réel (types 2-3) de `a` sont-elles à jeter ? Vrai
/// s'il y en a et que la somme des `Value` des lignes de type 2 (un seul
/// arrondi à la minute, R-001) diffère du travail réel du modèle, ou que
/// l'une d'elles est illisible : le travail réel a changé depuis la
/// lecture, elles ne le décrivent plus.
pub(crate) fn actuals_stale(a: &Assignment) -> bool {
    let rows: Vec<&str> = a
        .extras
        .0
        .iter()
        .filter(|e| e.origin == ORIGIN && is_actual(&e.key, &e.payload))
        .map(|e| e.payload.as_str())
        .collect();
    if rows.is_empty() {
        return false;
    }
    let mut nanos: i128 = 0;
    for payload in &rows {
        let Some(row) = Row::parse(payload) else {
            return true;
        };
        if row.kind != Some(ACTUAL_WORK) {
            continue;
        }
        let Some(v) = row
            .value
            .as_deref()
            .and_then(|v| iso_duration_nanos(v.trim()).ok())
        else {
            return true;
        };
        let Some(sum) = nanos.checked_add(v) else {
            return true;
        };
        nanos = sum;
    }
    plannercraft_core::round_div(nanos, ISO_NANOS_PER_MINUTE) != Some(a.actual_work.minutes().get())
}

/// R-199 : les lignes de type 1 de `a` sont-elles à jeter ? Vrai s'il y en
/// a et que le moteur ne place pas l'affectation (`placed` absent), que
/// l'une est incomplète ou illisible, ou que leur placement diffère de
/// celui du moteur (début, fin ou travail).
pub(crate) fn stale(a: &Assignment, placed: Option<&AssignmentSchedule>) -> bool {
    let rows: Vec<&str> = a
        .extras
        .0
        .iter()
        .filter(|e| e.origin == ORIGIN && is_scheduled_work(&e.key, &e.payload))
        .map(|e| e.payload.as_str())
        .collect();
    if rows.is_empty() {
        return false;
    }
    let Some(placed) = placed else {
        return true;
    };
    reference(&rows).is_none_or(|(start, finish, work)| {
        start != placed.span.start || finish != placed.span.finish || work != placed.work.minutes().get()
    })
}

/// Placement enregistré : premier début, dernière fin et travail (minutes,
/// R-001) des lignes de type 1 de `rows` (les autres sont ignorées) ;
/// `None` sans ligne de type 1 ou si l'une est incomplète ou illisible.
fn reference(rows: &[&str]) -> Option<(Moment, Moment, i64)> {
    let mut bounds: Option<(Moment, Moment)> = None;
    let mut nanos: i128 = 0;
    for payload in rows {
        let Some(row) = Row::parse(payload) else {
            continue;
        };
        if row.kind != Some(SCHEDULED_WORK) {
            continue;
        }
        let start = parse_datetime(row.start?.trim()).ok()?.moment;
        let finish = parse_datetime(row.finish?.trim()).ok()?.moment;
        nanos = nanos.checked_add(iso_duration_nanos(row.value?.trim()).ok()?)?;
        bounds = Some(match bounds {
            None => (start, finish),
            Some((s, f)) => (s.min(start), f.max(finish)),
        });
    }
    let (start, finish) = bounds?;
    let minutes = plannercraft_core::round_div(nanos, ISO_NANOS_PER_MINUTE)?;
    Some((start, finish, minutes))
}

/// Enfants directs lus d'une ligne.
#[derive(Default)]
struct Row {
    kind: Option<i64>,
    start: Option<String>,
    finish: Option<String>,
    value: Option<String>,
}

impl Row {
    /// Lit `Type`, `Start`, `Finish` et `Value` (enfants directs, nom local,
    /// préfixe admis) ; `None` si la charge est mal formée.
    fn parse(payload: &str) -> Option<Self> {
        let mut reader = Reader::from_str(payload);
        let mut depth: u32 = 0;
        let mut field: Option<String> = None;
        let mut text = String::new();
        let mut row = Self::default();
        loop {
            match reader.read_event().ok()? {
                Event::Start(e) => {
                    depth = depth.saturating_add(1);
                    field = (depth == 2).then(|| e.local_name().as_ref().to_owned());
                    text.clear();
                }
                Event::Text(t) if field.is_some() => text.push_str(&t.xml10_content()),
                Event::End(_) => {
                    if depth == 2
                        && let Some(name) = field.take()
                    {
                        let value = std::mem::take(&mut text);
                        match name.as_str() {
                            "Type" => row.kind = parse_int(value.trim()),
                            "Start" => row.start = Some(value),
                            "Finish" => row.finish = Some(value),
                            "Value" => row.value = Some(value),
                            _ => {}
                        }
                    }
                    depth = depth.saturating_sub(1);
                }
                Event::Eof => return Some(row),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{is_scheduled_work, reference};

    #[test]
    fn r199_reference_of_type_1_rows() {
        let rows = [
            "<TimephasedData><Type>1</Type><UID>1</UID><Start>2026-10-19T08:00:00</Start>\
             <Finish>2026-10-20T08:00:00</Finish><Unit>2</Unit><Value>PT0H0M30S</Value></TimephasedData>",
            "<TimephasedData><Type>1</Type><UID>1</UID><Start>2026-10-20T08:00:00</Start>\
             <Finish>2026-10-20T17:00:00</Finish><Unit>2</Unit><Value>PT8H0M29.97S</Value></TimephasedData>",
            "<TimephasedData><Type>2</Type><Start>2026-10-01T08:00:00</Start>\
             <Finish>2026-10-30T08:00:00</Finish><Value>PT1H0M0S</Value></TimephasedData>",
        ];
        // 30 s + 8 h 0 min 29,97 s = 8 h 0 min 59,97 s → 481 min (R-001) ;
        // la ligne de type 2 ne compte pas.
        let (s, f, w) = reference(&rows).unwrap();
        assert_eq!(s, "2026-10-19T08:00".parse().unwrap());
        assert_eq!(f, "2026-10-20T17:00".parse().unwrap());
        assert_eq!(w, 481);
        assert_eq!(reference(&rows[2..]), None);
        assert_eq!(
            reference(&["<TimephasedData><Type>1</Type></TimephasedData>"]),
            None
        );
        assert!(is_scheduled_work("TimephasedData", rows[0]));
        assert!(!is_scheduled_work("TimephasedData", rows[2]));
        assert!(!is_scheduled_work("Baseline#1/TimephasedData", rows[0]));
    }
}
