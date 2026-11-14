//! Tâches et liens (`TBkndTask`, `TBkndCons` ; R-284 à R-286, R-289,
//! R-290).

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::{
    Constraint, ConstraintKind, Duration, DurationUnit, Lag, LagValue, Link, Minutes, OutlineLevel, Percent,
    Priority, ProjectOptions, SchedulingMode, Task, TaskUid, TimeBasis,
};
use plannercraft_mspdi::codes::{self, DurationCode};

use crate::bytes::{le_i32, le_u8, le_u16, le_u32, utf16_z};
use crate::diag::{MppCode, Sink};
use crate::layout::{self, LINK_LEN, TaskLayout, task_var, task2};
use crate::tables::{FixedItem, VarValues};
use crate::time::{self, Stamp};
use crate::{Layout, RecordedTask};

/// Premier UID des enregistrements réservés (« `n 00 ff ff` », observé :
/// trois ou quatre en tête de chaque table, jamais une tâche).
const RESERVED_UID: u32 = 0xFFFF_0000;
/// Longueur d'un enregistrement court (ligne vide).
const SHORT_LEN: usize = 16;
/// Caractères gardés d'un nom.
const MAX_NAME: usize = 4_096;
/// Caractères gardés des notes.
const MAX_NOTES: usize = 65_536;

/// Tâches lues, dans l'ordre des ID.
#[derive(Debug, Default)]
pub struct Tasks {
    /// Récapitulative du projet (UID 0).
    pub summary: Option<Task>,
    /// Tâches, ID 1 d'abord.
    pub tasks: Vec<Task>,
    /// Dates enregistrées.
    pub recorded: BTreeMap<TaskUid, RecordedTask>,
}

/// Ligne lue avant mise en ordre.
struct Row<'a> {
    uid: u32,
    id: u32,
    pos: usize,
    meta: &'a [u8],
    /// `None` : ligne vide (enregistrement court).
    record: Option<&'a [u8]>,
}

/// Lecteur des champs d'un enregistrement : champ absent = diagnostic.
struct Fields<'a, 'b> {
    rec: &'a [u8],
    uid: u32,
    sink: &'b mut Sink,
}

impl Fields<'_, '_> {
    fn missing<T>(&mut self, what: &str) -> Option<T> {
        self.sink
            .push(MppCode::FieldMissing, Some(self.uid), Some(what.to_owned()));
        None
    }
    fn u16(&mut self, off: usize, what: &str) -> Option<u16> {
        le_u16(self.rec, off).or_else(|| self.missing(what))
    }
    fn i32(&mut self, off: usize, what: &str) -> Option<i32> {
        le_i32(self.rec, off).or_else(|| self.missing(what))
    }
    fn stamp(&mut self, off: usize, what: &str) -> Stamp {
        let Some(raw) = le_u32(self.rec, off) else {
            self.missing::<()>(what);
            return Stamp::Na;
        };
        let s = time::stamp(raw);
        if s == Stamp::Invalid {
            self.sink.push(
                MppCode::DateInvalid,
                Some(self.uid),
                Some(format!("{what} {raw:08x}")),
            );
        }
        s
    }
}

fn moment(s: Stamp) -> Option<plannercraft_core::Moment> {
    match s {
        Stamp::At(m) => Some(m),
        Stamp::Na | Stamp::Invalid => None,
    }
}

/// Durée depuis des dixièmes de minute et un code de format (R-285) ; le
/// format nul (21, 53) prend l'unité par défaut du projet.
#[must_use]
pub fn duration(tenths: i32, format: u16, default_unit: DurationUnit, uid: u32, sink: &mut Sink) -> Duration {
    let (unit, basis, estimated) = match codes::duration_format(i64::from(format)) {
        Some(DurationCode::Time {
            unit,
            basis,
            estimated,
        }) => (unit, basis, estimated),
        Some(DurationCode::Null { estimated }) => (default_unit, TimeBasis::Working, estimated),
        Some(DurationCode::Percent { .. }) | None => {
            sink.push(MppCode::FormatUnknown, Some(uid), Some(format.to_string()));
            (default_unit, TimeBasis::Working, false)
        }
    };
    let minutes = time::minutes_from_tenths(i64::from(tenths)).unwrap_or(Minutes::ZERO);
    Duration::new(minutes, unit, basis, estimated).unwrap_or_else(|_| {
        sink.push(MppCode::DurationInvalid, Some(uid), Some(tenths.to_string()));
        Duration::zero(unit)
    })
}

/// Durée nulle de même unité (récapitulative automatique : sa durée est
/// une valeur calculée, ADR-0004).
fn computed_zero(d: Duration) -> Duration {
    Duration::new(Minutes::ZERO, d.unit(), d.basis(), d.is_estimated()).unwrap_or(Duration::zero(d.unit()))
}

/// Lignes du tableau, sans les enregistrements réservés ni supprimés ;
/// UID en double : le premier gagne.
fn rows<'a>(items: &[FixedItem<'a>], tl: &TaskLayout, sink: &mut Sink) -> Vec<Row<'a>> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (pos, item) in items.iter().enumerate() {
        if item.deleted() {
            continue;
        }
        let row = if item.short() {
            // Enregistrement court : UID à l'octet 0, ID à l'octet 4 (les
            // deux dispositions).
            let Some(rec) = item.data.get(..SHORT_LEN) else {
                continue;
            };
            let (Some(uid), Some(id)) = (le_u32(rec, 0), le_u32(rec, 4)) else {
                continue;
            };
            if uid >= RESERVED_UID {
                continue;
            }
            Row {
                uid,
                id,
                pos,
                meta: item.meta,
                record: None,
            }
        } else {
            let rec = item.data.get(..tl.len).unwrap_or(item.data);
            let (Some(uid), Some(id)) = (le_u32(rec, tl.uid), le_u32(rec, tl.id)) else {
                continue;
            };
            if uid >= RESERVED_UID {
                continue;
            }
            Row {
                uid,
                id,
                pos,
                meta: item.meta,
                record: Some(rec),
            }
        };
        if !seen.insert(row.uid) {
            sink.push(MppCode::DuplicateUid, Some(row.uid), None);
            continue;
        }
        out.push(row);
    }
    out
}

/// Champs de l'enregistrement fixe d'une tâche (R-284, R-285, R-289) ;
/// rend les dates enregistrées.
fn fill_fixed(
    task: &mut Task,
    f: &mut Fields<'_, '_>,
    tl: &TaskLayout,
    options: &ProjectOptions,
    calendars: &BTreeSet<u32>,
) -> RecordedTask {
    // Niveau.
    if let Some(level) = f.u16(tl.outline_level, "outline_level") {
        match OutlineLevel::new(level.max(1)) {
            Ok(l) if f.uid != 0 => task.outline_level = l,
            Ok(_) => {}
            Err(_) => f.sink.push(
                MppCode::ValueInvalid,
                Some(f.uid),
                Some(format!("niveau {level}")),
            ),
        }
    }
    // Durée.
    let tenths = f.i32(tl.duration, "duration").unwrap_or(0);
    let format = f.u16(tl.duration_format, "duration_format").unwrap_or(21);
    task.duration = duration(tenths, format, options.default_duration_unit, f.uid, f.sink);
    // Dates enregistrées (comparées, jamais recopiées : règle 3).
    let start = moment(f.stamp(tl.start, "start"));
    let finish = moment(f.stamp(tl.finish, "finish"));
    // Contrainte.
    let kind_code = f.u16(tl.constraint_type, "constraint_type").unwrap_or(0);
    let date = moment(f.stamp(tl.constraint_date, "constraint_date"));
    let kind = codes::constraint_type(i64::from(kind_code)).unwrap_or_else(|| {
        f.sink.push(
            MppCode::CodeUnknown,
            Some(f.uid),
            Some(format!("contrainte {kind_code}")),
        );
        ConstraintKind::AsSoonAsPossible
    });
    let constraint = if kind.is_dated() {
        if date.is_none() {
            f.sink.push(MppCode::ConstraintWithoutDate, Some(f.uid), None);
        }
        date.and_then(|d| Constraint::from_parts(kind, Some(d)))
    } else {
        Constraint::from_parts(kind, None)
    };
    if let Some(c) = constraint {
        task.constraint = c;
    }
    // Priorité, % achevé, type, calendrier, création.
    if let Some(p) = f.u16(tl.priority, "priority") {
        match Priority::new(p) {
            Ok(p) => task.priority = p,
            Err(_) => f
                .sink
                .push(MppCode::ValueInvalid, Some(f.uid), Some(format!("priorité {p}"))),
        }
    }
    if let Some(pc) = f.u16(tl.percent_complete, "percent_complete") {
        match Percent::new(i64::from(pc).saturating_mul(100)) {
            Ok(p) => task.progress.percent_complete = p,
            Err(_) => f
                .sink
                .push(MppCode::ValueInvalid, Some(f.uid), Some(format!("% achevé {pc}"))),
        }
    }
    if let Some(t) = f.u16(tl.task_type, "task_type") {
        match codes::task_type(i64::from(t)) {
            Some(t) => task.task_type = t,
            None => f
                .sink
                .push(MppCode::CodeUnknown, Some(f.uid), Some(format!("type {t}"))),
        }
    }
    if let Some(c) = f.i32(tl.calendar, "calendar")
        && c >= 0
    {
        match u32::try_from(c).ok().filter(|c| calendars.contains(c)) {
            Some(c) => task.calendar = Some(plannercraft_core::CalendarUid::new(c)),
            None => f
                .sink
                .push(MppCode::TaskCalendarUnknown, Some(f.uid), Some(c.to_string())),
        }
    }
    task.created = moment(f.stamp(tl.created, "created"));
    task.deadline = moment(f.stamp(tl.deadline, "deadline"));
    RecordedTask { start, finish }
}

/// Mode manuel (R-290) : début, fin et durée saisis, lus dans
/// l'enregistrement `Fixed2Data` (64 octets) ; durée dans l'unité de la
/// durée de la tâche.
fn manual_mode(rec2: &[u8], task: &Task, uid: u32, sink: &mut Sink) -> SchedulingMode {
    let at = |off: usize| le_u32(rec2, off).map(time::stamp).and_then(moment);
    let duration = le_i32(rec2, task2::MANUAL_DURATION).and_then(|t| {
        let minutes = time::minutes_from_tenths(i64::from(t))?;
        let d = task.duration;
        Duration::new(minutes, d.unit(), d.basis(), d.is_estimated())
            .map_err(|_| sink.push(MppCode::DurationInvalid, Some(uid), Some(t.to_string())))
            .ok()
    });
    SchedulingMode::Manual {
        start: at(task2::MANUAL_START),
        finish: at(task2::MANUAL_FINISH),
        duration,
    }
}

/// Lit les tâches (R-284, R-285, R-289) ; liens posés ensuite par
/// [`attach_links`].
#[must_use]
pub fn read(
    items: &[FixedItem<'_>],
    items2: &[FixedItem<'_>],
    var: &VarValues,
    lay: Layout,
    options: &ProjectOptions,
    calendars: &BTreeSet<u32>,
    sink: &mut Sink,
) -> Tasks {
    let tl = layout::task(lay);
    // `Fixed2` n'est lu que s'il a autant d'éléments que `Fixed` (observé
    // sur 164 fichiers sur 164).
    let second = (items2.len() == items.len()).then_some(items2);
    let mut rows = rows(items, &tl, sink);
    // Ordre des ID enregistrés (à égalité : ordre des enregistrements).
    rows.sort_by_key(|r| (r.id, r.pos));
    let mut out = Tasks::default();
    let mut expected = 1_u32;
    let mut renumbered = 0_u32;
    for row in &rows {
        let uid = TaskUid::new(row.uid);
        let fields = var.get(&row.uid);
        let name = fields
            .and_then(|f| f.get(&task_var::NAME))
            .map(|b| utf16_z(b, MAX_NAME))
            .unwrap_or_default();
        let mut task = Task::new(uid, name, options);
        if row.uid != 0 {
            if row.id != expected {
                renumbered = renumbered.saturating_add(1);
            }
            expected = expected.saturating_add(1);
        }
        let Some(rec) = row.record else {
            task.is_null = true;
            task.name = String::new();
            out.tasks.push(task);
            continue;
        };
        let mut f = Fields {
            rec,
            uid: row.uid,
            sink,
        };
        let rec_dates = fill_fixed(&mut task, &mut f, &tl, options, calendars);
        out.recorded.insert(uid, rec_dates);
        // Jalon : bit de l'élément de méta.
        let (byte, mask) = tl.milestone;
        task.milestone = le_u8(row.meta, byte).is_some_and(|b| b & mask != 0);
        // Mode et activité : bits de l'élément de `Fixed2Meta` de même rang
        // (R-290) ; dates saisies d'une tâche manuelle dans `Fixed2Data`.
        if let Some(second) = second.and_then(|s| s.get(row.pos)) {
            let bit = |(byte, mask): (usize, u8)| le_u8(second.meta, byte).is_some_and(|b| b & mask != 0);
            task.active = bit(tl.active);
            if bit(tl.manual) {
                task.mode = manual_mode(second.data, &task, row.uid, sink);
            }
        }
        // Notes (valeur variable).
        if let Some(raw) = fields.and_then(|v| v.get(&task_var::NOTES)) {
            let (text, truncated) = crate::rtf::to_text(raw, MAX_NOTES);
            if truncated {
                sink.push(MppCode::NotesTruncated, Some(row.uid), None);
            }
            task.notes = text;
        }
        if row.uid == 0 {
            task.duration = computed_zero(task.duration);
            out.summary = Some(task);
        } else {
            out.tasks.push(task);
        }
    }
    if renumbered > 0 {
        sink.push(MppCode::IdRenumbered, None, Some(renumbered.to_string()));
    }
    if second.is_none() && !out.tasks.is_empty() {
        sink.push(MppCode::ModeNotRead, None, None);
    }
    // Récapitulatives (R-052) : la tâche non vide suivante est plus
    // profonde ; leur durée est calculée (ADR-0004).
    let mut next_level: Option<u16> = None;
    for task in out.tasks.iter_mut().rev() {
        if task.is_null {
            continue;
        }
        let level = task.outline_level.get();
        if next_level.is_some_and(|n| n > level) {
            task.duration = computed_zero(task.duration);
        }
        next_level = Some(level);
    }
    out
}

/// Lit les liens et les pose sur leur successeur (R-286). Lien vers une
/// tâche absente ou vide, vers elle-même, ou en double : ignoré.
pub fn attach_links(
    items: &[FixedItem<'_>],
    lay: Layout,
    tasks: &mut Tasks,
    default_unit: DurationUnit,
    sink: &mut Sink,
) {
    let ll = layout::link(lay);
    let index: BTreeMap<u32, usize> = tasks
        .tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.is_null)
        .map(|(i, t)| (t.uid().get(), i))
        .collect();
    let mut seen = BTreeSet::new();
    for item in items {
        if item.deleted() || item.short() {
            continue;
        }
        let Some(rec) = item.data.get(..LINK_LEN) else {
            continue;
        };
        let (Some(pred), Some(succ), Some(kind), Some(lag), Some(format)) = (
            le_u32(rec, ll.predecessor),
            le_u32(rec, ll.successor),
            le_u16(rec, ll.kind),
            le_i32(rec, ll.lag),
            le_u16(rec, ll.lag_format),
        ) else {
            continue;
        };
        let (Some(_), Some(&si)) = (index.get(&pred), index.get(&succ)) else {
            sink.push(MppCode::LinkIgnored, Some(succ), Some(format!("{pred} → {succ}")));
            continue;
        };
        if pred == succ {
            sink.push(MppCode::LinkIgnored, Some(succ), Some(format!("{pred} → {succ}")));
            continue;
        }
        if !seen.insert((pred, succ)) {
            sink.push(
                MppCode::LinkDuplicate,
                Some(succ),
                Some(format!("{pred} → {succ}")),
            );
            continue;
        }
        let Some(kind) = codes::link_type(i64::from(kind)) else {
            sink.push(MppCode::CodeUnknown, Some(succ), Some(format!("lien {kind}")));
            continue;
        };
        let lag = match codes::duration_format(i64::from(format)) {
            Some(DurationCode::Percent { .. }) => {
                sink.push(MppCode::LagPercentUnread, Some(succ), Some(lag.to_string()));
                Lag::ZERO
            }
            code => {
                let (unit, basis) = match code {
                    Some(DurationCode::Time { unit, basis, .. }) => (unit, basis),
                    Some(DurationCode::Null { .. }) => (default_unit, TimeBasis::Working),
                    _ => {
                        sink.push(MppCode::FormatUnknown, Some(succ), Some(format.to_string()));
                        (default_unit, TimeBasis::Working)
                    }
                };
                let minutes = time::minutes_from_tenths(i64::from(lag)).unwrap_or(Minutes::ZERO);
                Lag::new(basis, LagValue::Time { minutes, unit }).unwrap_or_else(|_| {
                    sink.push(MppCode::DurationInvalid, Some(succ), Some(lag.to_string()));
                    Lag::ZERO
                })
            }
        };
        if let Some(t) = tasks.tasks.get_mut(si) {
            t.predecessors.push(Link {
                predecessor: TaskUid::new(pred),
                kind,
                lag,
            });
        }
    }
}
