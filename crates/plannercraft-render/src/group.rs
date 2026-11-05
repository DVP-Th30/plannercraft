//! Regroupement des tâches (finitions P3, ADR-0022 D4, H-UI-129, H-R17).
//!
//! [`group_tasks`] range les tâches **feuilles** retenues (lignes vides et
//! récapitulatives écartées) sous une valeur de [`GroupBy`] : texte d'un
//! champ personnalisé, ressource affectée (une tâche à plusieurs ressources
//! est sous chacune), critique ou non, jalon ou non. Dans un groupe, les
//! tâches restent en ordre des ID. Ordre des groupes : texte en ordre
//! naturel (« Lot 2 » avant « Lot 10 », sans casse ni accents), valeur
//! absente en dernier ; Critique avant non critique ; Jalons d'abord.
//!
//! Début et fin d'un groupe : le plus petit début et la plus grande fin des
//! tâches **planifiées** du groupe, lus tels quels dans le `Schedule` du
//! moteur (règle 3 : aucune date calculée ici, une agrégation de valeurs
//! du moteur), avec la tâche qui les porte (ses textes de Début et de Fin
//! servent à l'en-tête : même format que la ligne de la tâche).
//!
//! Partagé par la scène (aperçu, PDF, SVG, `render_gantt`) et la table de
//! l'interface : mêmes groupes, même ordre, mêmes libellés.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::{CustomValue, Moment, Project, ResourceUid, Schedule, TaskOutcome, TaskUid};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::request::Lang;
use crate::style::GroupBy;

/// Identité d'un groupe (clé de repli de l'interface).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GroupKey {
    /// Valeur d'un champ personnalisé texte.
    Text(String),
    /// Ressource (UID).
    Resource(u32),
    /// Critique / jalon (`true`) ou non.
    Flag(bool),
    /// Sans valeur : texte vide, aucune ressource.
    Empty,
}

/// Un groupe : sa valeur, ses tâches et ses bornes lues du planning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// Identité.
    pub key: GroupKey,
    /// Valeur affichée d'un texte ou nom d'une ressource (`None` : drapeau
    /// ou sans valeur, voir [`group_label`]).
    pub value: Option<String>,
    /// Positions des tâches dans `Project::tasks`, en ordre des ID.
    pub tasks: Vec<usize>,
    /// Plus petit début planifié et la position de la tâche qui le porte
    /// (la première en ordre des ID à égalité).
    pub start: Option<(Moment, usize)>,
    /// Plus grande fin planifiée et la position de la tâche qui la porte.
    pub finish: Option<(Moment, usize)>,
}

impl Group {
    /// Groupe vide de clé `key`.
    fn new(key: GroupKey, value: Option<String>) -> Self {
        Self {
            key,
            value,
            tasks: Vec::new(),
            start: None,
            finish: None,
        }
    }

    /// Ajoute la tâche de position `position` (planifiée de `start` à
    /// `finish`, si elle l'est).
    fn push(&mut self, position: usize, span: Option<(Moment, Moment)>) {
        self.tasks.push(position);
        if let Some((start, finish)) = span {
            if self.start.is_none_or(|(s, _)| start < s) {
                self.start = Some((start, position));
            }
            if self.finish.is_none_or(|(f, _)| finish > f) {
                self.finish = Some((finish, position));
            }
        }
    }
}

/// Vrai si la tâche de position `i` est une récapitulative (la suivante
/// est plus profonde).
fn is_summary(p: &Project, i: usize) -> bool {
    match (p.tasks.at(i), p.tasks.at(i.saturating_add(1))) {
        (Some(t), Some(next)) => next.outline_level.get() > t.outline_level.get(),
        _ => false,
    }
}

/// Groupes des tâches feuilles de `p` retenues par `keep` (position dans
/// `Project::tasks`), dans l'ordre d'affichage ; aucun groupe vide.
#[must_use]
pub fn group_tasks(p: &Project, s: &Schedule, by: GroupBy, keep: &dyn Fn(usize) -> bool) -> Vec<Group> {
    let resources: BTreeMap<TaskUid, BTreeSet<ResourceUid>> = if by == GroupBy::Resource {
        let mut map: BTreeMap<TaskUid, BTreeSet<ResourceUid>> = BTreeMap::new();
        for a in &p.assignments {
            if let Some(r) = a.resource.filter(|r| p.resources.contains(*r)) {
                map.entry(a.task).or_default().insert(r);
            }
        }
        map
    } else {
        BTreeMap::new()
    };
    let mut groups: BTreeMap<GroupKey, Group> = BTreeMap::new();
    for (i, task) in p.tasks.iter().enumerate() {
        if task.is_null || !keep(i) || is_summary(p, i) {
            continue;
        }
        let uid = task.uid();
        let scheduled = match s.tasks.get(&uid) {
            Some(TaskOutcome::Scheduled(t)) => Some(t),
            _ => None,
        };
        let span = scheduled.map(|t| (t.start, t.finish));
        let mut put = |key: GroupKey, value: Option<String>| {
            groups
                .entry(key.clone())
                .or_insert_with(|| Group::new(key, value))
                .push(i, span);
        };
        match by {
            GroupBy::Custom(field) => match task.custom.get(&field) {
                Some(CustomValue::Text(t)) if !t.is_empty() => {
                    put(GroupKey::Text(t.clone()), Some(t.clone()))
                }
                _ => put(GroupKey::Empty, None),
            },
            GroupBy::Resource => match resources.get(&uid) {
                Some(set) if !set.is_empty() => {
                    for r in set {
                        let name = p.resources.get(*r).map(|res| res.name.clone());
                        put(GroupKey::Resource(r.get()), name);
                    }
                }
                _ => put(GroupKey::Empty, None),
            },
            GroupBy::Critical => put(GroupKey::Flag(scheduled.is_some_and(|t| t.critical)), None),
            GroupBy::Milestone => put(GroupKey::Flag(task.milestone || task.duration.is_zero()), None),
        }
    }
    let mut out: Vec<Group> = groups.into_values().collect();
    out.sort_by(compare_groups);
    out
}

/// Ordre des groupes : drapeau vrai d'abord ; valeurs en ordre naturel,
/// puis exactes, puis UID ; sans valeur en dernier.
fn compare_groups(a: &Group, b: &Group) -> Ordering {
    let rank = |g: &Group| match g.key {
        GroupKey::Flag(true) => 0,
        GroupKey::Flag(false) => 1,
        GroupKey::Text(_) | GroupKey::Resource(_) => 2,
        GroupKey::Empty => 3,
    };
    rank(a).cmp(&rank(b)).then_with(|| {
        let (va, vb) = (a.value.as_deref().unwrap_or(""), b.value.as_deref().unwrap_or(""));
        natural_cmp(va, vb)
            .then_with(|| va.cmp(vb))
            .then_with(|| a.key.cmp(&b.key))
    })
}

/// Texte sans casse ni accents (décomposition NFD, marques retirées,
/// minuscules), comme les filtres de `plannercraft-automation`.
fn fold(s: &str) -> String {
    s.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// Morceau d'un texte pour l'ordre naturel : suite de chiffres ou de
/// non-chiffres.
fn chunks(s: &str) -> Vec<(bool, String)> {
    let mut out: Vec<(bool, String)> = Vec::new();
    for c in s.chars() {
        let digit = c.is_ascii_digit();
        match out.last_mut() {
            Some((d, chunk)) if *d == digit => chunk.push(c),
            _ => out.push((digit, c.to_string())),
        }
    }
    out
}

/// Ordre naturel (H-R17, hypothèse) : textes repliés comparés morceau par
/// morceau, les suites de chiffres par leur valeur (zéros de tête ignorés,
/// sans limite de longueur), un nombre avant un mot.
#[must_use]
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (ca, cb) = (chunks(&fold(a)), chunks(&fold(b)));
    for ((da, xa), (db, xb)) in ca.iter().zip(cb.iter()) {
        let order = match (da, db) {
            (true, true) => {
                let (ta, tb) = (xa.trim_start_matches('0'), xb.trim_start_matches('0'));
                ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb))
            }
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => xa.cmp(xb),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    ca.len().cmp(&cb.len())
}

/// Libellé de la valeur d'un groupe : la valeur ou le nom de la ressource,
/// « (sans valeur) », « Critique » / « Non critique », « Jalons » /
/// « Autres tâches ».
#[must_use]
pub fn group_label(by: GroupBy, g: &Group, lang: Lang) -> String {
    let fr = lang == Lang::Fr;
    match (&g.key, by) {
        (GroupKey::Empty, _) => if fr { "(sans valeur)" } else { "(no value)" }.to_owned(),
        (GroupKey::Flag(on), GroupBy::Critical) => match (on, fr) {
            (true, true) => "Critique",
            (false, true) => "Non critique",
            (true, false) => "Critical",
            (false, false) => "Not critical",
        }
        .to_owned(),
        (GroupKey::Flag(on), _) => match (on, fr) {
            (true, true) => "Jalons",
            (false, true) => "Autres tâches",
            (true, false) => "Milestones",
            (false, false) => "Other tasks",
        }
        .to_owned(),
        (GroupKey::Text(_) | GroupKey::Resource(_), _) => g.value.clone().unwrap_or_default(),
    }
}

/// Texte de la ligne d'en-tête : libellé et nombre de tâches
/// (« Gros œuvre (3 tâches) », « Plomberie (1 tâche) »).
#[must_use]
pub fn header_text(label: &str, count: usize, lang: Lang) -> String {
    let noun = match (lang, count) {
        (Lang::Fr, 0 | 1) => "tâche",
        (Lang::Fr, _) => "tâches",
        (Lang::En, 1) => "task",
        (Lang::En, _) => "tasks",
    };
    format!("{label} ({count} {noun})")
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use super::*;

    #[test]
    fn h_r17_natural_order() {
        let mut v = vec![
            "Lot 10",
            "lot 2",
            "Électricité",
            "Lot 1",
            "eau",
            "Lot 02b",
            "Zinc",
        ];
        v.sort_by(|a, b| natural_cmp(a, b).then_with(|| a.cmp(b)));
        assert_eq!(
            v,
            [
                "eau",
                "Électricité",
                "Lot 1",
                "lot 2",
                "Lot 02b",
                "Lot 10",
                "Zinc"
            ]
        );
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
        assert_eq!(natural_cmp("1", "a"), Ordering::Less);
        // Très longues suites de chiffres : aucune conversion numérique.
        assert_eq!(
            natural_cmp("9".repeat(40).as_str(), &format!("1{}", "0".repeat(40))),
            Ordering::Less
        );
    }

    #[test]
    fn h_ui_129_header_text_plural() {
        assert_eq!(header_text("Lot A", 1, Lang::Fr), "Lot A (1 tâche)");
        assert_eq!(header_text("Lot A", 3, Lang::Fr), "Lot A (3 tâches)");
        assert_eq!(header_text("Lot A", 1, Lang::En), "Lot A (1 task)");
        assert_eq!(header_text("Lot A", 2, Lang::En), "Lot A (2 tasks)");
    }
}
