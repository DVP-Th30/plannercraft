//! Modèle de lignes partagé par la table et le Gantt.
//!
//! Tout ce qui coûte O(n) se fait une fois, hors de l'image : à l'ouverture
//! et au changement de langue, jamais pendant le défilement ni le zoom. Les
//! textes des cellules sont pré-formatés (ligne par ligne, une entrée par
//! colonne de [`Column::ALL`]) ; les liens sont triés par ligne minimale.
//!
//! Champs de la seconde passe UX (lot L7, H-UI-117) : leurs textes ne sont
//! calculés que pour les colonnes **affichées**, une colonne entière à la
//! fois, à la première demande ([`RowModel::ensure_columns`], appelée par
//! `App::ensure_row_columns` avant la vue des lignes) ; le modèle étant
//! reconstruit à chaque génération, le cache vaut pour une génération et
//! s'étend quand une colonne est insérée. Valeurs lues du modèle ou du
//! `Schedule` du moteur, formatées par `format.rs` : aucune arithmétique de
//! date ou de durée ici (règle 3), hormis la somme du travail restant des
//! affectations d'une feuille (R-211 par affectation, hypothèse H-UI-117).

use std::collections::BTreeMap;
use std::ops::Range;

use plannercraft_automation::text::format_resource_names;
use plannercraft_core::baseline::{TaskVariance, variance};
use plannercraft_core::model::custom::{CustomFieldId, CustomKind, CustomValue};
use plannercraft_core::{
    Assignment, BaselineNumber, Decimal, Link, LinkType, Minutes, Moment, Percent, ProgressState, Project,
    Schedule, SchedulingMode, Task, TaskBaseline, TaskOutcome, TaskRole, TaskSchedule, TaskUid, TimeBasis,
    Work,
};

use crate::app::{App, OpenDocument};
use crate::column::Column;
use crate::field::{Availability, availability};
use crate::format::{
    FieldLabels, FormatLabels, format_date, format_duration, format_minutes_as, format_notes_line,
    format_percent, format_predecessors, format_priority, format_slack, format_work, format_yes_no,
    write_decimal,
};

/// Nature d'une ligne, pour le dessin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// Ligne vide (tâche nulle).
    Null,
    /// Tâche feuille planifiée.
    Leaf,
    /// Tâche récapitulative.
    Summary,
    /// Jalon.
    Milestone,
    /// Tâche exclue du calcul (inactive…).
    Excluded,
}

/// Une ligne de la vue (ordre des ID, tri interdit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// UID de la tâche.
    pub uid: TaskUid,
    /// ID affiché (position + 1).
    pub id: u32,
    /// Niveau hiérarchique (≥ 1).
    pub level: u16,
    /// Nature de la ligne.
    pub kind: RowKind,
    /// Sur le chemin critique.
    pub critical: bool,
    /// Début et fin de la barre, issus du planning (aucun calcul ici).
    pub bar: Option<(Moment, Moment)>,
    /// % achevé (J7, lot L6) : calculé par le moteur pour une tâche
    /// planifiée (R-200, R-210), saisi sinon ; 1/10 000.
    pub percent: Percent,
    /// Début et fin de la ligne de base 0, si les deux sont enregistrés
    /// (J7, lot L6 ; dessin au lot L7).
    pub baseline: Option<(Moment, Moment)>,
    /// Variation de fin à la ligne de base 0, en minutes ouvrées signées
    /// (R-224, `baseline::variance`) ; `None` sans fin de base (J7, lot L6).
    pub finish_variance: Option<Minutes>,
    /// Fin de la partie réelle (`Stop`, R-204) d'une tâche commencée (J7,
    /// lot L6 ; bande d'avancement au lot L7).
    pub stop: Option<Moment>,
    /// Planifiée manuellement d'après le moteur (J8, lot L11 : barre du
    /// rôle `Manual`, H-UI-36).
    pub manual: bool,
    /// Tâche active (J8, lot L11 : une inactive est grisée et barrée dans
    /// la table, H-UI-37).
    pub active: bool,
}

/// Lien à dessiner, en indices de ligne.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkView {
    /// Ligne du prédécesseur.
    pub from: u32,
    /// Ligne du successeur.
    pub to: u32,
    /// Type de lien.
    pub kind: LinkType,
    /// Lien critique (les deux extrémités sont critiques).
    pub critical: bool,
}

/// Lignes, textes pré-formatés et liens d'un document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowModel {
    /// Lignes dans l'ordre des ID.
    rows: Vec<Row>,
    /// `rows.len() × Column::COUNT` textes, ligne par ligne.
    texts: Vec<String>,
    /// Liens triés par `min(from, to)`.
    links: Vec<LinkView>,
    /// Plus grand écart `|from − to|` d'un lien (bornage de la recherche).
    max_span: u32,
    /// Indice de ligne de chaque tâche.
    index: BTreeMap<TaskUid, usize>,
    /// Textes des champs de la seconde passe UX déjà demandés (lot L7) :
    /// une entrée par colonne, `rows.len()` textes dans l'ordre des lignes.
    fields: BTreeMap<Column, Vec<String>>,
    /// Écarts à la ligne de base 0 (dans l'ordre des UID ; vide sans ligne
    /// de base 0), gardés pour les colonnes d'écart.
    variances: Vec<TaskVariance>,
}

impl RowModel {
    /// Construit le modèle d'un document avec les libellés d'une langue.
    ///
    /// Une ligne par tâche, dans l'ordre des ID ; nature de la ligne :
    /// `Null` (ligne vide), `Excluded` (exclue du calcul, barre d'affichage
    /// R-062 si elle existe), `Summary`, `Milestone` (jalon, ou feuille de
    /// durée nulle : hypothèse H-G1), sinon `Leaf`. Textes : hypothèses
    /// H-UI-08 (durée d'une récapitulative dans l'unité par défaut du projet,
    /// en temps ouvré), H-UI-09 (marge en jours ouvrés), H-UI-10 (tâche
    /// exclue : colonnes calculées vides, sauf les dates d'affichage d'une
    /// inactive ; ligne vide : seul l'ID).
    #[must_use]
    pub fn build(doc: &OpenDocument, labels: &FormatLabels) -> Self {
        let project = doc.project();
        let count = project.tasks.len();
        let mut rows = Vec::with_capacity(count);
        let mut texts = Vec::with_capacity(count.saturating_mul(Column::COUNT));
        let mut index = BTreeMap::new();
        let extras = Extras::collect(doc);
        for (position, task) in project.tasks.iter().enumerate() {
            let uid = task.uid();
            let outcome = doc.schedule().tasks.get(&uid);
            let row = make_row(project, task, position, outcome, &extras);
            push_texts(&mut texts, project, task, &row, outcome, labels, &extras);
            index.insert(uid, position);
            rows.push(row);
        }
        let (links, max_span) = collect_links(project, &rows, &index);
        Self {
            rows,
            texts,
            links,
            max_span,
            index,
            fields: BTreeMap::new(),
            variances: extras.variances,
        }
    }

    /// Construit le modèle puis les textes des champs `columns` (lot L7 ;
    /// voir [`RowModel::ensure_columns`]).
    #[must_use]
    pub fn build_with(
        doc: &OpenDocument,
        labels: &FormatLabels,
        fields: &FieldLabels,
        columns: impl IntoIterator<Item = Column>,
    ) -> Self {
        let mut model = Self::build(doc, labels);
        model.ensure_columns(doc, labels, fields, columns);
        model
    }

    /// Vrai si les textes de toutes les colonnes `columns` sont prêts
    /// (colonnes historiques, champs déjà calculés, champs sans valeur).
    #[must_use]
    pub fn has_columns(&self, columns: impl IntoIterator<Item = Column>) -> bool {
        columns
            .into_iter()
            .all(|c| !served(c) || self.fields.contains_key(&c))
    }

    /// Calcule les textes des champs de `columns` qui ne le sont pas encore
    /// (lot L7, H-UI-117) : une colonne entière à la fois, `doc` étant le
    /// document dont le modèle a été construit. Les colonnes historiques
    /// ([`Column::ALL`]) et les coûts (grisés) n'ont rien à calculer ici ;
    /// les champs personnalisés (lot L9) sont lus du modèle comme les
    /// autres.
    pub fn ensure_columns(
        &mut self,
        doc: &OpenDocument,
        labels: &FormatLabels,
        fields: &FieldLabels,
        columns: impl IntoIterator<Item = Column>,
    ) {
        let mut missing: Vec<Column> = Vec::new();
        for c in columns {
            if served(c) && !self.fields.contains_key(&c) && !missing.contains(&c) {
                missing.push(c);
            }
        }
        if missing.is_empty() {
            return;
        }
        let project = doc.project();
        let ctx = FieldCtx {
            project,
            schedule: doc.schedule(),
            labels,
            fields,
            variances: &self.variances,
            successors: if missing.contains(&Column::Successors) {
                successors_of(project)
            } else {
                BTreeMap::new()
            },
            assignments: if missing.contains(&Column::RemainingWork) {
                assignments_of(project)
            } else {
                BTreeMap::new()
            },
        };
        for column in missing {
            let texts: Vec<String> = project
                .tasks
                .iter()
                .zip(&self.rows)
                .map(|(task, row)| field_text(&ctx, task, row, column))
                .collect();
            self.fields.insert(column, texts);
        }
    }

    /// Texte de la cellule `column` de la seule tâche `uid`, mis en forme
    /// comme dans le tableau (H-UI-186 : infobulle d'une barre) : texte
    /// gardé si la colonne est déjà servie ([`RowModel::text`]), sinon
    /// calculé pour cette tâche seulement, sans rien garder. Chaîne vide
    /// pour une tâche absente.
    #[must_use]
    pub fn cell_text(
        &self,
        doc: &OpenDocument,
        labels: &FormatLabels,
        fields: &FieldLabels,
        uid: TaskUid,
        column: Column,
    ) -> String {
        let Some(i) = self.index_of(uid) else {
            return String::new();
        };
        if !served(column) || self.fields.contains_key(&column) {
            return self.text(i, column).to_owned();
        }
        let project = doc.project();
        let (Some(task), Some(row)) = (project.tasks.get(uid), self.rows.get(i)) else {
            return String::new();
        };
        let ctx = FieldCtx {
            project,
            schedule: doc.schedule(),
            labels,
            fields,
            variances: &self.variances,
            successors: if column == Column::Successors {
                successors_of(project)
            } else {
                BTreeMap::new()
            },
            assignments: if column == Column::RemainingWork {
                assignments_of(project)
            } else {
                BTreeMap::new()
            },
        };
        field_text(&ctx, task, row, column)
    }

    /// Nombre de lignes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Aucune ligne.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Ligne d'indice `i`, si elle existe.
    #[must_use]
    pub fn row(&self, i: usize) -> Option<&Row> {
        self.rows.get(i)
    }

    /// Texte pré-formaté d'une cellule ; chaîne vide hors bornes.
    #[must_use]
    pub fn text(&self, i: usize, c: Column) -> &str {
        match c.ordinal() {
            Some(ordinal) => i
                .checked_mul(Column::COUNT)
                .and_then(|base| base.checked_add(ordinal))
                .and_then(|k| self.texts.get(k))
                .map_or("", String::as_str),
            // Champ de la seconde passe UX : texte de la colonne s'il a été
            // demandé ([`RowModel::ensure_columns`]), vide sinon.
            None => self
                .fields
                .get(&c)
                .and_then(|texts| texts.get(i))
                .map_or("", String::as_str),
        }
    }

    /// Indice de la ligne d'une tâche (O(log n)).
    #[must_use]
    pub fn index_of(&self, uid: TaskUid) -> Option<usize> {
        self.index.get(&uid).copied()
    }

    /// Tous les liens, triés par `min(from, to)` croissant.
    #[must_use]
    pub fn links(&self) -> &[LinkView] {
        &self.links
    }

    /// Plus grand écart `|from − to|` d'un lien.
    #[must_use]
    pub const fn max_span(&self) -> u32 {
        self.max_span
    }

    /// Liens dont l'intervalle de lignes `[min, max]` coupe `rows` (culling
    /// par intervalle : un lien qui traverse la fenêtre est gardé même si ses
    /// deux extrémités sont hors champ).
    ///
    /// Balayage des liens triés par `min` : on saute par dichotomie ceux dont
    /// `min + max_span < rows.start` (leur `max` est forcément avant la
    /// fenêtre), puis on parcourt tant que `min < rows.end` en filtrant
    /// `max >= rows.start`. Coût O(log L + liens dont `min` est dans
    /// `[rows.start − max_span, rows.end)`) : O(L) au pire si un lien très
    /// long existe, ce qui reste négligeable (10 000 paires d'entiers).
    pub fn links_touching(&self, rows: Range<usize>) -> impl Iterator<Item = &LinkView> + '_ {
        let span = usize::try_from(self.max_span).unwrap_or(usize::MAX);
        let skip = self
            .links
            .partition_point(|link| link_low(link).saturating_add(span) < rows.start);
        let tail = self.links.get(skip..).unwrap_or(&[]);
        tail.iter()
            .take_while(move |link| link_low(link) < rows.end)
            .filter(move |link| link_high(link) >= rows.start)
    }
}

/// Ligne minimale d'un lien.
fn link_low(link: &LinkView) -> usize {
    usize::try_from(link.from.min(link.to)).unwrap_or(usize::MAX)
}

/// Ligne maximale d'un lien.
fn link_high(link: &LinkView) -> usize {
    usize::try_from(link.from.max(link.to)).unwrap_or(usize::MAX)
}

/// Planning d'une tâche planifiée, s'il existe.
const fn scheduled(outcome: Option<&TaskOutcome>) -> Option<&TaskSchedule> {
    match outcome {
        Some(TaskOutcome::Scheduled(s)) => Some(s),
        _ => None,
    }
}

/// Ligne d'une tâche (sans ses textes).
fn make_row(
    project: &Project,
    task: &Task,
    position: usize,
    outcome: Option<&TaskOutcome>,
    extras: &Extras<'_>,
) -> Row {
    let uid = task.uid();
    let id = project.task_id(uid).map_or_else(
        || {
            u32::try_from(position)
                .ok()
                .and_then(|p| p.checked_add(1))
                .unwrap_or(u32::MAX)
        },
        plannercraft_core::TaskId::get,
    );
    let (kind, critical, bar) = if task.is_null {
        (RowKind::Null, false, None)
    } else {
        match outcome {
            Some(TaskOutcome::Scheduled(s)) => {
                let kind = if s.role == TaskRole::Summary {
                    RowKind::Summary
                } else if task.milestone || task.duration.is_zero() {
                    RowKind::Milestone
                } else {
                    RowKind::Leaf
                };
                (kind, s.critical, Some((s.start, s.finish)))
            }
            Some(TaskOutcome::Excluded { display, .. }) => (
                RowKind::Excluded,
                false,
                display.map(|span| (span.start, span.finish)),
            ),
            None => (RowKind::Excluded, false, None),
        }
    };
    let progress = scheduled(outcome).and_then(|s| s.progress);
    let base = task.baselines.get(&BaselineNumber::PRIMARY);
    Row {
        uid,
        id,
        level: task.outline_level.get(),
        kind,
        critical,
        bar,
        percent: progress.map_or(task.progress.percent_complete, |p| p.percent_complete),
        baseline: base.and_then(|b| b.start.zip(b.finish)),
        finish_variance: extras
            .variance(uid)
            .filter(|v| v.baseline.is_some_and(|b| b.finish.is_some()))
            .map(|v| v.variance.finish),
        stop: progress.map(|p| p.stop),
        manual: scheduled(outcome).is_some_and(|s| s.manual),
        active: task.active,
    }
}

/// Données des colonnes J7 calculées une fois par construction : écarts à
/// la ligne de base 0 (`baseline::variance`, calendriers compilés une fois,
/// seulement si une tâche a une ligne 0) et affectations par tâche (ordre
/// du projet).
struct Extras<'a> {
    /// Écarts des tâches planifiées, dans l'ordre des UID.
    variances: Vec<TaskVariance>,
    /// Affectations de chaque tâche, dans l'ordre du projet.
    assignments: BTreeMap<TaskUid, Vec<&'a Assignment>>,
}

impl<'a> Extras<'a> {
    /// Collecte les données d'un document.
    fn collect(doc: &'a OpenDocument) -> Self {
        let project = doc.project();
        let any_baseline = project
            .tasks
            .iter()
            .any(|t| t.baselines.contains_key(&BaselineNumber::PRIMARY));
        // Calendriers du cache du document (N3), compilés une fois par
        // génération : pas de seconde compilation à chaque reconstruction
        // des lignes (relecture finale J7, performances F5).
        let variances = if any_baseline {
            let set = doc.calendars();
            variance(project, doc.schedule(), &set, BaselineNumber::PRIMARY).tasks
        } else {
            Vec::new()
        };
        let mut assignments: BTreeMap<TaskUid, Vec<&'a Assignment>> = BTreeMap::new();
        for a in &project.assignments {
            assignments.entry(a.task).or_default().push(a);
        }
        Self {
            variances,
            assignments,
        }
    }

    /// Écarts d'une tâche planifiée (recherche dichotomique par UID).
    fn variance(&self, uid: TaskUid) -> Option<&TaskVariance> {
        self.variances
            .binary_search_by_key(&uid, |v| v.uid)
            .ok()
            .and_then(|i| self.variances.get(i))
    }

    /// Affectations d'une tâche.
    fn assignments(&self, uid: TaskUid) -> &[&'a Assignment] {
        self.assignments.get(&uid).map_or(&[], Vec::as_slice)
    }
}

/// Ajoute les `Column::COUNT` textes d'une ligne, dans l'ordre de
/// [`Column::ALL`].
fn push_texts(
    texts: &mut Vec<String>,
    project: &Project,
    task: &Task,
    row: &Row,
    outcome: Option<&TaskOutcome>,
    l: &FormatLabels,
    extras: &Extras<'_>,
) {
    let f = &project.options.factors;
    let schedule = scheduled(outcome);
    for column in Column::ALL {
        let text = if row.kind == RowKind::Null && column != Column::Id {
            String::new()
        } else {
            match column {
                Column::Id => row.id.to_string(),
                Column::Name => task.name.clone(),
                Column::Duration => match (row.kind, schedule) {
                    (RowKind::Summary, Some(s)) => format_minutes_as(
                        s.duration,
                        project.options.default_duration_unit,
                        TimeBasis::Working,
                        f,
                        l,
                    ),
                    _ => format_duration(&task.duration, f, l),
                },
                Column::Start => row
                    .bar
                    .map(|(start, _)| format_date(start, l))
                    .unwrap_or_default(),
                Column::Finish => row
                    .bar
                    .map(|(_, finish)| format_date(finish, l))
                    .unwrap_or_default(),
                Column::Predecessors => format_predecessors(&task.predecessors, project, f, l),
                Column::TotalSlack => schedule
                    .map(|s| format_slack(s.total_slack, f, l))
                    .unwrap_or_default(),
                Column::Critical => schedule
                    .map(|s| if s.critical { l.yes.clone() } else { l.no.clone() })
                    .unwrap_or_default(),
                // Colonnes J7 (lot L6).
                Column::PercentComplete => format_percent(row.percent, l),
                Column::ResourceNames => {
                    format_resource_names(extras.assignments(row.uid).iter().copied(), project, l)
                }
                Column::BaselineStart => base_date(task, |b| b.start, l),
                Column::BaselineFinish => base_date(task, |b| b.finish, l),
                // Jours ouvrés signés, comme la marge (H-UI-09).
                Column::FinishVariance => row
                    .finish_variance
                    .map(|m| format_slack(m, f, l))
                    .unwrap_or_default(),
                // Mode saisi dans le modèle (H-UI-36, J8 lot L11).
                Column::Mode => match task.mode {
                    SchedulingMode::Auto => l.mode_auto.clone(),
                    SchedulingMode::Manual { .. } => l.mode_manual.clone(),
                },
                // Champs de la seconde passe UX : hors de `Column::ALL`
                // (valeurs servies par le lot L7).
                _ => String::new(),
            }
        };
        texts.push(text);
    }
}

/// Date de la ligne de base 0 d'une tâche (vide si absente).
fn base_date(
    task: &Task,
    pick: impl Fn(&plannercraft_core::TaskBaseline) -> Option<Moment>,
    l: &FormatLabels,
) -> String {
    task.baselines
        .get(&BaselineNumber::PRIMARY)
        .and_then(pick)
        .map(|m| format_date(m, l))
        .unwrap_or_default()
}

/// Vrai si `column` est un champ de la seconde passe UX dont la valeur est
/// servie par [`RowModel::ensure_columns`] (ni colonne historique, ni coût
/// grisé ; champs personnalisés compris, lot L9).
fn served(column: Column) -> bool {
    column.ordinal().is_none() && !matches!(availability(column), Availability::Unavailable(_))
}

/// Texte d'un champ personnalisé (lot L9, H-UI-118) : texte tel quel,
/// nombre dans la langue (« 12,5 » en français, sans zéro final), date
/// comme le Début, indicateur Oui/Non (absent : Non) ; vide sinon.
fn custom_text(task: &Task, id: CustomFieldId, l: &FormatLabels) -> String {
    match task.custom.get(&id) {
        Some(CustomValue::Text(t)) => t.clone(),
        Some(CustomValue::Number(n)) => Decimal::new(*n, CUSTOM_NUMBER_DECIMALS).map_or_else(
            |_| l.unknown.clone(),
            |d| {
                let mut out = String::new();
                write_decimal(&mut out, d, l.decimal_sep);
                out
            },
        ),
        Some(CustomValue::Date(m)) => format_date(*m, l),
        Some(CustomValue::Flag(b)) => format_yes_no(*b, l),
        None if id.kind() == CustomKind::Flag => format_yes_no(false, l),
        None => String::new(),
    }
}

/// Décimales du point fixe des nombres personnalisés (1/10 000, R-263).
const CUSTOM_NUMBER_DECIMALS: u8 = 4;

/// Grandeur d'une ligne de base affichée par une colonne.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BaseField {
    /// Début.
    Start,
    /// Fin.
    Finish,
    /// Durée.
    Duration,
    /// Travail.
    Work,
}

/// Ligne de base et grandeur d'une colonne de ligne de base (0 à 10) ;
/// `None` pour une autre colonne.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "une ligne par colonne de ligne de base (44)"
)]
const fn baseline_field(column: Column) -> Option<(u8, BaseField)> {
    use BaseField::{Duration, Finish, Start, Work};
    Some(match column {
        Column::BaselineStart => (0, Start),
        Column::BaselineFinish => (0, Finish),
        Column::BaselineDuration => (0, Duration),
        Column::BaselineWork => (0, Work),
        Column::Baseline1Start => (1, Start),
        Column::Baseline1Finish => (1, Finish),
        Column::Baseline1Duration => (1, Duration),
        Column::Baseline1Work => (1, Work),
        Column::Baseline2Start => (2, Start),
        Column::Baseline2Finish => (2, Finish),
        Column::Baseline2Duration => (2, Duration),
        Column::Baseline2Work => (2, Work),
        Column::Baseline3Start => (3, Start),
        Column::Baseline3Finish => (3, Finish),
        Column::Baseline3Duration => (3, Duration),
        Column::Baseline3Work => (3, Work),
        Column::Baseline4Start => (4, Start),
        Column::Baseline4Finish => (4, Finish),
        Column::Baseline4Duration => (4, Duration),
        Column::Baseline4Work => (4, Work),
        Column::Baseline5Start => (5, Start),
        Column::Baseline5Finish => (5, Finish),
        Column::Baseline5Duration => (5, Duration),
        Column::Baseline5Work => (5, Work),
        Column::Baseline6Start => (6, Start),
        Column::Baseline6Finish => (6, Finish),
        Column::Baseline6Duration => (6, Duration),
        Column::Baseline6Work => (6, Work),
        Column::Baseline7Start => (7, Start),
        Column::Baseline7Finish => (7, Finish),
        Column::Baseline7Duration => (7, Duration),
        Column::Baseline7Work => (7, Work),
        Column::Baseline8Start => (8, Start),
        Column::Baseline8Finish => (8, Finish),
        Column::Baseline8Duration => (8, Duration),
        Column::Baseline8Work => (8, Work),
        Column::Baseline9Start => (9, Start),
        Column::Baseline9Finish => (9, Finish),
        Column::Baseline9Duration => (9, Duration),
        Column::Baseline9Work => (9, Work),
        Column::Baseline10Start => (10, Start),
        Column::Baseline10Finish => (10, Finish),
        Column::Baseline10Duration => (10, Duration),
        Column::Baseline10Work => (10, Work),
        _ => return None,
    })
}

/// Données partagées par le calcul des colonnes d'une passe
/// ([`RowModel::ensure_columns`]).
struct FieldCtx<'a> {
    /// Projet du document.
    project: &'a Project,
    /// Planning du moteur.
    schedule: &'a Schedule,
    /// Libellés d'affichage.
    labels: &'a FormatLabels,
    /// Libellés des champs à choix.
    fields: &'a FieldLabels,
    /// Écarts à la ligne de base 0, dans l'ordre des UID.
    variances: &'a [TaskVariance],
    /// Successeurs de chaque tâche (liens inverses : `predecessor` porte le
    /// successeur), dans l'ordre des ID ; rempli si la colonne Successeurs
    /// est demandée.
    successors: BTreeMap<TaskUid, Vec<Link>>,
    /// Affectations de chaque tâche ; rempli si la colonne Travail restant
    /// est demandée.
    assignments: BTreeMap<TaskUid, Vec<&'a Assignment>>,
}

impl FieldCtx<'_> {
    /// Écarts d'une tâche planifiée.
    fn variance(&self, uid: TaskUid) -> Option<&TaskVariance> {
        self.variances
            .binary_search_by_key(&uid, |v| v.uid)
            .ok()
            .and_then(|i| self.variances.get(i))
    }
}

/// Successeurs de chaque tâche, en liens inverses (`predecessor` = le
/// successeur, type et décalage du lien), dans l'ordre des ID des
/// successeurs.
fn successors_of(project: &Project) -> BTreeMap<TaskUid, Vec<Link>> {
    let mut out: BTreeMap<TaskUid, Vec<Link>> = BTreeMap::new();
    for task in &project.tasks {
        for link in &task.predecessors {
            out.entry(link.predecessor).or_default().push(Link {
                predecessor: task.uid(),
                kind: link.kind,
                lag: link.lag,
            });
        }
    }
    out
}

/// Affectations de chaque tâche, dans l'ordre du projet.
fn assignments_of(project: &Project) -> BTreeMap<TaskUid, Vec<&Assignment>> {
    let mut out: BTreeMap<TaskUid, Vec<&Assignment>> = BTreeMap::new();
    for a in &project.assignments {
        out.entry(a.task).or_default().push(a);
    }
    out
}

/// Texte d'un champ de la seconde passe UX pour une tâche (H-UI-117).
/// Ligne vide : rien. Valeurs du moteur (`TaskSchedule`) vides pour une
/// tâche exclue du calcul (H-UI-10) ; valeurs saisies (modèle) toujours.
#[allow(
    clippy::too_many_lines,
    reason = "une branche par champ de la liste H-UI-117, sans logique partagée"
)]
fn field_text(ctx: &FieldCtx<'_>, task: &Task, row: &Row, column: Column) -> String {
    if row.kind == RowKind::Null {
        return String::new();
    }
    let (project, l, fl) = (ctx.project, ctx.labels, ctx.fields);
    let f = &project.options.factors;
    let uid = task.uid();
    let schedule = scheduled(ctx.schedule.tasks.get(&uid));
    let progress = schedule.and_then(|s| s.progress);
    let date = |m: Option<Moment>| m.map(|m| format_date(m, l)).unwrap_or_default();
    // Durée dans l'unité de la tâche (récapitulative : unité par défaut du
    // projet, temps ouvré, comme la colonne Durée, H-UI-08).
    let in_task_unit = |m: Minutes| {
        if row.kind == RowKind::Summary {
            format_minutes_as(m, project.options.default_duration_unit, TimeBasis::Working, f, l)
        } else {
            format_minutes_as(m, task.duration.unit(), task.duration.basis(), f, l)
        }
    };
    if let Some((number, field)) = baseline_field(column) {
        let base: Option<&TaskBaseline> = BaselineNumber::new(number)
            .ok()
            .and_then(|n| task.baselines.get(&n));
        return match field {
            BaseField::Start => date(base.and_then(|b| b.start)),
            BaseField::Finish => date(base.and_then(|b| b.finish)),
            BaseField::Duration => base
                .and_then(|b| b.duration)
                .map(|d| format_duration(&d, f, l))
                .unwrap_or_default(),
            BaseField::Work => base
                .and_then(|b| b.work)
                .map(|w| format_work(w.minutes(), f, l))
                .unwrap_or_default(),
        };
    }
    match column {
        Column::UniqueId => uid.get().to_string(),
        Column::Wbs => task.wbs.clone().unwrap_or_default(),
        Column::OutlineLevel => task.outline_level.get().to_string(),
        Column::Summary => schedule
            .map(|s| format_yes_no(s.role == TaskRole::Summary, l))
            .unwrap_or_default(),
        Column::Milestone => format_yes_no(task.milestone, l),
        Column::Notes => format_notes_line(&task.notes),
        Column::Created => date(task.created),
        Column::Priority => format_priority(task.priority),
        Column::Active => format_yes_no(task.active, l),
        Column::Successors => {
            let links = ctx.successors.get(&uid).map_or(&[][..], Vec::as_slice);
            format_predecessors(links, project, f, l)
        }
        Column::ConstraintType => fl.constraint(task.constraint.kind()).to_owned(),
        Column::ConstraintDate => date(task.constraint.date()),
        Column::Deadline => date(task.deadline),
        Column::TaskType => fl.task_type(task.task_type).to_owned(),
        Column::EffortDriven => format_yes_no(task.effort_driven, l),
        // Travail du moteur (R-184) ; tâche exclue : travail saisi.
        Column::Work => format_work(schedule.map_or(task.work, |s| s.work).minutes(), f, l),
        Column::EarlyStart => date(schedule.map(|s| s.early.start)),
        Column::EarlyFinish => date(schedule.map(|s| s.early.finish)),
        Column::LateStart => date(schedule.map(|s| s.late.start)),
        Column::LateFinish => date(schedule.map(|s| s.late.finish)),
        Column::FreeSlack => schedule
            .map(|s| format_slack(s.free_slack, f, l))
            .unwrap_or_default(),
        Column::PercentWorkComplete => format_percent(
            progress.map_or(task.progress.percent_work_complete, |p| p.percent_work_complete),
            l,
        ),
        // Début réel : tâche commencée seulement (MS Project : « NA »).
        Column::ActualStart => date(
            progress
                .filter(|p| p.state != ProgressState::NotStarted)
                .map(|p| p.actual_start),
        ),
        Column::ActualFinish => date(progress.and_then(|p| p.actual_finish)),
        Column::ActualDuration => schedule
            .map(|_| in_task_unit(progress.map_or(Minutes::ZERO, |p| p.actual_duration)))
            .unwrap_or_default(),
        // Reste du moteur (R-200) ; tâche non commencée : R-200 sur sa
        // saisie (`progress::remaining_duration`), récapitulative : sa
        // durée planifiée.
        Column::RemainingDuration => schedule
            .map(|s| {
                let rest = match progress {
                    Some(p) => p.remaining_duration,
                    None if row.kind == RowKind::Summary => s.duration,
                    None => plannercraft_core::progress::remaining_duration(task),
                };
                in_task_unit(rest)
            })
            .unwrap_or_default(),
        Column::ActualWork => schedule
            .map(|_| format_work(progress.map_or(Work::ZERO, |p| p.actual_work).minutes(), f, l))
            .unwrap_or_default(),
        // Travail restant : somme des restes de ses affectations (R-211,
        // `progress::remaining_work`) ; récapitulative : vide (hypothèse,
        // aucun agrégat du moteur).
        Column::RemainingWork => match schedule {
            Some(_) if row.kind != RowKind::Summary => {
                let rest = ctx.assignments.get(&uid).map_or(0_i64, |list| {
                    list.iter().fold(0_i64, |sum, a| {
                        sum.saturating_add(plannercraft_core::progress::remaining_work(a).minutes().get())
                    })
                });
                format_work(Minutes::new(rest), f, l)
            }
            _ => String::new(),
        },
        Column::StartVariance => ctx
            .variance(uid)
            .filter(|v| v.baseline.is_some_and(|b| b.start.is_some()))
            .map(|v| format_slack(v.variance.start, f, l))
            .unwrap_or_default(),
        Column::DurationVariance => ctx
            .variance(uid)
            .filter(|v| v.baseline.is_some())
            .map(|v| format_slack(v.variance.duration, f, l))
            .unwrap_or_default(),
        Column::WorkVariance => ctx
            .variance(uid)
            .filter(|v| v.baseline.is_some())
            .map(|v| format_work(v.variance.work, f, l))
            .unwrap_or_default(),
        Column::Custom(id) => custom_text(task, id, l),
        // Colonnes historiques (textes pré-formatés) et coûts grisés : rien
        // ici.
        _ => String::new(),
    }
}

impl App {
    /// Textes des champs affichés prêts dans le modèle de lignes (lot L7) :
    /// calcule ceux des colonnes de `UiState.columns` qui manquent (après
    /// une reconstruction ou l'insertion d'une colonne) ; rien sinon (un
    /// parcours des colonnes affichées par appel). Appelée en tête de
    /// `App::ensure_row_view`.
    pub(crate) fn ensure_row_columns(&mut self) {
        let (Some(doc), Some(rows)) = (self.document.as_ref(), self.rows.as_mut()) else {
            return;
        };
        let shown = || self.state.columns.iter().map(|c| c.column);
        if rows.has_columns(shown()) {
            return;
        }
        let fields = crate::field::field_labels(&self.texts);
        rows.ensure_columns(doc, self.texts.format_labels(), &fields, shown());
    }
}

/// Liens entre lignes connues, triés par `min(from, to)` (tri stable :
/// ordre des successeurs puis de leurs prédécesseurs à égalité), et plus
/// grand écart.
fn collect_links(project: &Project, rows: &[Row], index: &BTreeMap<TaskUid, usize>) -> (Vec<LinkView>, u32) {
    let mut links = Vec::new();
    for (to, task) in project.tasks.iter().enumerate() {
        let Ok(to_u32) = u32::try_from(to) else {
            continue;
        };
        let to_critical = rows.get(to).is_some_and(|r| r.critical);
        for link in &task.predecessors {
            let Some(&from) = index.get(&link.predecessor) else {
                continue;
            };
            let Ok(from_u32) = u32::try_from(from) else {
                continue;
            };
            let from_critical = rows.get(from).is_some_and(|r| r.critical);
            links.push(LinkView {
                from: from_u32,
                to: to_u32,
                kind: link.kind,
                critical: from_critical && to_critical,
            });
        }
    }
    links.sort_by_key(|link| link.from.min(link.to));
    let max_span = links
        .iter()
        .map(|link| link.from.abs_diff(link.to))
        .max()
        .unwrap_or(0);
    (links, max_span)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::panic,
        reason = "tests : un échec doit s'arrêter net et lisiblement"
    )]

    use super::{RowKind, RowModel};
    use crate::app::{DocOrigin, OpenDocument};
    use crate::column::Column;
    use crate::i18n::{Language, Texts};

    fn demo() -> OpenDocument {
        let loaded = crate::document::load(crate::DEMO_MSPDI, "2026-10-17T08:00".parse().unwrap()).unwrap();
        let origin = DocOrigin {
            name: "demo".to_owned(),
            path: None,
            demo: true,
        };
        OpenDocument::new(loaded, origin, 1)
    }

    /// Ligne d'ID `id` (indice `id − 1`), colonnes J6.
    fn row_texts(model: &RowModel, id: usize) -> Vec<&str> {
        Column::ALL[..8].iter().map(|&c| model.text(id - 1, c)).collect()
    }

    /// Colonnes J7 (lot L6) sur l'exemple, sans ligne de base : % achevé
    /// du moteur, ressources des affectations, colonnes de référence vides.
    #[test]
    fn rows_j7_columns_demo() {
        let doc = demo();
        let model = RowModel::build(&doc, Texts::new(Language::Fr).format_labels());
        let project = doc.project();
        for (i, task) in project.tasks.iter().enumerate() {
            let row = model.row(i).unwrap();
            assert!(task.baselines.is_empty());
            assert_eq!(row.baseline, None);
            assert_eq!(row.finish_variance, None);
            for c in [
                Column::BaselineStart,
                Column::BaselineFinish,
                Column::FinishVariance,
            ] {
                assert_eq!(model.text(i, c), "", "{c:?}");
            }
            let percent = model.text(i, Column::PercentComplete);
            if task.is_null {
                assert_eq!(percent, "");
            } else {
                assert!(percent.ends_with('%'), "{percent}");
            }
            let expected = plannercraft_automation::text::format_resource_names(
                project.assignments.iter().filter(|a| a.task == task.uid()),
                project,
                Texts::new(Language::Fr).format_labels(),
            );
            assert_eq!(model.text(i, Column::ResourceNames), expected);
        }
    }

    /// Lot L7 : champs de la seconde passe calculés à la demande, une
    /// colonne entière ; colonnes historiques, coûts et champs
    /// personnalisés hors du cache ; Successeurs en liens inverses.
    #[test]
    fn rows_fields_on_demand() {
        let doc = demo();
        let texts = Texts::new(Language::Fr);
        let mut model = RowModel::build(&doc, texts.format_labels());
        assert_eq!(model.text(2, Column::Successors), "");
        assert!(!model.has_columns([Column::Successors]));
        assert!(model.has_columns([Column::Name, Column::Cost, Column::FinishVariance]));
        let fields = crate::field::field_labels(&texts);
        model.ensure_columns(
            &doc,
            texts.format_labels(),
            &fields,
            [Column::Successors, Column::Wbs],
        );
        assert!(model.has_columns([Column::Successors, Column::Wbs, Column::Name]));
        assert_eq!(model.fields.len(), 2);
        // La tâche 7 a « 7;6 » parmi les prédécesseurs de 9 ⇒ 9 parmi ses
        // successeurs ; chaque lien vu des deux côtés.
        let succ = model.text(6, Column::Successors);
        assert!(succ.split(';').any(|s| s == "9"), "{succ}");
        let pairs: usize = (0..model.len())
            .map(|i| {
                let t = model.text(i, Column::Successors);
                if t.is_empty() { 0 } else { t.split(';').count() }
            })
            .sum();
        assert_eq!(pairs, 46);
        // Construit d'un coup : mêmes textes.
        let again = RowModel::build_with(&doc, texts.format_labels(), &fields, [Column::Successors]);
        for i in 0..model.len() {
            assert_eq!(
                again.text(i, Column::Successors),
                model.text(i, Column::Successors)
            );
        }
        assert_eq!(model.text(usize::MAX, Column::Successors), "");
    }

    #[test]
    fn rows_demo_fr() {
        let doc = demo();
        let model = RowModel::build(&doc, Texts::new(Language::Fr).format_labels());
        assert_eq!(model.len(), 49);
        assert!(!model.is_empty());

        let first = model.row(0).unwrap();
        assert_eq!(first.id, 1);
        assert_eq!(first.level, 1);
        assert_eq!(first.kind, RowKind::Summary);
        assert!(first.critical);
        assert_eq!(
            row_texts(&model, 1),
            [
                "1",
                "Études et permis",
                "31 jours",
                "lun 02/11/26",
                "mar 15/12/26",
                "",
                "0 jour",
                "Oui"
            ]
        );

        let permit = model.row(6).unwrap();
        assert_eq!(permit.kind, RowKind::Milestone);
        assert_eq!(permit.level, 2);
        assert_eq!(model.text(6, Column::Name), "Permis obtenu");
        assert_eq!(model.text(6, Column::Duration), "0 jour");

        assert_eq!(model.row(1).unwrap().kind, RowKind::Leaf);
        assert_eq!(model.text(12, Column::Predecessors), "12FD+2 jours");
        assert_eq!(model.text(25, Column::Predecessors), "25FD-1 jour");
        assert_eq!(model.text(30, Column::Predecessors), "30DD");
        assert_eq!(model.text(17, Column::Predecessors), "17FF");
        assert_eq!(model.text(23, Column::Predecessors), "25DF");
        assert_eq!(model.text(8, Column::Predecessors), "7;6");
        // Tâche non critique : marge positive, « Non ».
        assert_eq!(model.text(5, Column::TotalSlack), "5 jours");
        assert_eq!(model.text(5, Column::Critical), "Non");
        assert!(!model.row(5).unwrap().critical);
        // Dernière ligne : jalon de fin du projet.
        let last = model.row(48).unwrap();
        assert_eq!(last.kind, RowKind::Milestone);
        assert_eq!(
            last.bar,
            Some((doc.schedule().project_finish, doc.schedule().project_finish))
        );
        // Barres issues du planning, ID = position + 1, index inverse cohérent.
        for (i, task) in doc.project().tasks.iter().enumerate() {
            let row = model.row(i).unwrap();
            assert_eq!(row.uid, task.uid());
            assert_eq!(usize::try_from(row.id).unwrap(), i + 1);
            assert_eq!(model.index_of(task.uid()), Some(i));
            let Some(plannercraft_core::TaskOutcome::Scheduled(s)) = doc.schedule().tasks.get(&task.uid())
            else {
                panic!("tâche non planifiée : {}", task.name);
            };
            assert_eq!(row.bar, Some((s.start, s.finish)));
        }
        // Jours fériés du calendrier de l'exemple, importés jusqu'à
        // l'affichage : la tâche 11 (3 jours) commence le jeudi 24/12 et
        // saute Noël et le week-end ; la tâche 3 (10 jours) saute le 11/11.
        assert_eq!(model.text(10, Column::Duration), "3 jours");
        assert_eq!(model.text(10, Column::Start), "jeu 24/12/26");
        assert_eq!(model.text(10, Column::Finish), "mar 29/12/26");
        assert_eq!(model.text(2, Column::Duration), "10 jours");
        assert_eq!(model.text(2, Column::Start), "lun 09/11/26");
        assert_eq!(model.text(2, Column::Finish), "lun 23/11/26");
        // Hors bornes : chaîne vide.
        assert_eq!(model.text(49, Column::Id), "");
        assert_eq!(model.text(usize::MAX, Column::Critical), "");
        assert!(model.row(49).is_none());
    }

    #[test]
    fn rows_demo_en() {
        let model = RowModel::build(&demo(), Texts::new(Language::En).format_labels());
        assert_eq!(
            row_texts(&model, 1),
            [
                "1",
                "Études et permis",
                "31 days",
                "Mon 11/2/26",
                "Tue 12/15/26",
                "",
                "0 days",
                "Yes"
            ]
        );
        assert_eq!(model.text(12, Column::Predecessors), "12FS+2 days");
        assert_eq!(model.text(23, Column::Predecessors), "25SF");
        assert_eq!(model.text(8, Column::Predecessors), "7,6");
    }

    #[test]
    fn rows_links_sorted() {
        let doc = demo();
        let model = RowModel::build(&doc, Texts::new(Language::Fr).format_labels());
        let links = model.links();
        let total: usize = doc.project().tasks.iter().map(|t| t.predecessors.len()).sum();
        assert_eq!(links.len(), total);
        assert_eq!(links.len(), 46);
        assert!(
            links
                .windows(2)
                .all(|w| w[0].from.min(w[0].to) <= w[1].from.min(w[1].to))
        );
        let max_span = links.iter().map(|l| l.from.abs_diff(l.to)).max().unwrap();
        assert_eq!(model.max_span(), max_span);
        // 28 → 43 (indices 27 → 42) est le lien le plus long.
        assert_eq!(max_span, 15);
        // Lien début à fin 25 → 24 : successeur au-dessus du prédécesseur.
        let sf = links
            .iter()
            .find(|l| l.kind == plannercraft_core::LinkType::StartToFinish)
            .unwrap();
        assert_eq!((sf.from, sf.to), (24, 23));
        // Critique si et seulement si les deux extrémités sont critiques.
        for l in links {
            let from = model.row(usize::try_from(l.from).unwrap()).unwrap();
            let to = model.row(usize::try_from(l.to).unwrap()).unwrap();
            assert_eq!(l.critical, from.critical && to.critical, "{l:?}");
        }
    }

    #[test]
    fn rows_links_touching_window() {
        let model = RowModel::build(&demo(), Texts::new(Language::Fr).format_labels());
        let brute = |rows: std::ops::Range<usize>| -> Vec<_> {
            model
                .links()
                .iter()
                .filter(|l| {
                    let low = usize::try_from(l.from.min(l.to)).unwrap();
                    let high = usize::try_from(l.from.max(l.to)).unwrap();
                    low < rows.end && high >= rows.start
                })
                .copied()
                .collect()
        };
        for start in 0..52 {
            for len in 0..20 {
                let rows = start..start + len;
                let got: Vec<_> = model.links_touching(rows.clone()).copied().collect();
                assert_eq!(got, brute(rows.clone()), "{rows:?}");
            }
        }
        // Fenêtre 30..35 traversée par le lien 28 → 43 (indices 27 → 42),
        // dont aucune extrémité n'est visible.
        assert!(model.links_touching(30..35).any(|l| (l.from, l.to) == (27, 42)));
        assert_eq!(model.links_touching(0..0).count(), 0);
        assert_eq!(model.links_touching(100..200).count(), 0);
        assert_eq!(model.links_touching(0..usize::MAX).count(), 46);
        // Modèle vide : aucun lien.
        assert_eq!(RowModel::default().links_touching(0..10).count(), 0);
    }
}
