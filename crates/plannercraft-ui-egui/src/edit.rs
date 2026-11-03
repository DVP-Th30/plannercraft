//! Édition des cellules de la table (J4, lot L12) : état de la saisie et
//! traduction du texte saisi en lot de commandes du registre.
//!
//! Aucune date n'est calculée ici (règle 3) : le texte est lu par
//! `plannercraft_automation::text` (R-140 à R-143) puis envoyé au registre, qui
//! applique R-158 (début ou fin saisis → contrainte) et recalcule. Seule la
//! résolution ID → UID des prédécesseurs se fait côté interface (R-150 :
//! le registre ne connaît que des UID).
//!
//! [`commit_edit`] est pure (aucun egui) : la vue (`view::table`) l'appelle à
//! la validation et pousse le lot rendu dans `Action::ExecOn` (document
//! visé par son numéro d'ouverture).

use std::collections::BTreeMap;

use plannercraft_automation::command::parse_custom_number;
use plannercraft_automation::command::{
    ConstraintKindJson, DurationJson, LagJson, LinkKey, LinkSpec, LinkTypeJson, ModeJson, PercentJson,
    ProgressUpdate, SetConstraint, SetDeadline, TaskAdd, TaskTypeJson, TaskUpdate,
};
use plannercraft_automation::text::{
    self, DateRole, PredecessorRef, TextContext, TextError, format_date, format_duration,
    format_predecessors, parse_date, parse_duration, parse_percent, parse_predecessors,
};
use plannercraft_automation::{Batch, Command, CommandError, Patch, Placement, Session, TaskRef};
use plannercraft_core::model::custom::MAX_CUSTOM_TEXT_CHARS;
use plannercraft_core::{
    Constraint, ConstraintKind, CustomFieldId, CustomKind, CustomValue, Duration, Lag, LinkType, Moment,
    Project, Schedule, SchedulingMode, Task, TaskOutcome, TaskType, TaskUid,
};

use crate::app::OpenDocument;
use crate::column::Column;
use crate::field::{fold, with_choice_labels};
use crate::format::{CONSTRAINT_KINDS, TASK_TYPES, format_notes_line};
use crate::i18n::{Arg, Language, TextId, Texts, error_text_id};
use crate::rows::{Row, RowKind, RowModel};

/// Étiquette de la tâche créée par la ligne vide, reprise par les liens du
/// même lot (R-150).
const BLANK_LABEL: &str = "$new";

/// Refus d'une saisie, affiché sous la cellule (la saisie reste ouverte).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    /// Texte illisible (R-140 à R-143).
    Text(TextError),
    /// Prédécesseur désigné par un ID qui n'existe pas dans le projet.
    UnknownId(u32),
    /// Colonne calculée, non éditable (ID, marge, critique).
    NotEditable,
    /// Saisie vide sur une cellule qui ne peut pas être vide (R-259 : durée,
    /// date d'une tâche manuelle ou d'une récapitulative…).
    NotClearable,
    /// Champ calculé de la seconde passe UX (marges, dates au plus tôt et
    /// au plus tard, lignes de base, écarts…, H-UI-117) : « Ce champ est
    /// calculé. »
    Computed,
    /// Valeur d'un champ à choix non reconnue (mode, type de contrainte, de
    /// tâche, Oui/Non) ; porte la liste des valeurs admises, dans la langue
    /// de la saisie.
    BadChoice(String),
}

impl From<TextError> for EditError {
    fn from(e: TextError) -> Self {
        Self::Text(e)
    }
}

impl EditError {
    /// Message traduit (clés `error-text-*` des refus du moteur, détail dans
    /// la langue de l'interface ; `edit-error-*` sinon).
    #[must_use]
    pub fn message(&self, texts: &Texts) -> String {
        match self {
            Self::Text(e) => {
                let detail = CommandError::text(e.clone()).message(text_language(texts.language()));
                texts.get_args(error_text_id(e.code()), &[("detail", Arg::Str(detail.into()))])
            }
            Self::UnknownId(id) => {
                texts.get_args(TextId::EditErrorUnknownId, &[("id", Arg::Count(i64::from(*id)))])
            }
            Self::NotEditable => texts.get(TextId::EditErrorNotEditable),
            Self::NotClearable => texts.get_args(TextId::CellsNotClearable, &[("count", Arg::Count(1))]),
            Self::Computed => texts.get(TextId::EditErrorComputed),
            Self::BadChoice(choices) => texts.get_args(
                TextId::EditErrorBadChoice,
                &[("detail", Arg::Str(choices.clone().into()))],
            ),
        }
    }
}

/// Saisie en cours dans une cellule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditState {
    /// Tâche éditée ; `None` = ligne vide de fin (création).
    pub uid: Option<TaskUid>,
    /// Colonne éditée (une de [`editable`]).
    pub column: Column,
    /// Texte saisi.
    pub buffer: String,
    /// Refus de la dernière validation, affiché sous la cellule.
    pub error: Option<EditError>,
    /// Texte à l'ouverture par F2 ou double-clic : valider sans rien
    /// changer n'envoie aucun lot (ni historique, ni contrainte posée par
    /// R-158 sur une date relue). `None` (saisie ouverte par la frappe) :
    /// toujours envoyé.
    pub original: Option<String>,
}

impl EditState {
    /// Saisie ouverte avec `text` comme texte initial (F2, double-clic) :
    /// le valider tel quel ne change rien.
    #[must_use]
    pub fn open(uid: Option<TaskUid>, column: Column, text: String) -> Self {
        Self {
            uid,
            column,
            buffer: text.clone(),
            error: None,
            original: Some(text),
        }
    }

    /// Saisie ouverte par la frappe d'un caractère : `typed` remplace la
    /// valeur, envoyée à la validation même si elle est identique au texte
    /// affiché (comme une saisie complète dans MS Project).
    #[must_use]
    pub fn typed(uid: Option<TaskUid>, column: Column, typed: &str) -> Self {
        Self {
            uid,
            column,
            buffer: typed.to_owned(),
            error: None,
            original: None,
        }
    }
}

/// Saisie en cours dans la table : état transitoire de la vue (jamais
/// sérialisé), tenu par `ViewTransient`.
///
/// La cellule courante (ligne et colonne) est le curseur
/// `ViewTransient::cursor` (J7, lot L1, ADR-0015 D1) : la ligne courante
/// est la tâche sélectionnée (`App::selection`) ou, sans sélection, la
/// ligne vide de fin.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableEdit {
    /// Saisie ouverte, s'il y en a une.
    pub editing: Option<EditState>,
    /// Donner le focus à la zone de saisie (curseur en fin de texte) à la
    /// prochaine image.
    pub focus_pending: bool,
    /// Amener la ligne éditée à l'écran (saisie ouverte par F2 ou la frappe
    /// sur une ligne hors de la plage dessinée, relecture finale J7, F2) :
    /// lu par la vue principale après le dessin des lignes.
    pub reveal: bool,
}

impl TableEdit {
    /// Ouvre une saisie, demande le focus et la ligne à l'écran.
    pub fn start(&mut self, state: EditState) {
        self.editing = Some(state);
        self.focus_pending = true;
        self.reveal = true;
    }
}

/// Colonnes éditables : Nom, Durée, Début, Fin, Prédécesseurs, % achevé
/// (J7, lot L6) ; seconde passe UX (lot L7, H-UI-117) : les champs dont le
/// registre a la commande — Mode, Jalon, Notes, Actif, Successeurs, Type et
/// Date de contrainte, Échéance, Type de tâche, Pilotée par l'effort,
/// Travail (`task.update`, `task.set_constraint`, `task.set_deadline`,
/// liens), % travail achevé, Début et Fin réels, Durées réelle et restante
/// (`progress.update`). La priorité n'a pas d'opération : lecture seule.
/// Lot L9 (H-UI-118) : les champs personnalisés (`task.update { custom }`,
/// R-264 ; un champ défini par le fichier est refusé par le registre,
/// `custom.read_only`).
#[must_use]
pub const fn editable(column: Column) -> bool {
    matches!(
        column,
        Column::Custom(_)
            | Column::Name
            | Column::Duration
            | Column::Start
            | Column::Finish
            | Column::Predecessors
            | Column::PercentComplete
            | Column::Mode
            | Column::Milestone
            | Column::Notes
            | Column::Active
            | Column::Successors
            | Column::ConstraintType
            | Column::ConstraintDate
            | Column::Deadline
            | Column::TaskType
            | Column::EffortDriven
            | Column::Work
            | Column::PercentWorkComplete
            | Column::ActualStart
            | Column::ActualFinish
            | Column::ActualDuration
            | Column::RemainingDuration
    )
}

/// Champs d'avancement (`progress.update`) : ni sur une récapitulative, ni
/// sur une tâche exclue du calcul (R-216).
const fn progress_field(column: Column) -> bool {
    matches!(
        column,
        Column::PercentComplete
            | Column::PercentWorkComplete
            | Column::ActualStart
            | Column::ActualFinish
            | Column::ActualDuration
            | Column::RemainingDuration
    )
}

/// Cellule modifiable pour ce type de ligne (ADR-0018, R-257, R-258) :
/// colonne éditable ([`editable`]) et champ que le registre accepte
/// d'écrire sur cette ligne. Une récapitulative n'a que son Nom et ses
/// Prédécesseurs (durée, dates et % calculés, R-054, R-055) ; une tâche
/// exclue n'a pas de % achevé ; une ligne vide MSPDI (`IsNull`) rien.
#[must_use]
pub fn writable(row: &Row, column: Column) -> bool {
    if !editable(column) {
        return false;
    }
    match row.kind {
        RowKind::Null => false,
        // Récapitulative (lot L7 : champs saisis qui ne se calculent pas
        // depuis les sous-tâches) ; hypothèse H-UI-117.
        RowKind::Summary => matches!(
            column,
            Column::Custom(_)
                | Column::Name
                | Column::Predecessors
                | Column::Successors
                | Column::Notes
                | Column::Active
                | Column::Deadline
        ),
        RowKind::Excluded => !progress_field(column),
        RowKind::Leaf | RowKind::Milestone => true,
    }
}

/// Effacement d'une cellule (R-259).
#[derive(Clone, Debug, PartialEq)]
pub enum CellClear {
    /// Opérations qui vident la cellule.
    Ops(Vec<Command>),
    /// Cellule déjà vide (nom vide, aucun lien, 0 %, contrainte par
    /// défaut) : rien à écrire.
    Nothing,
    /// La cellule ne peut pas être vide (durée, date d'une tâche manuelle
    /// ou d'une récapitulative, % d'une récapitulative ou d'une tâche
    /// inactive, colonnes calculées).
    NotClearable,
}

/// Vrai si la tâche `uid` est récapitulative (R-052 : la tâche non vide
/// suivante est plus profonde).
fn is_summary(project: &Project, uid: TaskUid) -> bool {
    let Some(pos) = project.tasks.position(uid) else {
        return false;
    };
    let Some(task) = project.tasks.at(pos) else {
        return false;
    };
    project
        .tasks
        .iter()
        .skip(pos.saturating_add(1))
        .find(|t| !t.is_null)
        .is_some_and(|next| next.outline_level.get() > task.outline_level.get())
}

/// R-259 : opérations qui vident la cellule (`task`, `column`) ; chemin
/// unique de Ctrl+Suppr, de Suppr sur une cellule et de la validation d'une
/// saisie vide.
///
/// - Nom ⇒ nom vide ; Prédécesseurs ⇒ liens retirés (même chemin que la
///   saisie) ; % achevé ⇒ 0 % (récapitulative ou tâche inactive : non
///   effaçable) ;
/// - Début ou Fin d'une tâche automatique ⇒ contrainte par défaut du sens
///   de planification (`task.set_constraint`) ; d'une tâche manuelle ou
///   d'une récapitulative ⇒ non effaçable ;
/// - Durée et colonnes calculées ⇒ non effaçables.
#[must_use]
pub fn clear_cell(task: &Task, project: &Project, column: Column, ctx: &TextContext) -> CellClear {
    let uid = task.uid();
    if task.is_null {
        return CellClear::Nothing;
    }
    let ops = match column {
        Column::Name if task.name.is_empty() => Vec::new(),
        Column::Name => {
            let mut op = update(uid);
            op.name = Patch::Set(String::new());
            vec![Command::TaskUpdate(op)]
        }
        Column::Predecessors => replace_links(task, &[], project, ctx),
        Column::PercentComplete | Column::PercentWorkComplete if is_summary(project, uid) || !task.active => {
            return CellClear::NotClearable;
        }
        Column::PercentComplete if task.progress.percent_complete.get() == 0 => Vec::new(),
        Column::PercentComplete => match percent_update(uid, "0", ctx) {
            Ok(op) => vec![op],
            Err(_) => return CellClear::NotClearable,
        },
        Column::Start | Column::Finish
            if is_summary(project, uid) || matches!(task.mode, SchedulingMode::Manual { .. }) =>
        {
            return CellClear::NotClearable;
        }
        Column::Start | Column::Finish => {
            let default = Constraint::default_for(&project.anchor);
            if task.constraint == default {
                Vec::new()
            } else {
                vec![Command::TaskSetConstraint(SetConstraint {
                    uid: TaskRef::Uid(uid),
                    kind: ConstraintKindJson::from(default.kind()),
                    date: None,
                })]
            }
        }
        // Seconde passe UX (lot L7, R-259 amendée).
        Column::Notes if task.notes.is_empty() => Vec::new(),
        Column::Notes => {
            let mut op = update(uid);
            op.notes = Patch::Set(String::new());
            vec![Command::TaskUpdate(op)]
        }
        Column::Successors => replace_successors(task, &[], project, ctx),
        Column::ConstraintType | Column::ConstraintDate => {
            let default = Constraint::default_for(&project.anchor);
            if task.constraint == default {
                Vec::new()
            } else {
                vec![set_constraint(TaskRef::Uid(uid), default.kind(), None)]
            }
        }
        Column::Deadline if task.deadline.is_none() => Vec::new(),
        Column::Deadline => vec![Command::TaskSetDeadline(SetDeadline {
            uid: TaskRef::Uid(uid),
            deadline: Patch::Clear,
        })],
        // Champ personnalisé (lot L9, H-UI-118) : vide ⇒ valeur retirée
        // (indicateur : « Non »).
        Column::Custom(id) if !task.custom.contains_key(&id) => Vec::new(),
        Column::Custom(id) => {
            let mut op = update(uid);
            op.custom.insert(id.to_string(), serde_json::Value::Null);
            vec![Command::TaskUpdate(op)]
        }
        Column::PercentWorkComplete if task.progress.percent_work_complete.get() == 0 => Vec::new(),
        Column::PercentWorkComplete => {
            let mut op = progress(TaskRef::Uid(uid));
            op.percent_work_complete = Some(percent_json("0"));
            vec![Command::ProgressUpdate(op)]
        }
        // Colonnes calculées ou en lecture seule ; champs à choix, travail,
        // dates et durées réelles : toujours une valeur (non effaçables).
        _ => return CellClear::NotClearable,
    };
    if ops.is_empty() {
        CellClear::Nothing
    } else {
        CellClear::Ops(ops)
    }
}

/// `progress.update { percent_complete: { text } }` d'un % saisi (J7, lot
/// L6) : lu par `parse_percent` pour signaler l'erreur sous la cellule,
/// envoyé en texte avec `%` (que le registre exige ; « 50 » se lit
/// « 50% »). Une récapitulative ou une tâche exclue est refusée par le
/// registre (bandeau).
fn percent_update(uid: TaskUid, buffer: &str, ctx: &TextContext) -> Result<Command, EditError> {
    parse_percent(buffer, ctx)?;
    let mut op = progress(TaskRef::Uid(uid));
    op.percent_complete = Some(percent_json(buffer));
    Ok(Command::ProgressUpdate(op))
}

/// Pourcentage envoyé en texte avec `%` (que le registre exige ; « 50 » se
/// lit « 50% »).
fn percent_json(buffer: &str) -> PercentJson {
    let trimmed = buffer.trim();
    let text = if trimmed.ends_with('%') {
        trimmed.to_owned()
    } else {
        format!("{trimmed}%")
    };
    PercentJson {
        value: None,
        text: Some(text),
    }
}

/// `progress.update` vide sur `target`.
const fn progress(target: TaskRef) -> ProgressUpdate {
    ProgressUpdate {
        task: target,
        percent_complete: None,
        actual_start: None,
        actual_finish: None,
        actual_duration: None,
        remaining_duration: None,
        percent_work_complete: None,
        actual_work: None,
        physical_percent_complete: None,
    }
}

/// `task.set_constraint` de `kind` (date pour un type daté).
fn set_constraint(target: TaskRef, kind: ConstraintKind, date: Option<Moment>) -> Command {
    Command::TaskSetConstraint(SetConstraint {
        uid: target,
        kind: ConstraintKindJson::from(kind),
        date,
    })
}

/// Langue de l'interface correspondant à la langue des textes du moteur.
const fn ui_language(lang: text::Language) -> Language {
    match lang {
        text::Language::Fr => Language::Fr,
        text::Language::En => Language::En,
    }
}

/// Lit un champ à choix (H-UI-117) : libellé exact (sans casse ni
/// accents), sinon préfixe d'un seul libellé (« man » ⇒ « Manuelle ») ;
/// sinon refus portant la liste des valeurs admises.
fn choose<T: Copy>(buffer: &str, options: &[(&str, T)]) -> Result<T, EditError> {
    let needle = fold(buffer.trim());
    let refuse = || {
        let list: Vec<&str> = options.iter().map(|(label, _)| *label).collect();
        EditError::BadChoice(list.join(", "))
    };
    if needle.is_empty() {
        return Err(refuse());
    }
    if let Some((_, value)) = options.iter().find(|(label, _)| fold(label) == needle) {
        return Ok(*value);
    }
    let mut found = options
        .iter()
        .filter(|(label, _)| fold(label).starts_with(&needle));
    match (found.next(), found.next()) {
        (Some((_, value)), None) => Ok(*value),
        _ => Err(refuse()),
    }
}

/// Oui ou Non dans la langue `lang`.
fn read_yes_no(buffer: &str, lang: text::Language) -> Result<bool, EditError> {
    with_choice_labels(ui_language(lang), |l, _| {
        choose(buffer, &[(l.yes.as_str(), true), (l.no.as_str(), false)])
    })
}

/// Langue des textes du moteur correspondant à la langue de l'interface.
#[must_use]
pub const fn text_language(language: Language) -> text::Language {
    match language {
        Language::Fr => text::Language::Fr,
        Language::En => text::Language::En,
    }
}

/// Lot vide (rien à faire : valeur inchangée, ligne vide laissée vide).
const fn empty_batch(lang: text::Language) -> Batch {
    Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: Some(lang),
        ops: Vec::new(),
        label: None,
    }
}

/// `task.update` vide sur `uid`.
const fn update(uid: TaskUid) -> TaskUpdate {
    update_ref(TaskRef::Uid(uid))
}

/// `task.update` vide sur `target` (UID ou étiquette du lot).
const fn update_ref(target: TaskRef) -> TaskUpdate {
    TaskUpdate {
        uid: target,
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

/// `task.add` en fin de projet (ligne vide), nom vide.
fn add_at_end(name: String) -> TaskAdd {
    TaskAdd {
        name,
        label: None,
        at: Some(Placement::End),
        duration: None,
        milestone: None,
        notes: None,
        start: None,
        finish: None,
    }
}

/// Décalage en JSON sans texte (minutes et unité, ou pourcentage) : relu
/// exactement par `LagJson::to_lag`.
fn lag_json(lag: &Lag, ctx: &TextContext) -> LagJson {
    LagJson {
        text: None,
        ..LagJson::from_lag(lag, ctx)
    }
}

/// Prédécesseur résolu : UID, type, décalage.
type Resolved = (TaskUid, LinkType, Lag);

/// R-150 côté interface : ID (position + 1) → UID. Un ID absent refuse la
/// saisie. R-260 a : un ID répété ne donne qu'un lien, à la place de sa
/// première occurrence, avec le type et le décalage de la dernière
/// (« 6;6SS » ⇒ un lien 6 SS).
fn resolve(refs: &[PredecessorRef], project: &Project) -> Result<Vec<Resolved>, EditError> {
    let mut out: Vec<Resolved> = Vec::with_capacity(refs.len());
    for r in refs {
        let position = usize::try_from(r.id).ok().and_then(|id| id.checked_sub(1));
        let uid = position
            .and_then(|p| project.tasks.at(p))
            .map(Task::uid)
            .ok_or(EditError::UnknownId(r.id))?;
        match out.iter_mut().find(|(u, _, _)| *u == uid) {
            Some(slot) => *slot = (uid, r.kind, r.lag),
            None => out.push((uid, r.kind, r.lag)),
        }
    }
    Ok(out)
}

/// Lit une liste de prédécesseurs : la première erreur de texte refuse la
/// saisie.
fn read_predecessors(s: &str, ctx: &TextContext, project: &Project) -> Result<Vec<Resolved>, EditError> {
    let (refs, errors) = parse_predecessors(s, ctx);
    if let Some(e) = errors.into_iter().next() {
        return Err(e.into());
    }
    resolve(&refs, project)
}

/// Lot qui remplace les prédécesseurs de `task` par `wanted` : retrait des
/// anciens absents, modification de ceux dont le type ou le décalage change,
/// ajout des nouveaux. Un lien vers un UID absent du projet (pendant) n'est
/// pas touché. Doublon, cycle, lien vers soi : refus du registre (R-156,
/// R-160).
fn replace_links(task: &Task, wanted: &[Resolved], project: &Project, ctx: &TextContext) -> Vec<Command> {
    let succ = task.uid();
    let old: BTreeMap<TaskUid, (LinkType, Lag)> = task
        .predecessors
        .iter()
        .map(|link| (link.predecessor, (link.kind, link.lag)))
        .collect();
    let mut ops = Vec::new();
    for link in &task.predecessors {
        let kept = wanted.iter().any(|(uid, _, _)| *uid == link.predecessor);
        if !kept && project.tasks.contains(link.predecessor) {
            ops.push(Command::LinkRemove(LinkKey {
                pred: TaskRef::Uid(link.predecessor),
                succ: TaskRef::Uid(succ),
            }));
        }
    }
    for (pred, kind, lag) in wanted {
        let spec = LinkSpec {
            pred: TaskRef::Uid(*pred),
            succ: TaskRef::Uid(succ),
            kind: Some(LinkTypeJson::from(*kind)),
            lag: Some(lag_json(lag, ctx)),
        };
        match old.get(pred) {
            Some((k, l)) if k == kind && l == lag => {}
            Some(_) => ops.push(Command::LinkUpdate(spec)),
            None => ops.push(Command::LinkAdd(spec)),
        }
    }
    ops
}

/// Successeurs de `task` (UID, type, décalage), dans l'ordre des ID.
fn successors(task: &Task, project: &Project) -> Vec<Resolved> {
    let pred = task.uid();
    project
        .tasks
        .iter()
        .filter_map(|t| {
            t.predecessors
                .iter()
                .find(|link| link.predecessor == pred)
                .map(|link| (t.uid(), link.kind, link.lag))
        })
        .collect()
}

/// Lot qui remplace les successeurs de `task` par `wanted` (colonne
/// Successeurs, lot L7) : liens inverses de [`replace_links`], même
/// non-changement (R-260 a, d).
fn replace_successors(
    task: &Task,
    wanted: &[Resolved],
    project: &Project,
    ctx: &TextContext,
) -> Vec<Command> {
    let pred = task.uid();
    let old = successors(task, project);
    let mut ops = Vec::new();
    for (succ, _, _) in &old {
        if !wanted.iter().any(|(uid, _, _)| uid == succ) {
            ops.push(Command::LinkRemove(LinkKey {
                pred: TaskRef::Uid(pred),
                succ: TaskRef::Uid(*succ),
            }));
        }
    }
    for (succ, kind, lag) in wanted {
        let spec = LinkSpec {
            pred: TaskRef::Uid(pred),
            succ: TaskRef::Uid(*succ),
            kind: Some(LinkTypeJson::from(*kind)),
            lag: Some(lag_json(lag, ctx)),
        };
        match old.iter().find(|(uid, _, _)| uid == succ) {
            Some((_, k, l)) if k == kind && l == lag => {}
            Some(_) => ops.push(Command::LinkUpdate(spec)),
            None => ops.push(Command::LinkAdd(spec)),
        }
    }
    ops
}

/// Début et fin planifiés d'une tâche (moteur), s'il y en a.
fn planned(schedule: &Schedule, uid: TaskUid) -> Option<(Moment, Moment)> {
    match schedule.tasks.get(&uid) {
        Some(TaskOutcome::Scheduled(s)) => Some((s.start, s.finish)),
        _ => None,
    }
}

/// Travail saisi : un nombre sans unité est en heures (unité par défaut du
/// travail dans MS Project, hypothèse H-UI-117).
fn work_text(buffer: &str) -> String {
    let trimmed = buffer.trim();
    if trimmed.chars().any(char::is_alphabetic) {
        trimmed.to_owned()
    } else {
        format!("{trimmed}h")
    }
}

/// Opérations d'une saisie dans un champ de la seconde passe UX (lot L7,
/// H-UI-117) sur `target` : la tâche `task` (non-changement de R-260 d
/// appliqué) ou, sans tâche, celle que le lot crée sur la ligne vide
/// (étiquette). Lectures par `plannercraft_automation::text` ; commandes du
/// registre existantes.
#[allow(
    clippy::too_many_lines,
    reason = "une branche par champ modifiable de H-UI-117"
)]
fn field_ops(
    target: &TaskRef,
    task: Option<&Task>,
    column: Column,
    buffer: &str,
    session: &Session,
    ctx: &TextContext,
    lang: text::Language,
) -> Result<Vec<Command>, EditError> {
    let project = session.project();
    let same = |eq: &dyn Fn(&Task) -> bool| task.is_some_and(eq);
    let mut op = update_ref(target.clone());
    match column {
        Column::Notes => {
            if same(&|t| t.notes == buffer || format_notes_line(&t.notes) == buffer) {
                return Ok(Vec::new());
            }
            op.notes = Patch::Set(buffer.to_owned());
        }
        Column::Milestone | Column::Active | Column::EffortDriven => {
            let value = read_yes_no(buffer, lang)?;
            let current = |t: &Task| match column {
                Column::Milestone => t.milestone,
                Column::Active => t.active,
                _ => t.effort_driven,
            };
            if same(&|t| current(t) == value) {
                return Ok(Vec::new());
            }
            match column {
                Column::Milestone => op.milestone = Patch::Set(value),
                Column::Active => op.active = Patch::Set(value),
                _ => op.effort_driven = Patch::Set(value),
            }
        }
        Column::Mode => {
            let manual = with_choice_labels(ui_language(lang), |l, _| {
                choose(
                    buffer,
                    &[(l.mode_auto.as_str(), false), (l.mode_manual.as_str(), true)],
                )
            })?;
            if same(&|t| matches!(t.mode, SchedulingMode::Manual { .. }) == manual) {
                return Ok(Vec::new());
            }
            op.mode = Patch::Set(if manual { ModeJson::Manual } else { ModeJson::Auto });
        }
        Column::TaskType => {
            let wanted = with_choice_labels(ui_language(lang), |_, fl| {
                let options: Vec<(&str, TaskType)> =
                    TASK_TYPES.iter().map(|t| (fl.task_type(*t), *t)).collect();
                choose(buffer, &options)
            })?;
            if same(&|t| t.task_type == wanted) {
                return Ok(Vec::new());
            }
            op.task_type = Patch::Set(TaskTypeJson::from(wanted));
        }
        Column::ConstraintType => {
            let kind = with_choice_labels(ui_language(lang), |_, fl| {
                let options: Vec<(&str, ConstraintKind)> =
                    CONSTRAINT_KINDS.iter().map(|k| (fl.constraint(*k), *k)).collect();
                choose(buffer, &options)
            })?;
            if same(&|t| t.constraint.kind() == kind) {
                return Ok(Vec::new());
            }
            // Type daté : date en place, sinon début (ou fin) planifié de la
            // tâche, comme MS Project quand on change le type seul.
            let date = if kind.is_dated() {
                let current = task.and_then(|t| t.constraint.date());
                let planned = task.and_then(|t| planned(session.schedule(), t.uid()));
                let pick = planned.map(|(start, finish)| if kind.is_start_side() { start } else { finish });
                Some(current.or(pick).ok_or(EditError::Text(TextError::BadDate))?)
            } else {
                None
            };
            return Ok(vec![set_constraint(target.clone(), kind, date)]);
        }
        Column::ConstraintDate => {
            // Type non daté : « Début au plus tôt le » (« le plus tard
            // possible » : « Fin au plus tard le ») ; hypothèse H-UI-117.
            let kind = match task.map_or(ConstraintKind::AsSoonAsPossible, |t| t.constraint.kind()) {
                ConstraintKind::AsSoonAsPossible => ConstraintKind::StartNoEarlierThan,
                ConstraintKind::AsLateAsPossible => ConstraintKind::FinishNoLaterThan,
                dated => dated,
            };
            let role = if kind.is_start_side() {
                DateRole::Start
            } else {
                DateRole::Finish
            };
            let date = parse_date(buffer, role, ctx)?;
            if same(&|t| t.constraint.kind() == kind && t.constraint.date() == Some(date)) {
                return Ok(Vec::new());
            }
            return Ok(vec![set_constraint(target.clone(), kind, Some(date))]);
        }
        Column::Deadline => {
            let date = parse_date(buffer, DateRole::Finish, ctx)?;
            if same(&|t| t.deadline == Some(date)) {
                return Ok(Vec::new());
            }
            return Ok(vec![Command::TaskSetDeadline(SetDeadline {
                uid: target.clone(),
                deadline: Patch::Set(date),
            })]);
        }
        Column::Work => {
            let text = work_text(buffer);
            let read = parse_duration(&text, ctx)?;
            if same(&|t| t.work.minutes() == read.minutes()) {
                return Ok(Vec::new());
            }
            op.work = Patch::Set(duration_text(&text));
        }
        Column::PercentWorkComplete => {
            let read = parse_percent(buffer, ctx)?;
            if same(&|t| t.progress.percent_work_complete == read) {
                return Ok(Vec::new());
            }
            let mut p = progress(target.clone());
            p.percent_work_complete = Some(percent_json(buffer));
            return Ok(vec![Command::ProgressUpdate(p)]);
        }
        Column::ActualStart | Column::ActualFinish => {
            let start = column == Column::ActualStart;
            let role = if start { DateRole::Start } else { DateRole::Finish };
            let date = parse_date(buffer, role, ctx)?;
            let current = |t: &Task| {
                if start {
                    t.progress.actual_start
                } else {
                    t.progress.actual_finish
                }
            };
            if same(&|t| current(t) == Some(date)) {
                return Ok(Vec::new());
            }
            let mut p = progress(target.clone());
            if start {
                p.actual_start = Some(date);
            } else {
                p.actual_finish = Some(date);
            }
            return Ok(vec![Command::ProgressUpdate(p)]);
        }
        Column::ActualDuration | Column::RemainingDuration => {
            let actual = column == Column::ActualDuration;
            let read = parse_duration(buffer, ctx)?;
            let current = |t: &Task| {
                if actual {
                    t.progress.actual_duration
                } else {
                    t.progress.remaining_duration
                }
            };
            if same(&|t| current(t) == read.minutes()) {
                return Ok(Vec::new());
            }
            let mut p = progress(target.clone());
            if actual {
                p.actual_duration = Some(duration_text(buffer));
            } else {
                p.remaining_duration = Some(duration_text(buffer));
            }
            return Ok(vec![Command::ProgressUpdate(p)]);
        }
        Column::Custom(id) => {
            let (value, json) = custom_value(id, buffer, ctx, lang)?;
            if same(&|t| t.custom.get(&id) == value.as_ref()) {
                return Ok(Vec::new());
            }
            op.custom.insert(id.to_string(), json);
        }
        Column::Successors => {
            let wanted = read_predecessors(buffer, ctx, project)?;
            return Ok(match task {
                Some(t) => replace_successors(t, &wanted, project, ctx),
                None => wanted
                    .iter()
                    .map(|(succ, kind, lag)| {
                        Command::LinkAdd(LinkSpec {
                            pred: target.clone(),
                            succ: TaskRef::Uid(*succ),
                            kind: Some(LinkTypeJson::from(*kind)),
                            lag: Some(lag_json(lag, ctx)),
                        })
                    })
                    .collect(),
            });
        }
        _ => return Err(not_editable(column)),
    }
    Ok(vec![Command::TaskUpdate(op)])
}

/// Valeur saisie d'un champ personnalisé (lot L9, H-UI-118, R-263) et sa
/// forme JSON pour `task.update { custom }` (R-264) : texte d'au plus 255
/// caractères ; nombre lu dans la langue du lot (« 12,5 » en français,
/// « 12.5 » en anglais), envoyé tel quel ; date lue comme le Début, envoyée
/// en `AAAA-MM-JJThh:mm` ; indicateur Oui/Non. `None` : valeur vide
/// (indicateur à Non), effacée.
fn custom_value(
    id: CustomFieldId,
    buffer: &str,
    ctx: &TextContext,
    lang: text::Language,
) -> Result<(Option<CustomValue>, serde_json::Value), EditError> {
    use serde_json::Value;
    Ok(match id.kind() {
        CustomKind::Text => {
            if buffer.chars().nth(MAX_CUSTOM_TEXT_CHARS).is_some() {
                return Err(EditError::Text(TextError::OutOfRange));
            }
            let value = (!buffer.is_empty()).then(|| CustomValue::Text(buffer.to_owned()));
            (value, Value::String(buffer.to_owned()))
        }
        CustomKind::Number => {
            let n =
                parse_custom_number(buffer, lang).ok_or(EditError::Text(TextError::BadNumber { at: 0 }))?;
            (
                Some(CustomValue::Number(n)),
                Value::String(buffer.trim().to_owned()),
            )
        }
        CustomKind::Date => {
            let m = parse_date(buffer, DateRole::Start, ctx)?;
            (Some(CustomValue::Date(m)), Value::String(m.to_string()))
        }
        CustomKind::Flag => {
            let b = read_yes_no(buffer, lang)?;
            (b.then_some(CustomValue::Flag(true)), Value::Bool(b))
        }
    })
}

/// Refus d'une colonne non modifiable : champ calculé de la seconde passe
/// UX (« Ce champ est calculé. »), colonne calculée d'avant sinon.
const fn not_editable(column: Column) -> EditError {
    if column.ordinal().is_none() {
        EditError::Computed
    } else {
        EditError::NotEditable
    }
}

/// Lot de commandes d'une saisie validée (fonction pure).
///
/// - texte inchangé depuis l'ouverture → lot vide ;
/// - Nom → `task.update name` (ligne vide : `task.add at End`, rien si le
///   nom est vide) ;
/// - Durée → lue par `parse_duration` (R-140) pour signaler l'erreur sous
///   la cellule, envoyée en texte (`task.update duration {text}`) ;
/// - Début, Fin → `parse_date` (R-143, rôle Start ou Finish) →
///   `task.update start|finish` (R-158 appliquée par le registre) ;
/// - Prédécesseurs → `parse_predecessors` (R-142), ID → UID (R-150), puis
///   `link.remove` des anciens absents, `link.update`, `link.add` ;
/// - % achevé → `parse_percent`, puis `progress.update { percent_complete:
///   { text } }` (J7, lot L6 ; refusé sur la ligne vide : une tâche se crée
///   par les autres colonnes) ;
/// - ligne vide et autre colonne que Nom : `task.add` (nom vide) portant la
///   valeur, ou suivi des `link.add` vers l'étiquette `$new`.
///
/// Le lot porte la langue de l'interface (`lang`) : la session est ouverte
/// en français et doit lire « 3d » en anglais.
///
/// # Errors
///
/// [`EditError`] : texte illisible, ID inconnu, colonne non éditable, tâche
/// absente (`text.empty` n'est jamais rendu pour la ligne vide).
pub fn commit_edit(state: &EditState, session: &Session, lang: text::Language) -> Result<Batch, EditError> {
    commit(state, session, lang, true)
}

/// Comme [`commit_edit`], sans le non-changement des dates (R-260 d) :
/// pour une cellule Début ou Fin dont la tâche a déjà reçu une date saisie
/// dans le même lot (la contrainte en place n'est plus celle qui sera
/// remplacée, R-158 : la dernière date saisie l'emporte).
///
/// # Errors
///
/// Comme [`commit_edit`].
pub fn commit_edit_dates_forced(
    state: &EditState,
    session: &Session,
    lang: text::Language,
) -> Result<Batch, EditError> {
    commit(state, session, lang, false)
}

/// Lot d'une saisie ; `same_dates` : applique R-260 d aux dates.
fn commit(
    state: &EditState,
    session: &Session,
    lang: text::Language,
    same_dates: bool,
) -> Result<Batch, EditError> {
    let mut batch = empty_batch(lang);
    if !editable(state.column) {
        return Err(not_editable(state.column));
    }
    if state.original.as_deref() == Some(state.buffer.as_str()) {
        return Ok(batch);
    }
    let project = session.project();
    let ctx = TextContext::for_project(project, lang);
    let buffer = state.buffer.as_str();
    batch.ops = match state.uid {
        Some(uid) => {
            let Some(task) = project.tasks.get(uid) else {
                return Ok(batch);
            };
            // Saisie vide (Retour arrière puis Entrée, collage d'une cellule
            // vide) : effacement de R-259 au lieu d'une lecture refusée.
            if buffer.trim().is_empty() && editable(state.column) {
                batch.ops = match clear_cell(task, project, state.column, &ctx) {
                    CellClear::Ops(ops) => ops,
                    CellClear::Nothing => Vec::new(),
                    CellClear::NotClearable => return Err(EditError::NotClearable),
                };
                return Ok(batch);
            }
            let mut op = update(uid);
            // R-260 d : une valeur relue égale à la valeur courante ne
            // produit aucune opération (aucune entrée d'annulation).
            match state.column {
                Column::Name if buffer == task.name => return Ok(batch),
                Column::Name => op.name = Patch::Set(buffer.to_owned()),
                Column::Duration => {
                    if parse_duration(buffer, &ctx)? == task.duration {
                        return Ok(batch);
                    }
                    op.duration = Patch::Set(duration_text(buffer));
                }
                Column::Start => {
                    let date = parse_date(buffer, DateRole::Start, &ctx)?;
                    if same_dates && same_entered(task, project, DateRole::Start, date) {
                        return Ok(batch);
                    }
                    op.start = Patch::Set(date);
                }
                Column::Finish => {
                    let date = parse_date(buffer, DateRole::Finish, &ctx)?;
                    if same_dates && same_entered(task, project, DateRole::Finish, date) {
                        return Ok(batch);
                    }
                    op.finish = Patch::Set(date);
                }
                Column::Predecessors => {
                    let wanted = read_predecessors(buffer, &ctx, project)?;
                    let ops = replace_links(task, &wanted, project, &ctx);
                    batch.ops = ops;
                    return Ok(batch);
                }
                Column::PercentComplete => {
                    let op = percent_update(uid, buffer, &ctx)?;
                    if parse_percent(buffer, &ctx).is_ok_and(|p| p == task.progress.percent_complete) {
                        return Ok(batch);
                    }
                    batch.ops = vec![op];
                    return Ok(batch);
                }
                // Champs de la seconde passe UX (lot L7).
                column => {
                    batch.ops = field_ops(
                        &TaskRef::Uid(uid),
                        Some(task),
                        column,
                        buffer,
                        session,
                        &ctx,
                        lang,
                    )?;
                    return Ok(batch);
                }
            }
            vec![Command::TaskUpdate(op)]
        }
        None => blank_row_ops(state.column, buffer, &ctx, session, lang)?,
    };
    Ok(batch)
}

/// R-260 d : vrai si saisir `date` côté `role` sur `task` ne changerait
/// rien : feuille automatique dont la contrainte est déjà celle que R-158
/// poserait (« ne pas commencer avant » cette date depuis le début, « ne pas
/// finir avant », SNLT et FNLT à rebours). Une tâche manuelle ou une
/// récapitulative envoie toujours la saisie (le registre décide).
fn same_entered(task: &Task, project: &Project, role: DateRole, date: Moment) -> bool {
    if matches!(task.mode, SchedulingMode::Manual { .. }) || is_summary(project, task.uid()) {
        return false;
    }
    let backward = matches!(project.anchor, plannercraft_core::ScheduleAnchor::FromFinish(_));
    let wanted = match (role, backward) {
        (DateRole::Start, false) => Constraint::StartNoEarlierThan { date },
        (DateRole::Finish, false) => Constraint::FinishNoEarlierThan { date },
        (DateRole::Start, true) => Constraint::StartNoLaterThan { date },
        (DateRole::Finish, true) => Constraint::FinishNoLaterThan { date },
    };
    task.constraint == wanted
}

/// Durée envoyée en texte (lue par le registre dans la langue du lot).
fn duration_text(buffer: &str) -> DurationJson {
    DurationJson {
        text: Some(buffer.to_owned()),
        ..DurationJson::default()
    }
}

/// Opérations d'une saisie sur la ligne vide de fin (création d'une tâche).
fn blank_row_ops(
    column: Column,
    buffer: &str,
    ctx: &TextContext,
    session: &Session,
    lang: text::Language,
) -> Result<Vec<Command>, EditError> {
    let project = session.project();
    if buffer.trim().is_empty() {
        return Ok(Vec::new());
    }
    let mut add = add_at_end(String::new());
    match column {
        Column::Name => buffer.clone_into(&mut add.name),
        Column::Duration => {
            parse_duration(buffer, ctx)?;
            add.duration = Some(duration_text(buffer));
        }
        Column::Start => add.start = Some(parse_date(buffer, DateRole::Start, ctx)?),
        Column::Finish => add.finish = Some(parse_date(buffer, DateRole::Finish, ctx)?),
        Column::Predecessors => {
            let wanted = read_predecessors(buffer, ctx, project)?;
            add.label = Some(BLANK_LABEL.to_owned());
            let mut ops = vec![Command::TaskAdd(add)];
            ops.extend(wanted.iter().map(|(pred, kind, lag)| {
                Command::LinkAdd(LinkSpec {
                    pred: TaskRef::Uid(*pred),
                    succ: TaskRef::Label(BLANK_LABEL.to_owned()),
                    kind: Some(LinkTypeJson::from(*kind)),
                    lag: Some(lag_json(lag, ctx)),
                })
            }));
            return Ok(ops);
        }
        // Champs de la seconde passe UX (lot L7) : la tâche créée (étiquette
        // du lot) reçoit la valeur ; % achevé refusé (ci-dessous).
        column if editable(column) && column != Column::PercentComplete => {
            let target = TaskRef::Label(BLANK_LABEL.to_owned());
            let fields = field_ops(&target, None, column, buffer, session, ctx, lang)?;
            add.label = Some(BLANK_LABEL.to_owned());
            let mut ops = vec![Command::TaskAdd(add)];
            ops.extend(fields);
            return Ok(ops);
        }
        // Colonnes calculées ou en lecture seule.
        column => return Err(not_editable(column)),
    }
    Ok(vec![Command::TaskAdd(add)])
}

/// Texte initial d'une durée : l'affichage s'il se relit exactement
/// (« 5 jours », « 2,5 days »), sinon l'écriture R-141 du moteur (« 5ej » :
/// « 5 jours écoulés » n'est pas une unité de R-140).
#[must_use]
pub fn duration_edit_text(d: &Duration, display: &str, ctx: &TextContext) -> String {
    if parse_duration(display, ctx).is_ok_and(|read| read == *d) {
        display.to_owned()
    } else {
        format_duration(d, ctx)
    }
}

/// Texte initial d'une date : l'affichage (« lun 02/11/26 », sans heure)
/// s'il se relit au même jour, sinon `JJ/MM/AAAA hh:mm` (R-143 : l'année sur
/// deux chiffres 84 à 99 se lit 1984-1999, une date de 2084 à 2149 ne se
/// relit donc pas).
#[must_use]
pub fn date_edit_text(m: Moment, display: &str, role: DateRole, ctx: &TextContext) -> String {
    if parse_date(display, role, ctx).is_ok_and(|read| read.day() == m.day()) {
        display.to_owned()
    } else {
        format_date(m, ctx)
    }
}

/// Texte initial des prédécesseurs : l'affichage s'il se relit aux mêmes
/// liens, sinon l'écriture R-142 du moteur (« 3FD+2ej »).
#[must_use]
pub fn predecessors_edit_text(task: &Task, project: &Project, display: &str, ctx: &TextContext) -> String {
    let current: Vec<Resolved> = task
        .predecessors
        .iter()
        .map(|link| (link.predecessor, link.kind, link.lag))
        .collect();
    if read_predecessors(display, ctx, project).is_ok_and(|read| read == current) {
        display.to_owned()
    } else {
        format_predecessors(task, project, ctx).0
    }
}

/// Texte initial de la cellule (ligne `row` du modèle, `RowModel::len()` =
/// ligne vide) : le texte affiché, sauf quand il ne se relirait pas à la
/// même valeur (voir [`duration_edit_text`], [`date_edit_text`],
/// [`predecessors_edit_text`]). Ligne vide : texte vide.
#[must_use]
pub fn initial_text(
    doc: &OpenDocument,
    rows: &RowModel,
    row: usize,
    column: Column,
    lang: Language,
) -> String {
    let project = doc.project();
    let ctx = TextContext::for_project(project, text_language(lang));
    initial_text_in(project, rows, row, column, &ctx)
}

/// [`initial_text`] sur un projet et un contexte de texte donnés (copier
/// et remplir un bloc de cellules, ADR-0018 : un contexte pour toutes les
/// cellules).
#[must_use]
pub fn initial_text_in(
    project: &Project,
    rows: &RowModel,
    row: usize,
    column: Column,
    ctx: &TextContext,
) -> String {
    let Some(info) = rows.row(row) else {
        return String::new();
    };
    let display = rows.text(row, column);
    let Some(task) = project.tasks.get(info.uid) else {
        return display.to_owned();
    };
    match column {
        // Récapitulative : durée calculée, affichée telle quelle (le registre
        // refuse de la modifier).
        Column::Duration if info.kind != RowKind::Summary => duration_edit_text(&task.duration, display, ctx),
        Column::Start => info.bar.map_or_else(
            || display.to_owned(),
            |(start, _)| date_edit_text(start, display, DateRole::Start, ctx),
        ),
        Column::Finish => info.bar.map_or_else(
            || display.to_owned(),
            |(_, finish)| date_edit_text(finish, display, DateRole::Finish, ctx),
        ),
        Column::Predecessors => predecessors_edit_text(task, project, display, ctx),
        // Dates saisies de la seconde passe UX (lot L7).
        Column::ConstraintDate => task.constraint.date().map_or_else(
            || display.to_owned(),
            |m| {
                let role = if task.constraint.kind().is_start_side() {
                    DateRole::Start
                } else {
                    DateRole::Finish
                };
                date_edit_text(m, display, role, ctx)
            },
        ),
        Column::Deadline => task.deadline.map_or_else(
            || display.to_owned(),
            |m| date_edit_text(m, display, DateRole::Finish, ctx),
        ),
        // Date personnalisée (lot L9) : relue comme le Début.
        Column::Custom(id) => match task.custom.get(&id) {
            Some(CustomValue::Date(m)) => date_edit_text(*m, display, DateRole::Start, ctx),
            _ => display.to_owned(),
        },
        _ => display.to_owned(),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_automation::command::{LinkKey, LinkSpec, LinkTypeJson};
    use plannercraft_automation::text::{Language as TextLanguage, TextError};
    use plannercraft_automation::{ApplyMode, Batch, Command, Placement, Session, TaskRef};
    use plannercraft_core::{LinkType, TaskUid};

    use super::{EditError, EditState, commit_edit, editable};
    use crate::column::Column;

    /// Session de l'exemple (UID n = ID n).
    fn session() -> Session {
        crate::document::load(crate::DEMO_MSPDI, "2027-01-15T08:00".parse().unwrap())
            .unwrap()
            .session
    }

    fn uid(n: u32) -> TaskUid {
        TaskUid::new(n)
    }

    fn typed(task: Option<TaskUid>, column: Column, text: &str) -> EditState {
        EditState::typed(task, column, text)
    }

    fn ops(state: &EditState) -> Vec<Command> {
        commit_edit(state, &session(), TextLanguage::Fr).unwrap().ops
    }

    #[test]
    fn edit_unchanged_text_sends_nothing() {
        let state = EditState::open(Some(uid(3)), Column::Duration, "10 jours".to_owned());
        let batch = commit_edit(&state, &session(), TextLanguage::Fr).unwrap();
        assert_eq!(batch.ops, Vec::<Command>::new());
        // Ligne vide laissée vide : rien non plus.
        assert_eq!(ops(&typed(None, Column::Name, "  ")), Vec::<Command>::new());
    }

    #[test]
    fn edit_invalid_duration_is_text_error() {
        let err = commit_edit(
            &typed(Some(uid(3)), Column::Duration, "abc"),
            &session(),
            TextLanguage::Fr,
        )
        .unwrap_err();
        assert!(
            matches!(err, EditError::Text(TextError::BadNumber { .. })),
            "{err:?}"
        );
        let err = commit_edit(
            &typed(Some(uid(3)), Column::Start, "31/02/2027"),
            &session(),
            TextLanguage::Fr,
        )
        .unwrap_err();
        assert_eq!(err, EditError::Text(TextError::BadDate));
    }

    #[test]
    fn edit_not_editable_columns_refused() {
        for column in [Column::Id, Column::TotalSlack, Column::Critical] {
            assert!(!editable(column));
            let err =
                commit_edit(&typed(Some(uid(3)), column, "1"), &session(), TextLanguage::Fr).unwrap_err();
            assert_eq!(err, EditError::NotEditable);
        }
    }

    /// Le lot porte la langue de l'interface : « 3d » est lu en anglais.
    #[test]
    fn edit_batch_carries_ui_language() {
        let state = typed(Some(uid(3)), Column::Duration, "3d");
        let batch = commit_edit(&state, &session(), TextLanguage::En).unwrap();
        assert_eq!(batch.lang, Some(TextLanguage::En));
        let mut s = session();
        s.apply(&batch, ApplyMode::Commit).unwrap();
        assert_eq!(
            s.project()
                .tasks
                .get(TaskUid::new(3))
                .unwrap()
                .duration
                .minutes()
                .get(),
            3 * 480
        );
    }

    /// Prédécesseurs : la tâche 9 a « 7;6 » ; « 6;8DD+1j » retire 7, garde
    /// 6, ajoute 8 en DD.
    #[test]
    fn edit_predecessors_diff() {
        let got = ops(&typed(Some(uid(9)), Column::Predecessors, "6;8DD+1j"));
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(
            got[0],
            Command::LinkRemove(LinkKey {
                pred: TaskRef::Uid(TaskUid::new(7)),
                succ: TaskRef::Uid(TaskUid::new(9)),
            })
        );
        let Command::LinkAdd(LinkSpec { pred, kind, lag, .. }) = &got[1] else {
            panic!("{got:?}");
        };
        assert_eq!(pred, &TaskRef::Uid(TaskUid::new(8)));
        assert_eq!(*kind, Some(LinkTypeJson::from(LinkType::StartToStart)));
        assert_eq!(lag.as_ref().and_then(|l| l.minutes), Some(480));
        // Même liste avec un type changé : `link.update`.
        let got = ops(&typed(Some(uid(9)), Column::Predecessors, "7FF;6"));
        assert!(matches!(&got[..], [Command::LinkUpdate(_)]), "{got:?}");
        // Liste vide : tout retirer.
        let got = ops(&typed(Some(uid(9)), Column::Predecessors, ""));
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|op| matches!(op, Command::LinkRemove(_))));
        // ID inconnu.
        let err = commit_edit(
            &typed(Some(uid(9)), Column::Predecessors, "7;900"),
            &session(),
            TextLanguage::Fr,
        )
        .unwrap_err();
        assert_eq!(err, EditError::UnknownId(900));
        let err = commit_edit(
            &typed(Some(uid(9)), Column::Predecessors, "7XY"),
            &session(),
            TextLanguage::Fr,
        )
        .unwrap_err();
        assert!(
            matches!(err, EditError::Text(TextError::BadLinkType { .. })),
            "{err:?}"
        );
    }

    #[test]
    fn edit_blank_row_creates_task() {
        let got = ops(&typed(None, Column::Name, "Réception"));
        let [Command::TaskAdd(add)] = &got[..] else {
            panic!("{got:?}")
        };
        assert_eq!(add.name, "Réception");
        assert_eq!(add.at, Some(Placement::End));
        let got = ops(&typed(None, Column::Duration, "3j"));
        let [Command::TaskAdd(add)] = &got[..] else {
            panic!("{got:?}")
        };
        assert_eq!(add.name, "");
        assert!(add.duration.is_some());
        // Prédécesseurs sur la ligne vide : la tâche créée est désignée par
        // son étiquette dans le même lot.
        let got = ops(&typed(None, Column::Predecessors, "3;4"));
        assert_eq!(got.len(), 3, "{got:?}");
        let mut s = session();
        let batch = Batch {
            format_version: 1,
            lang: None,
            ops: got,
            label: None,
        };
        s.apply(&batch, ApplyMode::Commit).unwrap();
        let last = s.project().tasks.iter().last().unwrap();
        assert_eq!(last.predecessors.len(), 2);
    }

    /// Lot L7 : Successeurs sur la ligne vide ⇒ tâche créée (étiquette) et
    /// liens vers les successeurs ; appliqué, la nouvelle tâche précède 3.
    #[test]
    fn l7_blank_row_successors_and_fields() {
        let got = ops(&typed(None, Column::Successors, "3"));
        assert!(
            matches!(&got[..], [Command::TaskAdd(_), Command::LinkAdd(_)]),
            "{got:?}"
        );
        let mut s = session();
        let batch = Batch {
            format_version: 1,
            lang: None,
            ops: got,
            label: None,
        };
        s.apply(&batch, ApplyMode::Commit).unwrap();
        let new = s.project().tasks.iter().last().unwrap().uid();
        let three = s.project().tasks.get(uid(3)).unwrap();
        assert!(three.predecessors.iter().any(|l| l.predecessor == new));
        // Autres champs sur la ligne vide : tâche créée puis modifiée par
        // son étiquette.
        let got = ops(&typed(None, Column::Notes, "À voir"));
        assert!(
            matches!(&got[..], [Command::TaskAdd(_), Command::TaskUpdate(_)]),
            "{got:?}"
        );
        let got = ops(&typed(None, Column::Deadline, "15/03/2027"));
        assert!(
            matches!(&got[..], [Command::TaskAdd(_), Command::TaskSetDeadline(_)]),
            "{got:?}"
        );
        // Champ calculé : refus.
        let err = commit_edit(
            &typed(None, Column::FreeSlack, "1j"),
            &session(),
            TextLanguage::Fr,
        )
        .unwrap_err();
        assert_eq!(err, EditError::Computed);
    }

    /// Lot L7 : champs d'avancement par `progress.update`, valeur identique
    /// sans opération ; travail sans unité lu en heures ; le texte affiché
    /// (« 16 heures ») se relit.
    #[test]
    fn l7_progress_and_work_fields() {
        for (column, text) in [
            (Column::PercentWorkComplete, "50"),
            (Column::ActualStart, "09/11/2026"),
            (Column::ActualDuration, "2j"),
            (Column::RemainingDuration, "3j"),
        ] {
            let got = ops(&typed(Some(uid(3)), column, text));
            assert!(
                matches!(&got[..], [Command::ProgressUpdate(_)]),
                "{column:?} {got:?}"
            );
        }
        assert_eq!(ops(&typed(Some(uid(3)), Column::PercentWorkComplete, "0")), []);
        assert_eq!(super::work_text(" 16 "), "16h");
        assert_eq!(super::work_text("2j"), "2j");
        let got = ops(&typed(Some(uid(3)), Column::Work, "16 heures"));
        assert!(matches!(&got[..], [Command::TaskUpdate(_)]), "{got:?}");
        // Mode, type et contrainte : préfixes.
        let read = |column, text: &str| {
            commit_edit(&typed(Some(uid(3)), column, text), &session(), TextLanguage::Fr)
        };
        assert!(read(Column::Mode, "auto").is_ok());
        assert!(read(Column::ConstraintType, "dès").is_ok());
        assert!(read(Column::TaskType, "capa").is_ok());
        // « Fin au plus tôt le » et « Fin au plus tard le » : ambigu.
        assert!(matches!(
            read(Column::ConstraintType, "fin au plus t"),
            Err(EditError::BadChoice(_))
        ));
        assert!(read(Column::ConstraintType, "fin au plus ta").is_ok());
    }

    /// R-260 a : un ID répété ne donne qu'un lien, la dernière occurrence
    /// donnant le type et le décalage (« 6;6 », « 6;6SS »).
    #[test]
    fn r260_duplicate_ids_merged() {
        // Ligne vide : « 6;6 » ⇒ une tâche, un seul `link.add`.
        let got = ops(&typed(None, Column::Predecessors, "6;6"));
        let adds: Vec<&LinkSpec> = got
            .iter()
            .filter_map(|op| match op {
                Command::LinkAdd(spec) => Some(spec),
                _ => None,
            })
            .collect();
        assert_eq!(adds.len(), 1, "{got:?}");
        assert_eq!(adds[0].pred, TaskRef::Uid(uid(6)));
        // Tâche 9 (« 7;6 ») : « 7;6;6 » ne change rien ; « 7;6;6SS » ⇒
        // un seul `link.update` de 6 en DD.
        assert_eq!(ops(&typed(Some(uid(9)), Column::Predecessors, "7;6;6")), []);
        let got = ops(&typed(Some(uid(9)), Column::Predecessors, "7;6;6DD"));
        let [Command::LinkUpdate(spec)] = &got[..] else {
            panic!("{got:?}")
        };
        assert_eq!(spec.pred, TaskRef::Uid(uid(6)));
        assert_eq!(spec.kind, Some(LinkTypeJson::from(LinkType::StartToStart)));
        // Appliqué : un lien 6 → 9, en DD, une entrée.
        let mut s = session();
        let batch = commit_edit(
            &typed(Some(uid(9)), Column::Predecessors, "6;6DD"),
            &s,
            TextLanguage::Fr,
        )
        .unwrap();
        s.apply(&batch, ApplyMode::Commit).unwrap();
        let links = &s.project().tasks.get(uid(9)).unwrap().predecessors;
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].kind, LinkType::StartToStart);
        assert_eq!(s.history().entries.len(), 1);
    }

    /// R-260 d : une valeur relue égale à la valeur courante ne produit
    /// aucune opération (durée, nom, %, date déjà contrainte, liens).
    #[test]
    fn r260_same_value_no_entry() {
        let s = session();
        let task = s.project().tasks.get(uid(3)).unwrap();
        let name = task.name.clone();
        assert_eq!(task.duration.minutes().get(), 10 * 480);
        for (column, text) in [
            (Column::Duration, "10 jours"),
            (Column::Duration, "10j"),
            (Column::Name, name.as_str()),
            (Column::PercentComplete, "0"),
            (Column::PercentComplete, "0%"),
        ] {
            assert_eq!(ops(&typed(Some(uid(3)), column, text)), [], "{column:?} {text}");
        }
        assert_eq!(ops(&typed(Some(uid(9)), Column::Predecessors, "6;7")), []);
        // Début saisi : la première fois pose la contrainte, la seconde ne
        // change rien.
        let mut s = session();
        let state = typed(Some(uid(3)), Column::Start, "15/03/2027");
        let batch = commit_edit(&state, &s, TextLanguage::Fr).unwrap();
        assert_eq!(batch.ops.len(), 1);
        s.apply(&batch, ApplyMode::Commit).unwrap();
        let again = commit_edit(&state, &s, TextLanguage::Fr).unwrap();
        assert_eq!(again.ops, []);
        // Une autre valeur passe toujours.
        assert_eq!(ops(&typed(Some(uid(3)), Column::Duration, "11j")).len(), 1);
    }
}
