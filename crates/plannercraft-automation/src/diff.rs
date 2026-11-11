//! Diff de planning entre deux états (R-163, lot L2) ; diff prêt à
//! afficher du panneau d'opérations IA ([`DiffView`], J8 lot L12).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, btree_map};
use std::iter::Peekable;

use plannercraft_core::calendar::CompiledCalendar;
use plannercraft_core::{
    Assignment, DurationUnit, Minutes, Moment, Project, ResourceLoad, ResourceUid, Schedule, TaskId,
    TaskOutcome, TaskSchedule, TaskUid,
};
use serde::Serialize;

use crate::delta::Delta;
pub use crate::field_change::{FieldChange, FieldKind, FieldValue};
use crate::field_change::{Side, Writer, assignment_tasks, task_changes};
use crate::render_text::Labels;
use crate::session::Simulation;
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
    /// Tâche touchée (R-163 amendée) sans date ni criticité changées :
    /// données seules (avancement, nom, ordre…, H-UI-189).
    DataOnly,
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
    /// Début planifié (avant, après) ; ligne [`RowChange::DataOnly`] :
    /// valeur d'après des deux côtés.
    pub start: (Option<Moment>, Option<Moment>),
    /// Fin planifiée (avant, après).
    pub finish: (Option<Moment>, Option<Moment>),
    /// Marge totale (avant, après).
    pub total_slack: (Option<Minutes>, Option<Minutes>),
    /// Criticité entrée ou sortie.
    pub critical: CriticalChange,
    /// Modifications directes de la tâche (H-UI-189), dans l'ordre de
    /// [`FieldKind`] ; vide pour une tâche ajoutée ou supprimée et pour une
    /// tâche que seul le recalcul a changée.
    pub changes: Vec<FieldChange>,
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
/// l'ordre de R-163, puis tâches dont seule la criticité change, par UID,
/// puis les autres tâches touchées, par ID d'après puis UID : H-UI-189),
/// le compte des autres (`more`), l'écart de fin du projet en temps ouvré
/// et le changement du chemin critique.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffView {
    /// Lignes, au plus [`MAX_VIEW_ROWS`].
    pub rows: Vec<DiffRow>,
    /// Tâches changées ou touchées non affichées.
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
    /// Vrai si une ligne affichée a des dates déplacées (tâche aux dates
    /// changées, ajoutée ou supprimée) : l'interface place alors la colonne
    /// « Modification » en dernier, sur une ligne, pour ne pas masquer les
    /// dates avant/après (H-UI-189, ADR-0028 D2). Calculé une fois avec le
    /// diff.
    pub dates_moved: bool,
}

/// Affectations par tâche, dans l'ordre du projet.
type ByTask<'a> = BTreeMap<TaskUid, Vec<&'a Assignment>>;

/// Lecture des modifications directes des lignes (H-UI-189) : delta par
/// entité du lot, tâches déplacées, affectations des tâches concernées.
struct RowChanges<'a> {
    before: DiffSide<'a>,
    after: DiffSide<'a>,
    entities: &'a Delta,
    moved: BTreeSet<TaskUid>,
    assignment_tasks: BTreeSet<TaskUid>,
    assignments: (ByTask<'a>, ByTask<'a>),
    writer: Writer,
}

impl<'a> RowChanges<'a> {
    fn new(before: DiffSide<'a>, after: DiffSide<'a>, entities: &'a Delta, text: &'a TextContext) -> Self {
        let assignment_tasks = assignment_tasks(&entities.assignments.changed);
        // Affectations des seules tâches dont une affectation a changé, dans
        // l'ordre du projet (celui de la colonne « Noms ressources »).
        let of = |p: &'a Project| {
            let mut m: ByTask<'a> = BTreeMap::new();
            if !assignment_tasks.is_empty() {
                for a in &p.assignments {
                    if assignment_tasks.contains(&a.task) {
                        m.entry(a.task).or_default().push(a);
                    }
                }
            }
            m
        };
        let assignments = (of(before.project), of(after.project));
        Self {
            before,
            after,
            entities,
            moved: moved_tasks(entities.tasks.order.as_ref()),
            assignment_tasks,
            assignments,
            writer: Writer {
                labels: Labels(text.language),
            },
        }
    }

    /// Modifications directes de `uid` (vide pour une tâche absente d'un
    /// côté).
    fn of(&self, uid: TaskUid) -> Vec<FieldChange> {
        let (Some(b), Some(a)) = (
            self.before.project.tasks.get(uid),
            self.after.project.tasks.get(uid),
        ) else {
            return Vec::new();
        };
        let moved = self.moved.contains(&uid).then(|| {
            let id = |p: &Project| p.task_id(uid).map_or(0, TaskId::get);
            (id(self.before.project), id(self.after.project))
        });
        let assignments_changed = self.assignment_tasks.contains(&uid);
        let in_delta = self
            .entities
            .tasks
            .changed
            .binary_search_by_key(&uid, |c| c.0)
            .is_ok();
        if !in_delta && moved.is_none() && !assignments_changed {
            return Vec::new();
        }
        let sides = (
            Side {
                project: self.before.project,
                assignments: self.assignments.0.get(&uid).map_or(&[][..], Vec::as_slice),
            },
            Side {
                project: self.after.project,
                assignments: self.assignments.1.get(&uid).map_or(&[][..], Vec::as_slice),
            },
        );
        task_changes(b, a, sides, moved, assignments_changed, &self.writer)
    }
}

/// Vrai si les dates planifiées de `uid` diffèrent, ou si la tâche apparaît
/// ou disparaît du planning (définition de [`diff_schedules`]).
fn dates_changed(uid: TaskUid, before: &Schedule, after: &Schedule) -> bool {
    let (b, a) = (before.tasks.get(&uid), after.tasks.get(&uid));
    if b.is_some() != a.is_some() {
        return true;
    }
    let dates = |o: Option<&TaskOutcome>| o.and_then(scheduled).map(|s| (s.start, s.finish));
    dates(b) != dates(a)
}

impl DiffView {
    /// Diff affiché du lot simulé `sim` (le `dry_run`, son état après et
    /// son delta par entité), comparé à `before` (la session) ; textes dans
    /// le contexte `text`.
    #[must_use]
    #[allow(
        clippy::too_many_lines,
        reason = "trois familles de lignes à la suite, chacune bornée par la même limite"
    )]
    pub fn new(before: DiffSide<'_>, sim: &Simulation, text: &TextContext) -> Self {
        let d = &sim.outcome.diff;
        let after = DiffSide {
            project: &sim.project,
            schedule: &sim.schedule,
        };
        let changes = RowChanges::new(before, after, &sim.entities, text);
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
                changes: if change == RowChange::Changed {
                    changes.of(c.uid)
                } else {
                    Vec::new()
                },
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
        let flipped: BTreeSet<TaskUid> = d
            .critical_entered
            .iter()
            .chain(d.critical_left.iter())
            .copied()
            .collect();
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
                changes: changes.of(uid),
            });
            shown_flips = shown_flips.saturating_add(1);
        }
        // Troisième famille (H-UI-189) : tâches touchées (R-163 amendée)
        // sans date ni criticité changées, par ID d'après puis UID.
        let mut data: Vec<(Option<TaskId>, TaskUid)> =
            touched_tasks(before.project, after.project, &sim.entities, std::iter::empty())
                .into_iter()
                .filter(|u| !flipped.contains(u) && !dates_changed(*u, before.schedule, after.schedule))
                .map(|u| (after.project.task_id(u), u))
                .collect();
        data.sort_unstable();
        let data_total = data.len();
        let mut shown_data = 0usize;
        for (id, uid) in data {
            if rows.len() >= MAX_VIEW_ROWS {
                break;
            }
            // Valeurs courantes, sans valeur barrée : celles d'après.
            let a = after.schedule.scheduled(uid).copied();
            let (start, finish, slack) = (a.map(|s| s.start), a.map(|s| s.finish), a.map(|s| s.total_slack));
            rows.push(DiffRow {
                uid,
                id,
                name: name_of(uid),
                change: RowChange::DataOnly,
                start: (start, start),
                finish: (finish, finish),
                total_slack: (slack, slack),
                critical: CriticalChange::Same,
                changes: changes.of(uid),
            });
            shown_data = shown_data.saturating_add(1);
        }
        let shown_changed = d.changed.len().min(MAX_VIEW_ROWS);
        let more = d
            .changed_total
            .saturating_sub(shown_changed)
            .saturating_add(flips_total.saturating_sub(shown_flips))
            .saturating_add(data_total.saturating_sub(shown_data));
        let (f0, f1) = d.project_finish;
        let working = CompiledCalendar::compile(&before.project.calendars, before.project.calendar)
            .ok()
            .and_then(|c| c.working_between(f0, f1).ok());
        let finish_delta_text = working
            .and_then(|m| crate::view::signed_minutes(m, DurationUnit::Day, text).text)
            .unwrap_or_default();
        let dates_moved = rows.iter().any(|r| {
            matches!(
                r.change,
                RowChange::Changed | RowChange::Added | RowChange::Removed
            )
        });
        Self {
            rows,
            more,
            dates_moved,
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

    use super::{
        CriticalChange, DiffSide, DiffView, FieldKind, FieldValue, MAX_VIEW_ROWS, RowChange, moved_tasks,
    };
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
        DiffView::new(before, &sim, &TextContext::for_project(s.project(), Language::Fr))
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

    fn kinds(v: &DiffView, name: &str) -> Vec<FieldKind> {
        v.rows
            .iter()
            .find(|r| r.name == name)
            .unwrap()
            .changes
            .iter()
            .map(|c| c.field)
            .collect()
    }

    /// H-UI-189 : cinq tâches passées à 100 % sans date déplacée ⇒ cinq
    /// lignes (achevées, elles quittent le chemin critique : lignes de
    /// criticité, par UID), chacune avec son « % achevé ».
    #[test]
    fn h_ui_189_progress_only_rows() {
        let ops: Vec<Value> = (1..=6)
            .map(|i| json!({ "op": "task.add", "name": format!("T{i}"), "duration": { "text": "2j" } }))
            .collect();
        let s = session(&Value::Array(ops));
        let ops: Vec<Value> = (1..=5)
            .map(
                |uid| json!({ "op": "progress.update", "task": uid, "percent_complete": { "text": "100%" } }),
            )
            .collect();
        let v = view(&s, &Value::Array(ops));
        assert_eq!(v.rows.len(), 5);
        assert_eq!(v.more, 0);
        assert!(!v.dates_moved);
        let ids: Vec<u32> = v.rows.iter().map(|r| r.id.unwrap().get()).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
        for r in &v.rows {
            assert_eq!(r.change, RowChange::CriticalOnly);
            assert_eq!(r.critical, CriticalChange::Left);
            assert_eq!(r.start.0, r.start.1);
            let pct = r
                .changes
                .iter()
                .find(|c| c.field == FieldKind::PercentComplete)
                .unwrap();
            assert_eq!(pct.before, FieldValue::Text("0\u{a0}%".to_owned()));
            assert_eq!(pct.after, FieldValue::Text("100\u{a0}%".to_owned()));
        }
    }

    /// H-UI-189 : lot mêlé. A passe à 7 j (A et B, son successeur, aux
    /// dates déplacées : famille 1 ; B sans modification directe), C est
    /// renommée et E passe en tête (famille 3, « Nom » et « N° »).
    #[test]
    fn h_ui_189_mixed_rows() {
        let s = session(&json!([
            { "op": "task.add", "name": "A", "as": "$a", "duration": { "text": "5j" } },
            { "op": "task.add", "name": "B", "as": "$b", "duration": { "text": "1j" } },
            { "op": "link.add", "pred": "$a", "succ": "$b" },
            { "op": "task.add", "name": "C", "duration": { "text": "1j" } },
            { "op": "task.add", "name": "D", "duration": { "text": "1j" } },
            { "op": "task.add", "name": "E", "duration": { "text": "1j" } }
        ]));
        let v = view(
            &s,
            &json!([
                { "op": "task.update", "uid": 1, "duration": { "text": "7j" } },
                { "op": "task.update", "uid": 3, "name": "C bis" },
                { "op": "task.move", "uid": 5, "to": { "before": 1 } }
            ]),
        );
        assert!(v.dates_moved);
        let changes: Vec<(String, RowChange)> = v.rows.iter().map(|r| (r.name.clone(), r.change)).collect();
        assert_eq!(
            changes,
            vec![
                ("A".to_owned(), RowChange::Changed),
                ("B".to_owned(), RowChange::Changed),
                ("E".to_owned(), RowChange::DataOnly),
                ("C bis".to_owned(), RowChange::DataOnly),
            ]
        );
        assert_eq!(kinds(&v, "A"), vec![FieldKind::Duration]);
        assert_eq!(kinds(&v, "B"), Vec::<FieldKind>::new(), "conséquence du recalcul");
        assert_eq!(kinds(&v, "C bis"), vec![FieldKind::Name]);
        let e = v.rows.iter().find(|r| r.name == "E").unwrap();
        assert_eq!(e.changes.len(), 1);
        assert_eq!(e.changes[0].field, FieldKind::Position);
        assert_eq!(e.changes[0].before, FieldValue::Text("5".to_owned()));
        assert_eq!(e.changes[0].after, FieldValue::Text("1".to_owned()));
        let a = v.rows.iter().find(|r| r.name == "A").unwrap();
        assert_eq!(a.changes[0].before, FieldValue::Text("5 jours".to_owned()));
        assert_eq!(a.changes[0].after, FieldValue::Text("7 jours".to_owned()));
    }

    /// H-UI-189 : ressource affectée ⇒ « Noms ressources » ; ajoutée ⇒
    /// ligne sans liste.
    #[test]
    fn h_ui_189_resources_and_added() {
        let s = session(&json!([
            { "op": "task.add", "name": "A", "duration": { "text": "2j" } },
            { "op": "resource.add", "name": "Maçon" }
        ]));
        let v = view(
            &s,
            &json!([
                { "op": "assignment.add", "task": 1, "resource": 1 },
                { "op": "task.add", "name": "N", "duration": { "text": "1j" } }
            ]),
        );
        let n = v.rows.iter().find(|r| r.name == "N").unwrap();
        assert_eq!(n.change, RowChange::Added);
        assert_eq!(n.changes.len(), 0);
        let a = v.rows.iter().find(|r| r.name == "A").unwrap();
        let res = a
            .changes
            .iter()
            .find(|c| c.field == FieldKind::Resources)
            .unwrap();
        assert_eq!(res.before, FieldValue::Text(String::new()));
        assert_eq!(res.after, FieldValue::Text("Maçon".to_owned()));
    }

    /// H-UI-189 : la limite de 200 lignes couvre la troisième famille.
    #[test]
    fn h_ui_189_data_rows_capped() {
        let ops: Vec<Value> = (0..250)
            .map(|i| json!({ "op": "task.add", "name": format!("T{i}"), "duration": { "text": "1j" } }))
            .collect();
        let s = session(&Value::Array(ops));
        let ops: Vec<Value> = (1..=250)
            .map(|uid| json!({ "op": "task.update", "uid": uid, "name": format!("R{uid}") }))
            .collect();
        let v = view(&s, &Value::Array(ops));
        assert_eq!(v.rows.len(), MAX_VIEW_ROWS);
        assert_eq!(v.more, 50);
        assert!(v.rows.iter().all(|r| r.change == RowChange::DataOnly));
    }
}
