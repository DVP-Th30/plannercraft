//! Erreurs des commandes, de l'historique et de l'ouverture (contrat J4,
//! ADR-0012).
//!
//! Chaque erreur porte un code stable (`task.not_found`…), repris tel quel
//! par le MCP, la CLI et l'interface (clé de traduction `.ftl`), et un
//! message en français ou en anglais ([`CommandError::message`]).

use plannercraft_core::{
    CalendarUid, CustomFieldId, CustomKind, Diagnostic, ModelError, Moment, ResourceUid, Subject, TaskUid,
};
use plannercraft_mspdi::MspdiError;

use crate::text::{Language, TextError};

/// Codes d'erreur stables. Les codes `text.*` sont ceux de
/// [`TextError::code`].
pub mod codes {
    /// Tâche (UID) inconnue.
    pub const TASK_NOT_FOUND: &str = "task.not_found";
    /// Étiquette `$x` inconnue dans le lot (R-150).
    pub const BATCH_UNKNOWN_LABEL: &str = "batch.unknown_label";
    /// Étiquette `$x` redéfinie dans le lot (R-150).
    pub const BATCH_DUPLICATE_LABEL: &str = "batch.duplicate_label";
    /// Paramètre invalide.
    pub const PARAM_INVALID: &str = "param.invalid";
    /// Paramètres contradictoires (`text` et `minutes` d'une durée…).
    pub const PARAM_MISMATCH: &str = "param.mismatch";
    /// Champ calculé d'une récapitulative (R-157).
    pub const TASK_COMPUTED_FIELD: &str = "task.computed_field";
    /// Le lot ferait apparaître des erreurs (R-160).
    pub const BATCH_WOULD_CREATE_ERRORS: &str = "batch.would_create_errors";
    /// Lien déjà présent (R-156).
    pub const LINK_DUPLICATE: &str = "link.duplicate";
    /// Lien d'une tâche vers elle-même (R-156).
    pub const LINK_SELF: &str = "link.self";
    /// Lien entre une récapitulative et sa descendante (R-156, R-059).
    pub const LINK_SUMMARY_DESCENDANT: &str = "link.summary_descendant";
    /// Lien absent (R-156, `link.update`, `link.remove`).
    pub const LINK_NOT_FOUND: &str = "link.not_found";
    /// Récapitulative supprimée sans `cascade` (R-155).
    pub const TASK_HAS_CHILDREN: &str = "task.has_children";
    /// Indentation ou désindentation refusée (R-153).
    pub const TASK_OUTLINE_REFUSED: &str = "task.outline_refused";
    /// Déplacement dans son propre sous-arbre (R-154).
    pub const TASK_MOVE_INTO_SUBTREE: &str = "task.move_into_subtree";
    /// Borne dépassée.
    pub const LIMIT_EXCEEDED: &str = "limit.exceeded";
    /// `format_version` non pris en charge.
    pub const BATCH_UNSUPPORTED_VERSION: &str = "batch.unsupported_version";
    /// Commande réservée à un jalon ultérieur.
    pub const COMMAND_NOT_YET_AVAILABLE: &str = "command.not_yet_available";
    /// Dates d'une tâche manuelle (R-157, ADR-0005).
    pub const TASK_MANUAL_UNSUPPORTED: &str = "task.manual_unsupported";
    /// Exception de calendrier en chevauchement partiel (R-165).
    pub const CALENDAR_EXCEPTION_OVERLAP: &str = "calendar.exception_overlap";
    /// Aucune exception ne commence ce jour (R-165, `calendar.remove_exception`).
    pub const CALENDAR_EXCEPTION_NOT_FOUND: &str = "calendar.exception_not_found";
    /// Calendrier (UID) inconnu.
    pub const CALENDAR_NOT_FOUND: &str = "calendar.not_found";
    /// Aucune session ouverte (`Workspace`).
    pub const SESSION_NONE: &str = "session.none";
    /// Verrou de session empoisonné (MCP).
    pub const SESSION_POISONED: &str = "session.poisoned";
    /// Fichier illisible (lecture impossible).
    pub const FILE_READ: &str = "file.read";
    /// Écriture impossible.
    pub const FILE_WRITE: &str = "file.write";
    /// Fichier trop gros.
    pub const FILE_TOO_LARGE: &str = "file.too_large";
    /// Document MSPDI inexploitable.
    pub const FILE_INVALID: &str = "file.invalid";
    /// Chemin hors de la racine autorisée (MCP HTTP).
    pub const FILE_OUTSIDE_ROOT: &str = "file.outside_root";
    /// Fichier tenu par une application vivante (ADR-0002).
    pub const FILE_OPEN_IN_APP: &str = "file.open_in_app";
    /// Rien à annuler ou à rétablir.
    pub const HISTORY_EMPTY: &str = "history.empty";
    /// Entrée d'historique incohérente avec le projet : annulation refusée,
    /// session intacte (R-164 ; jamais attendu).
    pub const HISTORY_INCONSISTENT: &str = "history.inconsistent";
    /// Ressource (UID) inconnue (J5, R-195).
    pub const RESOURCE_NOT_FOUND: &str = "resource.not_found";
    /// Ressource affectée supprimée sans `cascade` (J5, R-195).
    pub const RESOURCE_HAS_ASSIGNMENTS: &str = "resource.has_assignments";
    /// Aucune affectation de cette ressource à cette tâche (J5, R-196).
    pub const ASSIGNMENT_NOT_FOUND: &str = "assignment.not_found";
    /// Ressource déjà affectée à cette tâche (J5, R-196).
    pub const ASSIGNMENT_DUPLICATE: &str = "assignment.duplicate";
    /// Champ d'affectation contraire au type de la ressource (`quantity`
    /// pour une ressource de travail…) (J5, R-196).
    pub const ASSIGNMENT_KIND_MISMATCH: &str = "assignment.kind_mismatch";
    /// Unités nulles sur une tâche à travail fixe (J5, R-181, R-196).
    pub const ASSIGNMENT_ZERO_UNITS_FIXED_WORK: &str = "assignment.zero_units_fixed_work";
    /// Tâche à travail fixe non pilotée par l'effort (J5, R-193, R-197).
    pub const TASK_FIXED_WORK_EFFORT_DRIVEN: &str = "task.fixed_work_effort_driven";
    /// Champs qui s'excluent modifiés ensemble (durée et travail, R-197).
    pub const PARAM_CONFLICT: &str = "param.conflict";
    /// Avancement saisi sur une récapitulative (J6, R-213).
    pub const PROGRESS_SUMMARY_NOT_EDITABLE: &str = "progress.summary_not_editable";
    /// Avancement saisi sur une tâche inactive (J6, R-213).
    pub const PROGRESS_INACTIVE: &str = "progress.inactive";
    /// Fin réelle avant le début réel (J6, R-202).
    pub const PROGRESS_INVALID_ORDER: &str = "progress.invalid_order";
    /// Durée saisie sous la durée réelle (J6, R-213, arbitrage du
    /// 2026-10-20).
    pub const PROGRESS_DURATION_BELOW_ACTUAL: &str = "progress.duration_below_actual";
    /// Ligne de base déjà présente, sans `overwrite` (J6, R-223, R-227).
    pub const BASELINE_EXISTS: &str = "baseline.exists";
    /// Cumul d'une ligne de base sans sélection de tâches (J6, R-222).
    pub const BASELINE_ROLLUP_NEEDS_SELECTION: &str = "baseline.rollup_needs_selection";
    /// Tâche exclue du calcul : aucune date à expliquer (J6, R-230).
    pub const EXPLAIN_NOT_SCHEDULED: &str = "explain.not_scheduled";
    /// Nom de calendrier déjà pris, sans tenir compte de la casse (J7, R-248).
    pub const CALENDAR_NAME_TAKEN: &str = "calendar.name_taken";
    /// Base qui n'est pas un calendrier de base, ou calendrier de base qui sert de base rendu dérivé (J7, R-248).
    pub const CALENDAR_BASE_NOT_BASE: &str = "calendar.base_not_base";
    /// Calendrier utilisé (projet, tâche, ressource, base) : suppression refusée (J7, R-248).
    pub const CALENDAR_IN_USE: &str = "calendar.in_use";
    /// Barre qui ne se déplace pas par ce geste (récapitulative, tâche commencée, jalon, exclue, ligne vide ; J7, R-240 à R-242).
    pub const GESTURE_NOT_DRAGGABLE: &str = "gesture.not_draggable";
    /// Rapport d'utilisation trop grand (périodes, ressources ou lignes d'affectation ; J7, R-245).
    pub const USAGE_LIMIT: &str = "usage.limit";
    /// Écriture relayée refusée par l'utilisateur (J7, C-CTL-03).
    pub const CONTROL_REJECTED: &str = "control.rejected";
    /// Écriture relayée sans réponse de l'utilisateur dans le délai (J7, C-CTL-04).
    pub const CONTROL_APPROVAL_TIMEOUT: &str = "control.approval_timeout";
    /// Commande non relayable vers l'application (`project.open`, `project.new`, `project.import_csv` ; J7, C-CTL-05).
    pub const CONTROL_NOT_RELAYABLE: &str = "control.not_relayable";
    /// Commande d'interface (`app.*`) sans application rattachée (J7, C-CTL-10).
    pub const CONTROL_NOT_ATTACHED: &str = "control.not_attached";
    /// Application rattachée fermée pendant un appel (J7, C-CTL-06).
    pub const CONTROL_APP_GONE: &str = "control.app_gone";
    /// Aucune capture d'écran dans le délai (J7, C-CTL-08).
    pub const CONTROL_SCREENSHOT_UNAVAILABLE: &str = "control.screenshot_unavailable";
    /// Capture d'écran trop grande, même réduite (J7, C-CTL-08).
    pub const CONTROL_SCREENSHOT_TOO_LARGE: &str = "control.screenshot_too_large";
    /// Application rattachée occupée (connexions ou file pleines) : appel à
    /// refaire, le rattachement reste (relecture finale J7, relais F4).
    pub const CONTROL_BUSY: &str = "control.busy";
    /// L'application a changé de document depuis le rattachement (jeton du
    /// document précédent refusé) : rien n'est fait, le serveur MCP se
    /// détache ; `open_project` du fichier voulu rattache de nouveau
    /// (relecture finale J7, relais F2).
    pub const CONTROL_DOCUMENT_CHANGED: &str = "control.document_changed";
    /// Texte collé non reconnu : ni en-tête de CSV, ni colonne unique (J8,
    /// R-256).
    pub const PASTE_UNRECOGNIZED: &str = "paste.unrecognized";
    /// Texte collé trop grand (10 000 lignes, 4 Mio ; J8, R-256).
    pub const PASTE_TOO_LARGE: &str = "paste.too_large";
    /// Liens externes au tampon non recréés au collage (J8, R-244).
    pub const PASTE_LINKS_DROPPED: &str = "paste.links_dropped";
    /// Style du document ignoré à la lecture (trop grand, version inconnue,
    /// illisible ; J8, H-R12).
    pub const STYLE_IGNORED: &str = "style.ignored";
    /// Style ou surcharge invalide (`style.apply` ; J8).
    pub const STYLE_INVALID: &str = "style.invalid";
    /// Export PDF de plus de 1 000 pages (J8, P54).
    pub const PDF_TOO_MANY_PAGES: &str = "pdf.too_many_pages";
    /// Caractère absent des polices du PDF (J8, ADR-0016 D9).
    pub const PDF_GLYPH_MISSING: &str = "pdf.glyph_missing";
    /// Logo du PDF illisible ou hors bornes (256 Kio, 2 048 px ; J8).
    pub const PDF_LOGO_INVALID: &str = "pdf.logo_invalid";
    /// Fin saisie d'une tâche manuelle avant son début (J8, R-254).
    pub const TASK_MANUAL_FINISH_BEFORE_START: &str = "task.manual_finish_before_start";
    /// Adresse du canal de contrôle invalide (J8, C-CTL-11).
    pub const CONTROL_BAD_ENDPOINT: &str = "control.bad_endpoint";
    /// Champ personnalisé inconnu (`text31`, `cost1`…) (R-264).
    pub const CUSTOM_UNKNOWN_FIELD: &str = "custom.unknown_field";
    /// Valeur d'un champ personnalisé d'un autre type, illisible ou hors
    /// bornes (R-264).
    pub const CUSTOM_VALUE_TYPE: &str = "custom.value_type";
    /// Texte ou alias d'un champ personnalisé trop long (R-264).
    pub const CUSTOM_TOO_LONG: &str = "custom.too_long";
    /// Champ personnalisé défini par le fichier (formule, table de choix) :
    /// ni valeur ni alias modifiables (R-264, R-265).
    pub const CUSTOM_READ_ONLY: &str = "custom.read_only";
}

/// Commande refusée. L'état de la session est intact (atomicité du lot).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{code}")]
pub struct CommandError {
    /// Code stable ([`codes`]).
    pub code: &'static str,
    /// Index de l'opération fautive dans le lot.
    pub op_index: Option<usize>,
    /// Champ fautif (`duration`, `uid`…).
    pub field: Option<String>,
    /// Élément du projet concerné.
    pub subject: Option<Subject>,
    /// Détail.
    pub kind: CommandErrorKind,
}

/// Détail d'une [`CommandError`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandErrorKind {
    /// UID inconnu.
    UnknownTask(TaskUid),
    /// Étiquette inconnue.
    UnknownLabel(String),
    /// Étiquette redéfinie.
    DuplicateLabel(String),
    /// Paramètre invalide (explication).
    InvalidParam(String),
    /// Texte illisible.
    Text(TextError),
    /// Champ calculé d'une récapitulative.
    ComputedField,
    /// Erreurs nouvelles (R-160).
    WouldCreateErrors(Vec<Diagnostic>),
    /// Lien déjà présent.
    DuplicateLink,
    /// Lien d'une tâche vers elle-même.
    SelfLink,
    /// Lien récapitulative ↔ descendante.
    SummaryDescendantLink,
    /// Récapitulative et ses enfants.
    HasChildren {
        /// Nombre de descendants.
        count: usize,
    },
    /// Hiérarchie refusée.
    OutlineRefused,
    /// Déplacement dans son propre sous-arbre.
    MoveIntoOwnSubtree,
    /// Borne dépassée.
    Limit {
        /// Grandeur bornée (`ops`, `task_uid`…).
        what: &'static str,
        /// Maximum.
        max: u64,
    },
    /// Version de format non prise en charge.
    UnsupportedVersion(u32),
    /// Commande réservée.
    NotYetAvailable {
        /// Jalon de disponibilité.
        milestone: u8,
    },
    /// Tâche manuelle.
    ManualTaskUnsupported,
    /// Fin saisie d'une tâche manuelle avant son début (J8, R-254).
    ManualFinishBeforeStart {
        /// Début de la tâche manuelle.
        start: Moment,
        /// Fin saisie.
        finish: Moment,
    },
    /// Paramètres contradictoires.
    ParamMismatch,
    /// Ressource inconnue (J5).
    UnknownResource(ResourceUid),
    /// Ressource affectée (J5).
    ResourceHasAssignments {
        /// Nombre d'affectations.
        count: usize,
    },
    /// Affectation absente (J5).
    UnknownAssignment {
        /// Tâche.
        task: TaskUid,
        /// Ressource.
        resource: ResourceUid,
    },
    /// Affectation déjà présente (J5).
    DuplicateAssignment {
        /// Tâche.
        task: TaskUid,
        /// Ressource.
        resource: ResourceUid,
    },
    /// Champ contraire au type de la ressource (J5).
    AssignmentKindMismatch,
    /// Unités nulles sur une tâche à travail fixe (J5).
    ZeroUnitsFixedWork,
    /// Travail fixe non piloté par l'effort (J5).
    FixedWorkEffortDriven,
    /// Champs qui s'excluent (J5) : les deux noms.
    ParamConflict(&'static str, &'static str),
    /// Avancement d'une récapitulative (J6).
    ProgressSummaryNotEditable,
    /// Avancement d'une tâche inactive (J6).
    ProgressInactive,
    /// Fin réelle avant le début réel (J6).
    ProgressInvalidOrder,
    /// Durée saisie sous la durée réelle (J6, R-213).
    DurationBelowActual {
        /// Durée réelle stockée, minutes ouvrées.
        actual: i64,
        /// Durée saisie, minutes ouvrées.
        entered: i64,
    },
    /// Ligne de base déjà présente (J6).
    BaselineExists {
        /// Numéro de la ligne.
        number: u8,
        /// Entités de la portée qui l'ont déjà.
        count: u32,
    },
    /// Cumul sans sélection (J6).
    BaselineRollupNeedsSelection,
    /// Tâche non planifiée (J6).
    ExplainNotScheduled(TaskUid),
    /// Nom de calendrier déjà pris, sans tenir compte de la casse (J7,
    /// R-248) : le calendrier qui le porte.
    CalendarNameTaken {
        /// Nom demandé.
        name: String,
        /// Calendrier qui porte déjà ce nom.
        existing: CalendarUid,
    },
    /// Base désignée qui n'est pas un calendrier de base (J7, R-248).
    CalendarBaseNotBase {
        /// Base désignée.
        base: CalendarUid,
    },
    /// Calendrier de base qui sert de base à d'autres : il ne peut pas
    /// devenir dérivé (J7, R-248).
    CalendarServesAsBase {
        /// Calendrier.
        calendar: CalendarUid,
        /// Calendriers qui en dérivent.
        count: usize,
    },
    /// Champ personnalisé inconnu : la clé telle que reçue (R-264).
    CustomUnknownField(String),
    /// Valeur d'un champ personnalisé d'un autre type, illisible ou hors
    /// bornes (R-264).
    CustomValueType {
        /// Champ.
        field: CustomFieldId,
        /// Type attendu.
        expected: CustomKind,
    },
    /// Texte ou alias trop long (R-264) : longueur maximale, en caractères.
    CustomTooLong {
        /// Longueur maximale.
        max: usize,
    },
    /// Champ défini par le fichier (formule, table de choix) (R-264).
    CustomReadOnly(CustomFieldId),
    /// Calendrier utilisé : suppression refusée (J7, R-248).
    CalendarInUse {
        /// Calendrier.
        calendar: CalendarUid,
        /// Utilisateurs (projet, tâches, ressources, calendriers dérivés),
        /// au plus [`MAX_LISTED_CALENDAR_USERS`], dans cet ordre.
        users: Vec<Subject>,
        /// Nombre d'utilisateurs en tout.
        total: usize,
    },
}

/// Utilisateurs d'un calendrier listés au plus par `calendar.in_use` (J7,
/// R-248).
pub const MAX_LISTED_CALENDAR_USERS: usize = 20;

impl CommandError {
    /// Erreur sans index, champ ni sujet.
    #[must_use]
    pub const fn new(code: &'static str, kind: CommandErrorKind) -> Self {
        Self {
            code,
            op_index: None,
            field: None,
            subject: None,
            kind,
        }
    }

    /// Commande pas encore disponible : réservée au jalon `milestone`, ou
    /// squelette du contrat J4 pas encore rempli.
    #[must_use]
    pub const fn not_yet(milestone: u8) -> Self {
        Self::new(
            codes::COMMAND_NOT_YET_AVAILABLE,
            CommandErrorKind::NotYetAvailable { milestone },
        )
    }

    /// Même erreur, à l'index d'opération `i`.
    #[must_use]
    pub fn with_op_index(mut self, i: usize) -> Self {
        self.op_index = Some(i);
        self
    }

    /// Même erreur, sur le champ `field` (`duration`, `uid`…).
    #[must_use]
    pub fn with_field(mut self, field: &str) -> Self {
        self.field = Some(field.to_owned());
        self
    }

    /// Même erreur, à propos de `subject`.
    #[must_use]
    pub fn with_subject(mut self, subject: Subject) -> Self {
        self.subject = Some(subject);
        self
    }

    /// Texte illisible : code `text.*` de l'erreur.
    #[must_use]
    pub fn text(e: TextError) -> Self {
        Self::new(e.code(), CommandErrorKind::Text(e))
    }

    /// Message pour l'utilisateur, en français ou en anglais, borné à
    /// [`MAX_MESSAGE_CHARS`] caractères (C-MCP-03). Le code, le champ et
    /// l'index d'opération restent dans leurs champs ; le message les
    /// reprend pour un lecteur humain.
    #[must_use]
    pub fn message(&self, lang: Language) -> String {
        let mut s = kind_message(&self.kind, lang);
        let at = match (self.op_index, lang) {
            (Some(i), Language::Fr) => format!(" (opération {i}"),
            (Some(i), Language::En) => format!(" (operation {i}"),
            (None, _) => String::new(),
        };
        s.push_str(&at);
        if let Some(field) = &self.field {
            s.push_str(if at.is_empty() { " (" } else { ", " });
            match lang {
                Language::Fr => s.push_str("champ « "),
                Language::En => s.push_str("field \u{201c}"),
            }
            s.push_str(field);
            s.push_str(match lang {
                Language::Fr => " »",
                Language::En => "\u{201d}",
            });
        }
        if !at.is_empty() || self.field.is_some() {
            s.push(')');
        }
        truncate_chars(s, MAX_MESSAGE_CHARS)
    }
}

/// Longueur maximale d'un message d'erreur, en caractères (C-MCP-03).
pub const MAX_MESSAGE_CHARS: usize = 2_000;

/// Diagnostics cités au plus dans un message `batch.would_create_errors`.
const MAX_LISTED_DIAGNOSTICS: usize = 5;

/// Coupe `s` à `max` caractères, « … » final compris.
fn truncate_chars(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        return s;
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Message d'un détail d'erreur, sans contexte d'opération.
#[allow(
    clippy::too_many_lines,
    reason = "un bras par variante et par langue, rien à factoriser"
)]
fn kind_message(kind: &CommandErrorKind, lang: Language) -> String {
    use CommandErrorKind as K;
    let fr = lang == Language::Fr;
    match kind {
        K::UnknownTask(uid) => {
            if fr {
                format!("aucune tâche d'UID {uid}")
            } else {
                format!("no task with UID {uid}")
            }
        }
        K::UnknownLabel(l) => {
            if fr {
                format!(
                    "étiquette « {l} » inconnue : elle doit être créée par un task.add ou un resource.add antérieur du même lot (\"as\")"
                )
            } else {
                format!(
                    "unknown label \u{201c}{l}\u{201d}: it must be created by an earlier task.add or resource.add of the same batch (\"as\")"
                )
            }
        }
        K::DuplicateLabel(l) => {
            if fr {
                format!("étiquette « {l} » déjà définie dans ce lot")
            } else {
                format!("label \u{201c}{l}\u{201d} is already defined in this batch")
            }
        }
        K::InvalidParam(why) => {
            if fr {
                format!("paramètre invalide : {why}")
            } else {
                format!("invalid parameter: {why}")
            }
        }
        K::Text(e) => text_message(e, lang),
        K::ComputedField => {
            if fr {
                "champ calculé d'une récapitulative : il ne se saisit pas".to_owned()
            } else {
                "computed field of a summary task: it cannot be entered".to_owned()
            }
        }
        K::WouldCreateErrors(diags) => {
            let mut s = if fr {
                format!("lot refusé : il ferait apparaître {} erreur(s)", diags.len())
            } else {
                format!("batch refused: it would introduce {} error(s)", diags.len())
            };
            for (i, d) in diags.iter().take(MAX_LISTED_DIAGNOSTICS).enumerate() {
                s.push_str(if i == 0 { " : " } else { " ; " });
                s.push_str(d.code());
                s.push_str(" (");
                s.push_str(&d.to_string());
                s.push(')');
            }
            if diags.len() > MAX_LISTED_DIAGNOSTICS {
                s.push_str(if fr { " ; …" } else { "; …" });
            }
            s
        }
        K::DuplicateLink => {
            if fr {
                "ce lien existe déjà (utiliser link.update)".to_owned()
            } else {
                "this link already exists (use link.update)".to_owned()
            }
        }
        K::SelfLink => {
            if fr {
                "une tâche ne peut pas être liée à elle-même".to_owned()
            } else {
                "a task cannot be linked to itself".to_owned()
            }
        }
        K::SummaryDescendantLink => {
            if fr {
                "lien interdit entre une récapitulative et l'une de ses sous-tâches".to_owned()
            } else {
                "a summary task cannot be linked to one of its subtasks".to_owned()
            }
        }
        K::HasChildren { count } => {
            if fr {
                format!("récapitulative de {count} sous-tâche(s) : supprimer avec \"cascade\": true")
            } else {
                format!("summary task with {count} subtask(s): delete with \"cascade\": true")
            }
        }
        K::OutlineRefused => {
            if fr {
                "indentation ou désindentation impossible à cette position".to_owned()
            } else {
                "the task cannot be indented or outdented at this position".to_owned()
            }
        }
        K::MoveIntoOwnSubtree => {
            if fr {
                "une tâche ne peut pas être déplacée dans son propre sous-arbre".to_owned()
            } else {
                "a task cannot be moved into its own subtree".to_owned()
            }
        }
        K::Limit { what, max } => {
            if fr {
                format!("limite dépassée : {what} (au plus {max})")
            } else {
                format!("limit exceeded: {what} (at most {max})")
            }
        }
        K::UnsupportedVersion(v) => {
            if fr {
                format!("format_version {v} non pris en charge (attendu : 1)")
            } else {
                format!("format_version {v} is not supported (expected: 1)")
            }
        }
        K::NotYetAvailable { milestone } => {
            if fr {
                format!("commande pas encore disponible (jalon J{milestone})")
            } else {
                format!("command not available yet (milestone J{milestone})")
            }
        }
        K::ManualFinishBeforeStart { start, finish } => {
            if fr {
                format!("la fin saisie ({finish}) précède le début de la tâche manuelle ({start})")
            } else {
                format!("the entered finish ({finish}) is before the start of the manual task ({start})")
            }
        }
        K::ManualTaskUnsupported => {
            if fr {
                "les dates d'une tâche planifiée manuellement ne se modifient pas encore".to_owned()
            } else {
                "the dates of a manually scheduled task cannot be edited yet".to_owned()
            }
        }
        K::ParamMismatch => {
            if fr {
                "paramètres contradictoires (par exemple « text » et « minutes » d'une durée)".to_owned()
            } else {
                "contradictory parameters (for example the \"text\" and \"minutes\" of a duration)".to_owned()
            }
        }
        K::UnknownResource(uid) => {
            if fr {
                format!("aucune ressource d'UID {uid}")
            } else {
                format!("no resource with UID {uid}")
            }
        }
        K::ResourceHasAssignments { count } => {
            if fr {
                format!("ressource affectée à {count} tâche(s) : supprimer avec \"cascade\": true")
            } else {
                format!("resource assigned to {count} task(s): delete with \"cascade\": true")
            }
        }
        K::UnknownAssignment { task, resource } => {
            if fr {
                format!("la ressource {resource} n'est pas affectée à la tâche {task}")
            } else {
                format!("resource {resource} is not assigned to task {task}")
            }
        }
        K::DuplicateAssignment { task, resource } => {
            if fr {
                format!(
                    "la ressource {resource} est déjà affectée à la tâche {task} (utiliser assignment.update)"
                )
            } else {
                format!("resource {resource} is already assigned to task {task} (use assignment.update)")
            }
        }
        K::AssignmentKindMismatch => {
            if fr {
                "champ contraire au type de la ressource (« units » et « work » pour une ressource de travail, « quantity » pour une ressource matérielle)".to_owned()
            } else {
                "field does not match the resource type (\"units\" and \"work\" for a work resource, \"quantity\" for a material resource)".to_owned()
            }
        }
        K::ZeroUnitsFixedWork => {
            if fr {
                "unités nulles refusées sur une tâche à travail fixe".to_owned()
            } else {
                "zero units are refused on a fixed-work task".to_owned()
            }
        }
        K::FixedWorkEffortDriven => {
            if fr {
                "une tâche à travail fixe est toujours pilotée par l'effort".to_owned()
            } else {
                "a fixed-work task is always effort driven".to_owned()
            }
        }
        K::ParamConflict(a, b) => {
            if fr {
                format!("« {a} » et « {b} » ne se modifient pas ensemble")
            } else {
                format!("\"{a}\" and \"{b}\" cannot be changed together")
            }
        }
        K::ProgressSummaryNotEditable => {
            if fr {
                "l'avancement d'une récapitulative se calcule depuis ses sous-tâches : il ne se saisit pas"
                    .to_owned()
            } else {
                "the progress of a summary task is computed from its subtasks: it cannot be entered"
                    .to_owned()
            }
        }
        K::ProgressInactive => {
            if fr {
                "l'avancement d'une tâche inactive ne se saisit pas".to_owned()
            } else {
                "the progress of an inactive task cannot be entered".to_owned()
            }
        }
        K::ProgressInvalidOrder => {
            if fr {
                "la fin réelle précède le début réel".to_owned()
            } else {
                "the actual finish is before the actual start".to_owned()
            }
        }
        K::DurationBelowActual { actual, entered } => {
            if fr {
                format!(
                    "durée de {entered} min inférieure à la durée réelle déjà saisie ({actual} min) : réduire d'abord l'avancement (progress.update)"
                )
            } else {
                format!(
                    "duration of {entered} min is less than the actual duration already entered ({actual} min): reduce the progress first (progress.update)"
                )
            }
        }
        K::BaselineExists { number, count } => {
            if fr {
                format!(
                    "la ligne de base {number} existe déjà sur {count} élément(s) : la remplacer avec \"overwrite\": true"
                )
            } else {
                format!(
                    "baseline {number} already exists on {count} item(s): replace it with \"overwrite\": true"
                )
            }
        }
        K::BaselineRollupNeedsSelection => {
            if fr {
                "le cumul vers les récapitulatives demande une sélection de tâches (\"tasks\")".to_owned()
            } else {
                "rolling up to summary tasks requires a selection of tasks (\"tasks\")".to_owned()
            }
        }
        K::ExplainNotScheduled(uid) => {
            if fr {
                format!("la tâche {uid} n'est pas planifiée : aucune date à expliquer")
            } else {
                format!("task {uid} is not scheduled: there is no date to explain")
            }
        }
        K::CalendarNameTaken { name, existing } => {
            if fr {
                format!("le nom « {name} » est déjà celui du calendrier {existing} (casse ignorée)")
            } else {
                format!(
                    "the name \u{201c}{name}\u{201d} is already used by calendar {existing} (case ignored)"
                )
            }
        }
        K::CalendarBaseNotBase { base } => {
            if fr {
                format!(
                    "le calendrier {base} n'est pas un calendrier de base : il ne peut pas servir de base"
                )
            } else {
                format!("calendar {base} is not a base calendar: it cannot be used as a base")
            }
        }
        K::CalendarServesAsBase { calendar, count } => {
            if fr {
                format!(
                    "le calendrier {calendar} sert de base à {count} calendrier(s) : il ne peut pas devenir dérivé"
                )
            } else {
                format!("calendar {calendar} is the base of {count} calendar(s): it cannot become derived")
            }
        }
        K::CalendarInUse {
            calendar,
            users,
            total,
        } => calendar_in_use_message(*calendar, users, *total, fr),
        K::CustomUnknownField(key) => {
            if fr {
                format!(
                    "champ personnalisé inconnu : « {key} » (text1 à text30, number1 à number20, date1 à date10, flag1 à flag20)"
                )
            } else {
                format!(
                    "unknown custom field: \u{201c}{key}\u{201d} (text1 to text30, number1 to number20, date1 to date10, flag1 to flag20)"
                )
            }
        }
        K::CustomValueType { field, expected } => {
            let (what_fr, what_en) = match expected {
                CustomKind::Text => ("un texte", "a text"),
                CustomKind::Number => (
                    "un nombre (12,5 ; au plus 10^14)",
                    "a number (12.5; at most 10^14)",
                ),
                CustomKind::Date => ("une date (2026-11-02T08:00)", "a date (2026-11-02T08:00)"),
                CustomKind::Flag => ("oui ou non (true, false)", "yes or no (true, false)"),
            };
            if fr {
                format!("{field} attend {what_fr}")
            } else {
                format!("{field} expects {what_en}")
            }
        }
        K::CustomTooLong { max } => {
            if fr {
                format!("texte trop long : {max} caractères au plus")
            } else {
                format!("text too long: at most {max} characters")
            }
        }
        K::CustomReadOnly(field) => {
            if fr {
                format!(
                    "{field} est calculé par une formule ou lié à une table de choix du fichier : il ne se modifie pas ici"
                )
            } else {
                format!(
                    "{field} is computed by a formula or tied to a lookup table in the file: it cannot be edited here"
                )
            }
        }
    }
}

/// Message de `calendar.in_use` : utilisateurs listés (R-248).
fn calendar_in_use_message(calendar: CalendarUid, users: &[Subject], total: usize, fr: bool) -> String {
    let listed: Vec<String> = users
        .iter()
        .map(|u| match (u, fr) {
            (Subject::Project, true) => "projet".to_owned(),
            (Subject::Project, false) => "project".to_owned(),
            (Subject::Task(t), true) => format!("tâche {t}"),
            (Subject::Task(t), false) => format!("task {t}"),
            (Subject::Resource(r), true) => format!("ressource {r}"),
            (Subject::Resource(r), false) => format!("resource {r}"),
            (Subject::Calendar(c), true) => format!("calendrier {c}"),
            (Subject::Calendar(c), false) => format!("calendar {c}"),
            (other, _) => other.to_string(),
        })
        .collect();
    let mut s = if fr {
        format!(
            "le calendrier {calendar} est utilisé ({total} utilisateur(s)) : {}",
            listed.join(", ")
        )
    } else {
        format!(
            "calendar {calendar} is in use ({total} user(s)): {}",
            listed.join(", ")
        )
    };
    if total > users.len() {
        s.push_str(", …");
    }
    s
}

/// Message d'un texte illisible (le message français vient de
/// [`TextError`] ; l'anglais est rédigé ici).
fn text_message(e: &TextError, lang: Language) -> String {
    if lang == Language::Fr {
        return format!("texte illisible : {e}");
    }
    let detail = match e {
        TextError::Empty => "empty value".to_owned(),
        TextError::BadNumber { at } => format!("number expected or unreadable at character {at}"),
        TextError::UnknownUnit { unit } => format!("unknown unit \u{201c}{unit}\u{201d}"),
        TextError::Negative => "a duration cannot be negative".to_owned(),
        TextError::OutOfRange => "value out of range".to_owned(),
        TextError::BadLinkType { code } => format!("unknown link type \u{201c}{code}\u{201d}"),
        TextError::ExternalPredecessor => "external predecessor (other project) not supported".to_owned(),
        TextError::BadDate => "unreadable or nonexistent date".to_owned(),
        TextError::TooLong => format!(
            "text too long (at most {} characters)",
            crate::text::MAX_INPUT_CHARS
        ),
    };
    format!("unreadable text: {detail}")
}

/// Annulation ou rétablissement impossible.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HistoryError {
    /// Rien à annuler ou à rétablir (`history.empty`).
    #[error("rien à annuler ou à rétablir")]
    Empty,
    /// Delta d'historique inapplicable (`history.inconsistent`) : la
    /// session reste intacte.
    #[error("historique incohérent : {0}")]
    Inconsistent(ModelError),
}

impl HistoryError {
    /// Code stable.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Empty => codes::HISTORY_EMPTY,
            Self::Inconsistent(_) => codes::HISTORY_INCONSISTENT,
        }
    }
}

/// Ouverture d'un document impossible.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    /// Document MSPDI inexploitable (`file.invalid`).
    #[error(transparent)]
    Mspdi(#[from] MspdiError),
}

impl OpenError {
    /// Code stable.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Mspdi(_) => codes::FILE_INVALID,
        }
    }
}
