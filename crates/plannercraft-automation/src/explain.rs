//! Vue `analysis.explain_date` (J6, lot L7, R-230 à R-233) : l'explication
//! du cœur (`plannercraft_core::schedule::explain`) mise en JSON, et ses deux
//! résumés rédigés par gabarit (R-233).
//!
//! Contrat de surface (C-MCP-01, règle 3) : toute date, durée et marge vient
//! de l'[`Explanation`] calculée par le moteur ; ce module ne fait que les
//! recopier (dates ISO locales, durées `{minutes, unit, text}`, ADR-0012
//! D6) et les écrire en texte (R-143, précédé du jour de semaine). Aucune
//! date n'est calculée ici.
//!
//! Séparé de `view.rs` (partagé avec le lot L8) ; [`explain_view`] sert aussi
//! à `what_if` (explications sur l'état simulé, R-234).

use std::collections::BTreeMap;

use plannercraft_core::schedule::explain::{
    ChainLink, DEFAULT_DEPTH, ExplainError, Explanation, ResumeCause, Side, Step, explain,
};
use plannercraft_core::{
    CalendarUid, ConstraintKind, Driver, Duration, DurationUnit, LateDriver, LinkType, Minutes, Moment,
    Project, TaskUid, TimeBasis, Weekday,
};
use serde::Serialize;

use crate::command::TaskRef;
use crate::command::params::ExplainParams;
use crate::command::resolve_task;
use crate::error::{CommandError, CommandErrorKind, codes};
use crate::json::{ConstraintKindJson, DurationJson, LinkTypeJson, SideJson};
use crate::session::Session;
use crate::text::format_date;
use crate::text::{Language, TextContext};
use crate::view::signed_minutes;

/// Explication d'une date (`analysis.explain_date`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ExplainView {
    /// Tâche expliquée.
    pub task: TaskNameView,
    /// Date expliquée.
    pub side: SideJson,
    /// Valeur de la date (celle du moteur).
    pub date: String,
    /// Résumé en français (gabarit, R-233).
    pub summary_fr: String,
    /// Résumé en anglais (gabarit, R-233).
    pub summary_en: String,
    /// Étapes, dans l'ordre (R-230, R-231).
    pub steps: Vec<StepView>,
    /// Côté au plus tard (R-231).
    pub late: LateView,
    /// Chaîne des pilotes, de la tâche vers l'amont (R-232).
    pub chain: Vec<ChainView>,
    /// Chaîne coupée à la profondeur demandée.
    pub truncated: bool,
    /// L'explication repose sur une hypothèse (R-206 : tâche en cours).
    pub hypothesis: bool,
}

/// Tâche désignée par une explication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaskNameView {
    /// UID.
    pub uid: u32,
    /// ID (ligne) ; absent pour l'UID 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// Nom.
    pub name: String,
}

/// Calendrier cité.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CalendarRefView {
    /// UID.
    pub uid: u32,
    /// Nom (vide si le calendrier est inconnu).
    pub name: String,
}

/// Étape d'une explication (miroir de `Step`, dates en texte ISO).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum StepView {
    /// Pilote et borne brute.
    Driver {
        /// Pilote.
        driver: DriverView,
        /// Borne brute (absente pour `manual`, `rollup`, `actual`).
        #[serde(skip_serializing_if = "Option::is_none")]
        raw: Option<String>,
    },
    /// Recalage sur le calendrier de placement.
    Snap {
        /// Borne brute.
        raw: String,
        /// Instant recalé.
        snapped: String,
        /// Calendrier de placement (absent en temps écoulé).
        #[serde(skip_serializing_if = "Option::is_none")]
        calendar: Option<CalendarRefView>,
    },
    /// Retard de nivellement.
    LevelingDelay {
        /// Retard.
        delay: DurationJson,
        /// Début non retardé.
        from: String,
        /// Début retardé.
        to: String,
    },
    /// Dates réelles.
    Actual {
        /// Début réel.
        start: String,
        /// Fin réelle (tâche achevée).
        #[serde(skip_serializing_if = "Option::is_none")]
        finish: Option<String>,
    },
    /// Reprise d'une tâche en cours.
    Resume {
        /// Fin de la partie réelle.
        stop: String,
        /// Reprise.
        resume: String,
        /// Cause.
        cause: ResumeCauseView,
    },
    /// Durée placée.
    Duration {
        /// Durée (ou reste).
        duration: DurationJson,
        /// Calendrier de placement (absent en temps écoulé).
        #[serde(skip_serializing_if = "Option::is_none")]
        calendar: Option<CalendarRefView>,
        /// Affectation qui finit la dernière (UID).
        #[serde(skip_serializing_if = "Option::is_none")]
        longest: Option<u32>,
    },
    /// Contrainte de date qui ne lie pas.
    ConstraintNotBinding {
        /// Type.
        constraint: ConstraintKindJson,
        /// Date.
        date: String,
    },
    /// Dates de repli.
    Degraded {
        /// Code stable de la cause.
        code: String,
    },
}

/// Pilote du début (R-044) en JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DriverView {
    /// Début du projet.
    ProjectStart,
    /// Porte d'une récapitulative.
    SummaryGate {
        /// Récapitulative (UID).
        summary: u32,
    },
    /// Lien.
    Link {
        /// Prédécesseur (UID).
        predecessor: u32,
        /// Type.
        link: LinkTypeJson,
    },
    /// Dates saisies.
    Manual,
    /// Agrégat des sous-tâches.
    Rollup,
    /// Contrainte.
    Constraint {
        /// Type.
        constraint: ConstraintKindJson,
    },
    /// Début réel.
    Actual,
}

/// Cause de la fin au plus tard (R-047) en JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LateDriverView {
    /// Fin du projet.
    ProjectFinish,
    /// Lien sortant.
    Successor {
        /// Successeur (UID).
        successor: u32,
        /// Type.
        link: LinkTypeJson,
    },
    /// Plafond d'une récapitulative.
    SummaryCap {
        /// Récapitulative (UID).
        summary: u32,
    },
    /// Plusieurs chemins critiques.
    MultipleCriticalPaths,
    /// Contrainte.
    Constraint {
        /// Type.
        constraint: ConstraintKindJson,
    },
    /// Échéance.
    Deadline,
    /// Dates saisies.
    Manual,
    /// Agrégat.
    Rollup,
    /// Repli.
    Fallback,
    /// Dates réelles.
    Actual,
}

/// Cause d'une reprise (R-204, R-205) en JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResumeCauseView {
    /// Instant ouvré suivant la partie réelle.
    NextWorking,
    /// Début du projet.
    ProjectStart,
    /// Borne de la récapitulative parente.
    SummaryGate {
        /// Récapitulative (UID).
        summary: u32,
    },
    /// Lien.
    Link {
        /// Prédécesseur (UID).
        predecessor: u32,
        /// Type.
        link: LinkTypeJson,
    },
    /// Contrainte de début.
    Constraint {
        /// Type.
        constraint: ConstraintKindJson,
    },
    /// Reprise saisie.
    Entered,
}

/// Côté au plus tard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LateView {
    /// Début au plus tard.
    pub start: String,
    /// Fin au plus tard.
    pub finish: String,
    /// Cause de la fin au plus tard.
    pub driver: LateDriverView,
    /// Marge totale.
    pub total_slack: DurationJson,
    /// Marge libre.
    pub free_slack: DurationJson,
    /// Critique.
    pub critical: bool,
}

/// Maillon de la chaîne.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChainView {
    /// UID.
    pub uid: u32,
    /// ID (ligne).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    /// Nom.
    pub name: String,
    /// Début planifié.
    pub start: String,
    /// Fin planifiée.
    pub finish: String,
    /// Pilote de son début.
    pub driver: DriverView,
    /// Borne brute de son pilote.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

/// `analysis.explain_date` sur l'état courant de la session.
///
/// # Errors
///
/// `task.not_found` (champ `task`), `batch.unknown_label` pour une
/// étiquette (aucun lot en cours), `explain.not_scheduled`,
/// `param.invalid` (champ `depth`) hors de 1 à 50.
pub fn explain_date(session: &Session, params: &ExplainParams) -> Result<ExplainView, CommandError> {
    let p = session.project();
    let s = session.schedule();
    let uid = match &params.task {
        TaskRef::Uid(u) if *u == TaskUid::PROJECT_SUMMARY => *u,
        r => resolve_task(r, &BTreeMap::new(), p, "task")?,
    };
    let side = params.side.unwrap_or_default().into();
    let depth = params.depth.unwrap_or(DEFAULT_DEPTH);
    let x = explain(p, s, uid, side, depth).map_err(explain_error)?;
    Ok(explain_view(p, &x, &session.text_context()))
}

/// Erreur de commande d'une explication impossible.
#[must_use]
pub fn explain_error(e: ExplainError) -> CommandError {
    match e {
        ExplainError::UnknownTask(uid) => {
            CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid)).with_field("task")
        }
        ExplainError::NotScheduled(uid) => CommandError::new(
            codes::EXPLAIN_NOT_SCHEDULED,
            CommandErrorKind::ExplainNotScheduled(uid),
        )
        .with_field("task"),
        ExplainError::DepthOutOfRange(d) => CommandError::new(
            codes::PARAM_INVALID,
            CommandErrorKind::InvalidParam(format!("depth {d} : 1 à 50 / 1 to 50")),
        )
        .with_field("depth"),
    }
}

/// Vue d'une explication calculée sur `p` ; durées dans le contexte
/// `text` (unité par défaut du projet).
#[must_use]
pub fn explain_view(p: &Project, x: &Explanation, text: &TextContext) -> ExplainView {
    let iso = |m: Moment| m.to_string();
    let unit = p.options.default_duration_unit;
    let basis = p
        .tasks
        .get(x.task)
        .map_or(TimeBasis::Working, |t| t.duration.basis());
    let steps = x
        .steps
        .iter()
        .map(|st| match *st {
            Step::Driver { driver, raw } => StepView::Driver {
                driver: driver_view(driver),
                raw: raw.map(iso),
            },
            Step::Snap {
                raw,
                snapped,
                calendar,
            } => StepView::Snap {
                raw: iso(raw),
                snapped: iso(snapped),
                calendar: calendar.map(|c| calendar_ref(p, c)),
            },
            Step::LevelingDelay { delay, from, to } => {
                let basis = if delay.elapsed {
                    TimeBasis::Elapsed
                } else {
                    TimeBasis::Working
                };
                StepView::LevelingDelay {
                    delay: duration_out(delay.minutes, delay.unit, basis, text),
                    from: iso(from),
                    to: iso(to),
                }
            }
            Step::Actual { start, finish } => StepView::Actual {
                start: iso(start),
                finish: finish.map(iso),
            },
            Step::Resume { stop, resume, cause } => StepView::Resume {
                stop: iso(stop),
                resume: iso(resume),
                cause: cause_view(cause),
            },
            Step::Duration {
                minutes,
                calendar,
                longest,
            } => StepView::Duration {
                duration: duration_out(minutes, unit, basis, text),
                calendar: calendar.map(|c| calendar_ref(p, c)),
                longest: longest.map(plannercraft_core::AssignmentUid::get),
            },
            Step::ConstraintNotBinding { kind, date } => StepView::ConstraintNotBinding {
                constraint: constraint_json(kind),
                date: iso(date),
            },
            Step::Degraded { code } => StepView::Degraded {
                code: code.to_owned(),
            },
        })
        .collect();
    ExplainView {
        task: task_name(p, x.task),
        side: match x.side {
            Side::Start => SideJson::Start,
            Side::Finish => SideJson::Finish,
        },
        date: iso(x.date),
        summary_fr: summary(p, x, Language::Fr),
        summary_en: summary(p, x, Language::En),
        steps,
        late: LateView {
            start: iso(x.late.late.start),
            finish: iso(x.late.late.finish),
            driver: late_driver_view(x.late.driver),
            total_slack: signed_minutes(x.late.total_slack, unit, text),
            free_slack: signed_minutes(x.late.free_slack, unit, text),
            critical: x.late.critical,
        },
        chain: x
            .chain
            .iter()
            .map(|l: &ChainLink| {
                let t = task_name(p, l.uid);
                ChainView {
                    uid: t.uid,
                    id: t.id,
                    name: t.name,
                    start: iso(l.start),
                    finish: iso(l.finish),
                    driver: driver_view(l.driver),
                    raw: l.raw.map(iso),
                }
            })
            .collect(),
        truncated: x.truncated,
        hypothesis: x.hypothesis,
    }
}

/// Durée de sortie dans l'unité `unit`, sur la base `basis` ; hors des
/// bornes du modèle, comme [`signed_minutes`].
fn duration_out(minutes: Minutes, unit: DurationUnit, basis: TimeBasis, text: &TextContext) -> DurationJson {
    match Duration::new(minutes, unit, basis, false) {
        Ok(d) if basis == TimeBasis::Elapsed => DurationJson::from_duration(&d, text),
        _ => signed_minutes(minutes, unit, text),
    }
}

fn task_name(p: &Project, uid: TaskUid) -> TaskNameView {
    TaskNameView {
        uid: uid.get(),
        id: p.task_id(uid).map(plannercraft_core::TaskId::get),
        name: plain_name(p, uid),
    }
}

/// Nom brut d'une tâche (UID 0 : nom de la récapitulative du projet, sinon
/// du projet) ; vide si elle n'en a pas.
fn plain_name(p: &Project, uid: TaskUid) -> String {
    if uid == TaskUid::PROJECT_SUMMARY {
        return p
            .summary_task
            .as_ref()
            .map(|t| t.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| p.info.name.clone());
    }
    p.tasks.get(uid).map(|t| t.name.clone()).unwrap_or_default()
}

fn calendar_ref(p: &Project, uid: CalendarUid) -> CalendarRefView {
    CalendarRefView {
        uid: uid.get(),
        name: p.calendars.get(uid).map(|c| c.name.clone()).unwrap_or_default(),
    }
}

fn driver_view(d: Driver) -> DriverView {
    match d {
        Driver::ProjectStart => DriverView::ProjectStart,
        Driver::SummaryGate { summary } => DriverView::SummaryGate {
            summary: summary.get(),
        },
        Driver::Link { predecessor, link } => DriverView::Link {
            predecessor: predecessor.get(),
            link: link_json(link),
        },
        Driver::Manual => DriverView::Manual,
        Driver::Rollup => DriverView::Rollup,
        Driver::Constraint { kind } => DriverView::Constraint {
            constraint: constraint_json(kind),
        },
        Driver::Actual => DriverView::Actual,
    }
}

fn late_driver_view(d: LateDriver) -> LateDriverView {
    match d {
        LateDriver::ProjectFinish => LateDriverView::ProjectFinish,
        LateDriver::Successor { successor, link } => LateDriverView::Successor {
            successor: successor.get(),
            link: link_json(link),
        },
        LateDriver::SummaryCap { summary } => LateDriverView::SummaryCap {
            summary: summary.get(),
        },
        LateDriver::MultipleCriticalPaths => LateDriverView::MultipleCriticalPaths,
        LateDriver::Constraint { kind } => LateDriverView::Constraint {
            constraint: constraint_json(kind),
        },
        LateDriver::Deadline => LateDriverView::Deadline,
        LateDriver::Manual => LateDriverView::Manual,
        LateDriver::Rollup => LateDriverView::Rollup,
        LateDriver::Fallback => LateDriverView::Fallback,
        LateDriver::Actual => LateDriverView::Actual,
    }
}

fn cause_view(c: ResumeCause) -> ResumeCauseView {
    match c {
        ResumeCause::NextWorking => ResumeCauseView::NextWorking,
        ResumeCause::ProjectStart => ResumeCauseView::ProjectStart,
        ResumeCause::SummaryGate { summary } => ResumeCauseView::SummaryGate {
            summary: summary.get(),
        },
        ResumeCause::Link { predecessor, link } => ResumeCauseView::Link {
            predecessor: predecessor.get(),
            link: link_json(link),
        },
        ResumeCause::Constraint(kind) => ResumeCauseView::Constraint {
            constraint: constraint_json(kind),
        },
        ResumeCause::Entered => ResumeCauseView::Entered,
    }
}

fn link_json(l: LinkType) -> LinkTypeJson {
    l.into()
}

fn constraint_json(k: ConstraintKind) -> ConstraintKindJson {
    k.into()
}

// ---------------------------------------------------------------------------
// R-233 : résumés par gabarit.
// ---------------------------------------------------------------------------

/// R-233 : résumé de l'explication `x` dans la langue `lang`, construit par
/// gabarit fixe à partir des seuls champs de `x` (noms des tâches et des
/// calendriers lus dans `p`) ; dates au format de R-143 précédées du jour de
/// semaine abrégé.
#[must_use]
pub fn summary(p: &Project, x: &Explanation, lang: Language) -> String {
    let w = Writer {
        p,
        lang,
        text: TextContext::for_project(p, lang),
    };
    w.explanation(x)
}

/// Rédacteur des résumés.
struct Writer<'a> {
    p: &'a Project,
    lang: Language,
    text: TextContext,
}

impl Writer<'_> {
    const fn tr(&self, fr: &'static str, en: &'static str) -> &'static str {
        match self.lang {
            Language::Fr => fr,
            Language::En => en,
        }
    }

    fn fr(&self) -> bool {
        self.lang == Language::Fr
    }

    /// Date : jour de semaine abrégé et R-143 (« mer 21/10/2026 08:00 »).
    fn date(&self, m: Moment) -> String {
        let day = match (self.lang, m.weekday()) {
            (Language::Fr, Weekday::Monday) => "lun",
            (Language::Fr, Weekday::Tuesday) => "mar",
            (Language::Fr, Weekday::Wednesday) => "mer",
            (Language::Fr, Weekday::Thursday) => "jeu",
            (Language::Fr, Weekday::Friday) => "ven",
            (Language::Fr, Weekday::Saturday) => "sam",
            (Language::Fr, Weekday::Sunday) => "dim",
            (Language::En, Weekday::Monday) => "Mon",
            (Language::En, Weekday::Tuesday) => "Tue",
            (Language::En, Weekday::Wednesday) => "Wed",
            (Language::En, Weekday::Thursday) => "Thu",
            (Language::En, Weekday::Friday) => "Fri",
            (Language::En, Weekday::Saturday) => "Sat",
            (Language::En, Weekday::Sunday) => "Sun",
        };
        format!("{day} {}", format_date(m, &self.text))
    }

    /// Nom d'une tâche, ou « tâche N » si elle n'en a pas.
    fn name(&self, uid: TaskUid) -> String {
        let n = plain_name(self.p, uid);
        if n.is_empty() {
            format!("{} {uid}", self.tr("tâche", "task"))
        } else {
            n
        }
    }

    fn calendar(&self, uid: CalendarUid) -> String {
        self.p
            .calendars
            .get(uid)
            .map(|c| c.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| uid.to_string())
    }

    fn duration(&self, m: Minutes) -> String {
        signed_minutes(m, self.p.options.default_duration_unit, &self.text)
            .text
            .unwrap_or_else(|| format!("{} min", m.get()))
    }

    const fn constraint(&self, k: ConstraintKind) -> &'static str {
        match k {
            ConstraintKind::AsSoonAsPossible => self.tr("« dès que possible »", "\"as soon as possible\""),
            ConstraintKind::AsLateAsPossible => {
                self.tr("« le plus tard possible »", "\"as late as possible\"")
            }
            ConstraintKind::MustStartOn => self.tr("« doit commencer le »", "\"must start on\""),
            ConstraintKind::MustFinishOn => self.tr("« doit finir le »", "\"must finish on\""),
            ConstraintKind::StartNoEarlierThan => {
                self.tr("« début au plus tôt le »", "\"start no earlier than\"")
            }
            ConstraintKind::StartNoLaterThan => {
                self.tr("« début au plus tard le »", "\"start no later than\"")
            }
            ConstraintKind::FinishNoEarlierThan => {
                self.tr("« fin au plus tôt le »", "\"finish no earlier than\"")
            }
            ConstraintKind::FinishNoLaterThan => {
                self.tr("« fin au plus tard le »", "\"finish no later than\"")
            }
        }
    }

    const fn link_code(l: LinkType) -> &'static str {
        match l {
            LinkType::FinishToStart => "FS",
            LinkType::StartToStart => "SS",
            LinkType::FinishToFinish => "FF",
            LinkType::StartToFinish => "SF",
        }
    }

    /// Extrémité du prédécesseur lue par un lien (« fin de B »).
    fn anchor(&self, predecessor: TaskUid, link: LinkType) -> (String, bool) {
        let finish = matches!(link, LinkType::FinishToStart | LinkType::FinishToFinish);
        let n = self.name(predecessor);
        let text = match (self.fr(), finish) {
            (true, true) => format!("fin de {n}"),
            (true, false) => format!("début de {n}"),
            (false, true) => format!("finish of {n}"),
            (false, false) => format!("start of {n}"),
        };
        (text, finish)
    }

    /// Phrase d'un pilote et de sa borne brute ; `show_raw` : la borne est
    /// citée même pour un pilote sans lien. Rend aussi le genre du nom en
    /// tête (féminin), pour l'accord de « reporté ». `None` pour `Actual`.
    fn driver(&self, driver: Driver, raw: Option<Moment>, show_raw: bool) -> Option<(String, bool)> {
        let raw_txt = raw.map(|m| self.date(m));
        let paren = |r: &Option<String>| match r {
            Some(r) if show_raw => format!(" ({r})"),
            _ => String::new(),
        };
        Some(match driver {
            Driver::ProjectStart => (
                format!(
                    "{}{}",
                    self.tr("début du projet", "project start"),
                    paren(&raw_txt)
                ),
                false,
            ),
            Driver::SummaryGate { summary } => {
                let n = self.name(summary);
                let head = if self.fr() {
                    format!("borne de la récapitulative {n}")
                } else {
                    format!("bound of summary task {n}")
                };
                (format!("{head}{}", paren(&raw_txt)), true)
            }
            Driver::Link { predecessor, link } => {
                let (head, feminine) = self.anchor(predecessor, link);
                let code = Self::link_code(link);
                let detail = match (link, &raw_txt, self.fr()) {
                    (LinkType::FinishToStart | LinkType::StartToStart, Some(r), true) => {
                        format!("{r}, lien {code}")
                    }
                    (LinkType::FinishToStart | LinkType::StartToStart, Some(r), false) => {
                        format!("{r}, {code} link")
                    }
                    (_, Some(r), true) => format!("lien {code} : début ramené au {r}"),
                    (_, Some(r), false) => format!("{code} link: start set to {r}"),
                    (_, None, true) => format!("lien {code}"),
                    (_, None, false) => format!("{code} link"),
                };
                (format!("{head} ({detail})"), feminine)
            }
            Driver::Constraint { kind } => {
                let head = format!(
                    "{} {}",
                    self.tr("contrainte", "constraint"),
                    self.constraint(kind)
                );
                let r = raw_txt.map(|r| format!(" ({r})")).unwrap_or_default();
                (format!("{head}{r}"), true)
            }
            Driver::Manual => (
                self.tr("dates saisies (tâche manuelle)", "entered dates (manual task)")
                    .to_owned(),
                false,
            ),
            Driver::Rollup => (
                self.tr("début de sa première sous-tâche", "start of its earliest subtask")
                    .to_owned(),
                false,
            ),
            Driver::Actual => return None,
        })
    }

    fn cause(&self, c: ResumeCause) -> String {
        match c {
            ResumeCause::NextWorking => self
                .tr("premier instant ouvré suivant", "next working time")
                .to_owned(),
            ResumeCause::ProjectStart => self.tr("début du projet", "project start").to_owned(),
            ResumeCause::SummaryGate { summary } => {
                let n = self.name(summary);
                if self.fr() {
                    format!("borne de la récapitulative {n}")
                } else {
                    format!("bound of summary task {n}")
                }
            }
            ResumeCause::Link { predecessor, link } => {
                let (head, _) = self.anchor(predecessor, link);
                let code = Self::link_code(link);
                if self.fr() {
                    format!("{head}, lien {code}")
                } else {
                    format!("{head}, {code} link")
                }
            }
            ResumeCause::Constraint(kind) => {
                format!(
                    "{} {}",
                    self.tr("contrainte", "constraint"),
                    self.constraint(kind)
                )
            }
            ResumeCause::Entered => self.tr("reprise saisie", "entered resume date").to_owned(),
        }
    }

    /// Recalage : « , reportée au premier instant ouvré du calendrier X ».
    fn snap(&self, calendar: Option<CalendarUid>, feminine: bool) -> String {
        let cal = calendar.map(|c| self.calendar(c));
        match (self.fr(), cal) {
            (true, Some(c)) => format!(
                ", {} au premier instant ouvré du calendrier {c}",
                if feminine { "reportée" } else { "reporté" }
            ),
            (true, None) => format!(
                ", {} au premier instant ouvré",
                if feminine { "reportée" } else { "reporté" }
            ),
            (false, Some(c)) => format!(", moved to the next working time of calendar {c}"),
            (false, None) => ", moved to the next working time".to_owned(),
        }
    }

    /// Segments des étapes du début (sans la durée).
    fn start_segments(&self, steps: &[Step]) -> Vec<String> {
        let snapped = steps.iter().any(|s| matches!(s, Step::Snap { .. }));
        let mut segments: Vec<String> = Vec::new();
        let mut feminine = false;
        for st in steps {
            match *st {
                Step::Driver { driver, raw } => {
                    if let Some((text, f)) = self.driver(driver, raw, snapped) {
                        segments.push(text);
                        feminine = f;
                    }
                }
                Step::Snap { calendar, .. } => {
                    let tail = self.snap(calendar, feminine);
                    match segments.last_mut() {
                        Some(last) => last.push_str(&tail),
                        None => segments.push(tail.trim_start_matches(", ").to_owned()),
                    }
                }
                Step::LevelingDelay { delay, to, .. } => segments.push(if self.fr() {
                    format!(
                        "retard de nivellement de {} : début le {}",
                        self.duration(delay.minutes),
                        self.date(to)
                    )
                } else {
                    format!(
                        "leveling delay of {}: start on {}",
                        self.duration(delay.minutes),
                        self.date(to)
                    )
                }),
                Step::Actual { start, finish } => {
                    let mut text = if self.fr() {
                        format!("début réel le {}", self.date(start))
                    } else {
                        format!("actual start on {}", self.date(start))
                    };
                    if let Some(f) = finish {
                        let f = self.date(f);
                        let tail = self.tr(", fin réelle le ", ", actual finish on ");
                        text.push_str(tail);
                        text.push_str(&f);
                    }
                    segments.push(text);
                }
                Step::Resume { stop, resume, cause } => segments.push(if self.fr() {
                    format!(
                        "partie réelle jusqu'au {}, reprise le {} ({})",
                        self.date(stop),
                        self.date(resume),
                        self.cause(cause)
                    )
                } else {
                    format!(
                        "actual part until {}, resumes on {} ({})",
                        self.date(stop),
                        self.date(resume),
                        self.cause(cause)
                    )
                }),
                Step::ConstraintNotBinding { kind, date } => segments.push(if self.fr() {
                    format!(
                        "la contrainte {} du {} ne la déplace pas",
                        self.constraint(kind),
                        self.date(date)
                    )
                } else {
                    format!(
                        "constraint {} on {} does not move it",
                        self.constraint(kind),
                        self.date(date)
                    )
                }),
                Step::Degraded { code } => segments.push(if self.fr() {
                    format!("dates de repli ({code})")
                } else {
                    format!("fallback dates ({code})")
                }),
                Step::Duration { .. } => {}
            }
        }
        segments
    }

    /// « N commence le D : a ; b. »
    fn starts(&self, uid: TaskUid, date: Option<Moment>, segments: &[String]) -> String {
        let n = self.name(uid);
        let head = match (self.fr(), date) {
            (true, Some(d)) => format!("{n} commence le {}", self.date(d)),
            (true, None) => format!("{n} commence"),
            (false, Some(d)) => format!("{n} starts on {}", self.date(d)),
            (false, None) => format!("{n} starts"),
        };
        if segments.is_empty() {
            return format!("{head}.");
        }
        let sep = self.tr(" ; ", "; ");
        let colon = self.tr(" : ", ": ");
        format!("{head}{colon}{}.", segments.join(sep))
    }

    /// Début de la tâche lu dans ses étapes : retardé, recalé, réel ou
    /// borne brute.
    fn start_of(steps: &[Step]) -> Option<Moment> {
        let mut out = None;
        for st in steps {
            match *st {
                Step::Driver { raw, .. } => out = out.or(raw),
                Step::Snap { snapped, .. } => out = Some(snapped),
                Step::LevelingDelay { to, .. } => out = Some(to),
                Step::Actual { start, .. } => out = Some(start),
                _ => {}
            }
        }
        out
    }

    /// « N finit le D : 1j sur le calendrier Standard depuis son début. »
    fn finishes(&self, x: &Explanation) -> String {
        let n = self.name(x.task);
        let resume = x.steps.iter().find_map(|s| match s {
            Step::Resume { resume, .. } => Some(*resume),
            _ => None,
        });
        let duration = x.steps.iter().find_map(|s| match s {
            Step::Duration {
                minutes,
                calendar,
                longest,
            } => Some((*minutes, *calendar, *longest)),
            _ => None,
        });
        let body = match duration {
            None => self.tr("fin réelle", "actual finish").to_owned(),
            Some((minutes, calendar, longest)) => {
                let d = self.duration(minutes);
                let mut text = match (self.fr(), resume) {
                    (true, Some(r)) => format!("reste de {d} depuis la reprise du {}", self.date(r)),
                    (true, None) => format!("{d} depuis son début"),
                    (false, Some(r)) => format!("remaining {d} from the resume on {}", self.date(r)),
                    (false, None) => format!("{d} from its start"),
                };
                if let Some(c) = calendar {
                    text.push_str(self.tr(" sur le calendrier ", " on calendar "));
                    text.push_str(&self.calendar(c));
                }
                if let Some(a) = longest {
                    text.push_str(self.tr(" (affectation ", " (assignment "));
                    text.push_str(&a.to_string());
                    text.push_str(self.tr(", la dernière à finir)", ", the last to finish)"));
                }
                text
            }
        };
        if self.fr() {
            format!("{n} finit le {} : {body}.", self.date(x.date))
        } else {
            format!("{n} finishes on {}: {body}.", self.date(x.date))
        }
    }

    /// Phrase d'un maillon de la chaîne.
    fn chain_link(&self, l: &ChainLink) -> String {
        if l.driver == Driver::ProjectStart {
            let n = self.name(l.uid);
            return if self.fr() {
                format!("{n} commence au début du projet.")
            } else {
                format!("{n} starts at the project start.")
            };
        }
        let snapped = l.raw.is_some_and(|r| r != l.start);
        let mut segments = Vec::new();
        match self.driver(l.driver, l.raw, snapped) {
            Some((mut text, feminine)) => {
                if snapped {
                    text.push_str(&self.snap(None, feminine));
                }
                segments.push(text);
            }
            None => segments.push(self.tr("début réel", "actual start").to_owned()),
        }
        self.starts(l.uid, Some(l.start), &segments)
    }

    fn explanation(&self, x: &Explanation) -> String {
        let segments = self.start_segments(&x.steps);
        let mut sentences = Vec::new();
        match x.side {
            Side::Start => sentences.push(self.starts(x.task, Some(x.date), &segments)),
            Side::Finish => {
                sentences.push(self.finishes(x));
                sentences.push(self.starts(x.task, Self::start_of(&x.steps), &segments));
            }
        }
        sentences.extend(x.chain.iter().map(|l| self.chain_link(l)));
        if x.truncated {
            sentences.push(
                self.tr(
                    "La chaîne continue au-delà de la profondeur demandée.",
                    "The chain goes beyond the requested depth.",
                )
                .to_owned(),
            );
        }
        if x.hypothesis {
            sentences.push(
                self.tr(
                    "Les dates au plus tard reposent sur une hypothèse (tâche en cours, R-206).",
                    "Late dates rely on an assumption (task in progress, R-206).",
                )
                .to_owned(),
            );
        }
        sentences.join(" ")
    }
}
