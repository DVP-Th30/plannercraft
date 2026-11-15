//! Contexte d'exécution d'une commande et répartition (contrat J4, API
//! interne figée pour L4 et L5 ; propriétaire : L2).
//!
//! `Session::apply` exécute chaque opération sur un clone du projet ; une
//! erreur interrompt le lot, l'état de la session reste intact.

use std::collections::BTreeMap;

use plannercraft_core::calendar::{Calendar, CalendarSet};
use plannercraft_core::{Arena, CalendarUid, Diagnostic, Project, ResourceUid, Schedule, Subject, TaskUid};

use super::{
    Command, assignment, baseline, calendar, constraint, custom, group_by, link, progress, project, resource,
    shift, style, task, text_style,
};
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::outcome::SideEffect;
use crate::text::TextContext;
use crate::uid::UidAlloc;

/// État mutable d'un lot en cours d'application.
pub(crate) struct Ctx<'a> {
    /// Le clone du projet, modifié en place.
    pub project: &'a mut Project,
    /// Allocateur d'UID (R-151), cloné pour le lot.
    pub uids: &'a mut UidAlloc,
    /// Étiquettes `$x` → UID créées dans le lot (R-150).
    pub labels: &'a mut BTreeMap<String, TaskUid>,
    /// Effets de bord, dans l'ordre.
    pub effects: &'a mut Vec<SideEffect>,
    /// Sujet → dernier index d'opération qui l'a touché (R-160).
    pub touched: &'a mut BTreeMap<Subject, usize>,
    /// Index de l'opération courante.
    pub op_index: usize,
    /// Contexte du texte (langue du lot ou de la session).
    #[allow(dead_code, reason = "lu par les commandes de L4 et L5 (durées en texte)")]
    pub text: &'a TextContext,
    /// Tâches créées, dans l'ordre.
    pub created: &'a mut Vec<TaskUid>,
    /// Étiquettes `$x` → UID des ressources créées dans le lot (R-195) ;
    /// espace de noms distinct de celui des tâches.
    pub resource_labels: &'a mut BTreeMap<String, ResourceUid>,
    /// Ressources créées, dans l'ordre (R-195).
    pub created_resources: &'a mut Vec<ResourceUid>,
    /// Projet de la session avant le lot (J6, R-216) : tant que le clone
    /// lui est égal, `base_schedule` est son planning.
    pub base: &'a Project,
    /// Planning de la session avant le lot (J6, R-216, R-217, R-227).
    pub base_schedule: &'a Schedule,
    /// Avertissements et informations émis par les commandes elles-mêmes
    /// (J6 : `progress.milestone_rounded`, `progress.option_unsupported`,
    /// `baseline.cost_not_computed`, `baseline.task_skipped`), rendus avec
    /// ceux du delta de R-160.
    pub notes: &'a mut Vec<Diagnostic>,
    /// Planning et calendriers mémorisés du lot (relecture finale J6,
    /// J6-PERF-2).
    pub cache: &'a mut BatchCache,
}

/// Travail de recalcul permis dans un lot (relecture finale J6,
/// J6-PERF-2) : chaque recalcul du planning du clone coûte son nombre de
/// tâches et d'affectations (au moins 1) ; au-delà, `limit.exceeded`
/// (`what` = `schedule_recomputes`). 1 000 000 : 200 recalculs
/// d'un projet de 5 000 tâches (environ 4 s sur serveurdvp), 10 d'un projet de
/// 100 000 ; un petit projet n'est pas limité en pratique (10 000
/// opérations au plus par lot).
pub(crate) const MAX_BATCH_SCHEDULE_WORK: u64 = 1_000_000;

/// Mémoire d'un lot (J6-PERF-2) : le dernier planning recalculé du clone,
/// gardé tant que le clone ne change pas (une suite d'opérations qui lisent
/// sans écrire ne recalcule qu'une fois), et les calendriers compilés,
/// gardés tant que les calendriers du projet ne changent pas.
#[derive(Debug)]
pub(crate) struct BatchCache {
    /// Travail de recalcul permis (`SessionLimits::max_batch_schedule_work`).
    budget: u64,
    /// Projet et planning du dernier recalcul.
    last: Option<(Project, Schedule)>,
    /// Le planning courant est `last` (et non celui de la session).
    fresh: bool,
    /// Travail de recalcul consommé.
    spent: u64,
    /// Calendriers (et calendrier du projet) et leur compilation.
    calendars: Option<(Arena<CalendarUid, Calendar>, CalendarUid, CalendarSet)>,
    /// Planning de référence d'une suite de passages en manuel purs
    /// (`task.update { mode: manual }` seul, R-253 ; relecture finale de
    /// J8) : figer une tâche à ses dates courantes ne déplace aucune autre
    /// tâche, la suite lit donc ce planning sans recalcul ; retiré par toute
    /// autre opération.
    freeze: Option<Schedule>,
}

impl BatchCache {
    /// Mémoire vide d'un lot, `budget` de travail de recalcul.
    pub(crate) const fn new(budget: u64) -> Self {
        Self {
            budget,
            last: None,
            fresh: false,
            spent: 0,
            calendars: None,
            freeze: None,
        }
    }
}

impl Ctx<'_> {
    /// Rend à jour le planning courant du clone (J6, R-216, R-217, R-227) :
    /// celui de la session tant que le lot n'a rien changé, sinon le dernier
    /// recalcul s'il a été fait sur le même clone, sinon un recalcul (comme
    /// le correctif J5 (4) pour la durée d'une récapitulative). Le recalcul
    /// ne synchronise pas les durées des feuilles affectées : les dates sont
    /// celles du planning final du lot (R-173 amendée). Lire ensuite par
    /// [`Ctx::schedule`].
    ///
    /// # Errors
    ///
    /// `limit.exceeded` (`schedule_recomputes`) au-delà du budget du lot
    /// (`SessionLimits::max_batch_schedule_work`).
    pub(crate) fn ensure_schedule(&mut self) -> Result<(), CommandError> {
        if *self.project == *self.base {
            self.cache.fresh = false;
            return Ok(());
        }
        if self.cache.last.as_ref().is_some_and(|(p, _)| p == &*self.project) {
            self.cache.fresh = true;
            return Ok(());
        }
        let size = self
            .project
            .tasks
            .len()
            .saturating_add(self.project.assignments.len())
            .max(1);
        let cost = u64::try_from(size).unwrap_or(u64::MAX);
        let spent = self.cache.spent.saturating_add(cost);
        let budget = self.cache.budget;
        if spent > budget {
            return Err(CommandError::new(
                codes::LIMIT_EXCEEDED,
                CommandErrorKind::Limit {
                    what: "schedule_recomputes",
                    max: budget.checked_div(cost).unwrap_or(0),
                },
            ));
        }
        self.cache.spent = spent;
        let s = plannercraft_core::schedule(self.project);
        self.cache.last = Some((self.project.clone(), s));
        self.cache.fresh = true;
        Ok(())
    }

    /// Planning de référence de la suite de passages en manuel purs en
    /// cours (voir `BatchCache::freeze`).
    pub(crate) const fn freeze_schedule(&self) -> Option<&Schedule> {
        self.cache.freeze.as_ref()
    }

    /// Garde le planning courant comme référence des passages en manuel
    /// purs qui suivent.
    pub(crate) fn keep_freeze_schedule(&mut self) {
        let s = self.schedule().clone();
        self.cache.freeze = Some(s);
    }

    /// Planning courant posé par le dernier [`Ctx::ensure_schedule`].
    pub(crate) fn schedule(&self) -> &Schedule {
        match &self.cache.last {
            Some((_, s)) if self.cache.fresh => s,
            _ => self.base_schedule,
        }
    }

    /// Calendriers compilés du clone et le clone lui-même ; recompilés
    /// seulement si ses calendriers (ou le calendrier du projet) ont changé
    /// depuis la dernière compilation du lot.
    pub(crate) fn calendar_set(&mut self) -> (&Project, &CalendarSet) {
        let p = &*self.project;
        let same = self
            .cache
            .calendars
            .as_ref()
            .is_some_and(|(c, uid, _)| *uid == p.calendar && *c == p.calendars);
        if !same {
            self.cache.calendars = None;
        }
        let entry = self.cache.calendars.get_or_insert_with(|| {
            let (set, _) = CalendarSet::compile(p);
            (p.calendars.clone(), p.calendar, set)
        });
        (p, &entry.2)
    }
}

/// Exécution d'une commande sur le contexte.
pub(crate) trait Exec {
    /// Applique la commande au clone du projet.
    fn exec(&self, ctx: &mut Ctx<'_>) -> Result<(), CommandError>;
}

impl Exec for Command {
    fn exec(&self, ctx: &mut Ctx<'_>) -> Result<(), CommandError> {
        // Toute opération autre qu'un passage en manuel pur retire le
        // planning de référence de la suite (R-253, relecture finale J8).
        if !matches!(self, Self::TaskUpdate(op) if task::is_pure_freeze(op)) {
            ctx.cache.freeze = None;
        }
        match self {
            Self::TaskAdd(op) => task::add(op, ctx),
            Self::TaskUpdate(op) => task::update(op, ctx),
            Self::TaskDelete(op) => task::delete(op, ctx),
            Self::TaskMove(op) => task::move_to(op, ctx),
            Self::TaskIndent(op) => task::indent(op, ctx),
            Self::TaskOutdent(op) => task::outdent(op, ctx),
            Self::LinkAdd(op) => link::add(op, ctx),
            Self::LinkUpdate(op) => link::update(op, ctx),
            Self::LinkRemove(op) => link::remove(op, ctx),
            Self::TaskSetConstraint(op) => constraint::set_constraint(op, ctx),
            Self::TaskSetDeadline(op) => constraint::set_deadline(op, ctx),
            Self::ProjectSetInfo(op) => project::set_info(op, ctx),
            Self::ProjectSetAnchor(op) => project::set_anchor(op, ctx),
            Self::ProjectSetStatusDate(op) => project::set_status_date(op, ctx),
            Self::CalendarSetException(op) => calendar::set_exception(op, ctx),
            Self::CalendarRemoveException(op) => calendar::remove_exception(op, ctx),
            Self::ResourceAdd(op) => resource::add(op, ctx),
            Self::ResourceUpdate(op) => resource::update(op, ctx),
            Self::ResourceDelete(op) => resource::delete(op, ctx),
            Self::AssignmentAdd(op) => assignment::add(op, ctx),
            Self::AssignmentUpdate(op) => assignment::update(op, ctx),
            Self::AssignmentRemove(op) => assignment::remove(op, ctx),
            Self::ProgressUpdate(op) => progress::update(op, ctx),
            Self::ProgressReschedule(op) => progress::reschedule(op, ctx),
            Self::BaselineSet(op) => baseline::set(op, ctx),
            Self::CalendarAdd(op) => calendar::add(op, ctx),
            Self::CalendarUpdate(op) => calendar::update(op, ctx),
            Self::CalendarSetWeek(op) => calendar::set_week(op, ctx),
            Self::CalendarDelete(op) => calendar::delete(op, ctx),
            Self::BaselineClear(op) => baseline::clear(op, ctx),
            Self::StyleApply(op) => style::apply(op, ctx),
            Self::StyleSetBarRole(op) => style::set_bar_role(op, ctx),
            Self::StyleSetGroupBy(op) => group_by::set_group_by(op, ctx),
            Self::StyleResetToTemplate(op) => style::reset_to_template(op, ctx),
            Self::CustomFieldRename(op) => custom::rename(op, ctx),
            Self::TaskShift(op) => shift::shift(op, ctx),
            Self::TaskSetTextStyle(op) => text_style::set_text_style(op, ctx),
        }
    }
}
