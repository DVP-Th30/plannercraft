//! Lignes affichées ≠ lignes du modèle (ADR-0016 D1, P51, P53).
//!
//! [`RowView`] se pose au-dessus du `RowModel` (ordre des ID) : permutation
//! `order` (ligne affichée → ligne du modèle) et son inverse `display_of`,
//! liens renumérotés. Deux types distincts, [`DisplayRow`] et [`ModelRow`],
//! pour que le compilateur trouve chaque site d'indexation (règle 10 de J8 :
//! tout indice de ligne passe par [`RowView::model_row`] ou
//! [`RowView::display_row`]).
//!
//! Construction (lot L5, [`RowView::build`]) : filtre partagé de
//! `plannercraft_automation::filter` (R-251, `FilterHits`), puis repli (niveau
//! affiché et récapitulatives repliées), puis tri des frères (« conserver la
//! structure ») ou tri global, liens renumérotés et triés par (plus petite
//! ligne affichée, indice du lien). Sans filtre, tri ni repli, la vue est
//! l'identité : l'interface se comporte exactement comme en J7. Le cœur de
//! la construction ([`RowView::compose`]) est pur, sans document, pour les
//! propriétés P51 et P53.
//!
//! [`VisibleRows`] réunit le `RowModel` et sa vue : c'est par lui que la
//! table, le Gantt, la navigation et la sélection lisent une ligne
//! affichée. Rien n'est écrit dans le document (P50) ; aucune date n'est
//! calculée (règle 3) : les clés de tri sont lues dans le planning du moteur.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ops::Range;

use plannercraft_automation::filter::{FilterCtx, TaskFilter, fold_text};
use plannercraft_core::{Minutes, Moment, Percent, TaskOutcome, TaskUid};

use crate::column::Column;
use crate::rows::{LinkView, Row, RowKind, RowModel};
use crate::state::{FilterChoice, SortKey, SortSpec, TaskViewState};

/// Rang d'une ligne affichée (0 = première ligne visible de la table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DisplayRow(pub u32);

impl DisplayRow {
    /// Ligne affichée d'indice `i` ; au-delà de `u32::MAX - 1`, la marque
    /// [`HIDDEN`] (qui ne désigne aucune ligne : toute lecture rend `None`).
    #[must_use]
    pub fn at(i: usize) -> Self {
        Self(u32::try_from(i).unwrap_or(HIDDEN))
    }

    /// Indice de la ligne affichée.
    #[must_use]
    pub fn index(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

/// Rang d'une ligne du modèle (position dans le projet, ordre des ID).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelRow(pub u32);

impl ModelRow {
    /// Ligne du modèle d'indice `i` (même bornage que [`DisplayRow::at`]).
    #[must_use]
    pub fn at(i: usize) -> Self {
        Self(u32::try_from(i).unwrap_or(HIDDEN))
    }

    /// Indice de la ligne du modèle.
    #[must_use]
    pub fn index(self) -> usize {
        usize::try_from(self.0).unwrap_or(usize::MAX)
    }
}

/// Lien entre deux lignes affichées (bouts masqués : lien absent).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DisplayLink {
    /// Prédécesseur.
    pub from: DisplayRow,
    /// Successeur.
    pub to: DisplayRow,
    /// Indice du lien dans le `RowModel`.
    pub model_link: u32,
}

impl DisplayLink {
    /// Plus petite ligne affichée du lien.
    fn low(&self) -> usize {
        self.from.min(self.to).index()
    }

    /// Plus grande ligne affichée du lien.
    fn high(&self) -> usize {
        self.from.max(self.to).index()
    }
}

/// Marque « ligne masquée » de `display_of`.
pub const HIDDEN: u32 = u32::MAX;

/// Valeur d'une clé de tri (une seule variante par tri ; `None` dans
/// [`SortInput::keys`] : valeur absente, toujours en fin de liste).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SortValue {
    /// ID (ordre du modèle).
    Id(u32),
    /// Nom replié (sans casse ni accents), puis nom exact.
    Text(String, String),
    /// Date du planning.
    Moment(Moment),
    /// Durée ou marge du moteur, en minutes ouvrées.
    Minutes(Minutes),
    /// % achevé.
    Percent(Percent),
}

/// Ligne d'entrée de [`RowView::compose`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewRow {
    /// UID de la tâche.
    pub uid: TaskUid,
    /// Niveau hiérarchique (≥ 1 ; celui d'une ligne vide est celui que le
    /// document lui donne).
    pub level: u16,
    /// Récapitulative (seule à se replier).
    pub summary: bool,
    /// Retenue par le filtre (récapitulatives liées comprises).
    pub shown: bool,
}

/// Tri d'entrée de [`RowView::compose`].
#[derive(Clone, Copy, Debug)]
pub struct SortInput<'a> {
    /// Clé de chaque ligne du modèle (même longueur que les lignes).
    pub keys: &'a [Option<SortValue>],
    /// Décroissant (les valeurs absentes restent en fin de liste).
    pub descending: bool,
    /// Tri des frères sous leur parent (« conserver la structure »).
    pub keep_outline: bool,
}

/// Entrée pure de [`RowView::compose`].
#[derive(Clone, Copy, Debug)]
pub struct ViewInput<'a> {
    /// Lignes du modèle, dans l'ordre des ID.
    pub rows: &'a [ViewRow],
    /// Tri (`None` : ordre des ID).
    pub sort: Option<SortInput<'a>>,
    /// Niveau affiché au plus (`None` : tous).
    pub outline_level: Option<u8>,
    /// Récapitulatives repliées.
    pub collapsed: &'a BTreeSet<TaskUid>,
    /// Liens du modèle (prédécesseur, successeur), en lignes du modèle ;
    /// l'indice dans cette liste devient [`DisplayLink::model_link`].
    pub links: &'a [(u32, u32)],
}

/// Vue des lignes : permutation d'un sous-ensemble des lignes du modèle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowView {
    /// Ligne du modèle de chaque ligne affichée.
    order: Vec<ModelRow>,
    /// Ligne affichée de chaque ligne du modèle ([`HIDDEN`] : masquée).
    display_of: Vec<u32>,
    /// Liens visibles, triés par (plus petite ligne affichée, clé).
    links: Vec<DisplayLink>,
    /// Plus grand écart entre les deux bouts d'un lien visible.
    max_span: u32,
}

impl RowView {
    /// Vue identité de `len` lignes (ordre des ID, rien de masqué, aucun
    /// lien) ; plus de `u32::MAX - 1` lignes : vue vide (jamais atteint, le
    /// projet est borné bien en deçà).
    #[must_use]
    pub fn identity(len: usize) -> Self {
        let Ok(n) = u32::try_from(len) else {
            return Self::default();
        };
        if n == HIDDEN {
            return Self::default();
        }
        Self {
            order: (0..n).map(ModelRow).collect(),
            display_of: (0..n).collect(),
            links: Vec::new(),
            max_span: 0,
        }
    }

    /// Vue d'un `RowModel` selon l'état de la vue des tâches : filtre (sur
    /// le projet et le planning de `ctx`, dont les tâches sont les lignes
    /// du modèle), repli (`state.outline_level`, `collapsed`), tri
    /// (`state.sort`). Un projet dont le nombre de tâches diffère du modèle
    /// (jamais : le modèle est construit sur le même document) est montré
    /// sans filtre.
    #[must_use]
    pub fn build(
        model: &RowModel,
        ctx: &FilterCtx<'_>,
        state: &TaskViewState,
        collapsed: &BTreeSet<TaskUid>,
    ) -> Self {
        let shown = Self::filter_shown(model, ctx, state);
        Self::build_with_shown(model, ctx, state, collapsed, shown.as_deref())
    }

    /// Lignes du modèle retenues par le filtre de `state` (récapitulatives
    /// liées comprises), par position ; `None` sans filtre (ou si le projet
    /// et le modèle diffèrent, jamais) : toute ligne passe.
    #[must_use]
    pub fn filter_shown(model: &RowModel, ctx: &FilterCtx<'_>, state: &TaskViewState) -> Option<Vec<bool>> {
        let filter = match &state.filter {
            FilterChoice::Preset(preset) => TaskFilter::from_preset(preset),
            FilterChoice::Custom(filter) => filter.clone(),
        };
        (!filter.is_empty() && ctx.project.tasks.len() == model.len()).then(|| filter.hits(ctx).shown)
    }

    /// Comme [`RowView::build`], avec les lignes retenues par le filtre
    /// déjà calculées (`None` : toutes ; J8 lot L9, H-UI-30 : lignes
    /// gardées par [`FilterMemo`] ajoutées par l'appelant).
    #[must_use]
    pub fn build_with_shown(
        model: &RowModel,
        ctx: &FilterCtx<'_>,
        state: &TaskViewState,
        collapsed: &BTreeSet<TaskUid>,
        shown: Option<&[bool]>,
    ) -> Self {
        let len = model.len();
        let shown = shown.unwrap_or(&[]);
        let rows: Vec<ViewRow> = (0..len)
            .filter_map(|i| model.row(i).map(|row| (i, row)))
            .map(|(i, row)| ViewRow {
                uid: row.uid,
                level: row.level,
                summary: row.kind == RowKind::Summary,
                shown: shown.get(i).copied().unwrap_or(true),
            })
            .collect();
        let keys = state
            .sort
            .filter(|spec| !is_id_order(*spec))
            .map(|spec| sort_keys(model, ctx, spec.key));
        let sort = state.sort.zip(keys.as_deref()).map(|(spec, keys)| SortInput {
            keys,
            descending: spec.descending,
            keep_outline: spec.keep_outline,
        });
        let links: Vec<(u32, u32)> = model.links().iter().map(|l| (l.from, l.to)).collect();
        Self::compose(&ViewInput {
            rows: &rows,
            sort,
            outline_level: state.outline_level,
            collapsed,
            links: &links,
        })
    }

    /// Vue sans filtre, tri ni repli d'un `RowModel` : identité, liens du
    /// modèle compris (même résultat que [`RowView::build`] sans état de
    /// vue).
    #[must_use]
    pub fn unfiltered(model: &RowModel) -> Self {
        let rows: Vec<ViewRow> = (0..model.len())
            .filter_map(|i| model.row(i))
            .map(|row| ViewRow {
                uid: row.uid,
                level: row.level,
                summary: row.kind == RowKind::Summary,
                shown: true,
            })
            .collect();
        let links: Vec<(u32, u32)> = model.links().iter().map(|l| (l.from, l.to)).collect();
        Self::compose(&ViewInput {
            rows: &rows,
            sort: None,
            outline_level: None,
            collapsed: &BTreeSet::new(),
            links: &links,
        })
    }

    /// Cœur pur de la construction : filtre (`ViewRow::shown`), repli,
    /// tri, liens renumérotés (P51, P53).
    #[must_use]
    pub fn compose(input: &ViewInput<'_>) -> Self {
        let n = input.rows.len();
        if u32::try_from(n).map_or(true, |n| n == HIDDEN) {
            return Self::default();
        }
        let visible = visible_positions(input);
        let order: Vec<usize> = match input.sort.filter(|s| s.keys.len() == n) {
            Some(sort) if sort.keep_outline => sort_keep_outline(input.rows, &visible, &sort),
            Some(sort) => {
                let mut order = visible;
                order.sort_by(|&a, &b| compare(&sort, a, b));
                order
            }
            None => visible,
        };
        let mut display_of = vec![HIDDEN; n];
        let mut model_order = Vec::with_capacity(order.len());
        for (d, &m) in order.iter().enumerate() {
            let (Ok(d), Ok(m32)) = (u32::try_from(d), u32::try_from(m)) else {
                continue;
            };
            if let Some(slot) = display_of.get_mut(m) {
                *slot = d;
                model_order.push(ModelRow(m32));
            }
        }
        let display = |m: u32| {
            usize::try_from(m)
                .ok()
                .and_then(|i| display_of.get(i))
                .copied()
                .filter(|&d| d != HIDDEN)
                .map(DisplayRow)
        };
        let mut links: Vec<DisplayLink> = input
            .links
            .iter()
            .enumerate()
            .filter_map(|(k, &(from, to))| {
                let model_link = u32::try_from(k).ok()?;
                Some(DisplayLink {
                    from: display(from)?,
                    to: display(to)?,
                    model_link,
                })
            })
            .collect();
        links.sort_by_key(|link| (link.from.min(link.to), link.model_link));
        let max_span = links
            .iter()
            .map(|link| link.from.0.abs_diff(link.to.0))
            .max()
            .unwrap_or(0);
        Self {
            order: model_order,
            display_of,
            links,
            max_span,
        }
    }

    /// Nombre de lignes affichées.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Aucune ligne affichée.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Nombre de lignes du modèle couvertes par la vue.
    #[must_use]
    pub fn model_len(&self) -> usize {
        self.display_of.len()
    }

    /// Vrai si la vue est l'identité (toutes les lignes, ordre des ID).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        self.order.len() == self.display_of.len()
            && self
                .order
                .iter()
                .enumerate()
                .all(|(d, m)| u32::try_from(d).is_ok_and(|d| d == m.0))
    }

    /// Ligne du modèle d'une ligne affichée.
    #[must_use]
    pub fn model_row(&self, d: DisplayRow) -> Option<ModelRow> {
        usize::try_from(d.0).ok().and_then(|i| self.order.get(i)).copied()
    }

    /// Ligne affichée d'une ligne du modèle (`None` : masquée ou hors vue).
    #[must_use]
    pub fn display_row(&self, m: ModelRow) -> Option<DisplayRow> {
        usize::try_from(m.0)
            .ok()
            .and_then(|i| self.display_of.get(i))
            .copied()
            .filter(|&d| d != HIDDEN)
            .map(DisplayRow)
    }

    /// Lignes du modèle dans l'ordre d'affichage.
    #[must_use]
    pub fn order(&self) -> &[ModelRow] {
        &self.order
    }

    /// Liens visibles.
    #[must_use]
    pub fn links(&self) -> &[DisplayLink] {
        &self.links
    }

    /// Liens visibles dont l'intervalle de lignes affichées `[min, max]`
    /// coupe `rows` (même balayage que `RowModel::links_touching`, sur les
    /// lignes affichées).
    pub fn links_touching(&self, rows: Range<usize>) -> impl Iterator<Item = &DisplayLink> + '_ {
        let span = usize::try_from(self.max_span).unwrap_or(usize::MAX);
        let skip = self
            .links
            .partition_point(|link| link.low().saturating_add(span) < rows.start);
        let tail = self.links.get(skip..).unwrap_or(&[]);
        tail.iter()
            .take_while(move |link| link.low() < rows.end)
            .filter(move |link| link.high() >= rows.start)
    }
}

/// Tri par ID croissant : l'ordre du modèle, sans clé à calculer.
fn is_id_order(spec: SortSpec) -> bool {
    spec.key == SortKey::Id && !spec.descending
}

/// Clé de tri de chaque ligne du modèle, lue dans le `RowModel` et le
/// planning du moteur ; ligne vide : aucune valeur.
fn sort_keys(model: &RowModel, ctx: &FilterCtx<'_>, key: SortKey) -> Vec<Option<SortValue>> {
    (0..model.len())
        .map(|i| {
            let row = model.row(i)?;
            if row.kind == RowKind::Null {
                return None;
            }
            sort_value(model, ctx, i, row, key)
        })
        .collect()
}

/// Clé de tri d'une ligne non vide.
fn sort_value(model: &RowModel, ctx: &FilterCtx<'_>, i: usize, row: &Row, key: SortKey) -> Option<SortValue> {
    let scheduled = || match ctx.schedule.tasks.get(&row.uid) {
        Some(TaskOutcome::Scheduled(s)) => Some(s),
        _ => None,
    };
    match key {
        SortKey::Id => Some(SortValue::Id(row.id)),
        SortKey::Name => {
            let name = model.text(i, Column::Name);
            Some(SortValue::Text(fold_text(name), name.to_owned()))
        }
        SortKey::Start => row.bar.map(|(start, _)| SortValue::Moment(start)),
        SortKey::Finish => row.bar.map(|(_, finish)| SortValue::Moment(finish)),
        SortKey::Duration => scheduled().map(|s| SortValue::Minutes(s.duration)),
        SortKey::PercentComplete => Some(SortValue::Percent(row.percent)),
        SortKey::TotalSlack => scheduled().map(|s| SortValue::Minutes(s.total_slack)),
    }
}

/// Positions retenues par le filtre et non repliées, dans l'ordre du
/// modèle. Une ligne est repliée si son niveau dépasse le niveau affiché,
/// ou si elle suit une récapitulative repliée de niveau inférieur sans
/// qu'une ligne de niveau inférieur ou égal ne les sépare.
fn visible_positions(input: &ViewInput<'_>) -> Vec<usize> {
    let max_level = input.outline_level.map(u16::from);
    let mut hide_below: Option<u16> = None;
    let mut out = Vec::with_capacity(input.rows.len());
    for (i, row) in input.rows.iter().enumerate() {
        if hide_below.is_some_and(|l| row.level > l) {
            continue;
        }
        hide_below = None;
        if row.summary && input.collapsed.contains(&row.uid) {
            hide_below = Some(row.level);
        }
        if max_level.is_some_and(|max| row.level > max) || !row.shown {
            continue;
        }
        out.push(i);
    }
    out
}

/// Comparaison de deux positions selon le tri : valeur absente en fin de
/// liste, sens du tri sur les valeurs, ordre des ID à égalité.
fn compare(sort: &SortInput<'_>, left: usize, right: usize) -> Ordering {
    let left_key = sort.keys.get(left).and_then(Option::as_ref);
    let right_key = sort.keys.get(right).and_then(Option::as_ref);
    let by_value = match (left_key, right_key) {
        (Some(l), Some(r)) => {
            let order = l.cmp(r);
            if sort.descending { order.reverse() } else { order }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    };
    by_value.then(left.cmp(&right))
}

/// Tri des frères : chaque ligne visible a pour parent la plus proche
/// ligne visible qui la précède avec un niveau inférieur (racine sinon) ;
/// les enfants de chaque parent sont triés, puis l'arbre est parcouru en
/// profondeur (pile explicite : aucune récursion), chaque enfant après son
/// parent.
fn sort_keep_outline(rows: &[ViewRow], visible: &[usize], sort: &SortInput<'_>) -> Vec<usize> {
    // `children[k]` : enfants de la k-ième ligne visible ; `roots` : sans parent.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); visible.len()];
    let mut roots: Vec<usize> = Vec::new();
    let mut stack: Vec<(usize, u16)> = Vec::new();
    for (k, &pos) in visible.iter().enumerate() {
        let level = rows.get(pos).map_or(1, |r| r.level);
        while stack.last().is_some_and(|&(_, l)| l >= level) {
            stack.pop();
        }
        match stack.last().and_then(|&(parent, _)| children.get_mut(parent)) {
            Some(siblings) => siblings.push(k),
            None => roots.push(k),
        }
        stack.push((k, level));
    }
    let position = |k: usize| visible.get(k).copied().unwrap_or(usize::MAX);
    let by_key = |a: &usize, b: &usize| compare(sort, position(*a), position(*b));
    roots.sort_by(by_key);
    for siblings in &mut children {
        siblings.sort_by(by_key);
    }
    let mut out = Vec::with_capacity(visible.len());
    let mut todo: Vec<usize> = roots.into_iter().rev().collect();
    while let Some(k) = todo.pop() {
        out.push(position(k));
        if let Some(siblings) = children.get(k) {
            todo.extend(siblings.iter().rev().copied());
        }
    }
    out
}

/// Mémoire du filtre appliqué (J8 lot L9, H-UI-30) : un filtre est une
/// vue, appliquée quand on le choisit (ou le réapplique) ; une ligne éditée
/// qui ne le satisfait plus reste visible jusqu'à sa réapplication, et une
/// tâche créée sous le filtre aussi (hypothèse : sinon l'insertion sous un
/// filtre ferait disparaître la ligne créée). Tenue par la vue transitoire
/// d'`App`, jamais persistée.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterMemo {
    /// Application du filtre à la dernière construction : filtre, numéro
    /// d'ouverture et génération du document, version de réapplication ;
    /// `None` sans filtre.
    applied: Option<(FilterChoice, u64, u64, u64)>,
    /// Tâches montrées par le filtre (gardées comprises) à la dernière
    /// construction.
    shown: BTreeSet<TaskUid>,
    /// Tâches du document à la dernière construction.
    known: BTreeSet<TaskUid>,
    /// Tâches gardées visibles malgré le filtre jusqu'à sa réapplication.
    kept: BTreeSet<TaskUid>,
}

impl FilterMemo {
    /// Complète `shown` (lignes retenues par le filtre, par position du
    /// modèle ; `None` : aucun filtre, la mémoire est vidée) par les tâches
    /// gardées. Même filtre, même document et même version de
    /// réapplication qu'à la construction précédente, mais génération
    /// changée (édition, annulation, écriture d'un agent) : les tâches
    /// alors montrées et les tâches apparues depuis sont gardées ; filtre,
    /// document ou version changés : plus rien n'est gardé (filtre
    /// appliqué à neuf).
    pub fn update(
        &mut self,
        model: &RowModel,
        filter: &FilterChoice,
        document: (u64, u64),
        reapply: u64,
        shown: &mut Option<Vec<bool>>,
    ) {
        let Some(shown) = shown.as_mut() else {
            *self = Self::default();
            return;
        };
        let (opening, generation) = document;
        match &self.applied {
            Some((f, o, g, r)) if f == filter && *o == opening && *r == reapply => {
                if *g != generation {
                    let fresh: Vec<TaskUid> = (0..model.len())
                        .filter_map(|i| model.row(i))
                        .map(|row| row.uid)
                        .filter(|uid| self.shown.contains(uid) || !self.known.contains(uid))
                        .collect();
                    self.kept.extend(fresh);
                }
            }
            _ => self.kept.clear(),
        }
        self.known.clear();
        self.shown.clear();
        for (i, slot) in shown.iter_mut().enumerate() {
            let Some(row) = model.row(i) else {
                continue;
            };
            self.known.insert(row.uid);
            if self.kept.contains(&row.uid) {
                *slot = true;
            }
            if *slot {
                self.shown.insert(row.uid);
            }
        }
        self.applied = Some((filter.clone(), opening, generation, reapply));
    }

    /// Vrai si un filtre est appliqué et masque la tâche `uid` (compteur
    /// « n masquées par le filtre » de la recherche, H-UI-33).
    #[must_use]
    pub fn hides(&self, uid: TaskUid) -> bool {
        self.applied.is_some() && !self.shown.contains(&uid)
    }

    /// Nombre de tâches gardées visibles malgré le filtre.
    #[must_use]
    pub fn kept_len(&self) -> usize {
        self.kept.len()
    }
}

/// Lignes du modèle lues à travers leur vue : toute lecture d'une ligne
/// affichée par la table, le Gantt, la navigation ou la sélection passe par
/// ici ([`DisplayRow`] en entrée, ligne du modèle en sortie).
#[derive(Clone, Copy, Debug)]
pub struct VisibleRows<'a> {
    /// Lignes et textes du modèle.
    model: &'a RowModel,
    /// Vue (permutation affichée → modèle).
    view: &'a RowView,
}

impl<'a> VisibleRows<'a> {
    /// Réunit un modèle et sa vue (construite sur ce modèle : `App` les
    /// reconstruit ensemble).
    #[must_use]
    pub const fn new(model: &'a RowModel, view: &'a RowView) -> Self {
        Self { model, view }
    }

    /// Modèle de lignes (ordre des ID).
    #[must_use]
    pub const fn model(&self) -> &'a RowModel {
        self.model
    }

    /// Vue.
    #[must_use]
    pub const fn view(&self) -> &'a RowView {
        self.view
    }

    /// Nombre de lignes affichées.
    #[must_use]
    pub fn len(&self) -> usize {
        self.view.len()
    }

    /// Aucune ligne affichée.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.view.is_empty()
    }

    /// Ligne du modèle de la ligne affichée `d`.
    #[must_use]
    pub fn model_row(&self, d: DisplayRow) -> Option<ModelRow> {
        self.view.model_row(d)
    }

    /// Ligne affichée `d`.
    #[must_use]
    pub fn row(&self, d: DisplayRow) -> Option<&'a Row> {
        self.model_row(d).and_then(|m| self.model.row(m.index()))
    }

    /// UID de la tâche de la ligne affichée `d`.
    #[must_use]
    pub fn uid(&self, d: DisplayRow) -> Option<TaskUid> {
        self.row(d).map(|row| row.uid)
    }

    /// Texte pré-formaté d'une cellule de la ligne affichée `d` ; vide hors
    /// vue.
    #[must_use]
    pub fn text(&self, d: DisplayRow, c: Column) -> &'a str {
        self.model_row(d).map_or("", |m| self.model.text(m.index(), c))
    }

    /// Ligne affichée d'une tâche (`None` : absente ou masquée).
    #[must_use]
    pub fn display_of(&self, uid: TaskUid) -> Option<DisplayRow> {
        self.model
            .index_of(uid)
            .and_then(|i| self.view.display_row(ModelRow::at(i)))
    }

    /// Indice dans le modèle de la ligne affichée `d` ; la ligne vide de fin
    /// (`d` = nombre de lignes affichées) et toute ligne hors vue donnent
    /// la ligne vide du modèle (`RowModel::len()`).
    #[must_use]
    pub fn model_index_or_blank(&self, d: DisplayRow) -> usize {
        self.model_row(d).map_or(self.model.len(), ModelRow::index)
    }

    /// Liens visibles qui touchent les lignes affichées `rows`, avec le
    /// lien du modèle (type, criticité).
    pub fn links_touching(
        &self,
        rows: Range<usize>,
    ) -> impl Iterator<Item = (DisplayLink, &'a LinkView)> + 'a {
        let model = self.model;
        self.view.links_touching(rows).filter_map(move |link| {
            let k = usize::try_from(link.model_link).ok()?;
            model.links().get(k).map(|view| (*link, view))
        })
    }
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

    use std::collections::BTreeSet;

    use plannercraft_core::TaskUid;
    use proptest::prelude::*;

    use super::*;

    /// P51 (vue identité) : `display_of` est l'inverse de `order`.
    #[test]
    fn row_view_identity_is_its_own_inverse() {
        let v = RowView::identity(4);
        assert_eq!(v.len(), 4);
        for i in 0..4 {
            assert_eq!(v.model_row(DisplayRow(i)), Some(ModelRow(i)));
            assert_eq!(v.display_row(ModelRow(i)), Some(DisplayRow(i)));
        }
        assert_eq!(v.model_row(DisplayRow(4)), None);
        assert_eq!(v.display_row(ModelRow(u32::MAX)), None);
        assert!(RowView::identity(0).is_empty());
        assert_eq!(v.links().len(), 0);
        assert!(v.is_identity());
    }

    /// Projet « P » du cahier J8 (§ 2) : 1 ⊃ 2, 3, 4 ; 5 ⊃ 6, 7.
    fn p_rows() -> Vec<ViewRow> {
        [
            (1, 1, true),
            (2, 2, false),
            (3, 2, false),
            (4, 2, false),
            (5, 1, true),
            (6, 2, false),
            (7, 2, false),
        ]
        .iter()
        .map(|&(uid, level, summary)| ViewRow {
            uid: TaskUid::new(uid),
            level,
            summary,
            shown: true,
        })
        .collect()
    }

    /// Liens de P en lignes du modèle : 2→3, 3→4, 3→6, 6→7.
    const P_LINKS: [(u32, u32); 4] = [(1, 2), (2, 3), (2, 5), (5, 6)];

    fn uids(view: &RowView, rows: &[ViewRow]) -> Vec<u32> {
        view.order().iter().map(|m| rows[m.index()].uid.get()).collect()
    }

    fn input<'a>(rows: &'a [ViewRow], collapsed: &'a BTreeSet<TaskUid>) -> ViewInput<'a> {
        ViewInput {
            rows,
            sort: None,
            outline_level: None,
            collapsed,
            links: &P_LINKS,
        }
    }

    #[test]
    fn compose_without_view_state_is_identity_with_model_links() {
        let rows = p_rows();
        let none = BTreeSet::new();
        let v = RowView::compose(&input(&rows, &none));
        assert!(v.is_identity());
        let links: Vec<(u32, u32, u32)> = v
            .links()
            .iter()
            .map(|l| (l.from.0, l.to.0, l.model_link))
            .collect();
        assert_eq!(links, [(1, 2, 0), (2, 3, 1), (2, 5, 2), (5, 6, 3)]);
    }

    /// H-UI-34 (préparé) : niveau 1 sur P ⇒ lignes 1 et 5 seules.
    #[test]
    fn outline_level_one_shows_phases_only() {
        let rows = p_rows();
        let none = BTreeSet::new();
        let v = RowView::compose(&ViewInput {
            outline_level: Some(1),
            ..input(&rows, &none)
        });
        assert_eq!(uids(&v, &rows), [1, 5]);
        assert_eq!(v.links(), &[]);
        assert_eq!(v.display_row(ModelRow(1)), None);
    }

    #[test]
    fn collapsed_summary_hides_its_block_only() {
        let rows = p_rows();
        let collapsed = BTreeSet::from([TaskUid::new(1)]);
        let v = RowView::compose(&input(&rows, &collapsed));
        assert_eq!(uids(&v, &rows), [1, 5, 6, 7]);
        // Seul 6→7 reste, renuméroté (lignes affichées 2 → 3).
        let links: Vec<(u32, u32, u32)> = v
            .links()
            .iter()
            .map(|l| (l.from.0, l.to.0, l.model_link))
            .collect();
        assert_eq!(links, [(2, 3, 3)]);
        // Une feuille « repliée » ne masque rien.
        let leaf = BTreeSet::from([TaskUid::new(2)]);
        assert!(RowView::compose(&input(&rows, &leaf)).is_identity());
    }

    /// H-UI-31 (préparé) : tri par Début décroissant avec structure ⇒ les
    /// enfants restent sous leur parent, triés entre frères.
    #[test]
    fn sort_keep_outline_sorts_siblings() {
        let rows = p_rows();
        let none = BTreeSet::new();
        // Clés : la ligne k vaut k (Début croissant avec l'ID).
        let keys: Vec<Option<SortValue>> = (0..7).map(|k| Some(SortValue::Id(k))).collect();
        let sort = SortInput {
            keys: &keys,
            descending: true,
            keep_outline: true,
        };
        let v = RowView::compose(&ViewInput {
            sort: Some(sort),
            ..input(&rows, &none)
        });
        assert_eq!(uids(&v, &rows), [5, 7, 6, 1, 4, 3, 2]);
        let global = RowView::compose(&ViewInput {
            sort: Some(SortInput {
                keep_outline: false,
                ..sort
            }),
            ..input(&rows, &none)
        });
        assert_eq!(uids(&global, &rows), [7, 6, 5, 4, 3, 2, 1]);
        // Liens triés par plus petite ligne affichée.
        assert!(global.links().windows(2).all(|w| w[0].low() <= w[1].low()));
    }

    #[test]
    fn missing_sort_values_go_last_in_both_directions() {
        let rows = p_rows();
        let none = BTreeSet::new();
        let keys: Vec<Option<SortValue>> = (0..7).map(|k| (k != 2).then_some(SortValue::Id(k))).collect();
        for descending in [false, true] {
            let v = RowView::compose(&ViewInput {
                sort: Some(SortInput {
                    keys: &keys,
                    descending,
                    keep_outline: false,
                }),
                ..input(&rows, &none)
            });
            assert_eq!(uids(&v, &rows).last(), Some(&3), "{descending}");
        }
    }

    #[test]
    fn display_row_conversions_are_bounded() {
        assert_eq!(DisplayRow::at(usize::MAX), DisplayRow(HIDDEN));
        assert_eq!(DisplayRow(7).index(), 7);
        assert_eq!(ModelRow::at(3).index(), 3);
        let v = RowView::identity(2);
        assert_eq!(v.model_row(DisplayRow::at(usize::MAX)), None);
        assert_eq!(v.links_touching(0..usize::MAX).count(), 0);
    }

    /// Lignes générées : niveaux d'un plan valide (le premier à 1, chaque
    /// niveau au plus un de plus que le précédent), récapitulative si la
    /// ligne suivante est plus profonde.
    fn arb_case() -> impl Strategy<Value = Case> {
        (1usize..40)
            .prop_flat_map(|n| {
                (
                    proptest::collection::vec(0u16..3, n),
                    proptest::collection::vec(any::<bool>(), n),
                    proptest::collection::vec(any::<bool>(), n),
                    proptest::collection::vec(proptest::option::of(0u32..6), n),
                    proptest::collection::vec((0..n, 0..n), 0..(2 * n)),
                    (
                        any::<bool>(),
                        any::<bool>(),
                        proptest::option::of(1u8..4),
                        any::<bool>(),
                    ),
                )
            })
            .prop_map(
                |(steps, shown, folded, keys, links, (desc, keep, level, sorted))| {
                    let mut levels: Vec<u16> = Vec::with_capacity(steps.len());
                    for step in &steps {
                        let level = match levels.last() {
                            None => 1,
                            Some(&prev) => match step {
                                0 => prev + 1,
                                1 => prev,
                                _ => (prev.saturating_sub(1)).max(1),
                            },
                        };
                        levels.push(level);
                    }
                    let rows: Vec<ViewRow> = levels
                        .iter()
                        .enumerate()
                        .map(|(i, &level)| ViewRow {
                            uid: TaskUid::new(u32::try_from(i + 1).unwrap()),
                            level,
                            summary: levels.get(i + 1).is_some_and(|&next| next > level),
                            shown: shown[i],
                        })
                        .collect();
                    let collapsed = rows
                        .iter()
                        .zip(&folded)
                        .filter(|(_, f)| **f)
                        .map(|(r, _)| r.uid)
                        .collect();
                    Case {
                        rows,
                        collapsed,
                        keys: keys.into_iter().map(|k| k.map(SortValue::Id)).collect(),
                        links: links
                            .into_iter()
                            .map(|(a, b)| (u32::try_from(a).unwrap(), u32::try_from(b).unwrap()))
                            .collect(),
                        descending: desc,
                        keep_outline: keep,
                        outline_level: level,
                        sorted,
                    }
                },
            )
    }

    #[derive(Debug, Clone)]
    struct Case {
        rows: Vec<ViewRow>,
        collapsed: BTreeSet<TaskUid>,
        keys: Vec<Option<SortValue>>,
        links: Vec<(u32, u32)>,
        descending: bool,
        keep_outline: bool,
        outline_level: Option<u8>,
        sorted: bool,
    }

    impl Case {
        fn view(&self) -> RowView {
            RowView::compose(&ViewInput {
                rows: &self.rows,
                sort: self.sorted.then_some(SortInput {
                    keys: &self.keys,
                    descending: self.descending,
                    keep_outline: self.keep_outline,
                }),
                outline_level: self.outline_level,
                collapsed: &self.collapsed,
                links: &self.links,
            })
        }
    }

    /// Plus proche ancêtre (niveau inférieur qui précède) de la position `i`.
    fn parent(rows: &[ViewRow], i: usize) -> Option<usize> {
        let level = rows[i].level;
        (0..i).rev().find(|&j| rows[j].level < level)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(4_096))]

        /// P51 : `order` est une permutation d'un sous-ensemble de `0..len`,
        /// `display_of` son inverse ; sans filtre, tri ni repli ⇒ identité ;
        /// tri avec structure ⇒ chaque enfant après son parent ; liens
        /// visibles = liens du modèle dont les deux bouts sont affichés,
        /// triés par plus petite ligne affichée.
        #[test]
        fn p51_row_view_is_a_partial_permutation(case in arb_case()) {
            let v = case.view();
            let n = case.rows.len();
            prop_assert_eq!(v.model_len(), n);
            let mut seen = BTreeSet::new();
            for (d, m) in v.order().iter().enumerate() {
                prop_assert!(m.index() < n);
                prop_assert!(seen.insert(*m), "doublon {:?}", m);
                prop_assert_eq!(v.display_row(*m), Some(DisplayRow::at(d)));
                prop_assert_eq!(v.model_row(DisplayRow::at(d)), Some(*m));
            }
            for i in 0..n {
                let m = ModelRow::at(i);
                prop_assert_eq!(v.display_row(m).is_some(), seen.contains(&m));
            }
            // Masquées : jamais un ancêtre replié visible au-dessus d'elles
            // sans qu'elles soient masquées ; filtrées absentes.
            for m in &seen {
                prop_assert!(case.rows[m.index()].shown);
            }
            if case.sorted && case.keep_outline {
                for (d, m) in v.order().iter().enumerate() {
                    let mut p = parent(&case.rows, m.index());
                    while let Some(j) = p {
                        if let Some(pd) = v.display_row(ModelRow::at(j)) {
                            prop_assert!(pd.index() < d, "enfant {} avant son parent {}", m.index(), j);
                            break;
                        }
                        p = parent(&case.rows, j);
                    }
                }
            }
            let expected: usize = case.links.iter().filter(|&&(a, b)| {
                v.display_row(ModelRow(a)).is_some() && v.display_row(ModelRow(b)).is_some()
            }).count();
            prop_assert_eq!(v.links().len(), expected);
            prop_assert!(v.links().windows(2).all(|w| (w[0].low(), w[0].model_link) <= (w[1].low(), w[1].model_link)));
            // Fenêtres : même résultat que le filtre brut.
            for start in 0..=v.len() {
                let range = start..start + 3;
                let got: Vec<_> = v.links_touching(range.clone()).copied().collect();
                let brute: Vec<_> = v.links().iter()
                    .filter(|l| l.low() < range.end && l.high() >= range.start)
                    .copied().collect();
                prop_assert_eq!(got, brute);
            }
            // Identité sans état de vue.
            let all_shown: Vec<ViewRow> = case.rows.iter().map(|r| ViewRow { shown: true, ..*r }).collect();
            let none = BTreeSet::new();
            let id = RowView::compose(&ViewInput {
                rows: &all_shown, sort: None, outline_level: None, collapsed: &none, links: &case.links,
            });
            prop_assert!(id.is_identity());
            // Tri par ID croissant avec structure ⇒ identité aussi.
            let ids: Vec<Option<SortValue>> = (0..n).map(|k| Some(SortValue::Id(u32::try_from(k).unwrap()))).collect();
            for keep_outline in [false, true] {
                let by_id = RowView::compose(&ViewInput {
                    rows: &all_shown,
                    sort: Some(SortInput { keys: &ids, descending: false, keep_outline }),
                    outline_level: None, collapsed: &none, links: &case.links,
                });
                prop_assert!(by_id.is_identity());
            }
        }

        /// P53 : replier puis déplier une récapitulative est idempotent ;
        /// niveau n puis Tous ⇒ vue initiale.
        #[test]
        fn p53_fold_unfold_is_idempotent(case in arb_case(), pick in any::<prop::sample::Index>()) {
            let initial = case.view();
            let summaries: Vec<TaskUid> = case.rows.iter().filter(|r| r.summary).map(|r| r.uid).collect();
            if let Some(&uid) = (!summaries.is_empty()).then(|| pick.get(&summaries)) {
                let mut folded = case.clone();
                let was = folded.collapsed.contains(&uid);
                if was { folded.collapsed.remove(&uid); } else { folded.collapsed.insert(uid); }
                let toggled = folded.view();
                // Replier deux fois de suite : même vue.
                prop_assert_eq!(folded.view(), toggled);
                if was { folded.collapsed.insert(uid); } else { folded.collapsed.remove(&uid); }
                prop_assert_eq!(folded.view(), initial.clone());
            }
            let mut levelled = case.clone();
            levelled.outline_level = Some(1);
            let _ = levelled.view();
            levelled.outline_level = case.outline_level;
            prop_assert_eq!(levelled.view(), initial);
        }
    }
}
