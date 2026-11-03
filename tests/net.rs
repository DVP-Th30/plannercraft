//! Constructeur de petits réseaux pour les tests du moteur (spécification J2
//! § 5.1) : tâches nommées, UID attribués dans l'ordre d'ajout (UID = ID),
//! calendrier Standard, planification depuis le début, options par défaut.
//!
//! ```ignore
//! let run = Net::new("2026-10-19T08:00").task("A", 960).task("B", 480).fs("A", "B").run();
//! assert_eq!(run.at("B").early.start, moment("2026-10-21T08:00"));
//! ```

use std::collections::BTreeMap;

use plannercraft_core::calendar::BuiltinCalendarNames;
use plannercraft_core::{
    CalendarUid, Constraint, Duration, DurationUnit, Lag, LagValue, Link, LinkType, Minutes, OutlineLevel,
    Project, ProjectInit, ProjectOptions, Schedule, ScheduleAnchor, SchedulingMode, SignedPercent, Task,
    TaskOutcome, TaskSchedule, TaskUid, TimeBasis,
};

use crate::moment;

/// Réseau en construction.
pub struct Net {
    project: Project,
    names: BTreeMap<String, TaskUid>,
    next_uid: u32,
}

/// Résultat d'un calcul, avec les noms des tâches.
pub struct Run {
    /// Planning calculé.
    pub schedule: Schedule,
    /// Noms → UID.
    pub names: BTreeMap<String, TaskUid>,
}

/// Durée ouvrée ou écoulée, unité jour, non estimée.
fn duration(minutes: i64, basis: TimeBasis) -> Duration {
    match Duration::new(Minutes::new(minutes), DurationUnit::Day, basis, false) {
        Ok(d) => d,
        Err(e) => panic!("durée de test invalide {minutes} : {e}"),
    }
}

impl Net {
    /// Projet neuf commençant à `start` (`"2026-10-19T08:00"`) : calendrier
    /// Standard (UID 1), 24 heures (UID 2), Équipe de nuit (UID 3).
    #[must_use]
    pub fn new(start: &str) -> Self {
        let project = Project::new(ProjectInit {
            name: "Test".to_owned(),
            anchor: ScheduleAnchor::FromStart(moment(start)),
            current_date: moment(start),
            calendar_names: BuiltinCalendarNames::default(),
        });
        Self {
            project,
            names: BTreeMap::new(),
            next_uid: 1,
        }
    }

    /// UID de la tâche `name`.
    ///
    /// # Panics
    ///
    /// Si la tâche n'existe pas.
    #[must_use]
    pub fn uid(&self, name: &str) -> TaskUid {
        match self.names.get(name) {
            Some(u) => *u,
            None => panic!("tâche de test inconnue « {name} »"),
        }
    }

    fn task_mut(&mut self, name: &str) -> &mut Task {
        let uid = self.uid(name);
        match self.project.tasks.get_mut(uid) {
            Some(t) => t,
            None => panic!("tâche de test « {name} » absente du projet"),
        }
    }

    /// Ajoute une tâche d'UID donné.
    ///
    /// # Panics
    ///
    /// Si le nom ou l'UID est déjà pris.
    #[must_use]
    pub fn task_with_uid(mut self, name: &str, uid: u32, minutes: i64) -> Self {
        let options = self.project.options;
        let mut t = Task::new(TaskUid::new(uid), name.to_owned(), &options);
        t.duration = duration(minutes, TimeBasis::Working);
        // R-108 : ALAP par défaut à rebours.
        t.constraint = Constraint::default_for(&self.project.anchor);
        if let Err(e) = self.project.tasks.push(t) {
            panic!("tâche de test « {name} » : {e}");
        }
        assert!(
            self.names.insert(name.to_owned(), TaskUid::new(uid)).is_none(),
            "nom de tâche de test en double « {name} »"
        );
        self.next_uid = self.next_uid.max(uid.saturating_add(1));
        self
    }

    /// Ajoute une tâche automatique de `minutes` minutes ouvrées (unité jour).
    #[must_use]
    pub fn task(self, name: &str, minutes: i64) -> Self {
        let uid = self.next_uid;
        self.task_with_uid(name, uid, minutes)
    }

    /// Ajoute une tâche à durée écoulée.
    #[must_use]
    pub fn elapsed(self, name: &str, minutes: i64) -> Self {
        let mut net = self.task(name, 0);
        net.task_mut(name).duration = duration(minutes, TimeBasis::Elapsed);
        net
    }

    /// Ajoute une ligne vide (`IsNull`).
    #[must_use]
    pub fn null_row(self, name: &str) -> Self {
        let mut net = self.task(name, 0);
        net.task_mut(name).is_null = true;
        net
    }

    /// Niveau hiérarchique de la tâche.
    ///
    /// # Panics
    ///
    /// Si le niveau vaut 0.
    #[must_use]
    pub fn level(mut self, name: &str, level: u16) -> Self {
        self.task_mut(name).outline_level = match OutlineLevel::new(level) {
            Ok(l) => l,
            Err(e) => panic!("niveau de test invalide : {e}"),
        };
        self
    }

    /// Calendrier propre de la tâche.
    #[must_use]
    pub fn calendar(mut self, name: &str, uid: u32) -> Self {
        self.task_mut(name).calendar = Some(CalendarUid::new(uid));
        self
    }

    /// Passe la tâche en mode manuel avec ces dates saisies (durée saisie :
    /// aucune).
    #[must_use]
    pub fn manual(mut self, name: &str, start: Option<&str>, finish: Option<&str>) -> Self {
        self.task_mut(name).mode = SchedulingMode::Manual {
            start: start.map(moment),
            finish: finish.map(moment),
            duration: None,
        };
        self
    }

    /// Rend la tâche inactive.
    #[must_use]
    pub fn inactive(mut self, name: &str) -> Self {
        self.task_mut(name).active = false;
        self
    }

    /// Contrainte de la tâche (R-090 à R-105).
    #[must_use]
    pub fn constraint(mut self, name: &str, c: Constraint) -> Self {
        self.task_mut(name).constraint = c;
        self
    }

    /// Échéance de la tâche (R-102).
    #[must_use]
    pub fn deadline(mut self, name: &str, date: &str) -> Self {
        self.task_mut(name).deadline = Some(moment(date));
        self
    }

    /// Planification à rebours depuis la fin `finish` (R-106) ; les tâches
    /// ajoutées ensuite sont ALAP (contrainte par défaut à rebours, R-108).
    /// À appeler avant d'ajouter les tâches.
    #[must_use]
    pub fn from_finish(mut self, finish: &str) -> Self {
        self.project.anchor = ScheduleAnchor::FromFinish(moment(finish));
        self
    }

    /// Option « les tâches respectent toujours leurs dates de contrainte »
    /// (R-099).
    #[must_use]
    pub fn honor(mut self, honor: bool) -> Self {
        self.project.options.honor_constraints = honor;
        self
    }

    /// Lien `pred` → `succ`.
    #[must_use]
    pub fn link(mut self, pred: &str, succ: &str, kind: LinkType, lag: Lag) -> Self {
        let predecessor = self.uid(pred);
        self.task_mut(succ).predecessors.push(Link {
            predecessor,
            kind,
            lag,
        });
        self
    }

    /// Lien fin à début sans décalage.
    #[must_use]
    pub fn fs(self, pred: &str, succ: &str) -> Self {
        self.link(pred, succ, LinkType::FinishToStart, Lag::ZERO)
    }

    /// Modifie les options du projet.
    #[must_use]
    pub fn options(mut self, f: impl FnOnce(&mut ProjectOptions)) -> Self {
        f(&mut self.project.options);
        self
    }

    /// Modifie le projet directement (calendriers, ancre…).
    #[must_use]
    pub fn project(mut self, f: impl FnOnce(&mut Project)) -> Self {
        f(&mut self.project);
        self
    }

    /// Modifie une tâche directement.
    #[must_use]
    pub fn with_task(mut self, name: &str, f: impl FnOnce(&mut Task)) -> Self {
        f(self.task_mut(name));
        self
    }

    /// Projet construit.
    #[must_use]
    pub fn build(self) -> Project {
        self.project
    }

    /// Calcule le planning.
    #[must_use]
    pub fn run(self) -> Run {
        self.run_with(plannercraft_core::Limits::default())
    }

    /// Calcule le planning sous des plafonds explicites.
    #[must_use]
    pub fn run_with(self, limits: plannercraft_core::Limits) -> Run {
        Run {
            schedule: plannercraft_core::schedule_with(&self.project, limits),
            names: self.names,
        }
    }
}

impl Run {
    /// UID de la tâche `name`.
    ///
    /// # Panics
    ///
    /// Si la tâche n'existe pas.
    #[must_use]
    pub fn uid(&self, name: &str) -> TaskUid {
        match self.names.get(name) {
            Some(u) => *u,
            None => panic!("tâche de test inconnue « {name} »"),
        }
    }

    /// Résultat de la tâche `name`.
    ///
    /// # Panics
    ///
    /// Si la tâche n'a pas de résultat.
    #[must_use]
    pub fn outcome(&self, name: &str) -> &TaskOutcome {
        match self.schedule.get(self.uid(name)) {
            Some(o) => o,
            None => panic!("aucun résultat pour « {name} »"),
        }
    }

    /// Valeurs calculées de la tâche planifiée `name`.
    ///
    /// # Panics
    ///
    /// Si la tâche n'est pas planifiée.
    #[must_use]
    pub fn at(&self, name: &str) -> &TaskSchedule {
        match self.outcome(name) {
            TaskOutcome::Scheduled(s) => s,
            o @ TaskOutcome::Excluded { .. } => panic!("« {name} » n'est pas planifiée : {o:?}"),
        }
    }

    /// Codes des diagnostics, dans l'ordre du planning.
    #[must_use]
    pub fn codes(&self) -> Vec<&'static str> {
        self.schedule
            .diagnostics
            .iter()
            .map(plannercraft_core::Diagnostic::code)
            .collect()
    }
}

/// Décalage ouvré de `minutes` minutes (unité jour).
#[must_use]
pub fn lag_w(minutes: i64) -> Lag {
    lag(TimeBasis::Working, minutes)
}

/// Décalage écoulé de `minutes` minutes.
#[must_use]
pub fn lag_e(minutes: i64) -> Lag {
    lag(TimeBasis::Elapsed, minutes)
}

fn lag(basis: TimeBasis, minutes: i64) -> Lag {
    let value = LagValue::Time {
        minutes: Minutes::new(minutes),
        unit: DurationUnit::Day,
    };
    match Lag::new(basis, value) {
        Ok(l) => l,
        Err(e) => panic!("décalage de test invalide {minutes} : {e}"),
    }
}

/// Décalage ouvré en pourcentage (`pct` en %, 50 = 50 %).
#[must_use]
pub fn lag_pct(pct: i64) -> Lag {
    lag_percent(TimeBasis::Working, pct)
}

/// Décalage écoulé en pourcentage (« e% »).
#[must_use]
pub fn lag_epct(pct: i64) -> Lag {
    lag_percent(TimeBasis::Elapsed, pct)
}

fn lag_percent(basis: TimeBasis, pct: i64) -> Lag {
    let Some(Ok(percent)) = pct.checked_mul(100).map(SignedPercent::new) else {
        panic!("pourcentage de test invalide {pct}")
    };
    match Lag::new(basis, LagValue::Percent { percent }) {
        Ok(l) => l,
        Err(e) => panic!("décalage de test invalide {pct} % : {e}"),
    }
}
