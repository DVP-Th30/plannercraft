//! Couche fine au-dessus de [`Session`] pour la CLI, le MCP et le canal :
//! session facultative, « maintenant », langue, racine des chemins
//! (ADR-0012 D1, lot L6).
//!
//! [`Workspace::call`] est le routeur unique : un id du registre, des
//! paramètres JSON (forme du schéma de la commande, plus `dry_run` et
//! `detail` pour une écriture), une valeur JSON ([`crate::view`]) ou une
//! [`ToolError`]. Une écriture unitaire est un lot d'une commande ; un
//! appel d'écriture réussi hors `dry_run` crée une entrée d'historique
//! (C-MCP-04). Les fichiers passent par [`crate::files`] (absent en wasm :
//! les commandes de fichiers y sont refusées `file.read`/`file.write`).

use std::path::PathBuf;

use plannercraft_core::{Diagnostic, Moment, Subject, TaskUid};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::command::params::{
    ApplyBatchParams, ExplainParams, ExportMspdiParams, ExportPdfParams, ImportCsvParams, NewProjectParams,
    NoParams, OpenProjectParams, RenderGanttParams, ResourceListParams, SaveProjectParams, TaskGetParams,
    TaskListParams, UsageParams, VarianceParams, WhatIfParams, ZoomJson,
};
use crate::command::{AnchorSpec, Batch, Command, parse_params, style};
use crate::error::{CommandError, CommandErrorKind, HistoryError, codes};
use crate::history::EditSource;
use crate::outcome::ApplyOutcome;
use crate::registry::{Availability, Kind, lookup};
use crate::session::{ApplyMode, ApplyOptions, NewProject, Session};
use crate::text::Language;
use crate::view::{self, DiagView, MAX_VIEW_DIAGNOSTICS, ViewCtx};

/// Taille maximale d'un fichier MSPDI lu (256 Mio, borne du lecteur).
pub const MAX_MSPDI_BYTES: u64 = 256 * 1024 * 1024;
/// Taille maximale d'un fichier CSV lu (64 Mio, borne de l'import).
pub const MAX_CSV_BYTES: u64 = 64 * 1024 * 1024;
/// Lignes d'un rendu Gantt au plus (`render.gantt`).
pub const MAX_RENDER_ROWS: u32 = 200;
/// Largeur visée par l'échelle automatique d'un rendu.
const AUTO_WIDTH_PX: u32 = 1_200;

/// Environnement d'un appel sur une session tenue ailleurs (J7, ADR-0015
/// D9 : l'application exécute les appels relayés sur la session de son
/// document) : « maintenant » injecté, langue, racine des chemins, origine
/// des lots commités.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallEnv {
    /// « Maintenant » (règle 5).
    pub now: Moment,
    /// Langue des messages.
    pub lang: Language,
    /// Racine des chemins (MCP HTTP), sinon aucune.
    pub root: Option<PathBuf>,
    /// Origine inscrite dans l'historique pour une écriture commitée
    /// (`Agent` pour un appel relayé accepté, J7 lot L3).
    pub source: EditSource,
}

impl CallEnv {
    /// Environnement d'un appel de l'utilisateur, sans racine.
    #[must_use]
    pub const fn new(now: Moment, lang: Language) -> Self {
        Self {
            now,
            lang,
            root: None,
            source: EditSource::User,
        }
    }
}

/// Ouvertures qui remplacent la session : servies par [`Workspace::call`]
/// seulement, jamais sur une session tenue ailleurs (`control.not_relayable`,
/// C-CTL-05).
pub const NOT_RELAYABLE: [&str; 3] = ["project.import_csv", "project.new", "project.open"];

/// Contrôles de [`call_with`] qui ne demandent aucune session : id connu
/// (`param.invalid`), hors ouvertures (`control.not_relayable`), disponible
/// (`command.not_yet_available`, `control.not_attached`). L'application
/// s'en sert pour refuser un appel relayé sans document ouvert.
///
/// # Errors
///
/// Le refus, tel que [`call_with`] le rendrait.
pub fn relayable(id: &str, lang: Language) -> Result<&'static crate::registry::CommandSpec, ToolError> {
    let Some(spec) = lookup(id) else {
        return Err(unknown_command(id, lang));
    };
    if NOT_RELAYABLE.contains(&id) {
        return Err(ToolError::simple(
            codes::CONTROL_NOT_RELAYABLE,
            format!(
                "{} : {id}",
                tr(
                    lang,
                    "ouverture non relayable (ouvrir le fichier dans l'application)",
                    "opening cannot be relayed (open the file in the application)"
                )
            ),
        ));
    }
    check_available(spec.available, id, lang)?;
    Ok(spec)
}

/// Routeur de [`Workspace::call`] appliqué à `session` (J7, ADR-0015 D9) :
/// un seul registre pour le local et le relais. Les ouvertures
/// ([`NOT_RELAYABLE`]) sont refusées `control.not_relayable`, les entrées
/// `app.*` `control.not_attached` (l'application les sert elle-même). Une
/// écriture commitée porte l'origine `env.source`.
///
/// # Errors
///
/// [`ToolError`] pour tout refus : `param.invalid` (id inconnu, paramètres
/// mal formés), `command.not_yet_available`, `control.not_relayable`,
/// `control.not_attached`, `file.*`, ou l'erreur de la commande. La
/// session est intacte après un refus.
pub fn call_with(session: &mut Session, env: &CallEnv, id: &str, params: Value) -> Result<Value, ToolError> {
    route(Some(session), env, id, params)
}

/// Routeur commun : paramètres lus avant la session (un appel mal formé
/// est `param.invalid` même sans projet ouvert), puis `session.none` sans
/// session.
fn route(session: Option<&mut Session>, env: &CallEnv, id: &str, params: Value) -> Result<Value, ToolError> {
    let lang = env.lang;
    let cmd = |e: CommandError| ToolError::from_command(&e, lang);
    let spec = relayable(id, lang)?;
    match id {
        "project.save" => {
            let p = parse_params::<SaveProjectParams>(params).map_err(cmd)?;
            save(need(session, lang)?, env, p.path.as_deref(), true)
        }
        "project.export_mspdi" => {
            let p = parse_params::<ExportMspdiParams>(params).map_err(cmd)?;
            save(need(session, lang)?, env, Some(&p.path), false)
        }
        "project.export_pdf" => {
            let p = parse_params::<ExportPdfParams>(params).map_err(cmd)?;
            export_pdf(need(session, lang)?, env, &p)
        }
        "history.list" => {
            parse_params::<NoParams>(params).map_err(cmd)?;
            to_value(&need(session, lang)?.history())
        }
        "history.undo" | "history.redo" => {
            parse_params::<NoParams>(params).map_err(cmd)?;
            let session = need(session, lang)?;
            let outcome = if id == "history.undo" {
                session.undo()
            } else {
                session.redo()
            };
            let outcome = outcome.map_err(|e| history_error(&e, lang))?;
            to_value(&ViewCtx::new(session).outcome(&outcome))
        }
        "batch.apply" => {
            let mut obj = object(params, lang)?;
            let dry_run = take_flag(&mut obj, "dry_run", lang)?;
            let detail = take_flag(&mut obj, "detail", lang)?;
            check_batch_version(&obj).map_err(cmd)?;
            let p = parse_params::<ApplyBatchParams>(Value::Object(obj)).map_err(cmd)?;
            apply(need(session, lang)?, env, &p.into_batch(), dry_run, detail)
        }
        _ if spec.kind == Kind::Write => {
            let mut obj = object(params, lang)?;
            let dry_run = take_flag(&mut obj, "dry_run", lang)?;
            let detail = take_flag(&mut obj, "detail", lang)?;
            if obj.contains_key("op") {
                return Err(ToolError::simple(
                    codes::PARAM_INVALID,
                    tr(lang, "champ inconnu « op »", "unknown field \"op\"").to_owned(),
                )
                .with_field("op"));
            }
            obj.insert("op".to_owned(), Value::String(id.to_owned()));
            let op = parse_params::<Command>(Value::Object(obj)).map_err(cmd)?;
            let batch = Batch {
                format_version: Batch::FORMAT_VERSION,
                lang: None,
                ops: vec![op],
                label: None,
            };
            apply(need(session, lang)?, env, &batch, dry_run, detail)
        }
        _ => read(session.map(|s| &*s), lang, id, params),
    }
}

/// Lectures et analyses de [`call_with`].
fn read(session: Option<&Session>, lang: Language, id: &str, params: Value) -> Result<Value, ToolError> {
    let cmd = |e: CommandError| ToolError::from_command(&e, lang);
    match id {
        "project.summary" => {
            parse_params::<NoParams>(params).map_err(cmd)?;
            to_value(&view::summary(need(session, lang)?))
        }
        "task.list" => {
            let p = parse_params::<TaskListParams>(params).map_err(cmd)?;
            to_value(&view::list(need(session, lang)?, &p).map_err(cmd)?)
        }
        "task.get" => {
            let p = parse_params::<TaskGetParams>(params).map_err(cmd)?;
            let uid = TaskUid::new(p.uid);
            let v = ViewCtx::new(need(session, lang)?)
                .task_detail(uid)
                .ok_or_else(|| {
                    cmd(
                        CommandError::new(codes::TASK_NOT_FOUND, CommandErrorKind::UnknownTask(uid))
                            .with_field("uid"),
                    )
                })?;
            to_value(&v)
        }
        "resource.list" => {
            let p = parse_params::<ResourceListParams>(params).map_err(cmd)?;
            to_value(&view::list_resources(need(session, lang)?, &p).map_err(cmd)?)
        }
        "analysis.resource_usage" => {
            let p = parse_params::<UsageParams>(params).map_err(cmd)?;
            to_value(&view::resource_usage(need(session, lang)?, &p).map_err(cmd)?)
        }
        "analysis.explain_date" => {
            let p = parse_params::<ExplainParams>(params).map_err(cmd)?;
            to_value(&crate::explain::explain_date(need(session, lang)?, &p).map_err(cmd)?)
        }
        "analysis.what_if" => {
            let p = parse_params::<WhatIfParams>(params).map_err(cmd)?;
            to_value(&crate::analysis::what_if(need(session, lang)?, &p, lang).map_err(cmd)?)
        }
        "analysis.baseline_variance" => {
            let p = parse_params::<VarianceParams>(params).map_err(cmd)?;
            to_value(&crate::analysis::baseline_variance(need(session, lang)?, &p).map_err(cmd)?)
        }
        "analysis.critical_path" => {
            parse_params::<NoParams>(params).map_err(cmd)?;
            to_value(&ViewCtx::new(need(session, lang)?).critical_path())
        }
        "render.gantt" => {
            let p = parse_params::<RenderGanttParams>(params).map_err(cmd)?;
            render(need(session, lang)?, lang, &p)
        }
        "style.get" => {
            parse_params::<NoParams>(params).map_err(cmd)?;
            let project = need(session, lang)?.project();
            to_value(&style::style_view(project, style::HEADLESS_DEFAULT))
        }
        "style.list" => {
            parse_params::<NoParams>(params).map_err(cmd)?;
            let project = need(session, lang)?.project();
            to_value(&style::list_view(project, style::HEADLESS_DEFAULT))
        }
        "style.save_as_template" => {
            let p = parse_params::<style::StyleSaveAsTemplate>(params).map_err(cmd)?;
            let project = need(session, lang)?.project();
            to_value(&style::save_as_template(project, &p, style::HEADLESS_DEFAULT).map_err(cmd)?)
        }
        _ => Err(unknown_command(id, lang)),
    }
}

/// Session de l'appel, sinon `session.none`.
fn need<S>(session: Option<S>, lang: Language) -> Result<S, ToolError> {
    session.ok_or_else(|| no_session(lang))
}

/// Refus d'un id inconnu du registre.
fn unknown_command(id: &str, lang: Language) -> ToolError {
    ToolError::simple(
        codes::PARAM_INVALID,
        format!("{} : {id}", tr(lang, "commande inconnue", "unknown command")),
    )
}

/// Applique un lot à `session` (origine `env.source` s'il est commité).
fn apply(
    session: &mut Session,
    env: &CallEnv,
    batch: &Batch,
    dry_run: bool,
    detail: bool,
) -> Result<Value, ToolError> {
    let mode = if dry_run {
        ApplyMode::DryRun
    } else {
        ApplyMode::Commit
    };
    let outcome: ApplyOutcome = session
        .apply_with(
            batch,
            mode,
            ApplyOptions {
                detail,
                source: env.source,
            },
        )
        .map_err(|e| ToolError::from_command(&e, env.lang))?;
    to_value(&ViewCtx::new(session).outcome(&outcome))
}

/// Rendu SVG du Gantt (`render.gantt`).
fn render(s: &Session, lang: Language, p: &RenderGanttParams) -> Result<Value, ToolError> {
    let max_rows = p.max_rows.unwrap_or(MAX_RENDER_ROWS);
    if max_rows == 0 {
        return Err(ToolError::simple(
            codes::PARAM_INVALID,
            tr(
                lang,
                "« max_rows » doit valoir au moins 1",
                "\"max_rows\" must be at least 1",
            )
            .to_owned(),
        )
        .with_field("max_rows"));
    }
    if max_rows > MAX_RENDER_ROWS {
        let e = CommandError::new(
            codes::LIMIT_EXCEEDED,
            CommandErrorKind::Limit {
                what: "max_rows",
                max: u64::from(MAX_RENDER_ROWS),
            },
        )
        .with_field("max_rows");
        return Err(ToolError::from_command(&e, lang));
    }
    let sched = s.schedule();
    let range = match (p.from, p.to) {
        (None, None) => plannercraft_render::RangeSpec::Auto,
        (from, to) => {
            let from = from.unwrap_or_else(|| sched.project_start.day());
            let to = to.unwrap_or_else(|| sched.project_finish.day());
            if from > to {
                return Err(ToolError::simple(
                    codes::PARAM_INVALID,
                    tr(lang, "« from » est après « to »", "\"from\" is after \"to\"").to_owned(),
                )
                .with_field("from"));
            }
            plannercraft_render::RangeSpec::Span { from, to }
        }
    };
    let zoom = match p.zoom.unwrap_or_default() {
        ZoomJson::Auto => plannercraft_render::Zoom::Auto {
            width_px: AUTO_WIDTH_PX,
        },
        ZoomJson::Day => plannercraft_render::Zoom::Day,
        ZoomJson::Week => plannercraft_render::Zoom::Week,
        ZoomJson::Month => plannercraft_render::Zoom::Month,
        ZoomJson::Quarter => plannercraft_render::Zoom::Quarter,
    };
    let req = plannercraft_render::GanttRequest {
        range,
        zoom,
        filter: plannercraft_render::RowFilter {
            critical_only: p.critical_only,
            uids: None,
            max_outline: None,
        },
        language: match lang {
            Language::Fr => plannercraft_render::Lang::Fr,
            Language::En => plannercraft_render::Lang::En,
        },
        theme: plannercraft_render::Theme::Light,
        limits: plannercraft_render::RenderLimits {
            max_rows,
            ..plannercraft_render::RenderLimits::default()
        },
    };
    // Style : modèle demandé, sinon celui du document (L6, H-R12), sinon
    // Classique (modèle par défaut de J8 jusqu'au lot visible) ; textes
    // autour des barres formatés ici (règle 3).
    let style = p.style.map_or_else(
        || style::effective_style(s.project(), style::HEADLESS_DEFAULT),
        |t| {
            // Un modèle demandé garde le regroupement du document
            // (finitions P3, H-UI-129) : une vue, pas une mise en forme.
            let mut st = plannercraft_render::style::template(t.into());
            st.group_by = crate::command::group_by::group_by_of(s.project());
            st
        },
    );
    let texts = crate::render_text::SessionTexts { session: s, lang };
    let (svg, report) = plannercraft_render::svg_styled(s.project(), sched, &req, &style, &texts);
    to_value(&RenderView {
        svg,
        shown: report.shown,
        hidden: report.hidden,
        truncated_days: report.truncated_days,
        truncated_bytes: report.truncated_bytes,
        bytes: report.bytes,
    })
}

/// Espace de travail : au plus une session ouverte.
#[derive(Debug)]
pub struct Workspace {
    session: Option<Session>,
    now: Moment,
    lang: Language,
    root: Option<PathBuf>,
}

/// Erreur d'outil, sérialisée telle quelle (`structured_error` du MCP).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ToolError {
    /// Code stable.
    pub code: &'static str,
    /// Message (langue de l'espace de travail).
    pub message: String,
    /// Index de l'opération fautive.
    pub op_index: Option<usize>,
    /// Champ fautif.
    pub field: Option<String>,
    /// Élément concerné.
    pub subject: Option<Subject>,
    /// Diagnostics (refus R-160).
    pub diagnostics: Vec<Diagnostic>,
}

impl ToolError {
    /// Erreur d'outil d'une commande refusée, message dans `lang`.
    #[must_use]
    pub fn from_command(e: &CommandError, lang: Language) -> Self {
        let diagnostics = match &e.kind {
            CommandErrorKind::WouldCreateErrors(ds) => {
                ds.iter().take(MAX_VIEW_DIAGNOSTICS).cloned().collect()
            }
            _ => Vec::new(),
        };
        Self {
            code: e.code,
            message: e.message(lang),
            op_index: e.op_index,
            field: e.field.clone(),
            subject: e.subject,
            diagnostics,
        }
    }

    /// Erreur sans index, champ, sujet ni diagnostic.
    #[must_use]
    pub const fn simple(code: &'static str, message: String) -> Self {
        Self {
            code,
            message,
            op_index: None,
            field: None,
            subject: None,
            diagnostics: Vec::new(),
        }
    }

    /// Même erreur, sur le champ `field`.
    #[must_use]
    pub fn with_field(mut self, field: &str) -> Self {
        self.field = Some(field.to_owned());
        self
    }
}

/// Texte selon la langue.
const fn tr(lang: Language, fr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Language::Fr => fr,
        Language::En => en,
    }
}

/// Résultat d'une ouverture (`project.open`, `project.import_csv`,
/// `project.new`).
#[derive(Serialize)]
struct OpenedView {
    summary: view::SummaryView,
    /// Diagnostics de lecture, au plus [`MAX_VIEW_DIAGNOSTICS`].
    diagnostics: Vec<DiagView>,
    /// Diagnostics de lecture en tout.
    diagnostics_total: usize,
}

/// Résultat d'un enregistrement (`project.save`, `project.export_mspdi`).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize)]
struct SavedView {
    path: String,
    bytes: usize,
    /// Codes des diagnostics d'écriture (arrondis, troncatures), au plus
    /// [`MAX_VIEW_DIAGNOSTICS`].
    diagnostics: Vec<String>,
    dirty: bool,
}

/// Rapport d'un rendu (`render.gantt`).
#[derive(Serialize)]
struct RenderView {
    svg: String,
    shown: u32,
    hidden: u32,
    truncated_days: bool,
    truncated_bytes: bool,
    bytes: usize,
}

/// Sérialise une vue (les vues n'ont que des clés textuelles : l'échec
/// n'est pas attendu, il devient une erreur propre).
fn to_value<T: Serialize>(v: &T) -> Result<Value, ToolError> {
    serde_json::to_value(v)
        .map_err(|e| ToolError::simple(codes::PARAM_INVALID, format!("sérialisation impossible : {e}")))
}

/// Refus d'une entrée non servie ici : réservée à un jalon
/// (`command.not_yet_available`) ou servie par une application rattachée
/// seulement (`control.not_attached`, J7, C-CTL-10).
fn check_available(available: Availability, id: &str, lang: Language) -> Result<(), ToolError> {
    match available {
        Availability::Now => Ok(()),
        Availability::Milestone(m) => Err(ToolError::from_command(&CommandError::not_yet(m), lang)),
        Availability::Attached => Err(ToolError::simple(
            codes::CONTROL_NOT_ATTACHED,
            format!(
                "{} : {id}",
                tr(lang, "aucune application rattachée", "no attached application")
            ),
        )),
    }
}

/// Retire de `obj` le drapeau booléen `key` (absent : faux).
fn take_flag(obj: &mut Map<String, Value>, key: &str, lang: Language) -> Result<bool, ToolError> {
    match obj.remove(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(b),
        Some(_) => Err(ToolError::simple(
            codes::PARAM_INVALID,
            format!(
                "{} « {key} »",
                tr(lang, "booléen attendu pour", "boolean expected for")
            ),
        )
        .with_field(key)),
    }
}

/// Paramètres en objet JSON (`null` : objet vide).
fn object(params: Value, lang: Language) -> Result<Map<String, Value>, ToolError> {
    match params {
        Value::Object(m) => Ok(m),
        Value::Null => Ok(Map::new()),
        _ => Err(ToolError::simple(
            codes::PARAM_INVALID,
            tr(lang, "objet JSON attendu", "JSON object expected").to_owned(),
        )),
    }
}

impl Workspace {
    /// Espace vide.
    #[must_use]
    pub const fn new(now: Moment, lang: Language) -> Self {
        Self {
            session: None,
            now,
            lang,
            root: None,
        }
    }

    /// Confine les chemins à `root` (MCP HTTP).
    #[must_use]
    pub fn with_root(mut self, root: PathBuf) -> Self {
        self.root = Some(root);
        self
    }

    /// Langue des messages et des sessions créées ici.
    #[must_use]
    pub const fn lang(&self) -> Language {
        self.lang
    }

    /// « Maintenant » injecté.
    #[must_use]
    pub const fn now(&self) -> Moment {
        self.now
    }

    /// Session ouverte.
    #[must_use]
    pub const fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// Session ouverte, modifiable.
    pub const fn session_mut(&mut self) -> Option<&mut Session> {
        self.session.as_mut()
    }

    /// Remplace la session ouverte (une ouverture remplace la précédente,
    /// même modifiée : l'appelant enregistre avant s'il le faut).
    pub fn set_session(&mut self, session: Session) {
        self.session = Some(session);
    }

    /// Routeur unique : commande `id` du registre, paramètres JSON.
    ///
    /// # Errors
    ///
    /// [`ToolError`] pour tout refus : `param.invalid` (id inconnu,
    /// paramètres mal formés), `command.not_yet_available` (commande
    /// réservée), `session.none` (aucun projet ouvert), `file.*`, ou
    /// l'erreur de la commande (`task.not_found`,
    /// `batch.would_create_errors`…). L'état est intact après un refus.
    pub fn call(&mut self, id: &str, params: Value) -> Result<Value, ToolError> {
        let lang = self.lang;
        let cmd = |e: CommandError| ToolError::from_command(&e, lang);
        let Some(spec) = lookup(id) else {
            return Err(unknown_command(id, lang));
        };
        check_available(spec.available, id, lang)?;
        match id {
            "project.open" => self.open(&parse_params::<OpenProjectParams>(params).map_err(cmd)?),
            "project.new" => self.new_project(parse_params::<NewProjectParams>(params).map_err(cmd)?),
            "project.import_csv" => self.import_csv(&parse_params::<ImportCsvParams>(params).map_err(cmd)?),
            _ => {
                let env = CallEnv {
                    now: self.now,
                    lang,
                    root: self.root.clone(),
                    source: EditSource::User,
                };
                route(self.session.as_mut(), &env, id, params)
            }
        }
    }

    /// Ouvre une session, renvoie son résumé et les diagnostics de lecture.
    fn opened(
        &mut self,
        session: Session,
        diagnostics: Vec<DiagView>,
        total: usize,
    ) -> Result<Value, ToolError> {
        let summary = view::summary(&session);
        self.session = Some(session);
        to_value(&OpenedView {
            summary,
            diagnostics,
            diagnostics_total: total,
        })
    }

    fn new_project(&mut self, p: NewProjectParams) -> Result<Value, ToolError> {
        let lang = self.lang;
        let anchor = match (p.start, p.finish) {
            (Some(m), None) => AnchorSpec::Start(m),
            (None, Some(m)) => AnchorSpec::Finish(m),
            _ => {
                return Err(ToolError::simple(
                    codes::PARAM_INVALID,
                    tr(
                        lang,
                        "indiquer exactement une date : « start » ou « finish »",
                        "give exactly one date: \"start\" or \"finish\"",
                    )
                    .to_owned(),
                )
                .with_field("start"));
            }
        };
        let init = NewProject {
            name: p.name,
            anchor,
            lang: Some(p.lang.unwrap_or(lang)),
        };
        let session = Session::new_project(init, self.now).map_err(|e| ToolError::from_command(&e, lang))?;
        self.opened(session, Vec::new(), 0)
    }
}

/// Erreur `session.none`.
fn no_session(lang: Language) -> ToolError {
    ToolError::simple(
        codes::SESSION_NONE,
        tr(
            lang,
            "aucun projet ouvert : ouvrir (open_project), créer (new_project) ou importer (import_csv) d'abord",
            "no project is open: open (open_project), create (new_project) or import (import_csv) one first",
        )
        .to_owned(),
    )
}

/// Version d'un lot contrôlée avant ses opérations (ADR-0012 L3) : un lot
/// d'un format futur est refusé comme tel (`batch.unsupported_version`),
/// même si ses opérations sont inconnues ici. Absente : 1 ; non entière :
/// `param.invalid` par la lecture qui suit.
fn check_batch_version(obj: &Map<String, Value>) -> Result<(), CommandError> {
    match obj.get("format_version").and_then(Value::as_u64) {
        Some(v) if v != u64::from(Batch::FORMAT_VERSION) => Err(CommandError::new(
            codes::BATCH_UNSUPPORTED_VERSION,
            CommandErrorKind::UnsupportedVersion(u32::try_from(v).unwrap_or(u32::MAX)),
        )
        .with_field("format_version")),
        _ => Ok(()),
    }
}

/// Erreur d'annulation ou de rétablissement.
fn history_error(e: &HistoryError, lang: Language) -> ToolError {
    let message = match e {
        HistoryError::Empty => tr(lang, "rien à annuler ou à rétablir", "nothing to undo or redo").to_owned(),
        HistoryError::Inconsistent(m) => match lang {
            Language::Fr => format!("historique incohérent, session intacte : {m}"),
            Language::En => format!("inconsistent history, session unchanged: {m}"),
        },
    };
    ToolError::simple(e.code(), message)
}

/// Nom affiché d'un document : nom du fichier sans extension.
#[cfg(not(target_arch = "wasm32"))]
fn display_name(path: &std::path::Path) -> String {
    path.file_stem().map_or_else(
        || path.display().to_string(),
        |s| s.to_string_lossy().into_owned(),
    )
}

#[cfg(not(target_arch = "wasm32"))]
impl Workspace {
    /// Chemin effectif d'un paramètre `path`, tel que les outils de fichier
    /// le liront ou l'écriront : confiné à la racine s'il y en a une (un
    /// chemin relatif part alors de la racine), tel quel sinon. Sert à la
    /// garde ADR-0002 des surfaces, qui doit viser le même fichier.
    ///
    /// # Errors
    ///
    /// `param.invalid` (chemin vide) ; `file.outside_root` et les erreurs
    /// de confinement (champ `path`).
    pub fn resolve_path(&self, path: &str) -> Result<PathBuf, ToolError> {
        self.resolve(path, "path")
    }

    /// Chemin effectif : confiné à la racine s'il y en a une.
    fn resolve(&self, path: &str, field: &str) -> Result<PathBuf, ToolError> {
        resolve_in(self.root.as_deref(), self.lang, path, field)
    }

    fn open(&mut self, p: &OpenProjectParams) -> Result<Value, ToolError> {
        let lang = self.lang;
        let path = self.resolve(&p.path, "path")?;
        let bytes = crate::files::read_path(&path, MAX_MSPDI_BYTES)
            .map_err(|e| file_error(&e, lang).with_field("path"))?;
        let origin = crate::session::DocOrigin {
            name: display_name(&path),
            path: Some(path.display().to_string()),
        };
        let (session, diags) = Session::open_bytes(&bytes, origin, self.now, lang).map_err(|e| {
            ToolError::simple(
                e.code(),
                format!(
                    "{} : {e}",
                    tr(lang, "document MSPDI inexploitable", "unusable MSPDI document")
                ),
            )
            .with_field("path")
        })?;
        let views = diags
            .iter()
            .take(MAX_VIEW_DIAGNOSTICS)
            .map(DiagView::from_import)
            .collect();
        self.opened(session, views, diags.len())
    }

    fn import_csv(&mut self, p: &ImportCsvParams) -> Result<Value, ToolError> {
        let lang = self.lang;
        let path = self.resolve(&p.path, "path")?;
        let bytes = crate::files::read_path(&path, MAX_CSV_BYTES)
            .map_err(|e| file_error(&e, lang).with_field("path"))?;
        // Hypothèse : le projet importé est planifié depuis « maintenant »
        // (MS Project prend la date du jour pour un projet neuf).
        let mut options = crate::csv::CsvImportOptions::new(
            plannercraft_core::ScheduleAnchor::FromStart(self.now),
            self.now,
        );
        options.locale = p.lang.map(|l| match l {
            Language::Fr => crate::csv::CsvLocale::Fr,
            Language::En => crate::csv::CsvLocale::En,
        });
        let imported = crate::csv::import(&bytes, &options).map_err(|e| {
            ToolError::simple(
                e.code(),
                format!("{} : {e}", tr(lang, "import CSV impossible", "CSV import failed")),
            )
            .with_field("path")
        })?;
        let origin = crate::session::DocOrigin {
            name: display_name(&path),
            path: None,
        };
        let session = Session::from_project(imported.project, origin, self.now, p.lang.unwrap_or(lang));
        let total = imported.diagnostics.len();
        let views = imported
            .diagnostics
            .iter()
            .take(MAX_VIEW_DIAGNOSTICS)
            .map(|d| DiagView {
                code: d.code.code().to_owned(),
                severity: d.code.severity(),
                subject: None,
                message: format!(
                    "{} {}{}",
                    tr(lang, "ligne", "line"),
                    d.line,
                    d.column.as_deref().map(|c| format!(" ({c})")).unwrap_or_default()
                ),
            })
            .collect();
        self.opened(session, views, total)
    }
}

/// Chemin effectif d'un paramètre : confiné à `root` s'il y en a une.
#[cfg(not(target_arch = "wasm32"))]
fn resolve_in(
    root: Option<&std::path::Path>,
    lang: Language,
    path: &str,
    field: &str,
) -> Result<PathBuf, ToolError> {
    if path.is_empty() {
        return Err(ToolError::simple(
            codes::PARAM_INVALID,
            tr(lang, "chemin vide", "empty path").to_owned(),
        )
        .with_field(field));
    }
    let p = PathBuf::from(path);
    match root {
        None => Ok(p),
        Some(root) => crate::files::confine(root, &p).map_err(|e| file_error(&e, lang).with_field(field)),
    }
}

/// Enregistre `session` (`mark = true` : la marque enregistrée et suit le
/// nouveau chemin) ou en exporte une copie.
#[cfg(not(target_arch = "wasm32"))]
fn save(session: &mut Session, env: &CallEnv, path: Option<&str>, mark: bool) -> Result<Value, ToolError> {
    let lang = env.lang;
    let root = env.root.as_deref();
    let target = if let Some(p) = path {
        resolve_in(root, lang, p, "path")?
    } else {
        let Some(p) = session.origin().path.clone() else {
            return Err(ToolError::simple(
                codes::PARAM_INVALID,
                tr(
                    lang,
                    "projet sans fichier : indiquer « path »",
                    "the project has no file yet: give \"path\"",
                )
                .to_owned(),
            )
            .with_field("path"));
        };
        resolve_in(root, lang, &p, "path")?
    };
    let written = session.save_bytes(&plannercraft_mspdi::write::WriteOptions::default());
    crate::files::write_atomic(&target, &written.xml).map_err(|e| file_error(&e, lang).with_field("path"))?;
    if mark {
        session.set_origin(crate::session::DocOrigin {
            name: display_name(&target),
            path: Some(target.display().to_string()),
        });
        session.mark_saved();
    }
    to_value(&SavedView {
        path: target.display().to_string(),
        bytes: written.xml.len(),
        diagnostics: written
            .diagnostics
            .iter()
            .take(MAX_VIEW_DIAGNOSTICS)
            .map(|d| d.code.code().to_owned())
            .collect(),
        dirty: session.is_dirty(),
    })
}

/// Résultat d'un export PDF (`project.export_pdf`).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize)]
struct PdfView {
    path: String,
    bytes: usize,
    #[serde(flatten)]
    report: crate::pdf::PdfExportReport,
}

/// Exporte le planning en PDF vers `p.path` (écriture atomique ; le
/// document et son historique ne changent pas).
#[cfg(not(target_arch = "wasm32"))]
fn export_pdf(session: &Session, env: &CallEnv, p: &ExportPdfParams) -> Result<Value, ToolError> {
    let lang = env.lang;
    let target = resolve_in(env.root.as_deref(), lang, &p.path, "path")?;
    let out = crate::pdf::export(session, p, lang)?;
    crate::files::write_atomic(&target, &out.bytes).map_err(|e| file_error(&e, lang).with_field("path"))?;
    to_value(&PdfView {
        path: target.display().to_string(),
        bytes: out.bytes.len(),
        report: out.report,
    })
}

/// Erreur d'outil d'une erreur de fichier.
#[cfg(not(target_arch = "wasm32"))]
fn file_error(e: &crate::files::FileError, lang: Language) -> ToolError {
    ToolError::simple(e.code(), e.message(lang))
}

#[cfg(target_arch = "wasm32")]
impl Workspace {
    fn no_files(&self, code: &'static str) -> ToolError {
        ToolError::simple(
            code,
            tr(
                self.lang,
                "pas de système de fichiers dans cette version",
                "no file system in this build",
            )
            .to_owned(),
        )
    }

    fn open(&mut self, _: &OpenProjectParams) -> Result<Value, ToolError> {
        Err(self.no_files(codes::FILE_READ))
    }

    fn import_csv(&mut self, _: &ImportCsvParams) -> Result<Value, ToolError> {
        Err(self.no_files(codes::FILE_READ))
    }
}

/// Export PDF : pas de système de fichiers en wasm (l'interface web passe
/// par `crate::pdf::export` et rend les octets au navigateur).
#[cfg(target_arch = "wasm32")]
fn export_pdf(_: &Session, env: &CallEnv, _: &ExportPdfParams) -> Result<Value, ToolError> {
    save_unavailable(env.lang)
}

/// Erreur `file.write` de la version web.
#[cfg(target_arch = "wasm32")]
fn save_unavailable(lang: Language) -> Result<Value, ToolError> {
    Err(ToolError::simple(
        codes::FILE_WRITE,
        tr(
            lang,
            "pas de système de fichiers dans cette version",
            "no file system in this build",
        )
        .to_owned(),
    ))
}

/// Enregistrement : pas de système de fichiers en wasm.
#[cfg(target_arch = "wasm32")]
fn save(_: &mut Session, env: &CallEnv, _: Option<&str>, _: bool) -> Result<Value, ToolError> {
    Err(ToolError::simple(
        codes::FILE_WRITE,
        tr(
            env.lang,
            "pas de système de fichiers dans cette version",
            "no file system in this build",
        )
        .to_owned(),
    ))
}
