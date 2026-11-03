//! Diagnostics et erreurs d'ouverture traduits par code (`docs/ui-needs.md`
//! N5 et N9, J8 lot L14).
//!
//! Le moteur et le lecteur MSPDI gardent leur `Display` français ; ils
//! exposent un code stable et des arguments neutres
//! ([`plannercraft_core::DiagArg`]). Ce module choisit le message `diag-<code>`
//! des fichiers `.ftl` et met chaque argument en forme dans la langue des
//! [`Texts`] : UID tels quels, dates par le formateur de l'interface, types
//! de contrainte, passes, sujets et bornes par leurs propres messages. Un
//! code sans message (jamais attendu : un test parcourt tous les codes)
//! retombe sur le `Display` français.
//!
//! [`DiagTexts::with_project`] (refus d'un lot, H-UI-105) nomme les
//! tâches par leur **ID** dans le projet au lieu de leur UID ; une tâche
//! absente du projet se dit « une tâche supprimée ».

use std::borrow::Cow;
use std::collections::BTreeMap;

use plannercraft_core::model::ConstraintKind;
use plannercraft_core::{DiagArg, Diagnostic, OverriddenBound, Pass, Project, Subject, TaskUid};
use plannercraft_mspdi::{ImportCode, ImportDiagnostic, MspdiError};

use crate::format::format_date;
use crate::i18n::{Arg, TextId, Texts, diag_key};

/// Arguments de diagnostic qui portent des tâches (UID ⇒ ID avec
/// [`DiagTexts::with_project`]).
const TASK_ARGS: [&str; 4] = ["task", "predecessor", "successor", "tasks"];

/// Traducteur de diagnostics d'une langue : table des messages `diag-*`
/// construite une fois (un document peut porter 10 000 diagnostics).
pub struct DiagTexts<'a> {
    texts: &'a Texts,
    keys: BTreeMap<&'static str, TextId>,
    /// Projet où lire l'ID des tâches (sinon : UID tels quels).
    project: Option<&'a Project>,
}

impl<'a> DiagTexts<'a> {
    /// Traducteur sur les messages `texts`.
    #[must_use]
    pub fn new(texts: &'a Texts) -> Self {
        let keys = TextId::ALL
            .iter()
            .filter(|id| id.key().starts_with("diag-"))
            .map(|id| (id.key(), *id))
            .collect();
        Self {
            texts,
            keys,
            project: None,
        }
    }

    /// Même traducteur, tâches nommées par leur ID dans `project`
    /// (refus d'un lot, H-UI-105) ; `None` : UID tels quels.
    #[must_use]
    pub const fn with_project(mut self, project: Option<&'a Project>) -> Self {
        self.project = project;
        self
    }

    /// Tâche `uid` telle qu'affichée : son ID si un projet est posé (« une
    /// tâche supprimée » si elle n'y est plus), sinon son UID.
    fn task(&self, uid: u32) -> String {
        match self.project {
            None => uid.to_string(),
            Some(project) => project.task_id(TaskUid::new(uid)).map_or_else(
                || self.texts.get(TextId::RefusalDeletedTask),
                |id| id.get().to_string(),
            ),
        }
    }

    /// Message du code `code`, s'il existe.
    fn message(&self, code: &str) -> Option<TextId> {
        self.keys.get(diag_key(code).as_str()).copied()
    }

    /// Vrai si le code a son message traduit.
    #[must_use]
    pub fn has_message(&self, code: &str) -> bool {
        self.message(code).is_some()
    }

    /// Message traduit `id` avec les arguments neutres `args`.
    fn render(&self, id: TextId, args: &[(&'static str, DiagArg)]) -> String {
        let args: Vec<(&str, Arg<'_>)> = args
            .iter()
            .map(|(name, value)| (*name, self.arg(name, value)))
            .collect();
        self.texts.get_args(id, &args)
    }

    /// Mise en forme d'un argument dans la langue.
    fn arg(&self, name: &str, value: &DiagArg) -> Arg<'static> {
        match value {
            DiagArg::Int(n) => Arg::Count(*n),
            DiagArg::Uid(uid) if TASK_ARGS.contains(&name) => Arg::Str(Cow::Owned(self.task(*uid))),
            DiagArg::Uid(uid) => Arg::Str(Cow::Owned(uid.to_string())),
            DiagArg::Uids(uids) => {
                // Chaîne d'héritage en flèches, cycle de tâches en liste.
                let sep = if name == "chain" { " -> " } else { ", " };
                let list: Vec<String> = if TASK_ARGS.contains(&name) {
                    uids.iter().map(|uid| self.task(*uid)).collect()
                } else {
                    uids.iter().map(ToString::to_string).collect()
                };
                Arg::Str(Cow::Owned(list.join(sep)))
            }
            DiagArg::Moment(m) => Arg::Str(Cow::Owned(format_date(*m, self.texts.format_labels()))),
            DiagArg::Constraint(kind) => Arg::Str(Cow::Owned(self.texts.get(constraint_text(*kind)))),
            DiagArg::Pass(pass) => Arg::Str(Cow::Owned(self.texts.get(match pass {
                Pass::Forward => TextId::DiagPassForward,
                Pass::Backward => TextId::DiagPassBackward,
            }))),
            DiagArg::Subject(subject) => Arg::Str(Cow::Owned(self.subject(subject))),
            DiagArg::Bound(bound) => Arg::Str(Cow::Owned(self.bound(*bound))),
            DiagArg::Text(text) => Arg::Str(Cow::Owned(text.clone())),
            DiagArg::Position { line, column, byte } => Arg::Str(Cow::Owned(self.texts.get_args(
                TextId::DiagLocation,
                &[
                    ("line", Arg::Count(i64::from(*line))),
                    ("column", Arg::Count(i64::from(*column))),
                    ("byte", Arg::Count(i64::try_from(*byte).unwrap_or(i64::MAX))),
                ],
            ))),
        }
    }

    /// Élément du projet (« la tâche 3 », « task 3 »).
    #[must_use]
    pub fn subject(&self, subject: &Subject) -> String {
        let id = |id: TextId, uid: u32| {
            self.texts
                .get_args(id, &[("id", Arg::Str(Cow::Owned(uid.to_string())))])
        };
        match subject {
            Subject::Project => self.texts.get(TextId::DiagSubjectProject),
            Subject::Calendar(c) => id(TextId::DiagSubjectCalendar, c.get()),
            Subject::Task(t) => self.texts.get_args(
                TextId::DiagSubjectTask,
                &[("id", Arg::Str(Cow::Owned(self.task(t.get()))))],
            ),
            Subject::Link {
                predecessor,
                successor,
            } => self.texts.get_args(
                TextId::DiagSubjectLink,
                &[
                    ("predecessor", Arg::Str(Cow::Owned(self.task(predecessor.get())))),
                    ("successor", Arg::Str(Cow::Owned(self.task(successor.get())))),
                ],
            ),
            Subject::Resource(r) => id(TextId::DiagSubjectResource, r.get()),
            Subject::Assignment(a) => id(TextId::DiagSubjectAssignment, a.get()),
        }
    }

    /// Borne violée par une contrainte respectée de force.
    fn bound(&self, bound: OverriddenBound) -> String {
        let with = |id: TextId, name: &'static str, uid: u32| {
            self.texts
                .get_args(id, &[(name, Arg::Str(Cow::Owned(self.task(uid))))])
        };
        match bound {
            OverriddenBound::ProjectStart => self.texts.get(TextId::DiagBoundProjectStart),
            OverriddenBound::ProjectFinish => self.texts.get(TextId::DiagBoundProjectFinish),
            OverriddenBound::SummaryGate { summary } => {
                with(TextId::DiagBoundSummaryGate, "summary", summary.get())
            }
            OverriddenBound::SummaryCap { summary } => {
                with(TextId::DiagBoundSummaryCap, "summary", summary.get())
            }
            OverriddenBound::Predecessor { predecessor, .. } => {
                with(TextId::DiagBoundPredecessor, "predecessor", predecessor.get())
            }
            OverriddenBound::Successor { successor, .. } => {
                with(TextId::DiagBoundSuccessor, "successor", successor.get())
            }
        }
    }

    /// Diagnostic du moteur, traduit ; `Display` français si le code n'a pas
    /// de message.
    #[must_use]
    pub fn core(&self, d: &Diagnostic) -> String {
        match self.message(d.code()) {
            Some(id) => self.render(id, &d.args()),
            None => d.to_string(),
        }
    }

    /// Diagnostic d'import, traduit : message du code, puis élément XML,
    /// valeur brute, sujet et position s'ils sont connus.
    #[must_use]
    pub fn import(&self, d: &ImportDiagnostic) -> String {
        if let ImportCode::Core(core) = &d.code {
            return self.core(core);
        }
        let Some(id) = self.message(&d.code.code()) else {
            return d.to_string();
        };
        let mut text = self.render(id, &d.code.args());
        let mut wrap = |id: TextId, name: &'static str, value: String| {
            text = self.texts.get_args(
                id,
                &[
                    ("head", Arg::Str(Cow::Owned(std::mem::take(&mut text)))),
                    (name, Arg::Str(Cow::Owned(value))),
                ],
            );
        };
        if !d.element.is_empty() {
            wrap(TextId::DiagWithElement, "element", d.element.clone());
        }
        if let Some(value) = &d.value {
            wrap(TextId::DiagWithValue, "value", value.clone());
        }
        if let Some(subject) = &d.subject {
            wrap(TextId::DiagWithDetail, "detail", self.subject(subject));
        }
        if let Some(at) = &d.at {
            let at = self.arg("at", &at.arg());
            if let Arg::Str(at) = at {
                wrap(TextId::DiagWithDetail, "detail", at.into_owned());
            }
        }
        text
    }

    /// Erreur de lecture d'un document MSPDI (N9), traduite ; `Display`
    /// français si le code n'a pas de message.
    #[must_use]
    pub fn mspdi_error(&self, e: &MspdiError) -> String {
        match self.message(e.code()) {
            Some(id) => self.render(id, &e.args()),
            None => e.to_string(),
        }
    }
}

/// Libellé d'un type de contrainte (ceux de la fenêtre « Informations sur
/// la tâche »).
const fn constraint_text(kind: ConstraintKind) -> TextId {
    match kind {
        ConstraintKind::AsSoonAsPossible => TextId::TaskInfoConstraintAsap,
        ConstraintKind::AsLateAsPossible => TextId::TaskInfoConstraintAlap,
        ConstraintKind::MustStartOn => TextId::TaskInfoConstraintMso,
        ConstraintKind::MustFinishOn => TextId::TaskInfoConstraintMfo,
        ConstraintKind::StartNoEarlierThan => TextId::TaskInfoConstraintSnet,
        ConstraintKind::StartNoLaterThan => TextId::TaskInfoConstraintSnlt,
        ConstraintKind::FinishNoEarlierThan => TextId::TaskInfoConstraintFnet,
        ConstraintKind::FinishNoLaterThan => TextId::TaskInfoConstraintFnlt,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_core::{Diagnostic, Subject, TaskUid};
    use plannercraft_mspdi::{ImportCode, ImportDiagnostic, Location, MspdiError};

    use super::DiagTexts;
    use crate::i18n::{Language, Texts};

    /// Chaque code du moteur, de l'import et du lecteur a son message dans
    /// les deux langues, sans repli ni accolade restante, et l'anglais ne
    /// recopie pas le français.
    #[test]
    fn n5_every_code_has_a_message() {
        let fr = Texts::new(Language::Fr);
        let en = Texts::new(Language::En);
        let (dfr, den) = (DiagTexts::new(&fr), DiagTexts::new(&en));
        let mut seen = 0_usize;
        let mut check = |code: &str, f: String, e: String, display: String| {
            assert!(dfr.has_message(code), "{code} sans message");
            assert!(!f.contains('{') && !e.contains('{'), "{code} : {f} / {e}");
            assert_ne!(f, e, "{code} : anglais = français");
            assert_ne!(e, display, "{code} : anglais = Display français");
            seen += 1;
        };
        for d in Diagnostic::examples() {
            check(d.code(), dfr.core(&d), den.core(&d), d.to_string());
        }
        for code in ImportCode::examples() {
            let d = ImportDiagnostic::new(code, None, None, "", None);
            check(&d.code.code(), dfr.import(&d), den.import(&d), d.to_string());
        }
        for e in MspdiError::examples() {
            check(e.code(), dfr.mspdi_error(&e), den.mspdi_error(&e), e.to_string());
        }
        assert_eq!(seen, 61 + 15 + 12);
    }

    /// Le français traduit par code reprend le texte du moteur (dates mises
    /// en forme par l'interface, « → » écrit « -> »).
    #[test]
    fn n5_french_matches_engine_text() {
        let fr = Texts::new(Language::Fr);
        let t = DiagTexts::new(&fr);
        let d = Diagnostic::LinkSelf {
            task: TaskUid::new(3),
        };
        assert_eq!(t.core(&d), d.to_string());
        let d = Diagnostic::LinkIgnoredInCycle {
            predecessor: TaskUid::new(2),
            successor: TaskUid::new(3),
        };
        assert_eq!(t.core(&d), d.to_string().replace('\u{2192}', "->"));
        let plain = |s: String| s.replace('\u{a0}', " ");
        for d in Diagnostic::examples() {
            // Dates : ISO dans le moteur, format court dans l'interface ;
            // `calendar.too_many_segments` sur un seul calendrier, contraintes
            // et pluriels : formulation de l'interface.
            let text = d.to_string();
            if text.contains("1984-")
                || text.contains("« ")
                || text.contains("(s)")
                || d.code() == "constraint.override"
            {
                continue;
            }
            assert_eq!(plain(t.core(&d)), text.replace('\u{2192}', "->"), "{}", d.code());
        }
        let d = Diagnostic::BaselineTaskSkipped { count: 1 };
        assert_eq!(t.core(&d), "1 tâche non planifiée sans ligne de base");
    }

    #[test]
    fn n5_import_diagnostic_suffixes() {
        let en = Texts::new(Language::En);
        let t = DiagTexts::new(&en);
        let d = ImportDiagnostic::new(
            ImportCode::InvalidValue,
            Some(Subject::Task(TaskUid::new(7))),
            Some(Location {
                byte: 120,
                line: 4,
                column: 9,
            }),
            "Tasks/Task/Duration",
            Some("PTxyz"),
        );
        assert_eq!(
            t.import(&d),
            "unreadable or out-of-range value: default applied: Tasks/Task/Duration = \"PTxyz\", \
             task 7, line 4, column 9 (byte 120)"
        );
        // Diagnostic du cœur enveloppé : message du cœur.
        let d = ImportDiagnostic::core(Diagnostic::LinkSelf {
            task: TaskUid::new(3),
        });
        assert_eq!(t.import(&d), "task 3 is linked to itself");
    }

    #[test]
    fn n9_mspdi_error_translated() {
        let en = Texts::new(Language::En);
        let t = DiagTexts::new(&en);
        let at = Location {
            byte: 0,
            line: 1,
            column: 1,
        };
        let open = MspdiError::UnexpectedEof {
            at,
            open: vec!["Project".to_owned()],
        };
        assert_eq!(
            t.mspdi_error(&open),
            "unexpected end of document at line 1, column 1 (byte 0) (open elements: Project)"
        );
        let closed = MspdiError::UnexpectedEof { at, open: Vec::new() };
        assert_eq!(
            t.mspdi_error(&closed),
            "unexpected end of document at line 1, column 1 (byte 0)"
        );
        // Français : le `Display` du lecteur, espaces insécables avant « : »
        // en plus.
        let fr = Texts::new(Language::Fr);
        let t = DiagTexts::new(&fr);
        let plain = |s: String| s.replace('\u{a0}', " ");
        assert_eq!(plain(t.mspdi_error(&open)), open.to_string());
        assert_eq!(plain(t.mspdi_error(&closed)), closed.to_string());
        for e in MspdiError::examples() {
            assert_eq!(plain(t.mspdi_error(&e)), e.to_string(), "{}", e.code());
        }
    }
}
