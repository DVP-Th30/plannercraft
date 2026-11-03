//! Préparation du réseau (spécification J2 § 3.1 à 3.5) : échelles de temps,
//! hiérarchie (R-052), classification des tâches, nœuds canoniques, liens
//! effectifs et adjacence CSR.
//!
//! Nœuds canoniques, numérotés en un seul parcours en pré-ordre : la porte
//! d'une récapitulative (`Gate`) à son entrée, une feuille à sa position,
//! l'agrégat d'une récapitulative (`Rollup`) à la fermeture de son
//! sous-arbre (le plus intérieur d'abord). Tout arc de hiérarchie va donc
//! vers l'avant (porte → enfants → agrégat) : seuls des liens peuvent
//! remonter la numérotation, ce qui rend la coupure des cycles simple
//! (`order.rs`).
//!
//! Aucune récursion, aucune indexation qui panique : un invariant violé
//! rend `None` et l'appelant le signale (`schedule.internal`).

use std::collections::BTreeMap;
use std::ops::Range;

use super::constraint::{ConstraintRule, InProgress, Mode, NodeConstraint, Side, anchor, derive};
use super::placement::{Parts, Placer, Resolved};
use crate::calendar::{CalendarSet, CompiledCalendar, Span, TimeScale};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::duration::{LagValue, Minutes, TimeBasis};
use crate::ids::{CalendarUid, TaskUid};
use crate::model::{ConstraintKind, LinkType, Project, SchedulingMode, Task};
use crate::quantity::SignedPercent;
use crate::time::Moment;

/// Indice d'un nœud canonique.
pub(super) type NodeIx = u32;

/// Indice de l'échelle « temps écoulé » dans [`Network::scales`].
pub(super) const ELAPSED: u32 = 0;
/// Indice de l'échelle du calendrier du projet dans [`Network::scales`] :
/// toujours un calendrier de base (R-113), échelle de placement des tâches
/// sans affectation de travail ni calendrier propre (R-175) et de mesure des
/// durées des récapitulatives, des marges et des décalages des tâches sans
/// calendrier propre.
pub(super) const PROJECT: u32 = 1;

/// Échec du calcul d'un nœud : conversion impossible (code stable de
/// l'erreur, R-069) ou invariant interne violé (`schedule.internal`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fail {
    /// Conversion impossible.
    Conversion(&'static str),
    /// Invariant interne violé.
    Internal(&'static str),
}

impl From<crate::error::CalendarError> for Fail {
    fn from(e: crate::error::CalendarError) -> Self {
        Self::Conversion(e.code())
    }
}

impl From<crate::error::DurationError> for Fail {
    fn from(e: crate::error::DurationError) -> Self {
        Self::Conversion(e.code())
    }
}

/// Activité d'une tâche (R-062).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status {
    /// Active.
    Active,
    /// Inactive.
    Inactive,
    /// Sous une récapitulative inactive.
    InactiveAncestor,
}

/// Rôle d'une ligne (R-052).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Role {
    /// Ligne vide.
    Null,
    /// Feuille.
    Leaf,
    /// Récapitulative.
    Summary,
}

/// Classification d'une tâche du modèle, par position.
#[derive(Clone, Debug)]
pub(super) struct TaskInfo {
    /// UID.
    pub uid: TaskUid,
    /// Rôle.
    pub role: Role,
    /// Position du parent (récapitulative), s'il existe.
    pub parent: Option<usize>,
    /// Fin (exclue) du sous-arbre : `position + 1` pour une feuille.
    pub end: usize,
    /// Activité.
    pub status: Status,
    /// Feuille manuelle sans début ni fin (R-063) : aucun nœud.
    pub manual_undated: bool,
    /// Échelle ouvrée de la tâche : calendrier propre utilisable, sinon
    /// calendrier du projet (R-066). Sert au décalage des liens entrants
    /// (R-041) et à la marge d'une récapitulative.
    pub cal: u32,
    /// Nœud d'entrée : la feuille, ou la porte d'une récapitulative.
    pub entry: Option<NodeIx>,
    /// Nœud de sortie : la feuille, ou l'agrégat d'une récapitulative.
    pub exit: Option<NodeIx>,
}

impl TaskInfo {
    /// Inactive ou sous une récapitulative inactive.
    pub fn inactive(&self) -> bool {
        self.status != Status::Active
    }
}

/// Nature d'un nœud canonique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NodeKind {
    /// Porte d'une récapitulative : borne de début de ses sous-tâches
    /// (R-056).
    Gate,
    /// Feuille.
    Leaf,
    /// Agrégat d'une récapitulative (R-053).
    Rollup,
}

/// Nœud canonique.
#[derive(Clone, Debug)]
pub(super) struct Node {
    /// Nature.
    pub kind: NodeKind,
    /// Position de la tâche dans le modèle.
    pub task: usize,
    /// UID de la tâche.
    pub uid: TaskUid,
    /// Tâche inactive (R-062) : placée pour l'affichage, sans influence.
    pub inactive: bool,
    /// Feuille : échelle de placement `P(t)` (R-030, R-067 ; feuille
    /// affectée : échelle de son unique affectation, ou union des échelles
    /// de ses affectations, R-173) ; porte et agrégat : calendrier de la
    /// récapitulative.
    pub place: u32,
    /// Échelle de mesure `M` des marges (R-048, R-049), de la durée d'une
    /// récapitulative (R-054) et de l'épingle (R-092) : le calendrier du
    /// projet (de base) pour une tâche sans calendrier propre, R-113 ; une
    /// feuille affectée se mesure comme si elle ne l'était pas (R-174).
    pub measure: u32,
    /// Feuille : durée de la tâche ; feuille manuelle : durée utilisée
    /// (R-063) ; agrégat d'une récapitulative manuelle datée : durée
    /// utilisée (R-064) ; autre agrégat, porte : 0.
    pub duration: Minutes,
    /// Feuille manuelle active (R-063) ou agrégat d'une récapitulative
    /// manuelle datée (R-064) : dates saisies, ou l'échec de leur calcul.
    pub fixed: Option<Result<Span, Fail>>,
    /// Contrainte (ADR-0010) : feuille automatique active (R-090 à R-101) ;
    /// porte d'une récapitulative automatique (SNET, R-103) ; agrégat
    /// (FNLT, R-103). `None` sans effet (ASAP depuis le début).
    pub constraint: Option<NodeConstraint>,
    /// Échéance : feuille active (automatique ou manuelle, R-102, R-104),
    /// plafond brut de sa fin au plus tard ; agrégat d'une récapitulative
    /// active (R-103 : contribution à `C(S)` ; manuelle datée : plafond de
    /// ses propres dates au plus tard).
    pub deadline: Option<Moment>,
    /// Agrégat de la récapitulative parente.
    pub parent_rollup: Option<NodeIx>,
    /// Porte : son agrégat ; agrégat : sa porte.
    pub twin: Option<NodeIx>,
}

impl Node {
    /// Épingle réelle d'une feuille achevée (R-203) : début réel saisi (ou
    /// `None`, implicite) et fin réelle.
    pub fn actual(&self) -> Option<(Option<Moment>, Moment)> {
        match self.constraint {
            Some(NodeConstraint {
                rule: ConstraintRule::Actual { start, finish },
                ..
            }) => Some((start, finish)),
            _ => None,
        }
    }

    /// Avancement d'une feuille automatique en cours (R-204 à R-207).
    pub fn in_progress(&self) -> Option<InProgress> {
        match self.constraint {
            Some(NodeConstraint {
                rule: ConstraintRule::Progress(ip),
                ..
            }) => Some(ip),
            _ => None,
        }
    }
}

/// Décalage d'un lien, avant conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LagSpec {
    /// Minutes, signées.
    Time(Minutes),
    /// Pourcentage de la durée du prédécesseur (R-043).
    Percent(SignedPercent),
}

/// Lien effectif porté par un arc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LinkArc {
    /// Type.
    pub ty: LinkType,
    /// Décalage.
    pub lag: LagSpec,
    /// Échelle du décalage `σ` (R-041).
    pub sigma: u32,
    /// Prédécesseur d'origine.
    pub pred: TaskUid,
    /// Successeur d'origine.
    pub succ: TaskUid,
    /// Vers une tâche inactive (R-062) : sert à l'affichage seulement,
    /// ignoré en passe arrière et pour la marge libre.
    pub to_inactive: bool,
}

/// Arc du réseau : hiérarchie (`link == None`) ou lien.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Arc {
    /// Origine.
    pub from: NodeIx,
    /// Cible.
    pub to: NodeIx,
    /// Lien, ou `None` pour un arc de hiérarchie.
    pub link: Option<LinkArc>,
}

/// Réseau prêt pour les passes.
pub(super) struct Network<'a> {
    /// Échelles : `[0]` temps écoulé, `[1]` calendrier du projet (de base,
    /// R-113), `[2]` calendrier dérivé désigné par le projet s'il y en a un,
    /// puis les calendriers de tâche utilisés, dans l'ordre des UID.
    pub scales: Vec<TimeScale<'a>>,
    /// Classification, par position.
    pub tasks: Vec<TaskInfo>,
    /// Nœuds canoniques.
    pub nodes: Vec<Node>,
    /// Arcs triés par cible (tri stable : hiérarchie, puis liens dans
    /// l'ordre MSPDI).
    pub arcs: Vec<Arc>,
    /// Arcs entrants du nœud `v` : `arcs[in_off[v]..in_off[v + 1]]`.
    pub in_off: Vec<u32>,
    /// Arcs sortants du nœud `v` : `out_idx[out_off[v]..out_off[v + 1]]`.
    pub out_off: Vec<u32>,
    /// Indices d'arcs, triés par origine.
    pub out_idx: Vec<u32>,
    /// Arcs coupés pour rompre un cycle (R-065).
    pub cut: Vec<bool>,
    /// Parts des feuilles affectées (R-173) et affectations à ranger.
    pub parts: Parts,
}

/// Indice `u32` d'une position (`None` au-delà de `u32::MAX`).
pub(super) fn ix(i: usize) -> Option<u32> {
    u32::try_from(i).ok()
}

/// Position `usize` d'un indice `u32`.
pub(super) fn pos(i: u32) -> Option<usize> {
    usize::try_from(i).ok()
}

impl<'a> Network<'a> {
    /// Nœud `v`.
    pub fn node(&self, v: NodeIx) -> Option<&Node> {
        self.nodes.get(pos(v)?)
    }

    /// Arc `a`.
    pub fn arc(&self, a: u32) -> Option<&Arc> {
        self.arcs.get(pos(a)?)
    }

    /// Arc `a` coupé (R-065).
    pub fn is_cut(&self, a: u32) -> bool {
        pos(a).and_then(|i| self.cut.get(i)).copied().unwrap_or(true)
    }

    /// Échelle `i`.
    pub fn scale(&self, i: u32) -> Option<&TimeScale<'a>> {
        self.scales.get(pos(i)?)
    }

    /// Classification de la tâche en position `p`.
    pub fn task(&self, p: usize) -> Option<&TaskInfo> {
        self.tasks.get(p)
    }

    /// Indices des arcs entrants du nœud `v` (ordre CSR).
    pub fn incoming(&self, v: NodeIx) -> Option<Range<u32>> {
        let i = pos(v)?;
        let a = *self.in_off.get(i)?;
        let b = *self.in_off.get(i.checked_add(1)?)?;
        Some(a..b)
    }

    /// Indices des arcs sortants du nœud `v`.
    pub fn outgoing(&self, v: NodeIx) -> Option<&[u32]> {
        let i = pos(v)?;
        let a = pos(*self.out_off.get(i)?)?;
        let b = pos(*self.out_off.get(i.checked_add(1)?)?)?;
        self.out_idx.get(a..b)
    }

    /// Agrégat d'une récapitulative manuelle datée (R-064) : ses dates ne
    /// dépendent pas de ses enfants, qui n'héritent donc ni de ses liens
    /// sortants ni de ceux de ses ancêtres.
    pub fn is_fixed_rollup(&self, v: NodeIx) -> bool {
        self.node(v)
            .is_some_and(|n| n.kind == NodeKind::Rollup && n.fixed.is_some())
    }

    /// Nombre de nœuds.
    pub fn node_count(&self) -> Option<u32> {
        ix(self.nodes.len())
    }
}

/// Calendriers résolus pour le calcul (§ 3.1, R-066).
pub(super) struct Scales<'a> {
    /// Échelles (voir [`Network::scales`]).
    pub table: Vec<TimeScale<'a>>,
    /// Échelle ouvrée de chaque tâche, par position.
    pub task_cal: Vec<u32>,
}

/// Table des échelles : temps écoulé, calendrier du projet (déjà résolu,
/// replis compris ; de base, R-113 : la base d'un calendrier du projet
/// dérivé), puis un calendrier compilé par UID de tâche utilisé. Une tâche
/// sans calendrier propre est placée et mesurée sur le calendrier du projet
/// (R-175 : le calendrier dérivé désigné ne place plus rien ; ses ressources
/// placent les tâches qui leur sont affectées, R-171) ; un calendrier de
/// tâche inutilisable (inconnu, en échec, sans temps ouvré) est remplacé de
/// même, avec `task.calendar_fallback`. Les calendriers effectifs des
/// affectations s'ajoutent à la construction du réseau (`placement`).
pub(super) fn scales<'a>(
    project: &Project,
    set: &'a CalendarSet,
    project_cal: &'a CompiledCalendar,
    diags: &mut Diagnostics,
) -> Scales<'a> {
    let mut scales = vec![TimeScale::Elapsed, TimeScale::Working(project_cal)];
    let default_place = PROJECT;
    let mut index: BTreeMap<CalendarUid, Option<u32>> = BTreeMap::new();
    let mut task_cal = Vec::with_capacity(project.tasks.len());
    for task in &project.tasks {
        let Some(uid) = task.calendar.filter(|_| !task.is_null) else {
            task_cal.push(default_place);
            continue;
        };
        let slot = *index.entry(uid).or_insert_with(|| match set.get(uid) {
            Ok(c) if c.has_working_time() => {
                let i = ix(scales.len());
                if i.is_some() {
                    scales.push(TimeScale::Working(c));
                }
                i
            }
            _ => None,
        });
        if let Some(i) = slot {
            task_cal.push(i);
        } else {
            diags.push(Diagnostic::TaskCalendarFallback {
                task: task.uid(),
                calendar: uid,
            });
            task_cal.push(default_place);
        }
    }
    Scales {
        table: scales,
        task_cal,
    }
}

/// Étapes 1 et 2 : hiérarchie (R-052) et activité (R-062). Le parent est la
/// dernière tâche non vide de niveau strictement inférieur ; une tâche est
/// récapitulative si une tâche la désigne comme parent (ce qui équivaut à
/// « la tâche non vide suivante a un niveau supérieur »). Le drapeau
/// `Summary` de MSPDI n'est pas lu.
fn classify(project: &Project, task_cal: &[u32], diags: &mut Diagnostics) -> Option<Vec<TaskInfo>> {
    let n = project.tasks.len();
    let mut tasks: Vec<TaskInfo> = Vec::with_capacity(n);
    // Pile des tâches ouvertes : (niveau, position).
    let mut stack: Vec<(u16, usize)> = Vec::new();
    for (p, task) in project.tasks.iter().enumerate() {
        let cal = task_cal.get(p).copied().unwrap_or(PROJECT);
        let mut info = TaskInfo {
            uid: task.uid(),
            role: Role::Null,
            parent: None,
            end: p.checked_add(1)?,
            status: Status::Active,
            manual_undated: false,
            cal,
            entry: None,
            exit: None,
        };
        if !task.is_null {
            let level = task.outline_level.get();
            while let Some(&(l, top)) = stack.last() {
                if l < level {
                    break;
                }
                stack.pop();
                tasks.get_mut(top)?.end = p;
            }
            info.role = Role::Leaf;
            if let Some(&(_, parent)) = stack.last() {
                info.parent = Some(parent);
                let par = tasks.get_mut(parent)?;
                par.role = Role::Summary;
            }
            stack.push((level, p));
        }
        tasks.push(info);
    }
    for (_, top) in stack {
        tasks.get_mut(top)?.end = n;
    }
    // Activité et tâches manuelles sans date, parents avant enfants.
    for p in 0..n {
        let task = project.tasks.at(p)?;
        let parent_status = match tasks.get(p)?.parent {
            Some(par) => Some(tasks.get(par)?.status),
            None => None,
        };
        let info = tasks.get_mut(p)?;
        if info.role == Role::Null {
            continue;
        }
        info.status = match parent_status {
            Some(s) if s != Status::Active => Status::InactiveAncestor,
            _ if !task.active => Status::Inactive,
            _ => Status::Active,
        };
        if info.role == Role::Leaf
            && info.status == Status::Active
            && let SchedulingMode::Manual {
                start: None,
                finish: None,
                ..
            } = task.mode
        {
            info.manual_undated = true;
            diags.push(Diagnostic::TaskManualUndated { task: task.uid() });
        }
    }
    Some(tasks)
}

/// Échelle de placement `P(t)` d'une feuille : temps écoulé pour une durée
/// écoulée (R-067), sinon le calendrier de la tâche.
fn place_of(task: &Task, cal: u32) -> u32 {
    if task.duration.basis() == TimeBasis::Elapsed {
        ELAPSED
    } else {
        cal
    }
}

/// R-063, R-064 : dates d'une feuille (ou d'une récapitulative) manuelle et
/// durée utilisée. Les dates saisies
/// sont conservées sans recalage ; une seule saisie : l'autre par `P.add`
/// avec la durée saisie, sinon la durée de la tâche ; deux saisies : la
/// durée utilisée est le temps entre elles sur `P` ; fin avant début :
/// `task.manual_dates_inverted` et fin ramenée au début.
fn manual_span(
    task: &Task,
    scale: TimeScale<'_>,
    diags: &mut Diagnostics,
) -> (Option<Result<Span, Fail>>, Minutes) {
    let SchedulingMode::Manual {
        start,
        finish,
        duration,
    } = task.mode
    else {
        return (None, task.duration.minutes());
    };
    let entered = duration.map_or(task.duration.minutes(), |d| d.minutes());
    let span = |s: Moment, f: Moment| Span { start: s, finish: f };
    match (start, finish) {
        (Some(s), Some(f)) => {
            let f = if f < s {
                diags.push(Diagnostic::TaskManualDatesInverted { task: task.uid() });
                s
            } else {
                f
            };
            match scale.between(s, f) {
                Ok(d) => (Some(Ok(span(s, f))), d),
                Err(e) => (Some(Err(e.into())), entered),
            }
        }
        (Some(s), None) => (
            Some(scale.add(s, entered).map(|f| span(s, f)).map_err(Fail::from)),
            entered,
        ),
        (None, Some(f)) => {
            let back = entered.checked_neg().unwrap_or(Minutes::ZERO);
            (
                Some(scale.add(f, back).map(|s| span(s, f)).map_err(Fail::from)),
                entered,
            )
        }
        // Exclue plus haut (`ManualUndated`).
        (None, None) => (Some(Err(Fail::Internal("manuelle sans date"))), entered),
    }
}

/// Sens de planification et option « respecter les contraintes » (R-099),
/// lus une fois pour la construction du réseau.
#[derive(Clone, Copy, Debug)]
pub(super) struct BuildCtx {
    /// Sens de planification.
    pub mode: Mode,
    /// Option « les tâches respectent toujours leurs dates de contrainte ».
    pub honor: bool,
    /// Option « fractionner les tâches en cours » (R-205).
    pub split: bool,
}

/// Contrainte d'une feuille automatique active, placée par `placer` :
/// règle dérivée (R-099) et ancrage R-090 (R-174 pour une feuille
/// affectée). `None` si la contrainte est sans effet (ASAP depuis le début).
fn leaf_constraint(
    task: &Task,
    placer: Result<Placer<'_, '_>, Fail>,
    ctx: BuildCtx,
) -> Option<NodeConstraint> {
    let kind = task.constraint.kind();
    let rule = derive(kind, ctx.mode, ctx.honor);
    if rule == ConstraintRule::None {
        return None;
    }
    let date = task.constraint.date();
    let anchor = date.map(|date| placer.and_then(|p| p.anchor(kind, date)));
    Some(NodeConstraint {
        kind,
        rule,
        date,
        anchor,
    })
}

/// Nœud d'une feuille en position `p` : échelle de placement, dates saisies
/// d'une manuelle (R-063), contrainte d'une automatique active (R-090 à
/// R-101), échéance d'une active (R-102, R-104).
fn leaf_node(
    task: &Task,
    p: usize,
    info: &TaskInfo,
    (scales, parts): (&[TimeScale<'_>], &Parts),
    ctx: BuildCtx,
    diags: &mut Diagnostics,
) -> Option<Node> {
    // R-173 : feuille affectée, placée par ses affectations ; mesurée comme
    // si elle ne l'était pas (R-174 ; durée écoulée : temps civil, R-182).
    let (place, measure) = if let Some(place) = parts.place(p) {
        (place, place_of(task, info.cal))
    } else {
        let place = place_of(task, info.cal);
        (place, place)
    };
    let scale = *scales.get(pos(place)?)?;
    let facts = crate::progress::facts(task);
    // R-203, R-208 : feuille active achevée (automatique ou manuelle),
    // épinglée sur ses dates réelles ; sa durée est celle de l'avancement
    // (R-209 : hors union des affectations).
    if !info.inactive() && facts.state == crate::progress::ProgressState::Complete {
        return Some(completed_leaf(
            task,
            p,
            info,
            (place, measure, scale),
            facts,
            ctx,
            diags,
        ));
    }
    // R-204 : feuille automatique active commencée, non achevée : partie
    // réelle fixe, reste replacé (une seule coupure, ADR-0014 D2).
    if !info.inactive()
        && facts.state == crate::progress::ProgressState::Started
        && matches!(task.mode, SchedulingMode::Auto)
    {
        return Some(in_progress_leaf(
            task,
            p,
            info,
            (place, measure),
            facts,
            ctx,
            diags,
        ));
    }
    let (fixed, duration) = if info.inactive() {
        (None, task.duration.minutes())
    } else if let Some(d) = parts.duration(p) {
        (None, d)
    } else {
        let (fixed, duration) = manual_span(task, scale, diags);
        started_manual(fixed, duration, facts.actual_start, scale)
    };
    // R-104 : manuelle, contrainte ignorée (info), échéance gardée ;
    // inactive, tout est ignoré sans diagnostic.
    let constraint = if info.inactive() {
        None
    } else if fixed.is_some() {
        manual_ignored(task, ctx, diags);
        None
    } else {
        let placer = match parts.failure(p) {
            Some(code) => Err(Fail::Conversion(code)),
            None => Placer::of(scales, parts.of(p), place, duration),
        };
        leaf_constraint(task, placer, ctx)
    };
    let deadline = task.deadline.filter(|_| !info.inactive());
    Some(Node {
        kind: NodeKind::Leaf,
        task: p,
        uid: info.uid,
        inactive: info.inactive(),
        place,
        measure,
        duration,
        fixed,
        constraint,
        deadline,
        parent_rollup: None,
        twin: None,
    })
}

/// R-203, R-208 : nœud d'une feuille active achevée. Épingle réelle
/// (`ConstraintRule::Actual`, début réel saisi ou implicite, fin réelle au
/// moins égale au début), sans dates saisies (`fixed`) ; durée : celle de la
/// tâche pour une automatique, le temps ouvré entre le début et la fin
/// réels pour une manuelle (R-063 : durée utilisée entre deux dates) ;
/// contrainte d'une manuelle ignorée comme avant (R-104), échéance gardée.
fn completed_leaf(
    task: &Task,
    p: usize,
    info: &TaskInfo,
    (place, measure, scale): (u32, u32, TimeScale<'_>),
    facts: crate::progress::ProgressFacts,
    ctx: BuildCtx,
    diags: &mut Diagnostics,
) -> Node {
    let finish = facts.actual_finish.unwrap_or(Moment::MIN);
    let finish = facts.actual_start.map_or(finish, |s| finish.max(s));
    let manual = !matches!(task.mode, SchedulingMode::Auto);
    if manual {
        manual_ignored(task, ctx, diags);
    }
    let duration = match facts.actual_start {
        Some(s) if manual => scale.between(s, finish).unwrap_or(task.duration.minutes()),
        _ => task.duration.minutes(),
    };
    Node {
        kind: NodeKind::Leaf,
        task: p,
        uid: info.uid,
        inactive: false,
        place,
        measure,
        duration: duration.max(Minutes::ZERO),
        fixed: None,
        constraint: Some(NodeConstraint {
            kind: task.constraint.kind(),
            rule: ConstraintRule::Actual {
                start: facts.actual_start,
                finish,
            },
            date: None,
            anchor: None,
        }),
        deadline: task.deadline,
        parent_rollup: None,
        twin: None,
    }
}

/// R-204, R-205 : nœud d'une feuille automatique active en cours. Durée D
/// de la tâche (R-209 : hors union des affectations) ; règle
/// `ConstraintRule::Progress` (début réel saisi ou implicite, durée réelle
/// effective, reste D − réel, reprise saisie, fractionnement) ; la
/// contrainte saisie ne garde sa date que si elle est un plancher de la
/// reprise (SNET, MSO, fractionnement permis, depuis le début) ; toute autre
/// contrainte datée est ignorée avec `constraint.ignored_in_progress`
/// (info) ; échéance gardée (R-102). À rebours, le reste est placé au plus
/// tôt depuis la reprise sans borne (R-207) : aucune contrainte ne joue.
fn in_progress_leaf(
    task: &Task,
    p: usize,
    info: &TaskInfo,
    (place, measure): (u32, u32),
    facts: crate::progress::ProgressFacts,
    ctx: BuildCtx,
    diags: &mut Diagnostics,
) -> Node {
    let kind = task.constraint.kind();
    let floor = ctx.split
        && ctx.mode == Mode::FromStart
        && matches!(
            kind,
            ConstraintKind::StartNoEarlierThan | ConstraintKind::MustStartOn
        );
    if kind.is_dated() && !floor {
        diags.push(Diagnostic::ConstraintIgnoredInProgress {
            task: task.uid(),
            kind,
        });
    }
    let duration = task.duration.minutes().max(Minutes::ZERO);
    Node {
        kind: NodeKind::Leaf,
        task: p,
        uid: info.uid,
        inactive: false,
        place,
        measure,
        duration,
        fixed: None,
        constraint: Some(NodeConstraint {
            kind,
            rule: ConstraintRule::Progress(InProgress {
                start: facts.actual_start,
                actual: facts.actual_duration,
                remaining: crate::progress::remaining_duration(task),
                resume: task.progress.resume,
                split: ctx.split,
            }),
            date: task.constraint.date().filter(|_| floor),
            anchor: None,
        }),
        deadline: task.deadline,
        parent_rollup: None,
        twin: None,
    }
}

/// R-208 : feuille manuelle commencée avec un début réel saisi : le début
/// réel remplace le début saisi (la fin saisie est gardée, au moins égale
/// au début réel) ; durée utilisée = temps ouvré entre les deux.
fn started_manual(
    fixed: Option<Result<Span, Fail>>,
    duration: Minutes,
    actual_start: Option<Moment>,
    scale: TimeScale<'_>,
) -> (Option<Result<Span, Fail>>, Minutes) {
    let (Some(Ok(span)), Some(start)) = (fixed, actual_start) else {
        return (fixed, duration);
    };
    let span = Span {
        start,
        finish: span.finish.max(start),
    };
    match scale.between(span.start, span.finish) {
        Ok(d) => (Some(Ok(span)), d),
        Err(e) => (Some(Err(e.into())), duration),
    }
}

/// Récapitulative ouverte pendant la numérotation : position, et dates
/// saisies et durée utilisée d'une récapitulative manuelle datée (R-064).
#[derive(Clone, Copy, Debug)]
struct OpenSummary {
    /// Position de la récapitulative.
    pos: usize,
    /// Dates saisies d'une récapitulative manuelle datée (R-064).
    fixed: Option<Result<Span, Fail>>,
    /// Durée utilisée d'une récapitulative manuelle datée (R-064).
    duration: Minutes,
    /// Contrainte de l'agrégat : FNLT, plafond `C(S)` (R-103).
    constraint: Option<NodeConstraint>,
    /// Échéance de l'agrégat (R-103 : plafond `C(S)` ; récapitulative
    /// manuelle datée : plafond de ses propres dates au plus tard, R-104).
    deadline: Option<Moment>,
}

/// Contraintes d'une récapitulative (R-103, R-104) : porte (SNET,
/// plancher brut de ses sous-tâches), agrégat (FNLT, plafond `C(S)`),
/// échéance. Automatique active : SNET → porte ; FNLT → agrégat (ancrage
/// `f*` sur son calendrier) ; MSO, MFO, SNLT, FNET → ignorées avec
/// `constraint.summary_ignored` ; ASAP, ALAP → sans effet ni diagnostic.
/// Manuelle datée : contrainte ignorée (`constraint.manual_ignored` si
/// elle diffère du défaut du sens), échéance gardée. Inactive : tout est
/// ignoré, sans diagnostic.
fn summary_constraints(
    task: &Task,
    info: &TaskInfo,
    manual: bool,
    scale: TimeScale<'_>,
    ctx: BuildCtx,
    diags: &mut Diagnostics,
) -> (Option<NodeConstraint>, Option<NodeConstraint>, Option<Moment>) {
    if info.inactive() {
        return (None, None, None);
    }
    let kind = task.constraint.kind();
    let date = task.constraint.date();
    if manual {
        manual_ignored(task, ctx, diags);
        return (None, None, task.deadline);
    }
    let node = |rule, anchor| NodeConstraint {
        kind,
        rule,
        date,
        anchor,
    };
    match kind {
        ConstraintKind::AsSoonAsPossible | ConstraintKind::AsLateAsPossible => (None, None, task.deadline),
        ConstraintKind::StartNoEarlierThan => (
            Some(node(ConstraintRule::Floor(Side::Start), None)),
            None,
            task.deadline,
        ),
        ConstraintKind::FinishNoLaterThan => {
            let anchor = date.map(|c| anchor(scale, kind, c, Minutes::ZERO).map_err(Fail::from));
            (
                None,
                Some(node(ConstraintRule::Cap(Side::Finish), anchor)),
                task.deadline,
            )
        }
        ConstraintKind::MustStartOn
        | ConstraintKind::MustFinishOn
        | ConstraintKind::StartNoLaterThan
        | ConstraintKind::FinishNoEarlierThan => {
            diags.push(Diagnostic::ConstraintSummaryIgnored {
                task: task.uid(),
                kind,
            });
            (None, None, task.deadline)
        }
    }
}

/// R-104 : la contrainte d'une tâche manuelle (feuille ou récapitulative
/// datée) est stockée mais ignorée ; `constraint.manual_ignored` (info) si
/// elle diffère du défaut du sens (ASAP depuis le début, ALAP à rebours).
fn manual_ignored(task: &Task, ctx: BuildCtx, diags: &mut Diagnostics) {
    let kind = task.constraint.kind();
    let default = match ctx.mode {
        Mode::FromStart => ConstraintKind::AsSoonAsPossible,
        Mode::FromFinish => ConstraintKind::AsLateAsPossible,
    };
    if kind != default {
        diags.push(Diagnostic::ConstraintManualIgnored {
            task: task.uid(),
            kind,
        });
    }
}

/// Étape 3 : nœuds canoniques en pré-ordre (porte, feuille, agrégat).
fn number_nodes(
    project: &Project,
    tasks: &mut [TaskInfo],
    (scales, parts): (&[TimeScale<'_>], &Parts),
    ctx: BuildCtx,
    diags: &mut Diagnostics,
) -> Option<Vec<Node>> {
    let mut nodes: Vec<Node> = Vec::with_capacity(tasks.len().checked_add(tasks.len() / 4)?);
    // Récapitulatives ouvertes, la plus intérieure en dernier.
    let mut open: Vec<OpenSummary> = Vec::new();
    let n = tasks.len();
    let close = |o: OpenSummary, tasks: &mut [TaskInfo], nodes: &mut Vec<Node>| -> Option<()> {
        let info = tasks.get_mut(o.pos)?;
        let gate = info.entry?;
        let v = ix(nodes.len())?;
        nodes.push(Node {
            kind: NodeKind::Rollup,
            task: o.pos,
            uid: info.uid,
            inactive: info.inactive(),
            place: info.cal,
            measure: info.cal,
            duration: o.duration,
            fixed: o.fixed,
            constraint: o.constraint,
            deadline: o.deadline,
            parent_rollup: None,
            twin: Some(gate),
        });
        info.exit = Some(v);
        nodes.get_mut(pos(gate)?)?.twin = Some(v);
        Some(())
    };
    for p in 0..=n {
        while let Some(o) = open.last() {
            if tasks.get(o.pos)?.end > p {
                break;
            }
            close(open.pop()?, tasks, &mut nodes)?;
        }
        if p == n {
            break;
        }
        let task = project.tasks.at(p)?;
        let info = tasks.get_mut(p)?;
        if info.role == Role::Null || info.manual_undated {
            continue;
        }
        let v = ix(nodes.len())?;
        if info.role == Role::Summary {
            // R-064 : une récapitulative manuelle active avec au moins une
            // date saisie garde ses dates (comme une feuille manuelle, sur
            // son calendrier) ; sans date, elle est automatique.
            let dated = matches!(
                task.mode,
                SchedulingMode::Manual { start, finish, .. } if start.is_some() || finish.is_some()
            );
            let scale = *scales.get(pos(info.cal)?)?;
            let (fixed, duration) = if dated && !info.inactive() {
                manual_span(task, scale, diags)
            } else {
                (None, Minutes::ZERO)
            };
            let (gate, constraint, deadline) =
                summary_constraints(task, info, fixed.is_some(), scale, ctx, diags);
            nodes.push(Node {
                kind: NodeKind::Gate,
                task: p,
                uid: info.uid,
                inactive: info.inactive(),
                place: info.cal,
                measure: info.cal,
                duration: Minutes::ZERO,
                fixed: None,
                constraint: gate,
                deadline: None,
                parent_rollup: None,
                twin: None,
            });
            info.entry = Some(v);
            open.push(OpenSummary {
                pos: p,
                fixed,
                duration,
                constraint,
                deadline,
            });
        } else {
            nodes.push(leaf_node(task, p, info, (scales, parts), ctx, diags)?);
            info.entry = Some(v);
            info.exit = Some(v);
        }
    }
    // Agrégat du parent de chaque nœud de feuille ou de récapitulative.
    for node in &mut nodes {
        if node.kind == NodeKind::Gate {
            continue;
        }
        if let Some(par) = tasks.get(node.task)?.parent {
            node.parent_rollup = tasks.get(par)?.exit;
        }
    }
    Some(nodes)
}

/// Budget par défaut des arcs dépliés par R-058 (lien FF ou SF vers une
/// récapitulative, un arc par feuille) : 4 000 000 arcs, soit environ
/// 160 Mo d'arcs (40 octets chacun), CSR compris. Le budget de performance
/// (5 000 tâches, 10 000 liens) en reste loin ; sans plafond, un fichier de
/// 5 Mo (20 000 liens FF vers une récapitulative de 20 000 feuilles)
/// demanderait 400 millions d'arcs (R-058, règle 10).
pub const DEFAULT_EXPANDED_ARC_BUDGET: usize = 4_000_000;

/// Contexte de construction des arcs.
struct ArcBuilder<'p> {
    project: &'p Project,
    mode: Mode,
    arcs: Vec<Arc>,
    /// Budget des arcs dépliés (R-058), voir [`DEFAULT_EXPANDED_ARC_BUDGET`].
    expansion_budget: usize,
    /// Arcs dépliés encore permis.
    expansion_left: usize,
    /// Liens FF/SF vers une récapitulative ignorés faute de budget.
    expansion_ignored: u64,
    /// Nombre de feuilles cibles du dépliage, par position de
    /// récapitulative (calculé à la première rencontre).
    expansion_targets: Vec<Option<usize>>,
}

impl ArcBuilder<'_> {
    /// Arcs de hiérarchie : porte du parent → entrée de l'enfant ; sortie de
    /// l'enfant → agrégat du parent, si l'enfant a la même activité que le
    /// parent (une tâche inactive reste hors des agrégats actifs, R-062).
    fn hierarchy(&mut self, tasks: &[TaskInfo]) -> Option<()> {
        for info in tasks {
            let Some(par) = info.parent else { continue };
            let (Some(entry), Some(exit)) = (info.entry, info.exit) else {
                continue;
            };
            let parent = tasks.get(par)?;
            self.arcs.push(Arc {
                from: parent.entry?,
                to: entry,
                link: None,
            });
            if info.inactive() == parent.inactive() {
                self.arcs.push(Arc {
                    from: exit,
                    to: parent.exit?,
                    link: None,
                });
            }
        }
        Some(())
    }

    /// R-110 (miroir de R-058) : à rebours, le plafond de début `K(S)` d'un
    /// lien SS ou SF depuis une récapitulative automatique active (en
    /// position `p_pos`) s'applique à toutes ses sous-tâches, avec
    /// `link.start_from_summary` ; une récapitulative manuelle datée ne
    /// transmet pas ses liens (R-064).
    fn start_from_summary(&self, kind: LinkType, p_pos: usize, pred: &TaskInfo) -> bool {
        let start_kind = matches!(kind, LinkType::StartToStart | LinkType::StartToFinish);
        let dated_manual = self.project.tasks.at(p_pos).is_some_and(|t| {
            matches!(
                t.mode,
                SchedulingMode::Manual { start, finish, .. } if start.is_some() || finish.is_some()
            )
        });
        self.mode == Mode::FromFinish
            && start_kind
            && pred.role == Role::Summary
            && !dated_manual
            && !pred.inactive()
    }

    /// Étape 4 : liens effectifs (§ 3.5), dans l'ordre des successeurs puis
    /// l'ordre MSPDI.
    fn links(&mut self, tasks: &[TaskInfo], diags: &mut Diagnostics) -> Option<()> {
        // Doublons : position du dernier successeur qui a cité chaque
        // prédécesseur (aucune allocation par tâche).
        let mut seen_by: Vec<usize> = vec![usize::MAX; tasks.len()];
        for (s_pos, succ_task) in self.project.tasks.iter().enumerate() {
            let succ = tasks.get(s_pos)?;
            for link in &succ_task.predecessors {
                let p_uid = link.predecessor;
                let s_uid = succ.uid;
                // 1. Inconnu, auto-lien, doublon (A17).
                let Some(p_pos) = self.project.tasks.position(p_uid) else {
                    diags.push(Diagnostic::LinkUnknownTask {
                        predecessor: p_uid,
                        successor: s_uid,
                    });
                    continue;
                };
                if p_uid == s_uid {
                    diags.push(Diagnostic::LinkSelf { task: s_uid });
                    continue;
                }
                let seen = seen_by.get_mut(p_pos)?;
                if *seen == s_pos {
                    diags.push(Diagnostic::LinkDuplicate {
                        predecessor: p_uid,
                        successor: s_uid,
                    });
                    continue;
                }
                *seen = s_pos;
                let pred = tasks.get(p_pos)?;
                let excluded = |t: &TaskInfo| t.role == Role::Null || t.manual_undated;
                // 2. Extrémité non planifiée.
                if excluded(pred) || excluded(succ) {
                    diags.push(Diagnostic::LinkExcludedEndpoint {
                        predecessor: p_uid,
                        successor: s_uid,
                    });
                    continue;
                }
                // 3. Prédécesseur inactif vers un successeur actif : aucun
                //    arc, aucun pont (R-062). Entre deux tâches inactives
                //    (par exemple dans une récapitulative inactive), l'arc
                //    est gardé pour le placement d'affichage, marqué
                //    `to_inactive`.
                if pred.inactive() && !succ.inactive() {
                    diags.push(Diagnostic::LinkInactive {
                        predecessor: p_uid,
                        successor: s_uid,
                    });
                    continue;
                }
                // 4. Récapitulative et descendante (R-059).
                let contains = |anc_pos: usize, anc: &TaskInfo, q: usize| anc_pos < q && q < anc.end;
                if contains(p_pos, pred, s_pos) || contains(s_pos, succ, p_pos) {
                    diags.push(Diagnostic::LinkSummaryDescendant {
                        predecessor: p_uid,
                        successor: s_uid,
                    });
                    continue;
                }
                // 5. Origine : la sortie du prédécesseur (agrégat ou feuille).
                let from = pred.exit?;
                let lag = match link.lag.value {
                    LagValue::Time { minutes, .. } => LagSpec::Time(minutes),
                    LagValue::Percent { percent } => LagSpec::Percent(percent),
                };
                let sigma = match link.lag.basis {
                    TimeBasis::Elapsed => ELAPSED,
                    // R-041 : calendrier de tâche du successeur, sinon le
                    // calendrier du projet (de base, R-113).
                    TimeBasis::Working => succ.cal,
                };
                let arc = |to: NodeIx, to_inactive: bool| Arc {
                    from,
                    to,
                    link: Some(LinkArc {
                        ty: link.kind,
                        lag,
                        sigma,
                        pred: p_uid,
                        succ: s_uid,
                        to_inactive,
                    }),
                };
                // 6. Cible : la porte pour FS/SS vers une récapitulative ;
                //    dépliage sur les feuilles pour FF/SF (R-058).
                let finish_kind = matches!(link.kind, LinkType::FinishToFinish | LinkType::StartToFinish);
                if succ.role == Role::Summary && finish_kind {
                    self.expand_to_summary(tasks, s_pos, succ, &arc, (p_uid, s_uid), diags)?;
                } else {
                    self.arcs.push(arc(succ.entry?, succ.inactive()));
                }
                if self.start_from_summary(link.kind, p_pos, pred) {
                    diags.push(Diagnostic::LinkStartFromSummary {
                        predecessor: p_uid,
                        successor: s_uid,
                    });
                }
            }
        }
        if self.expansion_ignored > 0 {
            diags.push(Diagnostic::LinkExpansionLimit {
                limit: u64::try_from(self.expansion_budget).unwrap_or(u64::MAX),
                ignored: self.expansion_ignored,
            });
        }
        Some(())
    }

    /// R-058 : dépliage d'un lien FF ou SF `(prédécesseur, successeur)` vers
    /// la récapitulative `succ` (position `s_pos`) sur ses feuilles, sous le
    /// budget global d'arcs dépliés.
    fn expand_to_summary(
        &mut self,
        tasks: &[TaskInfo],
        s_pos: usize,
        succ: &TaskInfo,
        arc: &dyn Fn(NodeIx, bool) -> Arc,
        (p_uid, s_uid): (TaskUid, TaskUid),
        diags: &mut Diagnostics,
    ) -> Option<()> {
        // Budget global (R-058, règle 10) : un lien qui ne tient plus est
        // ignoré en entier, ainsi que tous les suivants (jamais de dépliage
        // partiel).
        let targets = self.expansion_targets(tasks, s_pos, succ)?;
        if self.expansion_ignored > 0 || targets > self.expansion_left {
            self.expansion_ignored = self.expansion_ignored.saturating_add(1);
            return Some(());
        }
        self.expansion_left = self.expansion_left.saturating_sub(targets);
        diags.push(Diagnostic::LinkFinishToSummary {
            predecessor: p_uid,
            successor: s_uid,
        });
        let first = s_pos.checked_add(1)?;
        for q in first..succ.end {
            let leaf = tasks.get(q)?;
            if !Self::expansion_target(leaf, succ) {
                continue;
            }
            if let Some(v) = leaf.entry {
                self.arcs.push(arc(v, leaf.inactive()));
            }
        }
        Some(())
    }

    /// Feuille `leaf` cible du dépliage d'un lien vers la récapitulative
    /// `succ` : feuille active, ou inactive sous une récapitulative inactive.
    fn expansion_target(leaf: &TaskInfo, succ: &TaskInfo) -> bool {
        leaf.role == Role::Leaf && (!leaf.inactive() || succ.inactive()) && leaf.entry.is_some()
    }

    /// Nombre d'arcs que déplie un lien vers la récapitulative en position
    /// `s_pos` (mis en cache : chaque sous-arbre n'est parcouru qu'une fois).
    fn expansion_targets(&mut self, tasks: &[TaskInfo], s_pos: usize, succ: &TaskInfo) -> Option<usize> {
        if let Some(Some(n)) = self.expansion_targets.get(s_pos) {
            return Some(*n);
        }
        let first = s_pos.checked_add(1)?;
        let n = tasks
            .get(first..succ.end)?
            .iter()
            .filter(|leaf| Self::expansion_target(leaf, succ))
            .count();
        *self.expansion_targets.get_mut(s_pos)? = Some(n);
        Some(n)
    }
}

/// Adjacence CSR : arcs triés par cible, index entrant et sortant.
struct Csr {
    arcs: Vec<Arc>,
    in_off: Vec<u32>,
    out_off: Vec<u32>,
    out_idx: Vec<u32>,
}

/// Tri par comptage stable des arcs par cible, en place (permutation
/// appliquée par cycles : aucune seconde copie des arcs, seulement 4 octets
/// par arc) ; index sortant.
fn csr(node_count: usize, mut arcs: Vec<Arc>) -> Option<Csr> {
    let mut in_off = vec![0u32; node_count.checked_add(1)?];
    for a in &arcs {
        let slot = in_off.get_mut(pos(a.to)?.checked_add(1)?)?;
        *slot = slot.checked_add(1)?;
    }
    for i in 1..in_off.len() {
        let prev = *in_off.get(i.checked_sub(1)?)?;
        let cur = in_off.get_mut(i)?;
        *cur = cur.checked_add(prev)?;
    }
    let mut fill = in_off.clone();
    // `dest[k]` : position finale de l'arc `k` (ordre d'origine conservé
    // entre arcs de même cible : tri stable).
    let mut dest: Vec<u32> = Vec::with_capacity(arcs.len());
    for a in &arcs {
        let slot = fill.get_mut(pos(a.to)?)?;
        dest.push(*slot);
        *slot = slot.checked_add(1)?;
    }
    // Une permutation se range en moins de `n` échanges : au-delà, `dest`
    // n'en est pas une (invariant violé), on s'arrête au lieu de boucler.
    let mut swaps_left = arcs.len();
    for k in 0..arcs.len() {
        loop {
            let d = pos(*dest.get(k)?)?;
            if d == k {
                break;
            }
            // Chaque échange place définitivement l'arc envoyé en `d`.
            if d >= arcs.len() {
                return None;
            }
            swaps_left = swaps_left.checked_sub(1)?;
            arcs.swap(k, d);
            dest.swap(k, d);
        }
    }
    drop(dest);
    let sorted = arcs;
    let mut out_off = vec![0u32; node_count.checked_add(1)?];
    for a in &sorted {
        let slot = out_off.get_mut(pos(a.from)?.checked_add(1)?)?;
        *slot = slot.checked_add(1)?;
    }
    for i in 1..out_off.len() {
        let prev = *out_off.get(i.checked_sub(1)?)?;
        let cur = out_off.get_mut(i)?;
        *cur = cur.checked_add(prev)?;
    }
    let mut fill = out_off.clone();
    let mut out_idx = vec![0u32; sorted.len()];
    for (k, a) in sorted.iter().enumerate() {
        let slot = fill.get_mut(pos(a.from)?)?;
        *out_idx.get_mut(pos(*slot)?)? = ix(k)?;
        *slot = slot.checked_add(1)?;
    }
    Some(Csr {
        arcs: sorted,
        in_off,
        out_off,
        out_idx,
    })
}

/// Étapes 1 à 4 : réseau complet. `None` si un invariant est violé (jamais
/// attendu) ; l'appelant le signale.
pub(super) fn build<'a>(
    project: &Project,
    mut sc: Scales<'a>,
    (resolved, set): (&'a Resolved, &'a CalendarSet),
    mode: Mode,
    expanded_arc_budget: usize,
    diags: &mut Diagnostics,
) -> Option<Network<'a>> {
    let ctx = BuildCtx {
        mode,
        honor: project.options.honor_constraints,
        split: project.options.progress.split_in_progress,
    };
    let mut tasks = classify(project, &sc.task_cal, diags)?;
    let parts = super::placement::attach(project, resolved, set, &mut sc.table, &tasks, diags)?;
    let nodes = number_nodes(project, &mut tasks, (&sc.table, &parts), ctx, diags)?;
    let mut builder = ArcBuilder {
        project,
        mode,
        arcs: Vec::new(),
        expansion_budget: expanded_arc_budget,
        expansion_left: expanded_arc_budget,
        expansion_ignored: 0,
        expansion_targets: vec![None; tasks.len()],
    };
    builder.hierarchy(&tasks)?;
    builder.links(&tasks, diags)?;
    let arc_count = builder.arcs.len();
    let c = csr(nodes.len(), builder.arcs)?;
    Some(Network {
        scales: sc.table,
        tasks,
        nodes,
        arcs: c.arcs,
        in_off: c.in_off,
        out_off: c.out_off,
        out_idx: c.out_idx,
        cut: vec![false; arc_count],
        parts,
    })
}
