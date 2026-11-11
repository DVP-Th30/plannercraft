//! Diff de planning entre deux états (R-163, lot L2) ; diff prêt à
//! afficher du panneau d'opérations IA ([`DiffView`], J8 lot L12).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, btree_map};
use std::iter::Peekable;

use plannercraft_core::calendar::CompiledCalendar;
use plannercraft_core::{
    DurationUnit, Minutes, Moment, Project, ResourceLoad, ResourceUid, Schedule, TaskId, TaskOutcome,
    TaskSchedule, TaskUid,
};
use serde::Serialize;

use crate::delta::Delta;
use crate::text::TextContext;

/// Diff de planning d'un lot (R-163).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScheduleDiff {
    /// Fin du projet (avant, après).
    pub project_finish: (Moment, Moment),
    /// Début du projet (avant, après).
    pub project_start: (Moment, Moment),
    /// Tâches devenues critiques, par UID.
    pub critical_entered: Vec<TaskUid>,
    /// Tâches qui ne sont plus critiques, par UID.
    pub critical_left: Vec<TaskUid>,
    /// Nombre exact de tâches aux dates changées (début ou fin planifiés
    /// déplacés), apparues ou disparues : « dates déplacées ».
    pub changed_total: usize,
    /// Nombre exact de tâches touchées par le lot (R-163 amendée) : donnée
    /// enregistrée changée (champ de la tâche ou de l'une de ses
    /// affectations), ajoutées ou supprimées, déplacées dans la liste
    /// (amendement du 2026-11-11, [`moved_tasks`]), réunies avec les tâches
    /// de `changed_total`. Toujours `>= changed_total`.
    pub touched_total: usize,
    /// Tâches changées, triées, au plus N.
    pub changed: Vec<TaskChange>,
    /// Vrai si `changed` a été coupé.
    pub truncated: bool,
    /// Ressources surutilisées après et pas avant (R-186 ; J5, lot L8),
    /// par UID ; absent du JSON quand vide.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overallocated_new: Vec<ResourceUid>,
    /// Ressources surutilisées avant et plus après (ou supprimées), par
    /// UID ; absent du JSON quand vide.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overallocated_resolved: Vec<ResourceUid>,
}

/// Changement d'une tâche (R-163). Chaque paire est (avant, après).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskChange {
    /// UID.
    pub uid: TaskUid,
    /// ID avant (absent : tâche ajoutée).
    pub id_before: Option<TaskId>,
    /// ID après (absent : tâche supprimée).
    pub id_after: Option<TaskId>,
    /// Début planifié.
    pub start: (Option<Moment>, Option<Moment>),
    /// Fin planifiée.
    pub finish: (Option<Moment>, Option<Moment>),
    /// Marge totale.
    pub total_slack: (Option<Minutes>, Option<Minutes>),
    /// Critique.
    pub critical: (bool, bool),
    /// Minutes écoulées fin après − fin avant (0 si un côté est absent).
    pub finish_delta_min: i64,
}

impl ScheduleDiff {
    /// Diff vide d'un planning inchangé.
    #[must_use]
    pub fn unchanged(s: &Schedule) -> Self {
        Self {
            project_finish: (s.project_finish, s.project_finish),
            project_start: (s.project_start, s.project_start),
            critical_entered: Vec::new(),
            critical_left: Vec::new(),
            changed_total: 0,
            touched_total: 0,
            changed: Vec::new(),
            truncated: false,
            overallocated_new: Vec::new(),
            overallocated_resolved: Vec::new(),
        }
    }
}

/// Nombre de tâches du diff par défaut (`detail` : `diff_detail_max`).
pub const DEFAULT_DIFF_TASKS: usize = 10;

/// R-163 : diff des plannings avant/après, `changed` limité à `n`.
///
/// Jointure triée des `Schedule.tasks` par UID ; une tâche est changée si
/// son début ou sa fin planifiés diffèrent, ou si elle apparaît ou
/// disparaît. La criticité est suivie sur toutes les tâches. Tri : |Δ fin|
/// décroissant, ID d'après croissant (absent en dernier), UID.
///
/// `entities` est le delta par entité des deux projets (R-164, celui que
/// l'historique garde) : il donne les tâches touchées (`touched_total`)
/// sans seconde comparaison.
pub(crate) fn diff_schedules(
    before_p: &Project,
    before: &Schedule,
    after_p: &Project,
    after: &Schedule,
    n: usize,
    entities: &Delta,
) -> ScheduleDiff {
    let mut d = ScheduleDiff {
        project_finish: (before.project_finish, after.project_finish),
        project_start: (before.project_start, after.project_start),
        critical_entered: Vec::new(),
        critical_left: Vec::new(),
        changed_total: 0,
        touched_total: 0,
        changed: Vec::new(),
        truncated: false,
        overallocated_new: Vec::new(),
        overallocated_resolved: Vec::new(),
    };
    overallocation_changes(&before.resources, &after.resources, &mut d);
    let mut changed = Vec::new();
    for (uid, b, a) in Join::new(&before.tasks, &after.tasks) {
        let bs = b.and_then(scheduled);
        let as_ = a.and_then(scheduled);
        let crit = (bs.is_some_and(|s| s.critical), as_.is_some_and(|s| s.critical));
        match crit {
            (false, true) => d.critical_entered.push(uid),
            (true, false) => d.critical_left.push(uid),
            _ => {}
        }
        let start = (bs.map(|s| s.start), as_.map(|s| s.start));
        let finish = (bs.map(|s| s.finish), as_.map(|s| s.finish));
        let presence_changed = b.is_some() != a.is_some();
        if !presence_changed && start.0 == start.1 && finish.0 == finish.1 {
            continue;
        }
        let finish_delta_min = match finish {
            (Some(f0), Some(f1)) => f0.minutes_until(f1).get(),
            _ => 0,
        };
        changed.push(TaskChange {
            uid,
            id_before: b.and(before_p.task_id(uid)),
            id_after: a.and(after_p.task_id(uid)),
            start,
            finish,
            total_slack: (bs.map(|s| s.total_slack), as_.map(|s| s.total_slack)),
            critical: crit,
            finish_delta_min,
        });
    }
    changed.sort_by(compare);
    d.changed_total = changed.len();
    d.touched_total = touched_tasks(before_p, after_p, entities, changed.iter().map(|c| c.uid)).len();
    d.truncated = changed.len() > n;
    changed.truncate(n);
    d.changed = changed;
    d
}

/// Tâches touchées (R-163 amendée), triées par UID : tâches du delta par
/// entité (champ changé, ajoutée, supprimée), tâches des affectations
/// changées, ajoutées ou retirées (tâche d'avant et d'après de
/// l'affectation, si elle existe d'un côté), tâches déplacées dans la liste
/// ([`moved_tasks`], amendement du 2026-11-11 : une tâche dont seul l'ID
/// change parce qu'une autre est passée devant elle n'est pas déplacée), et
/// tâches aux dates changées.
pub(crate) fn touched_tasks(
    before_p: &Project,
    after_p: &Project,
    entities: &Delta,
    changed: impl IntoIterator<Item = TaskUid>,
) -> BTreeSet<TaskUid> {
    let mut touched: BTreeSet<TaskUid> = entities.tasks.changed.iter().map(|c| c.0).collect();
    let exists = |uid: TaskUid| before_p.tasks.contains(uid) || after_p.tasks.contains(uid);
    for (_, b, a) in &entities.assignments.changed {
        for task in [b.as_ref(), a.as_ref()].into_iter().flatten().map(|x| x.task) {
            if exists(task) {
                touched.insert(task);
            }
        }
    }
    touched.extend(moved_tasks(entities.tasks.order.as_ref()));
    touched.extend(changed);
    touched
}

/// Tâches déplacées dans la liste (R-163 amendée le 2026-11-11), triées par
/// UID, calculées sur le seul ordre des UID (avant, après) du delta par
/// entité (R-164) : parmi les tâches présentes des deux côtés (une tâche
/// ajoutée ou supprimée ne rend pas ses voisines déplacées), celles qui
/// restent hors d'une plus longue sous-suite commune des deux ordres, soit
/// le plus petit ensemble de tâches à retirer pour que les autres gardent
/// leur ordre relatif.
///
/// Départage (plusieurs plus longues sous-suites) : la sous-suite gardée
/// est choisie de la dernière tâche vers la première, en prenant à chaque
/// pas la tâche placée le plus bas dans la liste d'après ; deux voisines
/// échangées ⇒ la tâche remontée est réputée déplacée. Calculé sur l'ordre
/// du lot (et non sur les deux projets) : annuler et rétablir, qui
/// reprennent le delta de l'entrée, trouvent le même ensemble.
///
/// Plus longue sous-suite croissante des rangs d'avant pris dans l'ordre
/// d'après (tri par paquets, recherche dichotomique) : O(n log n), sans
/// table quadratique.
pub(crate) fn moved_tasks(order: Option<&(Vec<TaskUid>, Vec<TaskUid>)>) -> BTreeSet<TaskUid> {
    let Some((before, after)) = order else {
        return BTreeSet::new();
    };
    let in_after: BTreeSet<TaskUid> = after.iter().copied().collect();
    // Rang d'avant des seules tâches présentes des deux côtés.
    let rank: BTreeMap<TaskUid, usize> = before
        .iter()
        .filter(|u| in_after.contains(u))
        .enumerate()
        .map(|(i, u)| (*u, i))
        .collect();
    let seq: Vec<(TaskUid, usize)> = after
        .iter()
        .filter_map(|u| rank.get(u).map(|r| (*u, *r)))
        .collect();
    // `tails[k]` : indice dans `seq` de la dernière tâche (dans l'ordre
    // d'après) qui termine une sous-suite croissante de longueur k + 1 ;
    // `prev[i]` : tâche qui précède `i` dans sa sous-suite.
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = Vec::with_capacity(seq.len());
    for (i, (_, r)) in seq.iter().enumerate() {
        let k = tails.partition_point(|t| seq.get(*t).is_some_and(|x| x.1 < *r));
        let before_k = k.checked_sub(1).and_then(|j| tails.get(j)).copied();
        prev.push(before_k);
        match tails.get_mut(k) {
            Some(slot) => *slot = i,
            None => tails.push(i),
        }
    }
    let mut kept = vec![false; seq.len()];
    let mut cur = tails.last().copied();
    while let Some(i) = cur {
        if let Some(k) = kept.get_mut(i) {
            *k = true;
        }
        cur = prev.get(i).copied().flatten();
    }
    seq.iter()
        .zip(kept)
        .filter(|(_, k)| !k)
        .map(|((u, _), _)| *u)
        .collect()
}

/// Surutilisations apparues et résolues (J5, ADR-0013 D9) : jointure
/// triée des charges par UID ; une ressource absente d'un côté compte comme
/// non surutilisée de ce côté.
fn overallocation_changes(
    before: &std::collections::BTreeMap<ResourceUid, ResourceLoad>,
    after: &std::collections::BTreeMap<ResourceUid, ResourceLoad>,
    d: &mut ScheduleDiff,
) {
    let over = |m: &std::collections::BTreeMap<ResourceUid, ResourceLoad>, uid: &ResourceUid| {
        m.get(uid).is_some_and(|l| l.overallocated)
    };
    for (uid, load) in after {
        if load.overallocated && !over(before, uid) {
            d.overallocated_new.push(*uid);
        }
    }
    for (uid, load) in before {
        if load.overallocated && !over(after, uid) {
            d.overallocated_resolved.push(*uid);
        }
    }
}

/// Ordre de R-163.
fn compare(x: &TaskChange, y: &TaskChange) -> Ordering {
    y.finish_delta_min
        .unsigned_abs()
        .cmp(&x.finish_delta_min.unsigned_abs())
        .then_with(|| match (x.id_after, y.id_after) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
        .then_with(|| x.uid.cmp(&y.uid))
}

const fn scheduled(o: &TaskOutcome) -> Option<&TaskSchedule> {
    match o {
        TaskOutcome::Scheduled(s) => Some(s),
        TaskOutcome::Excluded { .. } => None,
    }
}

/// Jointure externe triée de deux tables par UID.
struct Join<'a> {
    left: Peekable<btree_map::Iter<'a, TaskUid, TaskOutcome>>,
    right: Peekable<btree_map::Iter<'a, TaskUid, TaskOutcome>>,
}

impl<'a> Join<'a> {
    fn new(
        left: &'a std::collections::BTreeMap<TaskUid, TaskOutcome>,
        right: &'a std::collections::BTreeMap<TaskUid, TaskOutcome>,
    ) -> Self {
        Self {
            left: left.iter().peekable(),
            right: right.iter().peekable(),
        }
    }
}

impl<'a> Iterator for Join<'a> {
    type Item = (TaskUid, Option<&'a TaskOutcome>, Option<&'a TaskOutcome>);

    fn next(&mut self) -> Option<Self::Item> {
        let side = match (self.left.peek(), self.right.peek()) {
            (None, None) => return None,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(l), Some(r)) => l.0.cmp(r.0),
        };
        match side {
            Ordering::Less => self.left.next().map(|(k, v)| (*k, Some(v), None)),
            Ordering::Greater => self.right.next().map(|(k, v)| (*k, None, Some(v))),
            Ordering::Equal => {
                let l = self.left.next()?;
                let r = self.right.next();
                Some((*l.0, Some(l.1), r.map(|(_, v)| v)))
            }
        }
    }
}

/// Lignes au plus d'un diff affiché par le panneau d'opérations IA (J8,
/// lot L12, H-UI-41 ; plafond de la règle 10 de J8).
pub const MAX_VIEW_ROWS: usize = 200;

/// Nature du changement d'une ligne du panneau.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowChange {
    /// Tâche ajoutée par l'écriture proposée.
    Added,
    /// Tâche supprimée.
    Removed,
    /// Dates changées.
    Changed,
    /// Dates inchangées, criticité changée seulement.
    CriticalOnly,
}

/// Criticité entrée ou sortie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CriticalChange {
    /// Inchangée.
    Same,
    /// Devenue critique.
    Entered,
    /// N'est plus critique.
    Left,
}

/// Ligne avant/après d'une tâche (H-UI-41) : valeurs du moteur, mises en
/// texte par l'interface (règle 3 : rien n'est calculé ici).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffRow {
    /// UID.
    pub uid: TaskUid,
    /// ID (après, sinon avant pour une tâche supprimée).
    pub id: Option<TaskId>,
    /// Nom (après, sinon avant).
    pub name: String,
    /// Nature du changement.
    pub change: RowChange,
    /// Début planifié (avant, après).
    pub start: (Option<Moment>, Option<Moment>),
    /// Fin planifiée (avant, après).
    pub finish: (Option<Moment>, Option<Moment>),
    /// Marge totale (avant, après).
    pub total_slack: (Option<Minutes>, Option<Minutes>),
    /// Criticité entrée ou sortie.
    pub critical: CriticalChange,
}

/// Un côté d'un diff : projet et planning du moteur.
#[derive(Clone, Copy, Debug)]
pub struct DiffSide<'a> {
    /// Projet.
    pub project: &'a Project,
    /// Planning calculé par le moteur.
    pub schedule: &'a Schedule,
}

/// Diff prêt à afficher d'une écriture proposée par un agent (J8, lot L12,
/// H-UI-41, ADR-0016 D10), calculé **une fois** à l'arrivée de la demande :
/// au plus [`MAX_VIEW_ROWS`] lignes (tâches aux dates changées, dans
/// l'ordre de R-163, puis tâches dont seule la criticité change, par UID),
/// le compte des autres (`more`), l'écart de fin du projet en temps ouvré
/// et le changement du chemin critique.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffView {
    /// Lignes, au plus [`MAX_VIEW_ROWS`].
    pub rows: Vec<DiffRow>,
    /// Tâches changées non affichées.
    pub more: usize,
    /// Fin du projet (avant, après).
    pub project_finish: (Moment, Moment),
    /// Écart de fin du projet : temps ouvré du calendrier du projet
    /// (R-020), en minutes ; `None` si le calendrier ne le mesure pas.
    pub finish_delta: Option<Minutes>,
    /// Même écart en texte du moteur, en jours (« 1j », « -2j ») ; vide si
    /// le calendrier ne le mesure pas.
    pub finish_delta_text: String,
    /// Tâches devenues critiques.
    pub critical_entered: usize,
    /// Tâches qui ne sont plus critiques.
    pub critical_left: usize,
}

impl DiffView {
    /// Diff affiché de `d`, calculé entre `before` (la session) et `after`
    /// (l'état simulé du `dry_run`) ; textes dans le contexte `text`.
    #[must_use]
    pub fn new(d: &ScheduleDiff, before: DiffSide<'_>, after: DiffSide<'_>, text: &TextContext) -> Self {
        let name_of = |uid: TaskUid| {
            after
                .project
                .tasks
                .get(uid)
                .or_else(|| before.project.tasks.get(uid))
                .map(|t| t.name.clone())
                .unwrap_or_default()
        };
        let id_of = |uid: TaskUid| after.project.task_id(uid).or_else(|| before.project.task_id(uid));
        let critical_of = |c: (bool, bool)| match c {
            (false, true) => CriticalChange::Entered,
            (true, false) => CriticalChange::Left,
            _ => CriticalChange::Same,
        };
        let mut rows: Vec<DiffRow> = Vec::new();
        for c in d.changed.iter().take(MAX_VIEW_ROWS) {
            let change = match (c.id_before, c.id_after) {
                (None, Some(_)) => RowChange::Added,
                (Some(_), None) => RowChange::Removed,
                _ => RowChange::Changed,
            };
            rows.push(DiffRow {
                uid: c.uid,
                id: c.id_after.or(c.id_before),
                name: name_of(c.uid),
                change,
                start: c.start,
                finish: c.finish,
                total_slack: c.total_slack,
                critical: critical_of(c.critical),
            });
        }
        // Criticité changée sans date changée : jointure des deux listes
        // triées par UID, hors des tâches déjà listées par le diff.
        let listed: BTreeSet<TaskUid> = d.changed.iter().map(|c| c.uid).collect();
        let mut flips: Vec<(TaskUid, CriticalChange)> = d
            .critical_entered
            .iter()
            .map(|u| (*u, CriticalChange::Entered))
            .chain(d.critical_left.iter().map(|u| (*u, CriticalChange::Left)))
            .filter(|(u, _)| !listed.contains(u))
            .collect();
        flips.sort_by_key(|(u, _)| *u);
        let flips_total = flips.len();
        let mut shown_flips = 0usize;
        for (uid, critical) in flips {
            if rows.len() >= MAX_VIEW_ROWS {
                break;
            }
            let pair = |s: &Schedule| s.scheduled(uid).copied();
            let (b, a) = (pair(before.schedule), pair(after.schedule));
            rows.push(DiffRow {
                uid,
                id: id_of(uid),
                name: name_of(uid),
                change: RowChange::CriticalOnly,
                start: (b.map(|s| s.start), a.map(|s| s.start)),
                finish: (b.map(|s| s.finish), a.map(|s| s.finish)),
                total_slack: (b.map(|s| s.total_slack), a.map(|s| s.total_slack)),
                critical,
            });
            shown_flips = shown_flips.saturating_add(1);
        }
        let shown_changed = d.changed.len().min(MAX_VIEW_ROWS);
        let more = d
            .changed_total
            .saturating_sub(shown_changed)
            .saturating_add(flips_total.saturating_sub(shown_flips));
        let (f0, f1) = d.project_finish;
        let working = CompiledCalendar::compile(&before.project.calendars, before.project.calendar)
            .ok()
            .and_then(|c| c.working_between(f0, f1).ok());
        let finish_delta_text = working
            .and_then(|m| crate::view::signed_minutes(m, DurationUnit::Day, text).text)
            .unwrap_or_default();
        Self {
            rows,
            more,
            project_finish: d.project_finish,
            finish_delta: working,
            finish_delta_text,
            critical_entered: d.critical_entered.len(),
            critical_left: d.critical_left.len(),
        }
    }

    /// Vrai si le chemin critique change.
    #[must_use]
    pub const fn critical_changed(&self) -> bool {
        self.critical_entered > 0 || self.critical_left > 0
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        reason = "tests : un échec doit s'arrêter net et lisiblement"
    )]

    use plannercraft_core::Moment;
    use serde_json::{Value, json};

    use plannercraft_core::TaskUid;

    use super::{CriticalChange, DiffSide, DiffView, MAX_VIEW_ROWS, RowChange, moved_tasks};
    use crate::command::Batch;
    use crate::session::{ApplyOptions, Session};
    use crate::text::{Language, TextContext};
    use crate::workspace::Workspace;

    fn now() -> Moment {
        "2026-10-19T08:00".parse().unwrap()
    }

    /// Session du projet neuf (lun 19/10/2026) après `ops`.
    fn session(ops: &Value) -> Session {
        let mut ws = Workspace::new(now(), Language::Fr);
        ws.call("project.new", json!({ "name": "P", "start": "2026-10-19T08:00" }))
            .unwrap();
        ws.call("batch.apply", json!({ "ops": ops })).unwrap();
        ws.session().unwrap().clone()
    }

    /// Diff affiché du lot `ops` simulé sur `s` (diff détaillé).
    fn view(s: &Session, ops: &Value) -> DiffView {
        let batch = Batch::from_json(json!({ "format_version": 1, "ops": ops })).unwrap();
        let sim = s
            .simulate(
                &batch,
                ApplyOptions {
                    detail: true,
                    ..ApplyOptions::default()
                },
            )
            .unwrap();
        let before = DiffSide {
            project: s.project(),
            schedule: s.schedule(),
        };
        let after = DiffSide {
            project: &sim.project,
            schedule: &sim.schedule,
        };
        DiffView::new(
            &sim.outcome.diff,
            before,
            after,
            &TextContext::for_project(s.project(), Language::Fr),
        )
    }

    /// H-UI-41 : tâche ajoutée nommée d'après l'état simulé, tâche décalée,
    /// écart de fin du projet en jours ouvrés, criticité entrée.
    #[test]
    fn h_ui_41_diff_view_rows() {
        let s = session(&json!([
            { "op": "task.add", "name": "Fondations", "as": "$a", "duration": { "text": "5j" } },
            { "op": "task.add", "name": "Murs", "as": "$b", "duration": { "text": "10j" } },
            { "op": "link.add", "pred": "$a", "succ": "$b" },
            { "op": "task.add", "name": "Clôture", "duration": { "text": "2j" } }
        ]));
        // Nettoyage après Murs : fin du projet +1 j, Nettoyage critique.
        let v = view(
            &s,
            &json!([
                { "op": "task.add", "name": "Nettoyage", "duration": { "text": "1j" }, "as": "$n" },
                { "op": "link.add", "pred": 2, "succ": "$n" }
            ]),
        );
        let added = v.rows.iter().find(|r| r.change == RowChange::Added).unwrap();
        assert_eq!(added.name, "Nettoyage");
        assert_eq!(added.start.0, None);
        assert!(added.start.1.is_some());
        assert_eq!(v.finish_delta_text, "1j");
        assert_eq!(v.finish_delta.map(plannercraft_core::Minutes::get), Some(480));
        assert_eq!(v.more, 0);
        assert_eq!(added.critical, CriticalChange::Entered);
        assert!(v.critical_changed());
    }

    /// Criticité changée sans date changée : ligne `CriticalOnly`.
    #[test]
    fn diff_view_critical_only_rows() {
        let s = session(&json!([
            { "op": "task.add", "name": "Longue", "duration": { "text": "5j" } },
            { "op": "task.add", "name": "Courte", "duration": { "text": "2j" } }
        ]));
        let v = view(
            &s,
            &json!([{ "op": "task.update", "uid": 2, "duration": { "text": "6j" } }]),
        );
        let changed = v.rows.iter().find(|r| r.name == "Courte").unwrap();
        assert_eq!(changed.change, RowChange::Changed);
        assert_eq!(changed.critical, CriticalChange::Entered);
        let flip = v.rows.iter().find(|r| r.name == "Longue").unwrap();
        assert_eq!(flip.change, RowChange::CriticalOnly);
        assert_eq!(flip.critical, CriticalChange::Left);
        assert_eq!(flip.start.0, flip.start.1);
        assert_eq!(flip.total_slack.0.map(plannercraft_core::Minutes::get), Some(0));
        assert_eq!(flip.total_slack.1.map(plannercraft_core::Minutes::get), Some(480));
    }

    /// Règle 10 de J8 : au plus 200 lignes, le reste compté dans `more`.
    #[test]
    fn h_ui_41_diff_view_capped() {
        let ops: Vec<Value> = (0..250)
            .map(|i| json!({ "op": "task.add", "name": format!("T{i}"), "duration": { "text": "1j" } }))
            .collect();
        let s = session(&Value::Array(ops));
        let ops: Vec<Value> = (1..=250)
            .map(|uid| json!({ "op": "task.update", "uid": uid, "duration": { "text": "2j" } }))
            .collect();
        let v = view(&s, &Value::Array(ops));
        assert_eq!(v.rows.len(), MAX_VIEW_ROWS);
        assert_eq!(v.more, 50);
    }

    fn uids(v: &[u32]) -> Vec<TaskUid> {
        v.iter().map(|u| TaskUid::new(*u)).collect()
    }

    fn moved(before: &[u32], after: &[u32]) -> Vec<u32> {
        moved_tasks(Some(&(uids(before), uids(after))))
            .into_iter()
            .map(TaskUid::get)
            .collect()
    }

    /// R-163 (ordre) : plus petit ensemble de tâches à retirer pour que les
    /// autres gardent leur ordre ; départage : la tâche remontée.
    #[test]
    fn r163_moved_tasks_rule() {
        assert_eq!(moved_tasks(None).len(), 0);
        assert_eq!(moved(&[1, 2, 3, 4], &[4, 1, 2, 3]), vec![4]);
        assert_eq!(moved(&[1, 2, 3, 4], &[2, 3, 4, 1]), vec![1]);
        // Deux voisines échangées : la remontée (3) est déplacée.
        assert_eq!(moved(&[1, 2, 3, 4], &[1, 3, 2, 4]), vec![3]);
        // Deux blocs de même taille échangés : le bloc remonté.
        assert_eq!(moved(&[1, 2, 3, 4], &[3, 4, 1, 2]), vec![3, 4]);
        // Bloc de trois passé devant une tâche : la tâche passée.
        assert_eq!(moved(&[1, 2, 3, 4], &[2, 3, 4, 1]), vec![1]);
        // Ajouts et suppressions ne déplacent pas les voisines.
        assert_eq!(moved(&[1, 2, 3], &[1, 9, 3]), Vec::<u32>::new());
        assert_eq!(moved(&[1, 2, 3], &[9, 1, 2, 3]), Vec::<u32>::new());
        assert_eq!(moved(&[1, 2, 3], &[3, 9, 1]), vec![3]);
        // Ordre inversé : tout sauf la dernière d'après.
        assert_eq!(moved(&[1, 2, 3, 4], &[4, 3, 2, 1]), vec![2, 3, 4]);
    }

    /// R-163 (ordre) : 10 000 tâches, permutation quelconque ; le compte
    /// est n − (plus longue sous-suite), même valeur dans les deux sens.
    #[test]
    fn r163_moved_tasks_large() {
        let n: u32 = 10_000;
        let before: Vec<u32> = (1..=n).collect();
        // Permutation déterministe : multiplication modulaire.
        let after: Vec<u32> = (0..n).map(|i| (i * 7_919) % n + 1).collect();
        let forward = moved(&before, &after).len();
        let backward = moved(&after, &before).len();
        assert_eq!(forward, backward);
        assert!(forward > 0 && forward < n as usize);
        // Un seul déplacement dans 10 000 : une tâche.
        let mut one = before.clone();
        let last = one.pop().unwrap();
        one.insert(0, last);
        assert_eq!(moved(&before, &one), vec![n]);
    }
}
