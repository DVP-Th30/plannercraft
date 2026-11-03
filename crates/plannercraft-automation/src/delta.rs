//! Delta par entité entre deux projets : pré- et post-images des seules
//! entités changées (ADR-0012 D3, lot L2, R-164).
//!
//! Un lot s'applique à un clone du projet ; au commit, la comparaison du
//! projet d'avant et du clone par UID donne le delta gardé dans
//! l'historique. Annuler applique les pré-images, rétablir les
//! post-images : l'état obtenu est exactement l'un des deux projets
//! comparés (P30), par construction et non par instrumentation des
//! commandes.

use std::collections::BTreeMap;
use std::io;

use plannercraft_core::calendar::Calendar;
use plannercraft_core::{
    Arena, Assignment, AssignmentUid, CalendarUid, CustomFieldDef, CustomFieldId, EntityUid, Extras, HasUid,
    ModelError, Moment, Project, ProjectInfo, ProjectOptions, Resource, ResourceUid, ScheduleAnchor, Task,
    TaskUid,
};
use serde::Serialize;

/// Delta d'un lot commité.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Delta {
    pub tasks: ArenaDelta<TaskUid, Task>,
    pub resources: ArenaDelta<ResourceUid, Resource>,
    pub assignments: ArenaDelta<AssignmentUid, Assignment>,
    pub calendars: ArenaDelta<CalendarUid, Calendar>,
    /// Blocs du projet hors arènes (avant, après), s'ils ont changé.
    pub header: Option<Box<(ProjectHeader, ProjectHeader)>>,
}

/// Delta d'une arène : `(uid, avant, après)` des entités changées, triées
/// par UID, et l'ordre des UID (avant, après) s'il a changé.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArenaDelta<K, T> {
    pub changed: Vec<(K, Option<T>, Option<T>)>,
    pub order: Option<(Vec<K>, Vec<K>)>,
}

impl<K, T> Default for ArenaDelta<K, T> {
    fn default() -> Self {
        Self {
            changed: Vec::new(),
            order: None,
        }
    }
}

#[cfg(test)]
impl<K, T> ArenaDelta<K, T> {
    fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.order.is_none()
    }
}

/// Champs du projet hors arènes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ProjectHeader {
    pub info: ProjectInfo,
    pub anchor: ScheduleAnchor,
    pub status_date: Option<Moment>,
    pub current_date: Moment,
    pub calendar: CalendarUid,
    pub options: ProjectOptions,
    pub summary_task: Option<Task>,
    /// Définitions des champs personnalisés (R-264 : `custom_field.rename`
    /// s'annule ; absentes de la taille mesurée quand il n'y en a pas).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_fields: BTreeMap<CustomFieldId, CustomFieldDef>,
    pub extras: Extras,
}

impl ProjectHeader {
    fn of(p: &Project) -> Self {
        Self {
            info: p.info.clone(),
            anchor: p.anchor,
            status_date: p.status_date,
            current_date: p.current_date,
            calendar: p.calendar,
            options: p.options,
            summary_task: p.summary_task.clone(),
            custom_fields: p.custom_fields.clone(),
            extras: p.extras.clone(),
        }
    }

    /// Égalité avec les champs de `p`, sans copie.
    fn matches(&self, p: &Project) -> bool {
        self.info == p.info
            && self.anchor == p.anchor
            && self.status_date == p.status_date
            && self.current_date == p.current_date
            && self.calendar == p.calendar
            && self.options == p.options
            && self.summary_task == p.summary_task
            && self.custom_fields == p.custom_fields
            && self.extras == p.extras
    }

    fn write_to(&self, p: &mut Project) {
        p.info.clone_from(&self.info);
        p.anchor = self.anchor;
        p.status_date = self.status_date;
        p.current_date = self.current_date;
        p.calendar = self.calendar;
        p.options = self.options;
        p.summary_task.clone_from(&self.summary_task);
        p.custom_fields.clone_from(&self.custom_fields);
        p.extras.clone_from(&self.extras);
    }
}

/// Sens d'application d'un delta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dir {
    /// Pré-images.
    Undo,
    /// Post-images.
    Redo,
}

#[cfg(test)]
impl Delta {
    /// Vrai si les deux projets comparés étaient égaux.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
            && self.resources.is_empty()
            && self.assignments.is_empty()
            && self.calendars.is_empty()
            && self.header.is_none()
    }
}

/// Delta entre deux projets, par UID : O(n log n), comparaison `PartialEq`
/// entité par entité.
pub(crate) fn diff_projects(before: &Project, after: &Project) -> Delta {
    let a = ProjectHeader::of(before);
    let header = if a.matches(after) {
        None
    } else {
        Some(Box::new((a, ProjectHeader::of(after))))
    };
    Delta {
        tasks: diff_arena(&before.tasks, &after.tasks),
        resources: diff_arena(&before.resources, &after.resources),
        assignments: diff_arena(&before.assignments, &after.assignments),
        calendars: diff_arena(&before.calendars, &after.calendars),
        header,
    }
}

fn diff_arena<K, T>(before: &Arena<K, T>, after: &Arena<K, T>) -> ArenaDelta<K, T>
where
    K: EntityUid,
    T: HasUid<Uid = K> + Clone + PartialEq,
{
    let mut changed = Vec::new();
    for b in before {
        let uid = b.uid();
        match after.get(uid) {
            None => changed.push((uid, Some(b.clone()), None)),
            Some(a) if a != b => changed.push((uid, Some(b.clone()), Some(a.clone()))),
            Some(_) => {}
        }
    }
    for a in after {
        let uid = a.uid();
        if !before.contains(uid) {
            changed.push((uid, None, Some(a.clone())));
        }
    }
    changed.sort_by_key(|c| c.0);
    let same_order =
        before.len() == after.len() && before.iter().zip(after.iter()).all(|(b, a)| b.uid() == a.uid());
    let order = if same_order {
        None
    } else {
        Some((
            before.iter().map(HasUid::uid).collect(),
            after.iter().map(HasUid::uid).collect(),
        ))
    };
    ArenaDelta { changed, order }
}

/// Applique un delta : pré-images (`Undo`) ou post-images (`Redo`).
///
/// # Errors
///
/// Un delta incohérent avec `p` (UID absent de l'ordre visé, entité
/// manquante, UID en double) : [`ModelError`]. `p` est alors dans un état
/// quelconque ; l'appelant applique le delta à une copie.
pub(crate) fn apply_delta(p: &mut Project, d: &Delta, dir: Dir) -> Result<(), ModelError> {
    apply_arena(&mut p.tasks, &d.tasks, dir)?;
    apply_arena(&mut p.resources, &d.resources, dir)?;
    apply_arena(&mut p.assignments, &d.assignments, dir)?;
    apply_arena(&mut p.calendars, &d.calendars, dir)?;
    if let Some(h) = &d.header {
        let (before, after) = &**h;
        match dir {
            Dir::Undo => before.write_to(p),
            Dir::Redo => after.write_to(p),
        }
    }
    Ok(())
}

fn apply_arena<K, T>(arena: &mut Arena<K, T>, d: &ArenaDelta<K, T>, dir: Dir) -> Result<(), ModelError>
where
    K: EntityUid,
    T: HasUid<Uid = K> + Clone,
{
    let pick = |c: &(K, Option<T>, Option<T>)| -> Option<T> {
        match dir {
            Dir::Undo => c.1.clone(),
            Dir::Redo => c.2.clone(),
        }
    };
    let in_place = d.order.is_none() && d.changed.iter().all(|c| c.1.is_some() && c.2.is_some());
    if in_place {
        // Modifications seules : remplacement sur place, sans reconstruire
        // l'index (cas courant : renommer, changer une durée).
        for c in &d.changed {
            let slot = arena.get_mut(c.0).ok_or_else(|| missing(c.0))?;
            if let Some(image) = pick(c) {
                *slot = image;
            }
        }
        return Ok(());
    }
    // Ajouts, retraits ou réordonnancement : reconstruction en une passe,
    // O(n log n).
    let order: Vec<K> = match &d.order {
        Some((before, after)) => match dir {
            Dir::Undo => before.clone(),
            Dir::Redo => after.clone(),
        },
        None => arena.iter().map(HasUid::uid).collect(),
    };
    let items: Vec<T> = std::mem::take(arena).into();
    let mut by_uid: BTreeMap<K, T> = items.into_iter().map(|t| (t.uid(), t)).collect();
    for c in &d.changed {
        match pick(c) {
            Some(image) => {
                by_uid.insert(c.0, image);
            }
            None => {
                by_uid.remove(&c.0);
            }
        }
    }
    let mut rebuilt = Vec::with_capacity(order.len());
    for uid in &order {
        rebuilt.push(by_uid.remove(uid).ok_or_else(|| missing(*uid))?);
    }
    if let Some((uid, _)) = by_uid.first_key_value() {
        // Entité présente hors de l'ordre visé : delta incohérent.
        return Err(ModelError::DuplicateUid {
            entity: K::ENTITY,
            uid: uid.raw(),
        });
    }
    *arena = Arena::try_from(rebuilt)?;
    Ok(())
}

/// Entité attendue par le delta mais absente : signalée comme une position
/// hors de la liste (pas de variante dédiée dans le modèle).
fn missing<K: EntityUid>(uid: K) -> ModelError {
    ModelError::PositionOutOfRange {
        position: usize::try_from(uid.raw()).unwrap_or(usize::MAX),
    }
}

/// Taille approchée d'un delta, en octets (budget R-164) : taille fixe des
/// entrées, longueur JSON des images (chaînes, listes et `Extras`
/// comprises), ordres des UID.
pub(crate) fn approx_bytes(d: &Delta) -> usize {
    let mut total = std::mem::size_of::<Delta>();
    total = total
        .saturating_add(arena_bytes(&d.tasks))
        .saturating_add(arena_bytes(&d.resources))
        .saturating_add(arena_bytes(&d.assignments))
        .saturating_add(arena_bytes(&d.calendars));
    if let Some(h) = &d.header {
        total = total
            .saturating_add(std::mem::size_of::<(ProjectHeader, ProjectHeader)>())
            .saturating_add(json_len(&h.0))
            .saturating_add(json_len(&h.1));
    }
    total
}

fn arena_bytes<K, T: Serialize>(d: &ArenaDelta<K, T>) -> usize {
    let mut total = 0usize;
    for c in &d.changed {
        total = total.saturating_add(std::mem::size_of::<(K, Option<T>, Option<T>)>());
        for image in [&c.1, &c.2].into_iter().flatten() {
            total = total.saturating_add(json_len(image));
        }
    }
    if let Some((before, after)) = &d.order {
        let n = before.len().saturating_add(after.len());
        total = total.saturating_add(n.saturating_mul(std::mem::size_of::<K>()));
    }
    total
}

/// Longueur de la forme JSON d'une valeur, sans l'allouer.
pub(crate) fn json_len<T: Serialize + ?Sized>(v: &T) -> usize {
    let mut counter = Counter(0);
    // Une erreur de sérialisation (jamais attendue sur le modèle) compte 0.
    let _ = serde_json::to_writer(&mut counter, v);
    counter.0
}

/// Compteur d'octets écrits.
struct Counter(usize);

impl io::Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 = self.0.saturating_add(buf.len());
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use plannercraft_core::{Moment, Project, ProjectInit, ScheduleAnchor, Task, TaskUid};

    use super::{Dir, apply_delta, approx_bytes, diff_projects};

    fn project(n: u32) -> Project {
        let start: Moment = "2026-11-02T08:00".parse().unwrap();
        let mut p = Project::new(ProjectInit {
            name: "P".to_owned(),
            anchor: ScheduleAnchor::FromStart(start),
            current_date: start,
            calendar_names: plannercraft_core::BuiltinCalendarNames::default(),
        });
        for u in 1..=n {
            let t = Task::new(TaskUid::new(u), format!("T{u}"), &p.options);
            p.tasks.push(t).unwrap();
        }
        p
    }

    fn roundtrip(before: &Project, after: &Project) {
        let d = diff_projects(before, after);
        let mut p = after.clone();
        apply_delta(&mut p, &d, Dir::Undo).unwrap();
        assert_eq!(&p, before);
        apply_delta(&mut p, &d, Dir::Redo).unwrap();
        assert_eq!(&p, after);
    }

    #[test]
    fn delta_equal_projects_is_empty() {
        let p = project(3);
        assert!(diff_projects(&p, &p.clone()).is_empty());
    }

    #[test]
    fn delta_rename_in_place() {
        let before = project(3);
        let mut after = before.clone();
        after.tasks.get_mut(TaskUid::new(2)).unwrap().name = "X".to_owned();
        let d = diff_projects(&before, &after);
        assert_eq!(d.tasks.changed.len(), 1);
        assert!(d.tasks.order.is_none());
        roundtrip(&before, &after);
    }

    #[test]
    fn delta_add_remove_reorder_header() {
        let before = project(4);
        let mut after = before.clone();
        let removed = after.tasks.remove(TaskUid::new(2)).unwrap();
        after.tasks.insert(0, removed).unwrap();
        after.tasks.remove(TaskUid::new(3)).unwrap();
        let t = Task::new(TaskUid::new(9), "N".to_owned(), &after.options);
        after.tasks.insert(1, t).unwrap();
        after.info.name = "Q".to_owned();
        after.status_date = Some("2026-11-05T08:00".parse().unwrap());
        roundtrip(&before, &after);
    }

    #[test]
    fn delta_inconsistent_refused() {
        let before = project(2);
        let mut after = before.clone();
        after.tasks.remove(TaskUid::new(1)).unwrap();
        let d = diff_projects(&before, &after);
        // Le delta appliqué au mauvais projet (ordre visé : UID 1 absent).
        let mut wrong = project(0);
        assert!(apply_delta(&mut wrong, &d, Dir::Undo).is_err());
    }

    #[test]
    fn delta_bytes_of_rename_small() {
        let before = project(200);
        let mut after = before.clone();
        after.tasks.get_mut(TaskUid::new(100)).unwrap().name = "Renamed".to_owned();
        let bytes = approx_bytes(&diff_projects(&before, &after));
        assert!(bytes < 4096, "{bytes}");
    }
}
