//! Brouillon du « Formulaire tâche » (seconde passe UX, ADR-0019 D6,
//! H-UI-113), partagé par la fenêtre « Informations sur la tâche » (J7,
//! H-UI-19, `dialog::task_info`) et le volet de détails (`view::details`).
//!
//! Le brouillon ([`TaskFormDraft`]) est fait de textes : il est lu par
//! [`TaskFormDraft::from_session`] (écritures R-141, R-142, R-143 du moteur,
//! toujours relisibles) et traduit en **un** lot par
//! [`TaskFormDraft::to_batch`] (fenêtre) ou [`TaskFormDraft::to_pane_batch`]
//! (volet, libellé « Formulaire tâche »), fonctions pures testées sans egui.
//! Aucune date n'est calculée ici (règle 3) : les textes sont lus par
//! `plannercraft_automation::text`, le registre applique et recalcule. Seule la
//! résolution ID → UID des liens se fait côté interface (R-150), comme
//! dans la table.
//!
//! Comparaison **par texte** (R-260 d) : un champ dont le texte n'a pas
//! changé depuis la lecture n'envoie rien (une date relue ne pose pas de
//! contrainte par R-158, un décalage nul relu ne change pas d'unité, un
//! lien réécrit à l'identique n'est ni ajouté ni refusé). Ordre du lot :
//! `task.update`, liens des prédécesseurs (`link.add`, `link.update`,
//! `link.remove`), liens des successeurs (même ordre), `assignment.add`,
//! `assignment.update`, `assignment.remove`, `task.set_constraint`,
//! `task.set_deadline`, `progress.update`.

use std::collections::BTreeMap;

use plannercraft_automation::command::{
    AssignmentAdd, AssignmentKey, AssignmentUpdate, ConstraintKindJson, DurationJson, LagJson, LinkKey,
    LinkSpec, LinkTypeJson, ModeJson, PercentJson, ProgressUpdate, ResourceRef, SetConstraint, SetDeadline,
    TaskTypeJson, TaskUpdate, UnitsJson,
};
use plannercraft_automation::text::{
    DateRole, Language, TextContext, TextError, format_date, format_duration, format_lag, parse_date,
    parse_duration, parse_lag, parse_percent, write_decimal,
};
use plannercraft_automation::{Batch, Command, CommandError, Patch, Session, TaskRef};
use plannercraft_core::{
    ConstraintKind, Decimal, LinkType, Project, ResourceKind, ResourceUid, SchedulingMode, TaskOutcome,
    TaskRole, TaskType, TaskUid,
};

use crate::edit::EditError;
use crate::i18n::{self, Arg, TextId, Texts};

/// Onglet de la fenêtre « Informations sur la tâche » (section d'un champ
/// du brouillon ; le volet de détails place ses refus par champ).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tab {
    /// Nom, durée, % achevé, début, fin, active.
    #[default]
    General,
    /// Liens vers les prédécesseurs (S67) ; les successeurs du volet aussi.
    Predecessors,
    /// Affectations.
    Resources,
    /// Contrainte, échéance, type de tâche, pilotage par l'effort (S66).
    Advanced,
    /// Notes.
    Notes,
}

impl Tab {
    /// Onglets, dans l'ordre d'affichage.
    pub const ALL: [Self; 5] = [
        Self::General,
        Self::Predecessors,
        Self::Resources,
        Self::Advanced,
        Self::Notes,
    ];

    /// Titre de l'onglet.
    #[must_use]
    pub const fn title(self) -> TextId {
        match self {
            Self::General => TextId::TaskInfoTabGeneral,
            Self::Predecessors => TextId::TaskInfoTabPredecessors,
            Self::Resources => TextId::TaskInfoTabResources,
            Self::Advanced => TextId::TaskInfoTabAdvanced,
            Self::Notes => TextId::TaskInfoTabNotes,
        }
    }
}

/// Onglet Général (textes saisis).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct General {
    /// Nom.
    pub name: String,
    /// Durée (R-140, R-141).
    pub duration: String,
    /// % achevé (« 50% »).
    pub percent: String,
    /// Début (R-143) ; vide pour une tâche sans dates.
    pub start: String,
    /// Fin (R-143).
    pub finish: String,
    /// Tâche active.
    pub active: bool,
    /// Planifiée manuellement (R-253 ; case du volet de détails).
    pub manual: bool,
}

/// Ligne d'une grille de liens : prédécesseurs (onglet de la fenêtre,
/// grille du volet) ou successeurs (grille du volet).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRow {
    /// ID de l'autre tâche du lien (position + 1, R-150) : le prédécesseur
    /// ou le successeur ; une ligne dont l'ID est vide est ignorée (ID
    /// vidé : lien retiré).
    pub id: String,
    /// Type de lien.
    pub kind: LinkType,
    /// Décalage (R-141, « +2j », « -50% ») ; vide = aucun.
    pub lag: String,
}

impl LinkRow {
    /// Ligne neuve (bouton « Ajouter », ligne vide d'une grille) : ID vide,
    /// FD, sans décalage.
    #[must_use]
    pub const fn blank() -> Self {
        Self {
            id: String::new(),
            kind: LinkType::FinishToStart,
            lag: String::new(),
        }
    }
}

/// Ligne des affectations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssignRow {
    /// Ressource affectée.
    pub resource: ResourceUid,
    /// Unités (« 50% ») pour une ressource de travail, quantité (« 5 »)
    /// pour une ressource matérielle, ignorées pour une ressource coût.
    pub units: String,
    /// Travail de l'affectation (« 16h », ressource de travail ; vide
    /// sinon ou laissé au registre).
    pub work: String,
}

/// Onglet Avancé.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Advanced {
    /// Type de contrainte.
    pub constraint: ConstraintKind,
    /// Date de contrainte (types datés seulement).
    pub constraint_date: String,
    /// Échéance ; vide = aucune.
    pub deadline: String,
    /// Type de tâche.
    pub task_type: TaskType,
    /// Pilotée par l'effort.
    pub effort_driven: bool,
}

/// Brouillon d'une tâche, lu sur la session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskFormDraft {
    /// Tâche.
    pub uid: TaskUid,
    /// `cache_key` du document à la lecture (numéro d'ouverture,
    /// génération) : un « OK » sur un document changé depuis est relu.
    pub key: (u64, u64),
    /// Récapitulative (lu, non éditable) : durée, %, dates, type de tâche,
    /// pilotage et mode grisés.
    pub summary: bool,
    /// Onglet Général.
    pub general: General,
    /// Prédécesseurs, dans l'ordre du modèle (liens pendants omis).
    pub links: Vec<LinkRow>,
    /// Successeurs, dans l'ordre des ID (volet de détails).
    pub successors: Vec<LinkRow>,
    /// Affectations, dans l'ordre du projet (sans ressource omises).
    pub assignments: Vec<AssignRow>,
    /// Onglet Avancé.
    pub advanced: Advanced,
    /// Notes.
    pub notes: String,
}

/// Refus d'un champ, affiché sous lui.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldError {
    /// Onglet du champ.
    pub tab: Tab,
    /// Champ (`name`, `duration`, `percent`, `start`, `finish`,
    /// `predecessor`, `lag`, `successor`, `successor_lag`, `resource`,
    /// `units`, `work`, `constraint_date`, `deadline`, `task_type`,
    /// `effort_driven`, `notes`, `active`) ; vide si le refus ne désigne
    /// aucun champ connu.
    pub field: &'static str,
    /// Ligne d'une grille, s'il y a lieu.
    pub row: Option<usize>,
    /// Message traduit.
    pub message: String,
}

/// Côté d'une grille de liens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkSide {
    /// Prédécesseurs de la tâche (elle est le successeur du lien).
    Predecessors,
    /// Successeurs de la tâche (elle est le prédécesseur du lien).
    Successors,
}

impl LinkSide {
    /// Champs de l'ID et du décalage dans [`FieldError`].
    const fn fields(self) -> (&'static str, &'static str) {
        match self {
            Self::Predecessors => ("predecessor", "lag"),
            Self::Successors => ("successor", "successor_lag"),
        }
    }
}

/// Appelant du lot : la fenêtre (messages de la table, sans libellé) ou
/// le volet (« Il n'y a pas de tâche 99. », libellé « Formulaire tâche »).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Caller {
    Dialog,
    Pane,
}

/// Langue de l'interface pour une langue des textes du moteur.
const fn ui_language(lang: Language) -> i18n::Language {
    match lang {
        Language::Fr => i18n::Language::Fr,
        Language::En => i18n::Language::En,
    }
}

/// Pourcentage en centièmes de % (« 5000 » → « 50% »).
fn percent_text(hundredths: i64, ctx: &TextContext) -> String {
    let mut out = String::new();
    let value = Decimal::new(hundredths, 2).unwrap_or(Decimal::integer(0));
    write_decimal(&mut out, value, decimal_sep(ctx));
    out.push('%');
    out
}

/// Séparateur décimal du contexte.
const fn decimal_sep(ctx: &TextContext) -> char {
    match ctx.decimal {
        plannercraft_automation::text::DecimalMark::Comma => ',',
        plannercraft_automation::text::DecimalMark::Dot => '.',
    }
}

/// Texte des unités d'une affectation selon le type de la ressource :
/// pourcentage (travail), quantité (matériel), vide (coût).
pub(crate) fn units_text(kind: Option<&ResourceKind>, units: i64, ctx: &TextContext) -> String {
    match kind {
        Some(ResourceKind::Material { .. }) => {
            let mut out = String::new();
            let value = Decimal::new(units, 4).unwrap_or(Decimal::integer(0));
            write_decimal(&mut out, value, decimal_sep(ctx));
            out
        }
        Some(ResourceKind::Cost) => String::new(),
        Some(ResourceKind::Work) | None => percent_text(units, ctx),
    }
}

/// Rôle de lecture d'une date de contrainte : début pour les contraintes
/// de début, fin pour les contraintes de fin (R-143).
const fn constraint_role(kind: ConstraintKind) -> DateRole {
    match kind {
        ConstraintKind::MustFinishOn
        | ConstraintKind::FinishNoEarlierThan
        | ConstraintKind::FinishNoLaterThan => DateRole::Finish,
        _ => DateRole::Start,
    }
}

/// Ligne d'un lien lu : ID de l'autre tâche, type, décalage (vide s'il est
/// nul) ; `None` pour un lien pendant.
fn link_row(
    s: &Session,
    other: TaskUid,
    kind: LinkType,
    lag: &plannercraft_core::Lag,
    ctx: &TextContext,
) -> Option<LinkRow> {
    let id = s.project().task_id(other)?;
    Some(LinkRow {
        id: id.get().to_string(),
        kind,
        lag: if is_zero_lag(lag) {
            String::new()
        } else {
            format_lag(lag, ctx)
        },
    })
}

/// Lien existant entre `pred` et `succ`, s'il y en a un.
fn has_link(s: &Session, pred: TaskUid, succ: TaskUid) -> bool {
    s.project()
        .tasks
        .get(succ)
        .is_some_and(|t| t.predecessors.iter().any(|l| l.predecessor == pred))
}

impl TaskFormDraft {
    /// Brouillon de la tâche `uid` : textes écrits dans la langue `lang`
    /// (celle de l'interface) ; `None` si la tâche n'existe pas ou est une
    /// ligne vide MSPDI.
    #[must_use]
    pub fn from_session(s: &Session, uid: TaskUid, key: (u64, u64), lang: Language) -> Option<Self> {
        let project = s.project();
        let task = project.tasks.get(uid)?;
        if task.is_null {
            return None;
        }
        let ctx = TextContext::for_project(project, lang);
        let outcome = s.schedule().tasks.get(&uid);
        let (dates, summary) = match outcome {
            Some(TaskOutcome::Scheduled(sched)) => {
                (Some((sched.start, sched.finish)), sched.role == TaskRole::Summary)
            }
            Some(TaskOutcome::Excluded { display, .. }) => (display.map(|d| (d.start, d.finish)), false),
            None => (None, false),
        };
        let percent = match outcome {
            Some(TaskOutcome::Scheduled(sched)) => sched
                .progress
                .map_or(task.progress.percent_complete, |p| p.percent_complete),
            _ => task.progress.percent_complete,
        };
        let general = General {
            name: task.name.clone(),
            duration: format_duration(&task.duration, &ctx),
            percent: percent_text(percent.get(), &ctx),
            start: dates
                .map(|(start, _)| format_date(start, &ctx))
                .unwrap_or_default(),
            finish: dates
                .map(|(_, finish)| format_date(finish, &ctx))
                .unwrap_or_default(),
            active: task.active,
            manual: matches!(task.mode, SchedulingMode::Manual { .. }),
        };
        let links = task
            .predecessors
            .iter()
            .filter_map(|link| link_row(s, link.predecessor, link.kind, &link.lag, &ctx))
            .collect();
        // Successeurs : liens des autres tâches vers celle-ci, dans l'ordre
        // des ID (un parcours des tâches, à chaque lecture du brouillon).
        let successors = project
            .tasks
            .iter()
            .filter(|t| !t.is_null)
            .flat_map(|t| {
                t.predecessors
                    .iter()
                    .filter(|link| link.predecessor == uid)
                    .map(move |link| (t.uid(), link))
            })
            .filter_map(|(succ, link)| link_row(s, succ, link.kind, &link.lag, &ctx))
            .collect();
        let assignments = project
            .assignments
            .iter()
            .filter(|a| a.task == uid)
            .filter_map(|a| {
                let resource = a.resource?;
                let kind = project.resources.get(resource).map(|r| &r.kind);
                let work = match kind {
                    Some(ResourceKind::Work) => plannercraft_automation::view::work_out(a.work, &ctx)
                        .text
                        .unwrap_or_default(),
                    _ => String::new(),
                };
                Some(AssignRow {
                    resource,
                    units: units_text(kind, a.units.get(), &ctx),
                    work,
                })
            })
            .collect();
        let constraint = &task.constraint;
        let advanced = Advanced {
            constraint: constraint.kind(),
            constraint_date: constraint
                .date()
                .map(|d| format_date(d, &ctx))
                .unwrap_or_default(),
            deadline: task.deadline.map(|d| format_date(d, &ctx)).unwrap_or_default(),
            task_type: task.task_type,
            effort_driven: task.effort_driven,
        };
        Some(Self {
            uid,
            key,
            summary,
            general,
            links,
            successors,
            assignments,
            advanced,
            notes: task.notes.clone(),
        })
    }

    /// Lot de commandes de la fenêtre (fonction pure) : seuls les champs
    /// dont le texte a changé depuis la lecture (brouillon relu sur `s`)
    /// sont envoyés, dans l'ordre du module. Rien de changé (ou tâche
    /// disparue) ⇒ `Ok(None)`. Le lot porte la langue `lang` (celle des
    /// textes).
    ///
    /// # Errors
    ///
    /// Tous les champs illisibles (texte, ID inconnu, doublon, date de
    /// contrainte manquante), chacun avec son onglet.
    pub fn to_batch(&self, s: &Session, lang: Language) -> Result<Option<Batch>, Vec<FieldError>> {
        self.batch(s, lang, Caller::Dialog)
    }

    /// Comme [`Self::to_batch`], pour le volet de détails (H-UI-113) : lot
    /// libellé « Formulaire tâche », ID inconnu refusé par « Il n'y a pas
    /// de tâche 99. ».
    ///
    /// # Errors
    ///
    /// Comme [`Self::to_batch`].
    pub fn to_pane_batch(&self, s: &Session, lang: Language) -> Result<Option<Batch>, Vec<FieldError>> {
        self.batch(s, lang, Caller::Pane)
    }

    /// Lot du brouillon pour `caller`.
    fn batch(&self, s: &Session, lang: Language, caller: Caller) -> Result<Option<Batch>, Vec<FieldError>> {
        let Some(old) = Self::from_session(s, self.uid, self.key, lang) else {
            return Ok(None);
        };
        let project = s.project();
        let ctx = TextContext::for_project(project, lang);
        let texts = Texts::new(ui_language(lang));
        let mut errors = Vec::new();
        let mut ops = Vec::new();

        if let Some(update) = self.task_update(&old, &ctx, &texts, &mut errors) {
            ops.push(Command::TaskUpdate(update));
        }
        let env = Env {
            s,
            ctx: &ctx,
            texts: &texts,
            caller,
        };
        for side in [LinkSide::Predecessors, LinkSide::Successors] {
            self.link_ops(&old, side, &env, &mut ops, &mut errors);
        }
        self.assignment_ops(&old, &env, &mut ops, &mut errors);
        self.advanced_ops(&old, &ctx, &texts, &mut ops, &mut errors);

        // progress.update (dernier : l'avancement voit la durée nouvelle).
        let g = &self.general;
        if g.percent != old.general.percent {
            match parse_percent(&g.percent, &ctx) {
                Ok(_) => ops.push(Command::ProgressUpdate(percent_update(self.uid, &g.percent))),
                Err(e) => errors.push(text_error(&texts, Tab::General, "percent", None, e)),
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }
        if ops.is_empty() {
            return Ok(None);
        }
        Ok(Some(Batch {
            format_version: Batch::FORMAT_VERSION,
            lang: Some(lang),
            ops,
            label: (caller == Caller::Pane).then(|| texts.get(TextId::DetailsBatchLabel)),
        }))
    }

    /// `task.update` : Général, mode, type et pilotage (Avancé), notes ;
    /// `None` si rien n'a changé.
    fn task_update(
        &self,
        old: &Self,
        ctx: &TextContext,
        texts: &Texts,
        errors: &mut Vec<FieldError>,
    ) -> Option<TaskUpdate> {
        let uid = self.uid;
        let mut update = empty_update(uid);
        let g = &self.general;
        if g.name != old.general.name {
            update.name = Patch::Set(g.name.clone());
        }
        if g.duration != old.general.duration {
            match parse_duration(&g.duration, ctx) {
                Ok(_) => {
                    update.duration = Patch::Set(DurationJson {
                        text: Some(g.duration.clone()),
                        ..DurationJson::default()
                    });
                }
                Err(e) => errors.push(text_error(texts, Tab::General, "duration", None, e)),
            }
        }
        for (field, text, old_text, role) in [
            ("start", &g.start, &old.general.start, DateRole::Start),
            ("finish", &g.finish, &old.general.finish, DateRole::Finish),
        ] {
            if text == old_text {
                continue;
            }
            match parse_date(text, role, ctx) {
                Ok(m) if field == "start" => update.start = Patch::Set(m),
                Ok(m) => update.finish = Patch::Set(m),
                Err(e) => errors.push(text_error(texts, Tab::General, field, None, e)),
            }
        }
        if g.active != old.general.active {
            update.active = Patch::Set(g.active);
        }
        if g.manual != old.general.manual {
            update.mode = Patch::Set(if g.manual {
                ModeJson::Manual
            } else {
                ModeJson::Auto
            });
        }
        let a = &self.advanced;
        if a.task_type != old.advanced.task_type {
            update.task_type = Patch::Set(TaskTypeJson::from(a.task_type));
        }
        if a.effort_driven != old.advanced.effort_driven {
            update.effort_driven = Patch::Set(a.effort_driven);
        }
        if self.notes != old.notes {
            update.notes = Patch::Set(self.notes.clone());
        }
        (update != empty_update(uid)).then_some(update)
    }

    /// `task.set_constraint` et `task.set_deadline` de l'onglet Avancé.
    fn advanced_ops(
        &self,
        old: &Self,
        ctx: &TextContext,
        texts: &Texts,
        ops: &mut Vec<Command>,
        errors: &mut Vec<FieldError>,
    ) {
        let a = &self.advanced;
        let uid = self.uid;
        // task.set_constraint.
        let constraint_changed = a.constraint != old.advanced.constraint
            || (a.constraint.is_dated() && a.constraint_date != old.advanced.constraint_date);
        if constraint_changed {
            let date = if a.constraint.is_dated() {
                if a.constraint_date.trim().is_empty() {
                    errors.push(FieldError {
                        tab: Tab::Advanced,
                        field: "constraint_date",
                        row: None,
                        message: texts.get(TextId::TaskInfoDateRequired),
                    });
                    None
                } else {
                    match parse_date(&a.constraint_date, constraint_role(a.constraint), ctx) {
                        Ok(m) => Some(m),
                        Err(e) => {
                            errors.push(text_error(texts, Tab::Advanced, "constraint_date", None, e));
                            None
                        }
                    }
                }
            } else {
                None
            };
            ops.push(Command::TaskSetConstraint(SetConstraint {
                uid: TaskRef::Uid(uid),
                kind: ConstraintKindJson::from(a.constraint),
                date,
            }));
        }

        // task.set_deadline.
        if a.deadline != old.advanced.deadline {
            let deadline = if a.deadline.trim().is_empty() {
                Some(Patch::Clear)
            } else {
                match parse_date(&a.deadline, DateRole::Finish, ctx) {
                    Ok(m) => Some(Patch::Set(m)),
                    Err(e) => {
                        errors.push(text_error(texts, Tab::Advanced, "deadline", None, e));
                        None
                    }
                }
            };
            if let Some(deadline) = deadline {
                ops.push(Command::TaskSetDeadline(SetDeadline {
                    uid: TaskRef::Uid(uid),
                    deadline,
                }));
            }
        }
    }

    /// `link.add`, `link.update`, `link.remove` d'une grille de liens
    /// (prédécesseurs : la tâche est le successeur du lien ; successeurs :
    /// elle en est le prédécesseur).
    fn link_ops(
        &self,
        old: &Self,
        side: LinkSide,
        env: &Env<'_>,
        ops: &mut Vec<Command>,
        errors: &mut Vec<FieldError>,
    ) {
        let Env {
            s,
            ctx,
            texts,
            caller,
        } = *env;
        if !s.project().tasks.contains(self.uid) {
            return;
        }
        let (rows, old_rows) = match side {
            LinkSide::Predecessors => (&self.links, &old.links),
            LinkSide::Successors => (&self.successors, &old.successors),
        };
        let (id_field, lag_field) = side.fields();
        // Paire (prédécesseur, successeur) du lien vers l'autre tâche.
        let pair = |other: TaskUid| match side {
            LinkSide::Predecessors => (other, self.uid),
            LinkSide::Successors => (self.uid, other),
        };
        // Lignes lues par UID de l'autre tâche.
        let old_rows: BTreeMap<TaskUid, &LinkRow> = old_rows
            .iter()
            .filter_map(|row| resolve_id(&row.id, s).map(|uid| (uid, row)))
            .collect();
        let mut seen: BTreeMap<TaskUid, usize> = BTreeMap::new();
        let (mut adds, mut updates) = (Vec::new(), Vec::new());
        for (i, row) in rows.iter().enumerate() {
            let id_text = row.id.trim();
            if id_text.is_empty() {
                continue;
            }
            let Some(other) = resolve_id(id_text, s) else {
                errors.push(FieldError {
                    tab: Tab::Predecessors,
                    field: id_field,
                    row: Some(i),
                    message: unknown_id_message(id_text, texts, caller),
                });
                continue;
            };
            if seen.insert(other, i).is_some() {
                errors.push(FieldError {
                    tab: Tab::Predecessors,
                    field: id_field,
                    row: Some(i),
                    message: texts.get_args(
                        TextId::TaskInfoDuplicatePredecessor,
                        &[("id", Arg::Str(id_text.to_owned().into()))],
                    ),
                });
                continue;
            }
            let previous = old_rows.get(&other).copied();
            let lag = match lag_change(row, previous, ctx) {
                Ok(lag) => lag,
                Err(e) => {
                    errors.push(text_error(texts, Tab::Predecessors, lag_field, Some(i), e));
                    continue;
                }
            };
            let (pred, succ) = pair(other);
            let spec = |kind: Option<LinkTypeJson>, lag: Option<LagJson>| LinkSpec {
                pred: TaskRef::Uid(pred),
                succ: TaskRef::Uid(succ),
                kind,
                lag,
            };
            match previous {
                None => adds.push(Command::LinkAdd(spec(Some(LinkTypeJson::from(row.kind)), lag))),
                Some(p) => {
                    let kind = (p.kind != row.kind).then(|| LinkTypeJson::from(row.kind));
                    if kind.is_some() || lag.is_some() {
                        updates.push(Command::LinkUpdate(spec(kind, lag)));
                    }
                }
            }
        }
        ops.extend(adds);
        ops.extend(updates);
        for other in old_rows.keys() {
            let (pred, succ) = pair(*other);
            if !seen.contains_key(other) && has_link(s, pred, succ) {
                ops.push(Command::LinkRemove(LinkKey {
                    pred: TaskRef::Uid(pred),
                    succ: TaskRef::Uid(succ),
                }));
            }
        }
    }

    /// `assignment.add`, `assignment.update`, `assignment.remove` des
    /// affectations.
    fn assignment_ops(
        &self,
        old: &Self,
        env: &Env<'_>,
        ops: &mut Vec<Command>,
        errors: &mut Vec<FieldError>,
    ) {
        let Env { s, ctx, texts, .. } = *env;
        let project = s.project();
        let old_rows: BTreeMap<ResourceUid, &AssignRow> =
            old.assignments.iter().map(|row| (row.resource, row)).collect();
        let mut seen: BTreeMap<ResourceUid, usize> = BTreeMap::new();
        let (mut adds, mut updates) = (Vec::new(), Vec::new());
        for (i, row) in self.assignments.iter().enumerate() {
            let Some(resource) = project.resources.get(row.resource) else {
                errors.push(FieldError {
                    tab: Tab::Resources,
                    field: "resource",
                    row: Some(i),
                    message: texts.get(TextId::TaskInfoUnknownResource),
                });
                continue;
            };
            if seen.insert(row.resource, i).is_some() {
                errors.push(FieldError {
                    tab: Tab::Resources,
                    field: "resource",
                    row: Some(i),
                    message: texts.get_args(
                        TextId::TaskInfoDuplicateResource,
                        &[("name", Arg::Str(resource.name.clone().into()))],
                    ),
                });
                continue;
            }
            let previous = old_rows.get(&row.resource);
            // Unités (travail) ou quantité (matériel), relues pour signaler
            // l'erreur sur la ligne ; vides : valeur par défaut du registre.
            let units_changed = previous.is_none_or(|p| p.units.trim() != row.units.trim());
            let (units, quantity) = match units_change(&resource.kind, &row.units, units_changed, ctx) {
                Ok(amounts) => amounts,
                Err(e) => {
                    errors.push(command_error(texts, Tab::Resources, "units", Some(i), &e, ctx));
                    continue;
                }
            };
            // Travail (ressource de travail seulement), lu comme une durée.
            let work_changed = previous.is_none_or(|p| p.work.trim() != row.work.trim());
            let work = if work_changed && matches!(resource.kind, ResourceKind::Work) {
                match work_change(&row.work, ctx) {
                    Ok(work) => work,
                    Err(e) => {
                        errors.push(text_error(texts, Tab::Resources, "work", Some(i), e));
                        continue;
                    }
                }
            } else {
                None
            };
            match previous {
                None => adds.push(Command::AssignmentAdd(AssignmentAdd {
                    task: TaskRef::Uid(self.uid),
                    resource: ResourceRef::Uid(row.resource),
                    units,
                    work,
                    quantity,
                    delay: None,
                })),
                Some(_) if units.is_some() || quantity.is_some() || work.is_some() => {
                    updates.push(Command::AssignmentUpdate(AssignmentUpdate {
                        task: TaskRef::Uid(self.uid),
                        resource: ResourceRef::Uid(row.resource),
                        units: units.map_or(Patch::Keep, Patch::Set),
                        work: work.map_or(Patch::Keep, Patch::Set),
                        quantity: quantity.map_or(Patch::Keep, Patch::Set),
                        delay: Patch::Keep,
                    }));
                }
                Some(_) => {}
            }
        }
        ops.extend(adds);
        ops.extend(updates);
        for resource in old_rows.keys() {
            if !seen.contains_key(resource) {
                ops.push(Command::AssignmentRemove(AssignmentKey {
                    task: TaskRef::Uid(self.uid),
                    resource: ResourceRef::Uid(*resource),
                }));
            }
        }
    }

    /// Brouillon gardé sur un document changé ailleurs (H-UI-113) : chaque
    /// champ que l'utilisateur n'a pas touché (`self` égal à `base`, la
    /// lecture d'origine) prend la valeur relue `fresh` ; les champs
    /// modifiés sont gardés. Les grilles se reprennent en entier (une grille
    /// touchée est gardée telle quelle). Clé et caractère récapitulatif :
    /// ceux de `fresh`.
    #[must_use]
    pub fn rebase(&self, base: &Self, fresh: &Self) -> Self {
        let g = &self.general;
        let (bg, fg) = (&base.general, &fresh.general);
        let a = &self.advanced;
        let (ba, fa) = (&base.advanced, &fresh.advanced);
        Self {
            uid: self.uid,
            key: fresh.key,
            summary: fresh.summary,
            general: General {
                name: pick(&g.name, &bg.name, &fg.name),
                duration: pick(&g.duration, &bg.duration, &fg.duration),
                percent: pick(&g.percent, &bg.percent, &fg.percent),
                start: pick(&g.start, &bg.start, &fg.start),
                finish: pick(&g.finish, &bg.finish, &fg.finish),
                active: pick(&g.active, &bg.active, &fg.active),
                manual: pick(&g.manual, &bg.manual, &fg.manual),
            },
            links: pick(&self.links, &base.links, &fresh.links),
            successors: pick(&self.successors, &base.successors, &fresh.successors),
            assignments: pick(&self.assignments, &base.assignments, &fresh.assignments),
            advanced: Advanced {
                constraint: pick(&a.constraint, &ba.constraint, &fa.constraint),
                constraint_date: pick(&a.constraint_date, &ba.constraint_date, &fa.constraint_date),
                deadline: pick(&a.deadline, &ba.deadline, &fa.deadline),
                task_type: pick(&a.task_type, &ba.task_type, &fa.task_type),
                effort_driven: pick(&a.effort_driven, &ba.effort_driven, &fa.effort_driven),
            },
            notes: pick(&self.notes, &base.notes, &fresh.notes),
        }
    }
}

/// Refus d'un ID de lien illisible ou inconnu : message de la table
/// (fenêtre) ou « Il n'y a pas de tâche 99. » (volet).
fn unknown_id_message(id_text: &str, texts: &Texts, caller: Caller) -> String {
    match (id_text.parse::<u32>(), caller) {
        (Err(_), _) => texts.get_args(
            TextId::TaskInfoBadId,
            &[("id", Arg::Str(id_text.to_owned().into()))],
        ),
        (Ok(id), Caller::Dialog) => EditError::UnknownId(id).message(texts),
        (Ok(id), Caller::Pane) => {
            texts.get_args(TextId::DetailsUnknownTask, &[("id", Arg::Count(i64::from(id)))])
        }
    }
}

/// Décalage à envoyer pour la ligne `row` (lue `previous`) : `None` s'il
/// n'a pas changé ; zéro explicite s'il a été effacé.
fn lag_change(
    row: &LinkRow,
    previous: Option<&LinkRow>,
    ctx: &TextContext,
) -> Result<Option<LagJson>, TextError> {
    let changed = previous.is_none_or(|p| p.lag.trim() != row.lag.trim());
    let lag = if changed && !row.lag.trim().is_empty() {
        parse_lag(&row.lag, ctx)?
    } else if changed && previous.is_some() {
        plannercraft_core::Lag::ZERO
    } else {
        return Ok(None);
    };
    Ok(Some(LagJson {
        text: None,
        ..LagJson::from_lag(&lag, ctx)
    }))
}

/// Unités (ressource de travail) ou quantité (matérielle) d'une
/// affectation, relues pour signaler l'erreur ; texte inchangé ou vide :
/// rien.
fn units_change(
    kind: &ResourceKind,
    text: &str,
    changed: bool,
    ctx: &TextContext,
) -> Result<(Option<UnitsJson>, Option<UnitsJson>), CommandError> {
    if !changed || text.trim().is_empty() {
        return Ok((None, None));
    }
    match kind {
        ResourceKind::Work => {
            let json = units_json(text, true);
            json.to_units(ctx).map(|_| (Some(json), None))
        }
        ResourceKind::Material { .. } => {
            let json = units_json(text, false);
            json.to_quantity(ctx).map(|_| (None, Some(json)))
        }
        ResourceKind::Cost => Ok((None, None)),
    }
}

/// Travail d'une affectation (texte lu comme une durée) ; vide : rien.
fn work_change(text: &str, ctx: &TextContext) -> Result<Option<DurationJson>, TextError> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    parse_duration(text, ctx)?;
    Ok(Some(DurationJson {
        text: Some(text.trim().to_owned()),
        ..DurationJson::default()
    }))
}

/// Valeur fusionnée d'un champ : la saisie si elle diffère de la lecture
/// d'origine, sinon la valeur relue.
fn pick<T: Clone + PartialEq>(mine: &T, base: &T, fresh: &T) -> T {
    if mine == base { fresh.clone() } else { mine.clone() }
}

/// Contexte de lecture partagé par les grilles.
#[derive(Clone, Copy)]
struct Env<'a> {
    s: &'a Session,
    ctx: &'a TextContext,
    texts: &'a Texts,
    caller: Caller,
}

/// Décalage nul (temps ou pourcentage), quelle que soit son unité.
fn is_zero_lag(lag: &plannercraft_core::Lag) -> bool {
    match lag.value {
        plannercraft_core::LagValue::Time { minutes, .. } => minutes == plannercraft_core::Minutes::ZERO,
        plannercraft_core::LagValue::Percent { percent } => percent.get() == 0,
    }
}

/// R-150 côté interface : ID (position + 1, texte) → UID.
fn resolve_id(text: &str, s: &Session) -> Option<TaskUid> {
    let id = text.trim().parse::<usize>().ok()?;
    let task = s.project().tasks.at(id.checked_sub(1)?)?;
    (!task.is_null).then(|| task.uid())
}

/// Nom de la tâche d'ID `text` (colonne « Nom » lue des grilles de liens
/// du volet) ; `None` si l'ID est illisible ou inconnu.
#[must_use]
pub fn task_name_for_id<'a>(text: &str, s: &'a Session) -> Option<&'a str> {
    let uid = resolve_id(text, s)?;
    s.project().tasks.get(uid).map(|t| t.name.as_str())
}

/// Unités en texte : `%` ajouté pour une ressource de travail s'il manque
/// (« 50 » vaut 50 %, comme la colonne % achevé).
pub(crate) fn units_json(text: &str, percent: bool) -> UnitsJson {
    let trimmed = text.trim();
    let text = if percent && !trimmed.ends_with('%') {
        format!("{trimmed}%")
    } else {
        trimmed.to_owned()
    };
    UnitsJson {
        value: None,
        text: Some(text),
    }
}

/// `task.update` vide sur `uid`.
const fn empty_update(uid: TaskUid) -> TaskUpdate {
    TaskUpdate {
        uid: TaskRef::Uid(uid),
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

/// `progress.update { percent_complete: { text } }`, `%` ajouté s'il
/// manque (le registre l'exige ; « 50 » se lit « 50% »).
fn percent_update(uid: TaskUid, text: &str) -> ProgressUpdate {
    let trimmed = text.trim();
    let text = if trimmed.ends_with('%') {
        trimmed.to_owned()
    } else {
        format!("{trimmed}%")
    };
    ProgressUpdate {
        task: TaskRef::Uid(uid),
        percent_complete: Some(PercentJson {
            value: None,
            text: Some(text),
        }),
        actual_start: None,
        actual_finish: None,
        actual_duration: None,
        remaining_duration: None,
        percent_work_complete: None,
        actual_work: None,
        physical_percent_complete: None,
    }
}

/// Refus d'un texte (mêmes messages que sous une cellule de la table).
fn text_error(texts: &Texts, tab: Tab, field: &'static str, row: Option<usize>, e: TextError) -> FieldError {
    FieldError {
        tab,
        field,
        row,
        message: EditError::from(e).message(texts),
    }
}

/// Refus d'une valeur lue par le registre (unités).
fn command_error(
    texts: &Texts,
    tab: Tab,
    field: &'static str,
    row: Option<usize>,
    e: &CommandError,
    ctx: &TextContext,
) -> FieldError {
    FieldError {
        tab,
        field,
        row,
        message: command_message(texts, e, None, ctx.language),
    }
}

/// Phrase d'un refus du registre en langage d'utilisateur, comme la
/// notification (H-UI-105, [`crate::error_text::user_message`]) : sujets
/// nommés par leur ID dans `project`, jamais de code, d'UID ni de numéro
/// d'opération (le détail technique reste dans la notification).
#[must_use]
pub fn command_message(texts: &Texts, e: &CommandError, project: Option<&Project>, lang: Language) -> String {
    crate::error_text::user_message(e, project, texts, lang).0
}

/// Onglet et champ d'un refus du registre sur le lot `batch` : opération
/// fautive (`op_index`) et champ (`field`) ; à défaut, Général sans champ.
#[must_use]
pub fn locate(batch: &Batch, error: &CommandError) -> (Tab, &'static str) {
    let op = error.op_index.and_then(|i| batch.ops.get(i));
    let field = error.field.as_deref().unwrap_or_default();
    match op {
        Some(Command::TaskUpdate(_)) => match field {
            "name" => (Tab::General, "name"),
            "duration" | "work" | "milestone" => (Tab::General, "duration"),
            "start" => (Tab::General, "start"),
            "finish" => (Tab::General, "finish"),
            "active" => (Tab::General, "active"),
            "type" => (Tab::Advanced, "task_type"),
            "effort_driven" => (Tab::Advanced, "effort_driven"),
            "notes" => (Tab::Notes, "notes"),
            _ => (Tab::General, ""),
        },
        Some(Command::LinkAdd(_) | Command::LinkUpdate(_)) => {
            if field == "lag" {
                (Tab::Predecessors, "lag")
            } else {
                (Tab::Predecessors, "predecessor")
            }
        }
        Some(Command::LinkRemove(_)) => (Tab::Predecessors, "predecessor"),
        Some(Command::AssignmentAdd(_) | Command::AssignmentUpdate(_) | Command::AssignmentRemove(_)) => {
            if field == "units" || field == "quantity" {
                (Tab::Resources, "units")
            } else {
                (Tab::Resources, "resource")
            }
        }
        Some(Command::TaskSetConstraint(_)) => (Tab::Advanced, "constraint_date"),
        Some(Command::TaskSetDeadline(_)) => (Tab::Advanced, "deadline"),
        Some(Command::ProgressUpdate(_)) => (Tab::General, "percent"),
        _ => (Tab::General, ""),
    }
}

/// Type de lien et son libellé (FD, DD, FF, DF).
pub const LINK_TYPES: [(LinkType, TextId); 4] = [
    (LinkType::FinishToStart, TextId::TaskInfoLinkFs),
    (LinkType::StartToStart, TextId::TaskInfoLinkSs),
    (LinkType::FinishToFinish, TextId::TaskInfoLinkFf),
    (LinkType::StartToFinish, TextId::TaskInfoLinkSf),
];

/// Types de contrainte et leur libellé (S66).
pub const CONSTRAINTS: [(ConstraintKind, TextId); 8] = [
    (ConstraintKind::AsSoonAsPossible, TextId::TaskInfoConstraintAsap),
    (ConstraintKind::AsLateAsPossible, TextId::TaskInfoConstraintAlap),
    (ConstraintKind::MustStartOn, TextId::TaskInfoConstraintMso),
    (ConstraintKind::MustFinishOn, TextId::TaskInfoConstraintMfo),
    (ConstraintKind::StartNoEarlierThan, TextId::TaskInfoConstraintSnet),
    (ConstraintKind::StartNoLaterThan, TextId::TaskInfoConstraintSnlt),
    (
        ConstraintKind::FinishNoEarlierThan,
        TextId::TaskInfoConstraintFnet,
    ),
    (ConstraintKind::FinishNoLaterThan, TextId::TaskInfoConstraintFnlt),
];

/// Types de tâche et leur libellé.
pub const TASK_TYPES: [(TaskType, TextId); 3] = [
    (TaskType::FixedUnits, TextId::TaskInfoTypeFixedUnits),
    (TaskType::FixedDuration, TextId::TaskInfoTypeFixedDuration),
    (TaskType::FixedWork, TextId::TaskInfoTypeFixedWork),
];
