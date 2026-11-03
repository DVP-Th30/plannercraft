//! Presse-papiers de lignes (R-244, R-256 ; ADR-0016 D3).
//!
//! Tampon interne [`RowClip`] (tâches, liens internes, affectations par nom
//! de ressource) et texte tabulé des colonnes visibles pour le presse-papiers
//! du système ; au collage, une empreinte stable ([`fingerprint`], FNV-1a
//! 64) égale au dernier texte émis ⇒ collage complet ([`paste_batch`]),
//! sinon collage externe réduit ([`paste_text`]). Contrat posé par le lot
//! L0, implémenté en L3.
//!
//! Le collage rend un lot ordinaire (`task.add` étiquetés `$x`, R-150,
//! puis `link.add`, `resource.add`, `assignment.add`) : une entrée
//! d'annulation, `dry_run` possible ; rien n'est écrit ici. Couper = copier
//! puis `task.delete` dans un lot séparé (annulable seul), à la charge de
//! l'appelant.

use std::collections::{BTreeMap, BTreeSet};

use plannercraft_core::{
    Constraint, Decimal, Diagnostic, Duration, Exclusion, Lag, LagValue, LinkType, Moment, Priority, Project,
    ResourceKind, ResourceUid, Schedule, SchedulingMode, Task, TaskOutcome, TaskRole, TaskUid, TimeBasis,
    Units,
};

use crate::command::{
    AssignmentAdd, Batch, Command, ConstraintKindJson, DurationJson, LagJson, LinkSpec, ModeJson, Patch,
    Placement, ResourceAdd, ResourceRef, SetConstraint, SetDeadline, TaskAdd, TaskRef, TaskUpdate, UnitsJson,
};
use crate::csv::{self, CsvColumn, CsvEncoding, CsvError, CsvImportOptions, Recognized};
use crate::error::codes;
use crate::session::Session;
use crate::text::lex::format_decimal;
use crate::text::{Language, PredecessorRef, TextContext, format_date, format_duration, format_predecessor};

/// Lignes au plus dans un texte collé (`paste.too_large`) ; aussi le
/// nombre maximal de tâches d'un tampon interne.
pub const MAX_PASTE_LINES: usize = 10_000;
/// Octets au plus dans un texte collé (`paste.too_large`).
pub const MAX_PASTE_BYTES: usize = 4 << 20;

/// Longueur maximale d'un nom de tâche collé, en caractères (celle de
/// `task.add`).
const MAX_NAME_CHARS: usize = 512;

/// Champs copiés d'une tâche (ni avancement, ni réels, ni ligne de base, ni
/// UID d'origine : R-244).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipFields {
    /// Nom.
    pub name: String,
    /// Durée.
    pub duration: Duration,
    /// Planification manuelle.
    pub manual: bool,
    /// Valeurs manuelles (début, fin, durée).
    pub manual_values: Option<(Option<Moment>, Option<Moment>, Option<Duration>)>,
    /// Jalon.
    pub milestone: bool,
    /// Contrainte.
    pub constraint: Constraint,
    /// Échéance.
    pub deadline: Option<Moment>,
    /// Notes.
    pub notes: String,
    /// Active.
    pub active: bool,
    /// Priorité (copiée ; pas recollée tant que `task.update` ne la porte
    /// pas, R-244).
    pub priority: Priority,
    /// Calendrier de la tâche, par nom (copié ; pas recollé tant que
    /// `task.update` ne le porte pas, R-244).
    pub calendar: Option<String>,
}

/// Valeurs affichées d'une tâche copiée, pour le texte tabulé (H-UI-39) :
/// jamais recollées (ID, UID, dates et % de la source).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipShown {
    /// ID (position + 1) dans le projet source.
    pub id: u32,
    /// UID dans le projet source.
    pub uid: u32,
    /// Niveau hiérarchique dans le projet source.
    pub outline_level: u16,
    /// Code WBS.
    pub wbs: Option<String>,
    /// Durée affichée (récapitulative : celle du planning, R-054).
    pub duration: Option<Duration>,
    /// Début planifié (tâche inactive : placement d'affichage).
    pub start: Option<Moment>,
    /// Fin planifiée.
    pub finish: Option<Moment>,
    /// % achevé stocké, en centièmes.
    pub percent: i64,
    /// Prédécesseurs par ID.
    pub predecessors: Vec<PredecessorRef>,
    /// Prédécesseurs par UID.
    pub unique_predecessors: Vec<PredecessorRef>,
}

/// Tâche du tampon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipTask {
    /// Niveau relatif dans la forêt copiée : nombre d'ancêtres copiés
    /// (0 = racine du tampon, collée au niveau de la ligne d'insertion).
    pub rel_level: u16,
    /// Champs copiés.
    pub fields: ClipFields,
    /// Valeurs affichées (texte tabulé seulement).
    pub shown: ClipShown,
    /// La source porte un suivi que le collage ne recrée pas (R-244) :
    /// avancement ou réels (feuille), ligne de base, calendrier propre ou
    /// priorité autre que 500 (relecture finale de J8, F6).
    pub tracking_lost: bool,
}

/// Lien interne au tampon (indices dans `tasks`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipLink {
    /// Prédécesseur.
    pub from: u32,
    /// Successeur.
    pub to: u32,
    /// Type.
    pub kind: LinkType,
    /// Décalage.
    pub lag: Lag,
}

/// Affectation du tampon, par nom de ressource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipAssignment {
    /// Indice de la tâche dans `tasks`.
    pub task: u32,
    /// Nom de la ressource.
    pub resource: String,
    /// Unités (ressource de travail) ou quantité (ressource matérielle),
    /// en 1/10 000.
    pub units: Units,
}

/// Tampon interne de lignes copiées.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowClip {
    /// Tâches, dans l'ordre du projet.
    pub tasks: Vec<ClipTask>,
    /// Liens dont les deux bouts sont dans le tampon.
    pub links: Vec<ClipLink>,
    /// Affectations.
    pub assignments: Vec<ClipAssignment>,
    /// Liens écartés à la copie : un seul bout dans le tampon, ou lien
    /// qu'un `link.add` refuserait (sur soi, en double, entre une
    /// récapitulative et sa descendante).
    pub links_dropped: u32,
}

/// Traitement des ressources au collage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResourcePaste {
    /// Par nom ; nom absent ⇒ affectation omise et comptée.
    #[default]
    Match,
    /// Par nom ; nom absent ⇒ ressource de travail créée.
    MatchOrCreate,
    /// Aucune affectation collée (toutes comptées omises).
    Drop,
}

/// Options du collage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasteOptions {
    /// Recréer les liens internes.
    pub links: bool,
    /// Ressources.
    pub resources: ResourcePaste,
}

impl Default for PasteOptions {
    fn default() -> Self {
        Self {
            links: true,
            resources: ResourcePaste::Match,
        }
    }
}

/// Compte rendu d'un collage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PasteReport {
    /// Tâches créées.
    pub tasks: u32,
    /// Liens non recréés (`paste.links_dropped`).
    pub links_dropped: u32,
    /// Affectations omises.
    pub assignments_dropped: u32,
    /// Ressources créées.
    pub resources_created: u32,
    /// Tâches collées sans le suivi de leur source (`ClipTask::tracking_lost` :
    /// avancement, réels, ligne de base, calendrier ou priorité non
    /// recollés, R-244) ; l'interface l'affiche en avertissement.
    pub tracking_dropped: u32,
    /// Diagnostics (aucun en J8 : les comptes ci-dessus disent tout).
    pub diagnostics: Vec<Diagnostic>,
}

/// Refus d'une copie ou d'un collage.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ClipError {
    /// Tâche inconnue.
    #[error("tâche inconnue : {0:?}")]
    UnknownTask(TaskUid),
    /// Texte non reconnu ou rien à coller (`paste.unrecognized`).
    #[error("texte collé non reconnu")]
    Unrecognized,
    /// Texte ou tampon trop grand (`paste.too_large`).
    #[error("texte collé trop grand")]
    TooLarge,
    /// Livré par le lot L3 (plus rendu ; gardé pour la stabilité de l'API).
    #[error("presse-papiers pas encore disponible")]
    NotYetAvailable,
}

impl ClipError {
    /// Code stable de l'erreur.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownTask(_) => codes::TASK_NOT_FOUND,
            Self::Unrecognized => codes::PASTE_UNRECOGNIZED,
            Self::TooLarge => codes::PASTE_TOO_LARGE,
            Self::NotYetAvailable => codes::COMMAND_NOT_YET_AVAILABLE,
        }
    }
}

/// Entrée de la pile des ancêtres pendant la copie.
#[derive(Clone, Copy)]
struct Ancestor {
    level: u16,
    /// Profondeur dans la forêt copiée, si l'ancêtre est copié.
    depth: Option<u16>,
}

/// Durée affichée : celle du modèle pour une feuille ; pour une
/// récapitulative planifiée, celle du planning (R-054).
fn shown_duration(task: &Task, outcome: Option<&TaskOutcome>) -> Duration {
    if let Some(TaskOutcome::Scheduled(s)) = outcome
        && s.role == TaskRole::Summary
    {
        return Duration::new(s.duration, task.duration.unit(), TimeBasis::Working, false)
            .unwrap_or(task.duration);
    }
    task.duration
}

/// Dates affichées (comme l'export CSV, R-146) : planifiées, ou placement
/// d'affichage d'une tâche inactive.
fn shown_span(outcome: Option<&TaskOutcome>) -> Option<(Moment, Moment)> {
    match outcome {
        Some(TaskOutcome::Scheduled(s)) => Some((s.start, s.finish)),
        Some(TaskOutcome::Excluded {
            reason: Exclusion::Inactive,
            display: Some(d),
        }) => Some((d.start, d.finish)),
        _ => None,
    }
}

/// Quantité d'une affectation matérielle depuis son travail (`q × 60 min`,
/// O30) : `travail × 10 000 / 60`, arrondi au plus proche.
fn material_quantity(work_minutes: i64) -> Units {
    i64::try_from(
        i128::from(work_minutes)
            .saturating_mul(10_000)
            .saturating_add(30)
            .checked_div(60)
            .unwrap_or(0),
    )
    .ok()
    .and_then(|q| Units::new(q).ok())
    .unwrap_or(Units::FULL)
}

/// Champs copiés de `t`.
fn fields_of(project: &Project, t: &Task) -> ClipFields {
    let manual_values = match t.mode {
        SchedulingMode::Manual {
            start,
            finish,
            duration,
        } => Some((start, finish, duration)),
        SchedulingMode::Auto => None,
    };
    ClipFields {
        name: t.name.clone(),
        duration: t.duration,
        manual: manual_values.is_some(),
        manual_values,
        milestone: t.milestone,
        constraint: t.constraint,
        deadline: t.deadline,
        notes: t.notes.clone(),
        active: t.active,
        priority: t.priority,
        calendar: t
            .calendar
            .and_then(|c| project.calendars.get(c))
            .map(|c| c.name.clone()),
    }
}

/// Valeurs affichées de `t` (position `pos`).
fn shown_of(project: &Project, schedule: Option<&Schedule>, pos: usize, t: &Task) -> ClipShown {
    let outcome = schedule.and_then(|s| s.get(t.uid()));
    let span = shown_span(outcome);
    let id_of = |u: TaskUid| {
        project
            .tasks
            .position(u)
            .and_then(|p| u32::try_from(p.saturating_add(1)).ok())
    };
    ClipShown {
        id: u32::try_from(pos.saturating_add(1)).unwrap_or(u32::MAX),
        uid: t.uid().get(),
        outline_level: t.outline_level.get(),
        wbs: t.wbs.clone(),
        duration: Some(shown_duration(t, outcome)),
        start: span.map(|(s, _)| s),
        finish: span.map(|(_, f)| f),
        percent: t.progress.percent_complete.get(),
        predecessors: t
            .predecessors
            .iter()
            .filter_map(|l| {
                id_of(l.predecessor).map(|id| PredecessorRef {
                    id,
                    kind: l.kind,
                    lag: l.lag,
                })
            })
            .collect(),
        unique_predecessors: t
            .predecessors
            .iter()
            .map(|l| PredecessorRef {
                id: l.predecessor.get(),
                kind: l.kind,
                lag: l.lag,
            })
            .collect(),
    }
}

/// Vrai si `t` porte un suivi que le collage ne recrée pas (R-244) :
/// avancement ou réels d'une feuille (ceux d'une récapitulative se
/// recalculent), ligne de base, calendrier propre, priorité autre que 500.
fn tracking_lost(t: &Task, summary: bool) -> bool {
    let p = &t.progress;
    let progress = p.percent_complete.get() > 0
        || p.percent_work_complete.get() > 0
        || p.physical_percent_complete.get() > 0
        || p.actual_start.is_some()
        || p.actual_finish.is_some()
        || p.actual_duration.get() > 0
        || p.actual_work.minutes().get() > 0;
    (progress && !summary)
        || !t.baselines.is_empty()
        || t.calendar.is_some()
        || t.priority != Priority::default()
}

/// Copie des positions `picked` de `project` (descendants des
/// récapitulatives compris, lignes vides écartées), dans l'ordre des ID.
fn capture_positions(
    project: &Project,
    schedule: Option<&Schedule>,
    picked: &BTreeSet<usize>,
) -> Result<RowClip, ClipError> {
    let mut clip = RowClip::default();
    // UID copié → indice dans le tampon.
    let mut index: BTreeMap<TaskUid, u32> = BTreeMap::new();
    // Parent dans la forêt copiée, par indice.
    let mut parents: Vec<Option<u32>> = Vec::new();
    let mut stack: Vec<(Ancestor, Option<u32>)> = Vec::new();
    // Niveaux par position (récapitulative : la ligne suivante est plus
    // profonde).
    let levels: Vec<u16> = project.tasks.iter().map(|t| t.outline_level.get()).collect();
    for (pos, t) in project.tasks.iter().enumerate() {
        if t.is_null {
            continue;
        }
        let level = t.outline_level.get();
        while stack.last().is_some_and(|(a, _)| a.level >= level) {
            stack.pop();
        }
        let parent = stack.last().copied();
        let parent_depth = parent.and_then(|(a, _)| a.depth);
        let included = picked.contains(&pos) || parent_depth.is_some();
        if !included {
            stack.push((Ancestor { level, depth: None }, None));
            continue;
        }
        if clip.tasks.len() >= MAX_PASTE_LINES {
            return Err(ClipError::TooLarge);
        }
        let depth = parent_depth.map_or(0, |d| d.saturating_add(1));
        let i = u32::try_from(clip.tasks.len()).map_err(|_| ClipError::TooLarge)?;
        index.insert(t.uid(), i);
        parents.push(parent.and_then(|(_, idx)| idx));
        stack.push((
            Ancestor {
                level,
                depth: Some(depth),
            },
            Some(i),
        ));
        let summary = levels
            .get(pos.saturating_add(1))
            .is_some_and(|&next| next > level);
        clip.tasks.push(ClipTask {
            rel_level: depth,
            fields: fields_of(project, t),
            shown: shown_of(project, schedule, pos, t),
            tracking_lost: tracking_lost(t, summary),
        });
    }
    // Liens : internes recréés (sauf ceux qu'un `link.add` refuserait),
    // ceux qui n'ont qu'un bout dans le tampon comptés.
    let mut seen = BTreeSet::new();
    for t in &project.tasks {
        let succ = index.get(&t.uid()).copied();
        for l in &t.predecessors {
            let pred = index.get(&l.predecessor).copied();
            match (pred, succ) {
                (Some(from), Some(to)) => {
                    if from == to || related(&parents, from, to) || !seen.insert((from, to)) {
                        clip.links_dropped = clip.links_dropped.saturating_add(1);
                    } else {
                        clip.links.push(ClipLink {
                            from,
                            to,
                            kind: l.kind,
                            lag: l.lag,
                        });
                    }
                }
                (Some(_), None) | (None, Some(_)) => {
                    clip.links_dropped = clip.links_dropped.saturating_add(1);
                }
                (None, None) => {}
            }
        }
    }
    // Affectations à une ressource existante, dans l'ordre des tâches.
    let mut assignments: Vec<ClipAssignment> = project
        .assignments
        .iter()
        .filter_map(|a| {
            let task = *index.get(&a.task)?;
            let r = project.resources.get(a.resource?)?;
            let units = match r.kind {
                ResourceKind::Material { .. } => material_quantity(a.work.minutes().get()),
                ResourceKind::Work | ResourceKind::Cost => a.units,
            };
            Some(ClipAssignment {
                task,
                resource: r.name.clone(),
                units,
            })
        })
        .collect();
    assignments.sort_by_key(|a| a.task);
    clip.assignments = assignments;
    Ok(clip)
}

/// Vrai si `a` est ancêtre de `b` ou l'inverse dans la forêt copiée.
fn related(parents: &[Option<u32>], a: u32, b: u32) -> bool {
    let ancestor_of = |x: u32, y: u32| {
        let mut cur = parents
            .get(usize::try_from(y).unwrap_or(usize::MAX))
            .copied()
            .flatten();
        // Borne : une chaîne d'ancêtres ne dépasse pas le nombre de tâches.
        for _ in 0..parents.len() {
            match cur {
                Some(c) if c == x => return true,
                Some(c) => {
                    cur = parents
                        .get(usize::try_from(c).unwrap_or(usize::MAX))
                        .copied()
                        .flatten();
                }
                None => return false,
            }
        }
        false
    };
    ancestor_of(a, b) || ancestor_of(b, a)
}

/// R-146 : oui / non dans la langue du texte.
const fn yes_no(b: bool, language: Language) -> &'static str {
    match (b, language) {
        (true, Language::Fr) => "Oui",
        (false, Language::Fr) => "Non",
        (true, Language::En) => "Yes",
        (false, Language::En) => "No",
    }
}

/// Liste de prédécesseurs (R-142).
fn predecessor_list(refs: &[PredecessorRef], ctx: &TextContext) -> String {
    refs.iter()
        .map(|r| format_predecessor(r, ctx))
        .collect::<Vec<_>>()
        .join(&ctx.list_separator.to_string())
}

/// Cellule du texte tabulé : textes du formateur, garde de formule sur les
/// textes libres (R-146), tabulations et retours remplacés par une espace.
fn tsv_cell(t: &ClipTask, column: CsvColumn, ctx: &TextContext) -> String {
    let f = &t.fields;
    let s = &t.shown;
    let raw = match column {
        CsvColumn::Id => s.id.to_string(),
        CsvColumn::UniqueId => s.uid.to_string(),
        CsvColumn::Name => csv::guard(&f.name),
        CsvColumn::Duration => format_duration(&s.duration.unwrap_or(f.duration), ctx),
        CsvColumn::Start => s.start.map(|m| format_date(m, ctx)).unwrap_or_default(),
        CsvColumn::Finish => s.finish.map(|m| format_date(m, ctx)).unwrap_or_default(),
        CsvColumn::Predecessors => predecessor_list(&s.predecessors, ctx),
        CsvColumn::UniquePredecessors => predecessor_list(&s.unique_predecessors, ctx),
        CsvColumn::OutlineLevel => s.outline_level.to_string(),
        CsvColumn::Milestone => yes_no(f.milestone, ctx.language).to_owned(),
        CsvColumn::Notes => csv::guard(&f.notes),
        CsvColumn::Wbs => s.wbs.as_deref().map(csv::guard).unwrap_or_default(),
        CsvColumn::PercentComplete => {
            let mut out = format_decimal(
                Decimal::new(s.percent, 2).unwrap_or(Decimal::integer(0)),
                ctx.decimal,
            );
            out.push('%');
            out
        }
    };
    raw.chars()
        .map(|c| if matches!(c, '\t' | '\r' | '\n') { ' ' } else { c })
        .collect()
}

impl RowClip {
    /// Copie des tâches `uids` (descendants des récapitulatives inclus,
    /// lignes vides écartées, ordre des ID, doublons fusionnés).
    ///
    /// # Errors
    ///
    /// [`ClipError::UnknownTask`] : UID absent du projet ;
    /// [`ClipError::TooLarge`] : plus de [`MAX_PASTE_LINES`] tâches.
    pub fn capture(s: &Session, uids: &[TaskUid]) -> Result<Self, ClipError> {
        let project = s.project();
        let mut picked = BTreeSet::new();
        for &uid in uids {
            picked.insert(project.tasks.position(uid).ok_or(ClipError::UnknownTask(uid))?);
        }
        capture_positions(project, Some(s.schedule()), &picked)
    }

    /// Texte tabulé des colonnes `cols` : une ligne par tâche, sans
    /// en-tête, lignes séparées par `\n` (sans fin de ligne finale).
    #[must_use]
    pub fn to_tsv(&self, cols: &[CsvColumn], ctx: &TextContext) -> String {
        self.tasks
            .iter()
            .map(|t| {
                cols.iter()
                    .map(|c| tsv_cell(t, *c, ctx))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Empreinte stable d'un texte du presse-papiers : FNV-1a 64 bits sur le
/// texte aux fins de ligne normalisées en `\n` (`\r\n` et `\r` seul lus
/// `\n`). Jamais `RandomState` : même texte, même empreinte partout.
#[must_use]
pub fn fingerprint(text: &str) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut feed = |b: u8| {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(PRIME);
    };
    let mut bytes = text.bytes().peekable();
    while let Some(b) = bytes.next() {
        if b == b'\r' {
            if bytes.peek() == Some(&b'\n') {
                bytes.next();
            }
            feed(b'\n');
        } else {
            feed(b);
        }
    }
    hash
}

/// Lot ordinaire des opérations `ops`.
const fn batch_of(ops: Vec<Command>) -> Batch {
    Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: None,
        ops,
        label: None,
    }
}

/// `task.update` de `r` sans champ.
const fn task_update(r: TaskRef) -> TaskUpdate {
    TaskUpdate {
        uid: r,
        name: Patch::Keep,
        duration: Patch::Keep,
        milestone: Patch::Keep,
        notes: Patch::Keep,
        start: Patch::Keep,
        finish: Patch::Keep,
        active: Patch::Keep,
        task_type: Patch::Keep,
        effort_driven: Patch::Keep,
        work: Patch::Keep,
        mode: Patch::Keep,
        custom: std::collections::BTreeMap::new(),
    }
}

/// Durée en JSON, sans texte (minutes, unité, base, estimation : relue à
/// l'identique).
fn duration_json(d: &Duration) -> DurationJson {
    DurationJson {
        minutes: Some(d.minutes().get()),
        unit: Some(d.unit().into()),
        text: None,
        elapsed: Some(d.basis() == TimeBasis::Elapsed),
        estimated: Some(d.is_estimated()),
    }
}

/// Décalage en JSON, sans texte.
fn lag_json(l: &Lag) -> LagJson {
    let (minutes, percent, unit) = match l.value {
        LagValue::Time { minutes, unit } => (Some(minutes.get()), None, Some(unit.into())),
        LagValue::Percent { percent } => (None, Some(percent.get()), None),
    };
    LagJson {
        minutes,
        percent,
        unit,
        text: None,
        elapsed: Some(l.basis == TimeBasis::Elapsed),
    }
}

/// Étiquette de la `i`-ième tâche collée.
fn task_label(i: usize) -> String {
    format!("$t{}", i.saturating_add(1))
}

/// Opérations propres à une tâche collée (après son `task.add`, avant ses
/// enfants : elle est encore une feuille, sa contrainte et ses valeurs
/// manuelles sont acceptées telles quelles).
fn task_ops(f: &ClipFields, r: &TaskRef, summary: bool, project: &Project, ops: &mut Vec<Command>) {
    if f.constraint != Constraint::default_for(&project.anchor) {
        ops.push(Command::TaskSetConstraint(SetConstraint {
            uid: r.clone(),
            kind: ConstraintKindJson::from(f.constraint.kind()),
            date: f.constraint.date(),
        }));
    }
    if let Some(d) = f.deadline {
        ops.push(Command::TaskSetDeadline(SetDeadline {
            uid: r.clone(),
            deadline: Patch::Set(d),
        }));
    }
    if f.manual {
        // R-253 puis R-254 : feuille, début et durée manuels (la fin suit) ;
        // récapitulative (ou feuille sans durée), début puis fin (la durée
        // suit : celle d'une récapitulative est son temps ouvré, R-054),
        // si la fin ne précède pas le début.
        let (start, finish, duration) = f.manual_values.unwrap_or((None, None, None));
        let duration = duration.filter(|_| !summary);
        let mut up = TaskUpdate {
            mode: Patch::Set(ModeJson::Manual),
            ..task_update(r.clone())
        };
        if let Some(s) = start {
            up.start = Patch::Set(s);
        }
        if let Some(d) = duration {
            up.duration = Patch::Set(duration_json(&d));
        }
        ops.push(Command::TaskUpdate(up));
        if let (Some(s), Some(fin), None) = (start, finish, duration)
            && fin >= s
        {
            ops.push(Command::TaskUpdate(TaskUpdate {
                finish: Patch::Set(fin),
                ..task_update(r.clone())
            }));
        }
        // Une durée manuelle nulle pose le drapeau jalon (R-157) : celui de
        // la source l'emporte.
        ops.push(Command::TaskUpdate(TaskUpdate {
            milestone: Patch::Set(f.milestone),
            ..task_update(r.clone())
        }));
    }
    if !f.active {
        ops.push(Command::TaskUpdate(TaskUpdate {
            active: Patch::Set(false),
            ..task_update(r.clone())
        }));
    }
}

/// Ressource cible d'un nom au collage.
#[derive(Clone)]
enum Target {
    /// Ressource existante et son type.
    Existing(ResourceUid, ResourceKind),
    /// Ressource créée par le lot (étiquette).
    Created(String),
    /// Aucune.
    Missing,
}

/// Ressources cibles des affectations du tampon, par nom exact (homonymes :
/// la première dans l'ordre) ; `MatchOrCreate` ajoute un `resource.add` de
/// travail par nom absent.
fn resource_targets<'c>(
    project: &Project,
    c: &'c RowClip,
    mode: ResourcePaste,
    report: &mut PasteReport,
    ops: &mut Vec<Command>,
) -> BTreeMap<&'c str, Target> {
    let mut targets: BTreeMap<&str, Target> = BTreeMap::new();
    if mode == ResourcePaste::Drop {
        return targets;
    }
    for a in &c.assignments {
        if targets.contains_key(a.resource.as_str()) {
            continue;
        }
        let found = project.resources.iter().find(|r| r.name == a.resource);
        let target = match (found, mode) {
            (Some(r), _) => Target::Existing(r.uid(), r.kind.clone()),
            (None, ResourcePaste::MatchOrCreate) => {
                report.resources_created = report.resources_created.saturating_add(1);
                let label = format!("$r{}", report.resources_created);
                ops.push(Command::ResourceAdd(ResourceAdd {
                    name: a.resource.clone(),
                    label: Some(label.clone()),
                    kind: None,
                    material_label: None,
                    max_units: None,
                    calendar: None,
                    initials: None,
                    group: None,
                }));
                Target::Created(label)
            }
            (None, _) => Target::Missing,
        };
        targets.insert(a.resource.as_str(), target);
    }
    targets
}

/// Tâches du tampon : racines l'une après l'autre (la première à `at`),
/// enfants en dernier enfant de leur parent copié ; chaque `task.add`
/// suivi des opérations propres à la tâche.
fn task_add_ops(c: &RowClip, at: &Placement, project: &Project, ops: &mut Vec<Command>) {
    let mut stack: Vec<String> = Vec::new();
    for (i, t) in c.tasks.iter().enumerate() {
        let label = task_label(i);
        let depth = if i == 0 {
            0
        } else {
            usize::from(t.rel_level).min(stack.len())
        };
        let placement = if i == 0 {
            at.clone()
        } else if let Some(parent) = depth.checked_sub(1).and_then(|d| stack.get(d)) {
            Placement::LastChildOf(TaskRef::Label(parent.clone()))
        } else if let Some(root) = stack.first() {
            Placement::After(TaskRef::Label(root.clone()))
        } else {
            at.clone()
        };
        stack.truncate(depth);
        stack.push(label.clone());
        let f = &t.fields;
        ops.push(Command::TaskAdd(TaskAdd {
            name: f.name.chars().take(MAX_NAME_CHARS).collect(),
            label: Some(label.clone()),
            at: Some(placement),
            duration: Some(duration_json(&f.duration)),
            milestone: Some(f.milestone),
            notes: (!f.notes.is_empty()).then(|| f.notes.clone()),
            start: None,
            finish: None,
        }));
        let summary = c
            .tasks
            .get(i.saturating_add(1))
            .is_some_and(|n| n.rel_level > t.rel_level);
        task_ops(f, &TaskRef::Label(label), summary, project, ops);
    }
}

/// Indice `i` du tampon en `usize` (hors bornes : `usize::MAX`).
fn idx(i: u32) -> usize {
    usize::try_from(i).unwrap_or(usize::MAX)
}

/// Affectations du tampon vers leurs ressources cibles ; nom sans cible,
/// tâche hors du tampon ou doublon (tâche, ressource) : omise et comptée.
fn assignment_ops(
    c: &RowClip,
    targets: &BTreeMap<&str, Target>,
    report: &mut PasteReport,
    ops: &mut Vec<Command>,
) {
    let mut assigned: BTreeSet<(u32, String)> = BTreeSet::new();
    for a in &c.assignments {
        let target = targets
            .get(a.resource.as_str())
            .cloned()
            .unwrap_or(Target::Missing);
        let (resource, kind, key) = match target {
            Target::Existing(uid, kind) => (ResourceRef::Uid(uid), kind, uid.get().to_string()),
            Target::Created(label) => (ResourceRef::Label(label.clone()), ResourceKind::Work, label),
            Target::Missing => {
                report.assignments_dropped = report.assignments_dropped.saturating_add(1);
                continue;
            }
        };
        if idx(a.task) >= c.tasks.len() || !assigned.insert((a.task, key)) {
            report.assignments_dropped = report.assignments_dropped.saturating_add(1);
            continue;
        }
        let value = UnitsJson {
            value: Some(a.units.get()),
            text: None,
        };
        let (units, quantity) = match kind {
            ResourceKind::Work => (Some(value), None),
            ResourceKind::Material { .. } => (None, Some(value)),
            ResourceKind::Cost => (None, None),
        };
        ops.push(Command::AssignmentAdd(AssignmentAdd {
            task: TaskRef::Label(task_label(idx(a.task))),
            resource,
            units,
            work: None,
            quantity,
            delay: None,
        }));
    }
}

/// R-244 : lot qui colle le tampon à `at` : une tâche nouvelle par tâche
/// du tampon (UID neufs, R-151), niveaux relatifs gardés (la racine prend
/// le niveau de la ligne d'insertion), liens internes recréés, affectations
/// reprises par nom de ressource selon `o.resources`.
///
/// # Errors
///
/// [`ClipError::Unrecognized`] : tampon vide ; [`ClipError::TooLarge`] :
/// plus de [`MAX_PASTE_LINES`] tâches.
pub fn paste_batch(
    s: &Session,
    c: &RowClip,
    at: &Placement,
    o: &PasteOptions,
) -> Result<(Batch, PasteReport), ClipError> {
    if c.tasks.is_empty() {
        return Err(ClipError::Unrecognized);
    }
    if c.tasks.len() > MAX_PASTE_LINES {
        return Err(ClipError::TooLarge);
    }
    let project = s.project();
    let mut report = PasteReport {
        tasks: u32::try_from(c.tasks.len()).map_err(|_| ClipError::TooLarge)?,
        links_dropped: c.links_dropped,
        tracking_dropped: u32::try_from(c.tasks.iter().filter(|t| t.tracking_lost).count())
            .unwrap_or(u32::MAX),
        ..PasteReport::default()
    };
    let mut ops = Vec::new();
    let targets = resource_targets(project, c, o.resources, &mut report, &mut ops);
    task_add_ops(c, at, project, &mut ops);
    let n = c.tasks.len();
    for l in &c.links {
        if !o.links || idx(l.from) >= n || idx(l.to) >= n || l.from == l.to {
            report.links_dropped = report.links_dropped.saturating_add(1);
            continue;
        }
        ops.push(Command::LinkAdd(LinkSpec {
            pred: TaskRef::Label(task_label(idx(l.from))),
            succ: TaskRef::Label(task_label(idx(l.to))),
            kind: Some(l.kind.into()),
            lag: Some(lag_json(&l.lag)),
        }));
    }
    assignment_ops(c, &targets, &mut report, &mut ops);
    Ok((batch_of(ops), report))
}

/// Lignes d'un texte collé (`\n` ou `\r\n`), bornées.
fn lines_of(text: &str) -> Result<Vec<&str>, ClipError> {
    if text.len() > MAX_PASTE_BYTES {
        return Err(ClipError::TooLarge);
    }
    let lines: Vec<&str> = text.lines().take(MAX_PASTE_LINES.saturating_add(1)).collect();
    if lines.len() > MAX_PASTE_LINES {
        return Err(ClipError::TooLarge);
    }
    Ok(lines)
}

/// R-256 (a) : texte à en-tête reconnu, lu par l'import CSV (séparateur
/// tabulation, UTF-8, paramètres régionaux déduits des en-têtes), puis
/// collé comme un tampon (sans ressources).
fn paste_with_header(s: &Session, text: &str, at: &Placement) -> Result<(Batch, PasteReport), ClipError> {
    let project = s.project();
    let mut options = CsvImportOptions::new(project.anchor, project.current_date);
    options.separator = Some(b'\t');
    options.encoding = Some(CsvEncoding::Utf8);
    let imported = csv::import(text.as_bytes(), &options).map_err(|e| match e {
        CsvError::TooLarge | CsvError::TooManyRows | CsvError::TooManyColumns => ClipError::TooLarge,
        _ => ClipError::Unrecognized,
    })?;
    let all: BTreeSet<usize> = (0..imported.project.tasks.len()).collect();
    let clip = capture_positions(&imported.project, None, &all)?;
    paste_batch(
        s,
        &clip,
        at,
        &PasteOptions {
            links: true,
            resources: ResourcePaste::Drop,
        },
    )
}

/// R-256 : lot qui colle un texte externe à `at` (collage réduit) :
/// (a) première ligne non vide dont une cellule est reconnue comme Nom par
/// [`csv::recognize`] ⇒ lignes nouvelles par le chemin de l'import CSV ;
/// (b) aucune tabulation ⇒ une tâche par ligne non vide (nom réduit aux
/// espaces de bord, 512 caractères au plus, durée par défaut du projet) ;
/// (c) sinon [`ClipError::Unrecognized`]. `lang` : langue du lot.
///
/// # Errors
///
/// [`ClipError::TooLarge`] au-delà de [`MAX_PASTE_BYTES`] octets ou de
/// [`MAX_PASTE_LINES`] lignes ; [`ClipError::Unrecognized`] pour un texte
/// vide ou à plusieurs colonnes sans en-tête reconnu.
pub fn paste_text(
    s: &Session,
    text: &str,
    at: &Placement,
    lang: Language,
) -> Result<(Batch, PasteReport), ClipError> {
    let lines = lines_of(text)?;
    let Some(first) = lines.iter().find(|l| !l.trim().is_empty()) else {
        return Err(ClipError::Unrecognized);
    };
    let has_name = first
        .split('\t')
        .any(|h| csv::recognize(h) == Recognized::Column(CsvColumn::Name));
    let (mut batch, report) = if has_name {
        paste_with_header(s, text, at)?
    } else if !text.contains('\t') {
        let mut ops = Vec::new();
        let mut previous: Option<String> = None;
        for name in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            let label = task_label(ops.len());
            let placement = previous
                .as_ref()
                .map_or_else(|| at.clone(), |p| Placement::After(TaskRef::Label(p.clone())));
            ops.push(Command::TaskAdd(TaskAdd {
                name: name.chars().take(MAX_NAME_CHARS).collect(),
                label: Some(label.clone()),
                at: Some(placement),
                duration: None,
                milestone: None,
                notes: None,
                start: None,
                finish: None,
            }));
            previous = Some(label);
        }
        let report = PasteReport {
            tasks: u32::try_from(ops.len()).map_err(|_| ClipError::TooLarge)?,
            ..PasteReport::default()
        };
        (batch_of(ops), report)
    } else {
        return Err(ClipError::Unrecognized);
    };
    batch.lang = Some(lang);
    Ok((batch, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vecteurs FNV-1a 64 publiés (chaîne vide, « a ») et normalisation
    /// des fins de ligne.
    #[test]
    fn fingerprint_fnv1a_vectors_and_newlines() {
        assert_eq!(fingerprint(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fingerprint("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fingerprint("A\r\nB"), fingerprint("A\nB"));
        assert_eq!(fingerprint("A\rB"), fingerprint("A\nB"));
        assert_ne!(fingerprint("A\tB"), fingerprint("A\nB"));
    }

    #[test]
    fn clip_error_codes() {
        assert_eq!(ClipError::Unrecognized.code(), "paste.unrecognized");
        assert_eq!(ClipError::TooLarge.code(), "paste.too_large");
        assert_eq!(ClipError::UnknownTask(TaskUid::new(3)).code(), "task.not_found");
    }

    #[test]
    fn material_quantity_inverts_work() {
        // 3 unités ⇒ 180 min ⇒ 3 unités ; 1,5 ⇒ 90 min.
        assert_eq!(material_quantity(180).get(), 30_000);
        assert_eq!(material_quantity(90).get(), 15_000);
    }
}
