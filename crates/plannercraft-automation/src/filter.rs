//! Filtre de tâches partagé (R-251, ADR-0016 D2).
//!
//! Prédicat pur, sans interface : le même pour l'interface (vue filtrée,
//! H-UI-30), `task.list` et l'outil MCP `list_tasks` (une seule sémantique
//! pour l'IA, la CLI et l'interface). Les critères « critique », « en
//! retard » et « inachevée » lisent le planning du moteur (règle 3). Un
//! filtre est une vue : il n'écrit rien (P50).
//!
//! Implémenté en L2 (R-251) : conjonction des critères posés, récapitulatives
//! liées en option, repli du texte par NFD (`unicode-normalization`).

use std::collections::BTreeSet;

use plannercraft_core::schedule::{TaskRole, TaskSchedule};
use plannercraft_core::{
    BaselineNumber, Day, Moment, Percent, ProgressState, Project, Schedule, Task, TaskUid,
};

/// Critères d'un filtre ; une tâche passe si elle satisfait **tous** les
/// critères posés (`None` : critère absent).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskFilter {
    /// Critique du moteur.
    pub critical: Option<bool>,
    /// En retard (sémantique de `task.list`, `view.rs`).
    pub late: Option<bool>,
    /// Feuille, non exclue, % achevé < 100 %.
    pub incomplete: Option<bool>,
    /// Jalon.
    pub milestone: Option<bool>,
    /// Récapitulative.
    pub summary: Option<bool>,
    /// Active.
    pub active: Option<bool>,
    /// Affectée à la ressource de ce nom (comparaison exacte, espaces de
    /// bord ignorés des deux côtés).
    pub resource: Option<String>,
    /// Dates planifiées touchant ce jour ou un jour suivant.
    pub from: Option<Day>,
    /// Dates planifiées touchant ce jour ou un jour précédent.
    pub to: Option<Day>,
    /// Niveau hiérarchique maximal.
    pub outline_max: Option<u16>,
    /// Sous-chaîne du nom, sans casse ni accents ([`fold_text`]).
    pub name_contains: Option<String>,
    /// Ajoute les récapitulatives ancêtres d'une tâche retenue (vrai par
    /// défaut dans l'interface, faux pour `task.list`).
    pub show_related_summaries: bool,
}

/// Filtres prédéfinis du menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilterPreset {
    /// Toutes les tâches.
    All,
    /// Tâches critiques.
    Critical,
    /// Tâches en retard.
    Late,
    /// Tâches inachevées.
    Incomplete,
    /// Jalons.
    Milestones,
    /// Récapitulatives.
    Summaries,
    /// Tâches affectées à une ressource (nom).
    UsingResource(String),
    /// Tâches touchant une plage de jours.
    DateRange {
        /// Premier jour.
        from: Day,
        /// Dernier jour.
        to: Day,
    },
}

/// Contexte d'évaluation : projet, planning du moteur, date d'état.
#[derive(Clone, Copy, Debug)]
pub struct FilterCtx<'a> {
    /// Projet.
    pub project: &'a Project,
    /// Planning du moteur.
    pub schedule: &'a Schedule,
    /// Date d'état effective.
    pub status: Moment,
}

/// Résultat d'un filtre sur tout le projet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterHits {
    /// Ligne montrée, par position dans le projet.
    pub shown: Vec<bool>,
    /// Tâches retenues par les critères.
    pub direct: u32,
    /// Récapitulatives ajoutées par `show_related_summaries`.
    pub related: u32,
}

impl TaskFilter {
    /// Filtre d'un préréglage (récapitulatives liées affichées).
    #[must_use]
    pub fn from_preset(p: &FilterPreset) -> Self {
        let mut f = Self {
            show_related_summaries: true,
            ..Self::default()
        };
        match p {
            FilterPreset::All => {}
            FilterPreset::Critical => f.critical = Some(true),
            FilterPreset::Late => f.late = Some(true),
            FilterPreset::Incomplete => f.incomplete = Some(true),
            FilterPreset::Milestones => f.milestone = Some(true),
            FilterPreset::Summaries => f.summary = Some(true),
            FilterPreset::UsingResource(name) => f.resource = Some(name.clone()),
            FilterPreset::DateRange { from, to } => {
                f.from = Some(*from);
                f.to = Some(*to);
            }
        }
        f
    }

    /// Vrai si aucun critère n'est posé (filtre « Toutes ») : toute ligne
    /// passe, lignes vides comprises.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.critical.is_none()
            && self.late.is_none()
            && self.incomplete.is_none()
            && self.milestone.is_none()
            && self.summary.is_none()
            && self.active.is_none()
            && self.resource.is_none()
            && self.from.is_none()
            && self.to.is_none()
            && self.outline_max.is_none()
            && self.name_contains.is_none()
    }

    /// Vrai si la tâche `t` (position `pos`) satisfait tous les critères
    /// posés (R-251). Une ligne vide ne passe que le filtre vide. Pour tout
    /// le projet, préférer [`TaskFilter::hits`] (préparation faite une fois).
    #[must_use]
    pub fn matches(&self, ctx: &FilterCtx<'_>, pos: usize, t: &Task) -> bool {
        let prepared = Prepared::new(self, ctx);
        let summary = summary_at(ctx.project, pos);
        prepared.matches(self, ctx, t, summary)
    }

    /// Lignes montrées sur tout le projet (R-251) : tâches retenues par les
    /// critères, plus, avec `show_related_summaries`, leurs récapitulatives
    /// ancêtres. Une passe sur les tâches, plus une remontée par ancêtre
    /// ajouté (chaque ancêtre n'est ajouté qu'une fois).
    #[must_use]
    pub fn hits(&self, ctx: &FilterCtx<'_>) -> FilterHits {
        let tasks = &ctx.project.tasks;
        let len = tasks.len();
        let summaries = summary_flags(ctx.project);
        let prepared = Prepared::new(self, ctx);
        let mut shown = vec![false; len];
        let mut direct = 0u32;
        let mut related = 0u32;
        // Ancêtres de la tâche courante : positions de niveaux croissants
        // (lignes vides ignorées, R-052).
        let mut stack: Vec<(usize, u16)> = Vec::new();
        for (pos, t) in tasks.iter().enumerate() {
            if t.is_null {
                if self.is_empty()
                    && let Some(slot) = shown.get_mut(pos)
                {
                    *slot = true;
                }
                continue;
            }
            let level = t.outline_level.get();
            while stack.last().is_some_and(|&(_, l)| l >= level) {
                stack.pop();
            }
            let summary = summaries.get(pos).copied().unwrap_or(false);
            if prepared.matches(self, ctx, t, summary) {
                direct = direct.saturating_add(1);
                if let Some(slot) = shown.get_mut(pos) {
                    *slot = true;
                }
                if self.show_related_summaries {
                    for &(anc, _) in stack.iter().rev() {
                        match shown.get_mut(anc) {
                            Some(slot) if !*slot => {
                                *slot = true;
                                related = related.saturating_add(1);
                            }
                            // Déjà montré : ses propres ancêtres aussi.
                            _ => break,
                        }
                    }
                }
            }
            stack.push((pos, level));
        }
        FilterHits {
            shown,
            direct,
            related,
        }
    }
}

/// Récapitulative à la position `pos` (R-052, lignes vides ignorées).
fn summary_at(project: &Project, pos: usize) -> bool {
    crate::command::outline::is_summary_at(&project.tasks, pos)
}

/// Drapeau récapitulatif de chaque position, en une passe à rebours : une
/// tâche non vide est récapitulative si la tâche non vide suivante est plus
/// profonde (R-052).
fn summary_flags(project: &Project) -> Vec<bool> {
    let tasks = &project.tasks;
    let mut flags = vec![false; tasks.len()];
    let mut next_level: Option<u16> = None;
    for (pos, t) in tasks.iter().enumerate().rev() {
        if t.is_null {
            continue;
        }
        let level = t.outline_level.get();
        if let Some(slot) = flags.get_mut(pos) {
            *slot = next_level.is_some_and(|n| n > level);
        }
        next_level = Some(level);
    }
    flags
}

/// Préparation d'un filtre : texte replié une fois, tâches affectées à la
/// ressource nommée.
struct Prepared {
    /// `name_contains` replié.
    needle: Option<String>,
    /// Tâches affectées à une ressource du nom cherché.
    with_resource: BTreeSet<TaskUid>,
}

impl Prepared {
    fn new(f: &TaskFilter, ctx: &FilterCtx<'_>) -> Self {
        let mut with_resource = BTreeSet::new();
        if let Some(name) = &f.resource {
            // Espaces de bord ignorés (relecture finale de J8, F5 du lot
            // règles) : la fenêtre « Utilisant la ressource… » propose les
            // noms sans eux, un nom relu d'un MSPDI peut en porter.
            let name = name.trim();
            let wanted: BTreeSet<_> = ctx
                .project
                .resources
                .iter()
                .filter(|r| r.name.trim() == name)
                .map(plannercraft_core::HasUid::uid)
                .collect();
            if !wanted.is_empty() {
                with_resource.extend(
                    ctx.project
                        .assignments
                        .iter()
                        .filter(|a| a.resource.is_some_and(|r| wanted.contains(&r)))
                        .map(|a| a.task),
                );
            }
        }
        Self {
            needle: f.name_contains.as_deref().map(fold_text),
            with_resource,
        }
    }

    /// Conjonction des critères posés sur la tâche non vide `t`.
    fn matches(&self, f: &TaskFilter, ctx: &FilterCtx<'_>, t: &Task, summary: bool) -> bool {
        if t.is_null {
            return f.is_empty();
        }
        let s = ctx.schedule.scheduled(t.uid());
        let want = |c: Option<bool>, v: bool| c.is_none_or(|w| w == v);
        let critical = s.is_some_and(|s| s.critical);
        let late = s.is_some_and(|s| is_late(t, s, ctx.status));
        let incomplete = s.is_some_and(|s| s.role == TaskRole::Leaf && s.percent_complete < Percent::FULL);
        want(f.critical, critical)
            && want(f.late, late)
            && want(f.incomplete, incomplete)
            && want(f.milestone, t.milestone)
            && want(f.summary, summary)
            && want(f.active, t.active)
            && (f.resource.is_none() || self.with_resource.contains(&t.uid()))
            && f.from
                .is_none_or(|from| s.is_some_and(|s| s.finish.day() >= from))
            && f.to.is_none_or(|to| s.is_some_and(|s| s.start.day() <= to))
            && f.outline_max.is_none_or(|m| t.outline_level.get() <= m)
            && self
                .needle
                .as_deref()
                .is_none_or(|n| fold_text(&t.name).contains(n))
    }
}

/// R-236 : tâche planifiée en retard. Vrai si sa fin planifiée dépasse son
/// échéance (J4) ou la fin de sa ligne de base 0, ou si, à la date d'état
/// effective `status` (R-034), elle n'est pas achevée alors que sa fin
/// planifiée est atteinte, ou pas commencée alors que son début planifié
/// est passé. État d'avancement lu dans le `Schedule` (R-200, R-210).
pub(crate) fn is_late(t: &Task, s: &TaskSchedule, status: Moment) -> bool {
    let past_deadline = t.deadline.is_some_and(|d| s.finish > d);
    let past_baseline = t
        .baselines
        .get(&BaselineNumber::PRIMARY)
        .and_then(|b| b.finish)
        .is_some_and(|f| s.finish > f);
    let state = s.progress.map_or(ProgressState::NotStarted, |g| g.state);
    let unfinished = state != ProgressState::Complete && s.finish <= status;
    let unstarted = state == ProgressState::NotStarted && s.start < status;
    past_deadline || past_baseline || unfinished || unstarted
}

/// Texte replié pour la recherche (R-251) : décomposition canonique (NFD,
/// `unicode-normalization`), marques combinantes retirées, minuscules
/// (`char::to_lowercase`, tables Unicode de la bibliothèque standard :
/// même résultat sur toutes les plateformes). « Plâtrerie » ⇒
/// « platrerie » ; « Œ » ⇒ « œ » (ligature gardée).
#[must_use]
pub fn fold_text(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    use unicode_normalization::char::is_combining_mark;
    s.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}
