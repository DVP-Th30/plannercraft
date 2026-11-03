//! Commandes de structure sur la sélection (lot L14) : insérer, supprimer,
//! indenter, désindenter, lier, délier ; monter et descendre (J7 lot L1,
//! H-UI-29).
//!
//! [`plan`] traduit une [`StructureOp`] et la [`Selection`] en **un** lot de
//! commandes du registre (une transaction, une entrée d'historique) ; il ne
//! calcule rien du planning (règle 3) et ne lit que l'ordre et les niveaux
//! hiérarchiques du modèle. Le registre reste seul juge : un doublon, un
//! cycle ou une hiérarchie refusée font refuser tout le lot (R-153, R-156,
//! R-160), refus affiché dans le bandeau.
//!
//! Ruban (ADR-0017, H-UI-83, lot L3) : Insérer → Jalon et Récapitulative
//! sont aussi des lots ordinaires (`task.add`, puis `task.indent` des
//! tâches sélectionnées pour la récapitulative) : une entrée d'annulation.
//!
//! Hiérarchie (R-052) : une tâche est récapitulative si la tâche suivante
//! est plus profonde ; son bloc est la suite des tâches plus profondes qui la
//! suivent (lignes vides comprises, comme `task.delete`). Une tâche dont un
//! ancêtre est aussi sélectionné est retirée du lot de suppression,
//! d'indentation et de désindentation : elle suit son bloc (comme dans MS
//! Project, chaque ligne sélectionnée ne bouge que d'un niveau).

use std::collections::BTreeSet;

use plannercraft_automation::command::{LinkKey, LinkSpec, TaskAdd, TaskDelete, TaskMove, TaskOne};
use plannercraft_automation::json::DurationJson;
use plannercraft_automation::{Batch, Command, Placement, TaskRef};
use plannercraft_core::{Project, TaskUid};

use crate::action::Action;
use crate::app::{App, EditOrigin};
use crate::i18n::{Arg, TextId, Texts};
use crate::selection::Selection;

/// Commande de structure, appliquée à la sélection courante.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureOp {
    /// Inser : nouvelle tâche juste avant la première tâche sélectionnée
    /// (ordre des ID), à la fin sans sélection.
    InsertBefore,
    /// Suppr : supprime les tâches sélectionnées ; une récapitulative est
    /// supprimée avec ses sous-tâches (`cascade`) après confirmation.
    Delete,
    /// Alt+Maj+→ : indente les tâches sélectionnées, dans l'ordre des ID.
    Indent,
    /// Alt+Maj+← : désindente les tâches sélectionnées, dans l'ordre des ID.
    Outdent,
    /// Ctrl+F2 : liens FS en chaîne (ordre des clics, ou des ID pour une
    /// plage).
    Link,
    /// Ctrl+Maj+F2 : retire tous les liens entre tâches sélectionnées.
    Unlink,
    /// Alt+Maj+↑ (J7, H-UI-29) : monte les lignes sélectionnées au-dessus
    /// de la tâche sœur précédente (`task.move`).
    MoveUp,
    /// Alt+Maj+↓ (J7, H-UI-29) : descend les lignes sélectionnées sous la
    /// tâche sœur suivante (`task.move`).
    MoveDown,
    /// Ruban, Tâche → Insérer → Jalon (ADR-0017, H-UI-83, lot L3) :
    /// `task.add { milestone: true }` avant la première tâche sélectionnée.
    InsertMilestone,
    /// Ruban, Tâche → Insérer → Récapitulative (H-UI-83, lot L3) :
    /// `task.add` puis `task.indent` des tâches sélectionnées, un lot.
    InsertSummary,
}

/// Lot préparé par [`plan`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructurePlan {
    /// Lot à appliquer.
    pub batch: Batch,
    /// Nombre de tâches supprimées, sous-tâches comprises, si le lot
    /// supprime au moins une récapitulative : l'interface demande d'abord
    /// confirmation (ADR-0012 § 0 : `cascade:true` obligatoire).
    pub confirm_cascade: Option<usize>,
}

/// Suppression en attente de confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingDelete {
    /// Lot de suppression (avec `cascade`).
    pub batch: Batch,
    /// Nombre de tâches supprimées, sous-tâches comprises.
    pub count: usize,
    /// Clé de cache du document au moment de la demande : une édition, une
    /// annulation ou un autre document entre-temps rendent la demande
    /// caduque.
    pub cache_key: (u64, u64),
}

/// Lot à une suite d'opérations.
fn batch(ops: Vec<Command>) -> Batch {
    Batch {
        format_version: Batch::FORMAT_VERSION,
        lang: None,
        ops,
        label: None,
    }
}

/// Niveau de la tâche à la position `pos`.
fn level_at(project: &Project, pos: usize) -> Option<u16> {
    project.tasks.at(pos).map(|t| t.outline_level.get())
}

/// Vrai si la tâche à `pos` est récapitulative (R-052 : la tâche non vide
/// suivante est plus profonde, lignes vides intercalées ignorées). Même
/// règle que l'automatisation et le moteur.
fn is_summary_at(project: &Project, pos: usize) -> bool {
    plannercraft_automation::command::is_summary_at(&project.tasks, pos)
}

/// Fin (exclue) du bloc de la tâche à `pos` : tâches suivantes plus
/// profondes, lignes vides comprises (même parcours que `task.delete`).
fn block_end(project: &Project, pos: usize) -> usize {
    let Some(level) = level_at(project, pos) else {
        return pos;
    };
    let start = pos.saturating_add(1);
    let mut end = start;
    for (q, task) in project.tasks.iter().enumerate().skip(start) {
        if task.is_null {
            continue;
        }
        if task.outline_level.get() <= level {
            break;
        }
        end = q.saturating_add(1);
    }
    end
}

/// Positions des tâches sélectionnées existantes, dans l'ordre des ID.
fn positions(project: &Project, selection: &Selection) -> Vec<(usize, TaskUid)> {
    let mut out: Vec<(usize, TaskUid)> = selection
        .uids()
        .iter()
        .filter_map(|uid| project.tasks.position(*uid).map(|pos| (pos, *uid)))
        .collect();
    out.sort_unstable();
    out
}

/// Tâches sélectionnées sans ancêtre sélectionné, dans l'ordre des ID, avec
/// leur position.
fn top_level(project: &Project, selection: &Selection) -> Vec<(usize, TaskUid)> {
    let mut out = Vec::new();
    let mut covered_until = 0_usize;
    for (pos, uid) in positions(project, selection) {
        if pos < covered_until {
            continue;
        }
        covered_until = block_end(project, pos);
        out.push((pos, uid));
    }
    out
}

/// Refus d'une commande de structure préparée par [`plan_named`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureRefusal {
    /// Insérer → Récapitulative sur des tâches qui ne sont pas des sœurs
    /// voisines (H-UI-83) : rien n'est écrit. Aussi le déplacement de
    /// lignes non sœurs voisines (H-UI-94, ADR-0018, lot L3).
    NotAdjacent,
    /// Déplacement de lignes lâché dans leur propre bloc (H-UI-94).
    InsideOwnBlock,
    /// Déplacement de lignes sous un tri ou un filtre (H-UI-32, H-UI-94).
    ViewReorderLocked,
}

impl StructureRefusal {
    /// Message du refus.
    #[must_use]
    pub const fn text_id(self) -> TextId {
        match self {
            Self::NotAdjacent => TextId::InsertSummaryNotAdjacent,
            Self::InsideOwnBlock => TextId::RowsMoveInsideBlock,
            Self::ViewReorderLocked => TextId::RowsMoveLocked,
        }
    }

    /// Message du refus d'un déplacement de lignes par l'ID (H-UI-94,
    /// info-bulle du curseur interdit et bandeau).
    #[must_use]
    pub const fn move_text_id(self) -> TextId {
        match self {
            Self::NotAdjacent => TextId::RowsMoveNotAdjacent,
            Self::InsideOwnBlock => TextId::RowsMoveInsideBlock,
            Self::ViewReorderLocked => TextId::RowsMoveLocked,
        }
    }
}

/// Info-bulle de l'ID d'une ligne sélectionnée en lignes entières, qui se
/// glisse pour déplacer les lignes (H-UI-94, H-UI-103 ; lot L3).
pub const ROW_MOVE_TIP: TextId = TextId::TipRowMove;

/// Étiquette de la récapitulative créée dans le lot (R-150).
const SUMMARY_LABEL: &str = "$s";

/// Prépare le lot de `op` sur `selection` ; `None` s'il n'y a rien à faire
/// (sélection vide, un seul maillon à lier, aucun lien entre les tâches
/// sélectionnées) ou si la commande est refusée. Les tâches créées par
/// Insérer → Jalon et Récapitulative reçoivent un nom vide : voir
/// [`plan_named`].
#[must_use]
pub fn plan(op: StructureOp, project: &Project, selection: &Selection) -> Option<StructurePlan> {
    plan_named(op, project, selection, "").ok().flatten()
}

/// Comme [`plan`] ; `name` : nom de la tâche créée par Insérer → Jalon ou
/// Récapitulative (texte traduit de l'interface, H-UI-83).
///
/// # Errors
///
/// [`StructureRefusal`] : Récapitulative sur une sélection non voisine.
pub fn plan_named(
    op: StructureOp,
    project: &Project,
    selection: &Selection,
    name: &str,
) -> Result<Option<StructurePlan>, StructureRefusal> {
    let ops = match op {
        StructureOp::InsertBefore => {
            let at = positions(project, selection)
                .first()
                .map_or(Placement::End, |(_, uid)| Placement::Before(TaskRef::Uid(*uid)));
            vec![Command::TaskAdd(TaskAdd {
                name: String::new(),
                label: None,
                at: Some(at),
                duration: None,
                milestone: None,
                notes: None,
                start: None,
                finish: None,
            })]
        }
        StructureOp::Delete => return Ok(plan_delete(project, selection)),
        StructureOp::InsertMilestone => vec![plan_milestone(project, selection, name)],
        StructureOp::InsertSummary => plan_summary(project, selection, name)?,
        StructureOp::MoveUp | StructureOp::MoveDown => {
            plan_move(project, selection, op == StructureOp::MoveUp)
                .into_iter()
                .collect()
        }
        StructureOp::Indent | StructureOp::Outdent => top_level(project, selection)
            .into_iter()
            .map(|(_, uid)| {
                let one = TaskOne {
                    uid: TaskRef::Uid(uid),
                };
                if op == StructureOp::Indent {
                    Command::TaskIndent(one)
                } else {
                    Command::TaskOutdent(one)
                }
            })
            .collect(),
        StructureOp::Link => {
            let chain: Vec<TaskUid> = selection
                .click_order()
                .iter()
                .copied()
                .filter(|uid| project.tasks.contains(*uid))
                .collect();
            // R-260 b : seules les paires sans lien, dans un sens ou dans
            // l'autre (ni déjà liées dans le projet, ni deux fois dans la
            // chaîne) ; tout lié ⇒ aucun lot.
            let mut seen: BTreeSet<(TaskUid, TaskUid)> = BTreeSet::new();
            chain
                .windows(2)
                .filter(|pair| match pair {
                    [pred, succ] => {
                        let key = if pred <= succ {
                            (*pred, *succ)
                        } else {
                            (*succ, *pred)
                        };
                        pred != succ && !linked_either_way(project, *pred, *succ) && seen.insert(key)
                    }
                    _ => false,
                })
                .filter_map(|pair| match pair {
                    [pred, succ] => Some(Command::LinkAdd(LinkSpec {
                        pred: TaskRef::Uid(*pred),
                        succ: TaskRef::Uid(*succ),
                        kind: Some(plannercraft_automation::command::LinkTypeJson::Fs),
                        lag: None,
                    })),
                    _ => None,
                })
                .collect()
        }
        StructureOp::Unlink => positions(project, selection)
            .into_iter()
            .filter_map(|(pos, succ)| project.tasks.at(pos).map(|task| (task, succ)))
            .flat_map(|(task, succ)| {
                task.predecessors
                    .iter()
                    .filter(|link| selection.contains(link.predecessor))
                    .map(move |link| {
                        Command::LinkRemove(LinkKey {
                            pred: TaskRef::Uid(link.predecessor),
                            succ: TaskRef::Uid(succ),
                        })
                    })
            })
            .collect(),
    };
    Ok((!ops.is_empty()).then(|| StructurePlan {
        batch: batch(ops),
        confirm_cascade: None,
    }))
}

/// Vrai si `a` et `b` sont liées dans un sens ou dans l'autre (R-260 b).
fn linked_either_way(project: &Project, a: TaskUid, b: TaskUid) -> bool {
    let has = |succ: TaskUid, pred: TaskUid| {
        project
            .tasks
            .get(succ)
            .is_some_and(|t| t.predecessors.iter().any(|l| l.predecessor == pred))
    };
    has(b, a) || has(a, b)
}

/// Placement d'une tâche insérée : avant la première tâche sélectionnée
/// (ordre des ID), à la fin sans sélection.
fn insert_at(project: &Project, selection: &Selection) -> Placement {
    positions(project, selection)
        .first()
        .map_or(Placement::End, |(_, uid)| Placement::Before(TaskRef::Uid(*uid)))
}

/// Insérer → Jalon (H-UI-83) : `task.add { milestone: true }` de durée
/// nulle (R-157 : un jalon explicite garderait sinon la durée par défaut
/// d'un jour), avant la première tâche sélectionnée, à la fin sans
/// sélection.
fn plan_milestone(project: &Project, selection: &Selection, name: &str) -> Command {
    Command::TaskAdd(TaskAdd {
        name: name.to_owned(),
        label: None,
        at: Some(insert_at(project, selection)),
        duration: Some(DurationJson {
            minutes: Some(0),
            ..DurationJson::default()
        }),
        milestone: Some(true),
        notes: None,
        start: None,
        finish: None,
    })
}

/// Insérer → Récapitulative (H-UI-83) : `task.add` étiqueté `$s` avant la
/// première tâche sélectionnée (au niveau de celle-ci, R-150 `Before`),
/// puis `task.indent` de chaque tâche sélectionnée sans ancêtre
/// sélectionné : elles passent sous la tâche créée, leurs blocs avec
/// elles. Les tâches retenues doivent être des sœurs voisines (même
/// niveau, blocs contigus, comme Monter/Descendre) ; sinon refus. Sans
/// sélection : récapitulative vide en fin de projet.
fn plan_summary(
    project: &Project,
    selection: &Selection,
    name: &str,
) -> Result<Vec<Command>, StructureRefusal> {
    let top = top_level(project, selection);
    if !siblings_adjacent(project, &top) {
        return Err(StructureRefusal::NotAdjacent);
    }
    let mut ops = Vec::with_capacity(top.len().saturating_add(1));
    ops.push(Command::TaskAdd(TaskAdd {
        name: name.to_owned(),
        label: Some(SUMMARY_LABEL.to_owned()),
        at: Some(insert_at(project, selection)),
        duration: None,
        milestone: None,
        notes: None,
        start: None,
        finish: None,
    }));
    ops.extend(top.into_iter().map(|(_, uid)| {
        Command::TaskIndent(TaskOne {
            uid: TaskRef::Uid(uid),
        })
    }));
    Ok(ops)
}

/// Vrai si les tâches `top` (sans ancêtre sélectionné, ordre des ID) sont
/// des sœurs consécutives : même niveau, chaque bloc commence où finit le
/// précédent (lignes vides ignorées). Liste vide ou d'un élément : vrai.
fn siblings_adjacent(project: &Project, top: &[(usize, TaskUid)]) -> bool {
    let Some((first, _)) = top.first() else {
        return true;
    };
    let level = level_at(project, *first);
    let mut expected = *first;
    for (pos, _) in top {
        if *pos != expected || level_at(project, *pos) != level {
            return false;
        }
        expected = next_non_null(project, block_end(project, *pos)).unwrap_or(usize::MAX);
    }
    true
}

/// Position de la première tâche non vide à partir de `from` (incluse).
fn next_non_null(project: &Project, from: usize) -> Option<usize> {
    project
        .tasks
        .iter()
        .enumerate()
        .skip(from)
        .find(|(_, task)| !task.is_null)
        .map(|(pos, _)| pos)
}

/// Position de la dernière tâche non vide strictement avant `before`.
fn previous_non_null(project: &Project, before: usize) -> Option<usize> {
    (0..before)
        .rev()
        .find(|pos| project.tasks.at(*pos).is_some_and(|task| !task.is_null))
}

/// Monter / descendre (H-UI-29, comme Alt+Maj+↑/↓ de MS Project, S61) :
/// les tâches sélectionnées sans ancêtre sélectionné doivent former une
/// suite de sœurs consécutives (même niveau, blocs contigus). Monter
/// déplace la sœur précédente après la dernière d'entre elles ; descendre
/// déplace la sœur suivante avant la première : **une** commande
/// `task.move`, le bloc sélectionné garde son ordre et sa hiérarchie. Pas
/// de sœur dans ce sens (premier ou dernier enfant, bord du projet) ou
/// sélection non contiguë : rien (hypothèse : au bord, rien ne bouge,
/// comme la navigation).
fn plan_move(project: &Project, selection: &Selection, up: bool) -> Option<Command> {
    let top = top_level(project, selection);
    let (first, _) = *top.first()?;
    let (last, _) = *top.last()?;
    let level = level_at(project, first)?;
    // Sœurs consécutives : chaque bloc commence où finit le précédent.
    if !siblings_adjacent(project, &top) {
        return None;
    }
    let first_uid = project.tasks.at(first)?.uid();
    let last_uid = project.tasks.at(last)?.uid();
    let (neighbour, to) = if up {
        // Sœur précédente : la tâche de même niveau qui précède, sans
        // passer par le parent (niveau inférieur).
        let mut pos = first;
        let found = loop {
            let prev = previous_non_null(project, pos)?;
            let prev_level = level_at(project, prev)?;
            if prev_level < level {
                return None;
            }
            if prev_level == level {
                break prev;
            }
            pos = prev;
        };
        (
            project.tasks.at(found)?.uid(),
            Placement::After(TaskRef::Uid(last_uid)),
        )
    } else {
        let next = next_non_null(project, block_end(project, last))?;
        if level_at(project, next)? != level {
            return None;
        }
        (
            project.tasks.at(next)?.uid(),
            Placement::Before(TaskRef::Uid(first_uid)),
        )
    };
    Some(Command::TaskMove(TaskMove {
        uid: TaskRef::Uid(neighbour),
        to,
    }))
}

/// Lot de suppression : une récapitulative part avec son bloc
/// (`cascade:true`), les tâches de ce bloc ne sont pas reprises.
fn plan_delete(project: &Project, selection: &Selection) -> Option<StructurePlan> {
    let mut ops = Vec::new();
    let mut count = 0_usize;
    let mut cascade_any = false;
    for (pos, uid) in top_level(project, selection) {
        let cascade = is_summary_at(project, pos);
        cascade_any |= cascade;
        count = count.saturating_add(block_end(project, pos).saturating_sub(pos));
        ops.push(Command::TaskDelete(TaskDelete {
            uid: TaskRef::Uid(uid),
            cascade,
        }));
    }
    (!ops.is_empty()).then(|| StructurePlan {
        batch: batch(ops),
        confirm_cascade: cascade_any.then_some(count),
    })
}

/// Tâches déplacées par un glisser de lignes (H-UI-94) : tâches
/// sélectionnées sans ancêtre sélectionné, dans l'ordre des lignes (leur
/// bloc les suit).
#[must_use]
pub fn move_roots(project: &Project, selection: &Selection) -> Vec<TaskUid> {
    top_level(project, selection)
        .into_iter()
        .map(|(_, uid)| uid)
        .collect()
}

/// Déplacement des lignes sélectionnées avant la tâche `before` (`None` :
/// en fin de projet), H-UI-94 (ADR-0018 D6, S85) : une commande `task.move`
/// par tâche sélectionnée sans ancêtre sélectionné, dans l'ordre (chacune
/// `Placement::Before(before)` ; en fin, `After` de la dernière tâche de
/// premier niveau puis de la tâche déplacée précédente), dans **un** lot
/// (une entrée d'annulation). UID, avancement et liens gardés (le registre
/// déplace les tâches, il n'en crée pas). Rien n'est calculé ici (règle 3) :
/// seuls l'ordre et les niveaux du modèle sont lus ; le registre reste juge
/// de la hiérarchie (R-154). Sélection vide ou cible inconnue : lot vide.
///
/// # Errors
///
/// [`StructureRefusal::NotAdjacent`] : tâches qui ne sont pas des sœurs
/// voisines (même règle que Monter/Descendre) ;
/// [`StructureRefusal::InsideOwnBlock`] : cible dans le bloc déplacé (ses
/// sous-tâches comprises), ou lâcher qui ne changerait rien (juste après le
/// bloc, au même niveau ; en fin, bloc déjà dernier).
pub fn plan_move_to(
    project: &Project,
    selection: &Selection,
    before: Option<TaskUid>,
) -> Result<Batch, StructureRefusal> {
    let top = top_level(project, selection);
    let (Some(&(first, _)), Some(&(last, _))) = (top.first(), top.last()) else {
        return Ok(batch(Vec::new()));
    };
    if !siblings_adjacent(project, &top) {
        return Err(StructureRefusal::NotAdjacent);
    }
    let span = first..block_end(project, last);
    let level = level_at(project, first);
    let ops = if let Some(target) = before {
        let Some(at) = project.tasks.position(target) else {
            return Ok(batch(Vec::new()));
        };
        let unchanged = next_non_null(project, span.end) == Some(at) && level_at(project, at) == level;
        if span.contains(&at) || unchanged {
            return Err(StructureRefusal::InsideOwnBlock);
        }
        top.iter()
            .map(|(_, uid)| {
                Command::TaskMove(TaskMove {
                    uid: TaskRef::Uid(*uid),
                    to: Placement::Before(TaskRef::Uid(target)),
                })
            })
            .collect()
    } else {
        // Dernière tâche de premier niveau : son bloc finit le projet.
        let Some(anchor) = (0..project.tasks.len()).rev().find(|pos| {
            project
                .tasks
                .at(*pos)
                .is_some_and(|t| !t.is_null && t.outline_level.get() <= 1)
        }) else {
            return Ok(batch(Vec::new()));
        };
        if span.contains(&anchor) {
            return Err(StructureRefusal::InsideOwnBlock);
        }
        let Some(mut after) = project.tasks.at(anchor).map(plannercraft_core::HasUid::uid) else {
            return Ok(batch(Vec::new()));
        };
        top.iter()
            .map(|(_, uid)| {
                let op = Command::TaskMove(TaskMove {
                    uid: TaskRef::Uid(*uid),
                    to: Placement::After(TaskRef::Uid(after)),
                });
                after = *uid;
                op
            })
            .collect()
    };
    Ok(batch(ops))
}

/// Refus d'un lâcher de lignes avant `before` (H-UI-94), calculé pendant
/// le glisser pour le curseur interdit et son info-bulle : tri ou filtre
/// (`locked`, H-UI-32), puis ceux de [`plan_move_to`].
#[must_use]
pub fn move_refusal(
    project: &Project,
    selection: &Selection,
    before: Option<TaskUid>,
    locked: bool,
) -> Option<StructureRefusal> {
    if locked {
        return Some(StructureRefusal::ViewReorderLocked);
    }
    plan_move_to(project, selection, before).err()
}

impl App {
    /// Déplace les lignes sélectionnées en lignes entières avant `before`
    /// (`None` : en fin), un lot de `task.move` (`Action::MoveRows`,
    /// ADR-0018 D6, H-UI-94 ; info-bulle du geste : [`ROW_MOVE_TIP`]) :
    /// [`plan_move_to`] ; refus (tri ou filtre, bloc, lignes non voisines)
    /// au bandeau, rien d'écrit ; refus du registre : bandeau
    /// (`execute`). La sélection garde les tâches déplacées (UID gardés).
    pub(crate) fn move_rows(&mut self, before: Option<TaskUid>) {
        self.sync_selection();
        let Some(document) = &self.document else {
            return;
        };
        let plan = if self.view.task_view.reorder_locked() {
            Err(StructureRefusal::ViewReorderLocked)
        } else {
            plan_move_to(document.project(), &self.selected, before)
        };
        match plan {
            Ok(batch) if batch.ops.is_empty() => {}
            // Un refus du registre est affiché par `execute`.
            Ok(batch) => drop(self.execute(&batch, EditOrigin::User)),
            Err(refusal) => {
                let text = self.texts.get(refusal.move_text_id());
                self.push_message(plannercraft_core::Severity::Info, text);
            }
        }
    }

    /// Applique une commande de structure à la sélection courante. Une
    /// suppression de récapitulative attend la confirmation
    /// ([`App::pending_delete`]) ; rien ne se passe tant qu'une
    /// confirmation est ouverte.
    pub fn run_structure(&mut self, op: StructureOp) {
        self.sync_selection();
        if self.pending_delete.is_some() {
            return;
        }
        let name = match op {
            StructureOp::InsertMilestone => self.texts.get(TextId::NewMilestoneName),
            StructureOp::InsertSummary => self.texts.get(TextId::NewSummaryName),
            _ => String::new(),
        };
        // H-UI-83 : Jalon et Récapitulative grisés sous tri ou filtre, comme
        // Abaisser (H-UI-32) ; refus au bandeau si l'action arrive quand même.
        if matches!(op, StructureOp::InsertMilestone | StructureOp::InsertSummary)
            && self.view.task_view.reorder_locked()
        {
            let text = self.texts.get(TextId::ViewReorderLocked);
            self.push_message(plannercraft_core::Severity::Info, text);
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let plan = match plan_named(op, document.project(), &self.selected, &name) {
            Ok(Some(plan)) => plan,
            Ok(None) => return,
            Err(refusal) => {
                let text = self.texts.get(refusal.text_id());
                self.push_message(plannercraft_core::Severity::Info, text);
                return;
            }
        };
        if let Some(count) = plan.confirm_cascade {
            self.pending_delete = Some(PendingDelete {
                batch: plan.batch,
                count,
                cache_key: document.cache_key(),
            });
            return;
        }
        // Un refus est affiché par `execute`.
        if let Ok(outcome) = self.execute(&plan.batch, EditOrigin::User)
            && matches!(
                op,
                StructureOp::InsertBefore | StructureOp::InsertMilestone | StructureOp::InsertSummary
            )
            && let Some(created) = outcome.created.first()
        {
            self.selection = Some(*created);
            self.sync_selection();
        }
    }

    /// Réponse à la confirmation de suppression : `true` applique le lot
    /// (s'il n'est pas devenu caduc), `false` l'abandonne.
    pub fn answer_delete(&mut self, confirmed: bool) {
        let Some(pending) = self.pending_delete.take() else {
            return;
        };
        let current = self.document.as_ref().map(crate::app::OpenDocument::cache_key);
        if confirmed && current == Some(pending.cache_key) {
            // Un refus est affiché par `execute`.
            drop(self.execute(&pending.batch, EditOrigin::User));
        }
    }

    /// Abandonne une confirmation devenue caduque (document modifié, fermé
    /// ou remplacé) : la fenêtre ne reste pas ouverte sur un autre état.
    pub(crate) fn drop_stale_delete(&mut self) {
        let current = self.document.as_ref().map(crate::app::OpenDocument::cache_key);
        if self
            .pending_delete
            .as_ref()
            .is_some_and(|pending| Some(pending.cache_key) != current)
        {
            self.pending_delete = None;
        }
    }

    /// Suppression en attente de confirmation, s'il y en a une.
    #[must_use]
    pub const fn pending_delete(&self) -> Option<&PendingDelete> {
        self.pending_delete.as_ref()
    }
}

/// Fenêtre modale de confirmation d'une suppression de récapitulative :
/// « Supprimer » pousse `ConfirmDelete(true)`, « Annuler », Échap ou un clic
/// hors de la fenêtre `ConfirmDelete(false)`.
pub fn confirm_modal(ctx: &egui::Context, texts: &Texts, pending: &PendingDelete, actions: &mut Vec<Action>) {
    let count = i64::try_from(pending.count).unwrap_or(i64::MAX);
    let response = egui::Modal::new(egui::Id::unique("plannercraft.confirm-delete")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.heading(texts.get(TextId::ConfirmDeleteTitle));
        ui.label(texts.get_args(TextId::ConfirmDeleteText, &[("count", Arg::Count(count))]));
        ui.add_space(8.0);
        let mut answer = None;
        ui.horizontal(|ui| {
            if ui.button(texts.get(TextId::ConfirmDeleteYes)).clicked() {
                answer = Some(true);
            }
            if ui.button(texts.get(TextId::ConfirmDeleteNo)).clicked() {
                answer = Some(false);
            }
        });
        answer
    });
    match response.inner {
        Some(answer) => actions.push(Action::ConfirmDelete(answer)),
        None if response.should_close() => actions.push(Action::ConfirmDelete(false)),
        None => {}
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_automation::{Batch, Command, Placement, TaskRef};
    use plannercraft_core::{Arena, Moment, Project, ProjectOptions, Task, TaskUid};

    use super::{StructureOp, block_end, is_summary_at, plan};
    use crate::app::{App, Env};
    use crate::selection::{SelectMode, Selection};

    fn demo() -> App {
        let now: Moment = "2027-01-15T08:00".parse().unwrap();
        let mut app = App::new(crate::UiState::default(), Env::fixed(now));
        app.open_demo();
        app
    }

    fn project(app: &App) -> &Project {
        app.document.as_ref().unwrap().project()
    }

    fn uid_at(app: &App, pos: usize) -> TaskUid {
        project(app).tasks.at(pos).unwrap().uid()
    }

    /// Première récapitulative de l'exemple (position).
    fn first_summary(app: &App) -> usize {
        (0..project(app).tasks.len())
            .find(|pos| is_summary_at(project(app), *pos))
            .unwrap()
    }

    fn ops(batch: &Batch) -> Vec<String> {
        batch.ops.iter().map(|op| op.id().to_owned()).collect()
    }

    #[test]
    fn insert_before_first_in_id_order_or_end() {
        let app = demo();
        let rows = app.visible_rows().unwrap();
        let (a, b) = (uid_at(&app, 4), uid_at(&app, 2));
        let mut sel = Selection::single(Some(a));
        sel.click(b, SelectMode::Toggle, rows);
        let plan = plan(StructureOp::InsertBefore, project(&app), &sel).unwrap();
        assert_eq!(plan.confirm_cascade, None);
        match &plan.batch.ops[..] {
            [Command::TaskAdd(add)] => assert_eq!(add.at, Some(Placement::Before(TaskRef::Uid(b)))),
            other => panic!("{other:?}"),
        }
        let plan = super::plan(StructureOp::InsertBefore, project(&app), &Selection::default()).unwrap();
        match &plan.batch.ops[..] {
            [Command::TaskAdd(add)] => assert_eq!(add.at, Some(Placement::End)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn delete_summary_skips_its_block_and_asks() {
        let app = demo();
        let rows = app.visible_rows().unwrap();
        let s = first_summary(&app);
        let end = block_end(project(&app), s);
        assert!(end > s + 1);
        let mut sel = Selection::single(Some(uid_at(&app, s)));
        sel.click(uid_at(&app, s + 1), SelectMode::Toggle, rows);
        let plan = plan(StructureOp::Delete, project(&app), &sel).unwrap();
        assert_eq!(plan.confirm_cascade, Some(end - s));
        match &plan.batch.ops[..] {
            [Command::TaskDelete(d)] => {
                assert_eq!(d.uid, TaskRef::Uid(uid_at(&app, s)));
                assert!(d.cascade);
            }
            other => panic!("{other:?}"),
        }
        // Une feuille : sans confirmation, sans cascade.
        let leaf = (0..project(&app).tasks.len())
            .find(|p| !is_summary_at(project(&app), *p))
            .unwrap();
        let plan = super::plan(
            StructureOp::Delete,
            project(&app),
            &Selection::single(Some(uid_at(&app, leaf))),
        )
        .unwrap();
        assert_eq!(plan.confirm_cascade, None);
        assert!(matches!(&plan.batch.ops[..], [Command::TaskDelete(d)] if !d.cascade));
    }

    #[test]
    fn nothing_to_do_gives_none() {
        let app = demo();
        let p = project(&app);
        for op in [
            StructureOp::Delete,
            StructureOp::Indent,
            StructureOp::Outdent,
            StructureOp::Link,
            StructureOp::Unlink,
        ] {
            assert_eq!(plan(op, p, &Selection::default()), None, "{op:?}");
        }
        // Un seul maillon : rien à lier.
        assert_eq!(
            plan(StructureOp::Link, p, &Selection::single(Some(uid_at(&app, 1)))),
            None
        );
        // Tâche inconnue : ignorée.
        assert_eq!(
            plan(
                StructureOp::Indent,
                p,
                &Selection::single(Some(TaskUid::new(999_999)))
            ),
            None
        );
    }

    #[test]
    fn link_and_unlink_ops() {
        let app = demo();
        let rows = app.visible_rows().unwrap();
        let (a, b, c) = (uid_at(&app, 6), uid_at(&app, 2), uid_at(&app, 4));
        let mut sel = Selection::single(Some(a));
        sel.click(b, SelectMode::Toggle, rows);
        sel.click(c, SelectMode::Toggle, rows);
        let plan = plan(StructureOp::Link, project(&app), &sel).unwrap();
        assert_eq!(ops(&plan.batch), ["link.add", "link.add"]);
        let pairs: Vec<(TaskRef, TaskRef)> = plan
            .batch
            .ops
            .iter()
            .map(|op| match op {
                Command::LinkAdd(spec) => (spec.pred.clone(), spec.succ.clone()),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            pairs,
            [
                (TaskRef::Uid(a), TaskRef::Uid(b)),
                (TaskRef::Uid(b), TaskRef::Uid(c))
            ]
        );
        // Délier : seulement les liens dont les deux bouts sont sélectionnés.
        let p = project(&app);
        let (succ_pos, succ) = p
            .tasks
            .iter()
            .enumerate()
            .find_map(|(pos, t)| (!t.predecessors.is_empty()).then_some((pos, t.uid())))
            .unwrap();
        let pred = p.tasks.at(succ_pos).unwrap().predecessors[0].predecessor;
        let mut sel = Selection::single(Some(succ));
        assert_eq!(plan_unlink_count(p, &sel), 0);
        sel.click(pred, SelectMode::Toggle, rows);
        assert!(plan_unlink_count(p, &sel) >= 1);
    }

    /// R-052 : une ligne vide entre une tâche et sa première sous-tâche ne
    /// la rend pas feuille ; sa suppression demande toujours la cascade.
    #[test]
    fn summary_ignores_null_row_before_child() {
        let app = demo();
        let s = first_summary(&app);
        let mut p = project(&app).clone();
        let mut null = Task::new(TaskUid::new(999_999), String::new(), &ProjectOptions::default());
        null.outline_level = p.tasks.at(s).unwrap().outline_level;
        null.is_null = true;
        let mut tasks: Vec<Task> = p.tasks.iter().cloned().collect();
        tasks.insert(s + 1, null);
        p.tasks = Arena::try_from(tasks).unwrap();
        assert!(is_summary_at(&p, s));
        assert!(!is_summary_at(&p, s + 1));
        let uid = p.tasks.at(s).unwrap().uid();
        let plan = plan(StructureOp::Delete, &p, &Selection::single(Some(uid))).unwrap();
        assert_eq!(plan.confirm_cascade, Some(block_end(&p, s) - s));
        assert!(matches!(&plan.batch.ops[..], [Command::TaskDelete(d)] if d.cascade));
    }

    /// H-UI-29 : monter = la sœur précédente passe après le bloc ;
    /// descendre = la sœur suivante passe avant ; une commande ; rien au
    /// bord ni pour une sélection non contiguë.
    #[test]
    fn move_up_down_neighbour_sibling() {
        let app = demo();
        let rows = app.visible_rows().unwrap();
        let p = project(&app);
        let s = first_summary(&app);
        // Deux premières sous-tâches de la première récapitulative.
        let (first, second) = (s + 1, s + 2);
        assert!(!is_summary_at(p, first));
        let one = |pos: usize| Selection::single(Some(uid_at(&app, pos)));
        // Premier enfant : rien à monter (le parent n'est pas une sœur).
        assert_eq!(plan(StructureOp::MoveUp, p, &one(first)), None);
        let plan_up = plan(StructureOp::MoveUp, p, &one(second)).unwrap();
        match &plan_up.batch.ops[..] {
            [Command::TaskMove(m)] => {
                assert_eq!(m.uid, TaskRef::Uid(uid_at(&app, first)));
                assert_eq!(m.to, Placement::After(TaskRef::Uid(uid_at(&app, second))));
            }
            other => panic!("{other:?}"),
        }
        let plan_down = plan(StructureOp::MoveDown, p, &one(first)).unwrap();
        match &plan_down.batch.ops[..] {
            [Command::TaskMove(m)] => {
                assert_eq!(m.uid, TaskRef::Uid(uid_at(&app, second)));
                assert_eq!(m.to, Placement::Before(TaskRef::Uid(uid_at(&app, first))));
            }
            other => panic!("{other:?}"),
        }
        // Bloc de deux sœurs : la sœur suivante passe avant le bloc.
        let mut pair = one(first);
        pair.click(uid_at(&app, second), SelectMode::Range, rows);
        if let Some(plan) = plan(StructureOp::MoveDown, p, &pair) {
            assert!(matches!(&plan.batch.ops[..], [Command::TaskMove(m)]
                if m.to == Placement::Before(TaskRef::Uid(uid_at(&app, first)))));
        }
        // Sélection non contiguë (récapitulative et une tâche hors de son
        // bloc) : rien.
        let mut gap = one(s);
        gap.click(uid_at(&app, block_end(p, s) + 1), SelectMode::Toggle, rows);
        assert_eq!(plan(StructureOp::MoveUp, p, &gap), None);
        assert_eq!(plan(StructureOp::MoveDown, p, &Selection::default()), None);
        // Dernière tâche du projet : rien à descendre.
        let last = p.tasks.len() - 1;
        assert_eq!(plan(StructureOp::MoveDown, p, &one(last)), None);
    }

    /// Application ouverte sur « P » (UID = ID).
    fn p_app() -> App {
        let now: Moment = "2027-01-15T08:00".parse().unwrap();
        let mut app = App::new(crate::UiState::default(), Env::fixed(now));
        app.open_project(
            plannercraft_tests::daily::plan_p().project().clone(),
            crate::DocOrigin {
                name: "P".to_owned(),
                path: None,
                demo: false,
            },
        );
        app
    }

    /// Lignes entières `first` à `last` de « P ».
    fn p_rows(app: &App, first: u32, last: u32) -> Selection {
        Selection::range(
            TaskUid::new(first),
            TaskUid::new(last),
            crate::selection::SelectionKind::Rows,
            app.visible_rows().unwrap(),
        )
    }

    fn moves(batch: &Batch) -> Vec<(TaskRef, Placement)> {
        batch
            .ops
            .iter()
            .map(|op| match op {
                Command::TaskMove(m) => (m.uid.clone(), m.to.clone()),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    /// H-UI-94 : un `task.move` par tâche sans ancêtre sélectionné, dans
    /// l'ordre ; avant la tâche sous le trait ; en fin, après la dernière
    /// tâche de premier niveau puis après la tâche déplacée précédente ;
    /// refus dans le bloc, sans effet, non voisines.
    #[test]
    fn h_ui_94_plan_move_to() {
        use super::{StructureRefusal, move_refusal, move_roots, plan_move_to};
        let app = p_app();
        let p = project(&app);
        let u = |n: u32| TaskUid::new(n);
        let r = |n: u32| TaskRef::Uid(u(n));
        let sel = p_rows(&app, 6, 7);
        assert_eq!(move_roots(p, &sel), [u(6), u(7)]);
        let batch = plan_move_to(p, &sel, Some(u(2))).unwrap();
        assert_eq!(
            moves(&batch),
            [(r(6), Placement::Before(r(2))), (r(7), Placement::Before(r(2)))]
        );
        let batch = plan_move_to(p, &sel, None).unwrap();
        assert_eq!(
            moves(&batch),
            [(r(6), Placement::After(r(5))), (r(7), Placement::After(r(6)))]
        );
        // Récapitulative 5 et ses sous-tâches : seule 5 est déplacée.
        let sel = p_rows(&app, 5, 7);
        assert_eq!(move_roots(p, &sel), [u(5)]);
        assert_eq!(
            moves(&plan_move_to(p, &sel, Some(u(1))).unwrap()),
            [(r(5), Placement::Before(r(1)))]
        );
        // Dans son bloc (sous-tâche comprise), ou en fin alors que le bloc
        // finit déjà le projet : refus.
        for before in [Some(u(5)), Some(u(6)), Some(u(7)), None] {
            assert_eq!(
                plan_move_to(p, &sel, before),
                Err(StructureRefusal::InsideOwnBlock),
                "{before:?}"
            );
        }
        // Juste après le bloc, au même niveau : rien ne changerait.
        let sel = p_rows(&app, 2, 3);
        assert_eq!(
            plan_move_to(p, &sel, Some(u(4))),
            Err(StructureRefusal::InsideOwnBlock)
        );
        // Juste après le bloc, mais à un autre niveau : déplacement réel.
        let sel = p_rows(&app, 3, 4);
        assert_eq!(moves(&plan_move_to(p, &sel, Some(u(5))).unwrap()).len(), 2);
        // Non voisines.
        let mut gap = Selection::single(Some(u(2)));
        gap.click(u(6), SelectMode::Toggle, app.visible_rows().unwrap());
        assert_eq!(
            plan_move_to(p, &gap, Some(u(1))),
            Err(StructureRefusal::NotAdjacent)
        );
        // Tri ou filtre : refus d'abord.
        assert_eq!(
            move_refusal(p, &p_rows(&app, 6, 7), Some(u(2)), true),
            Some(StructureRefusal::ViewReorderLocked)
        );
        assert_eq!(move_refusal(p, &p_rows(&app, 6, 7), Some(u(2)), false), None);
        // Sélection vide, cible inconnue : lot vide.
        let none = |sel: &Selection, before| plan_move_to(p, sel, before).unwrap().ops.len();
        assert_eq!(none(&Selection::default(), Some(u(2))), 0);
        assert_eq!(none(&p_rows(&app, 6, 7), Some(u(999))), 0);
    }

    /// H-UI-94 : le lot appliqué par le registre donne l'ordre attendu, UID
    /// gardés, une entrée d'annulation.
    #[test]
    fn h_ui_94_move_rows_applies_one_entry() {
        let mut app = p_app();
        app.selected = p_rows(&app, 6, 7);
        app.selection = app.selected.focus();
        let entries = app.document.as_ref().unwrap().session.history().entries.len();
        app.move_rows(Some(TaskUid::new(2)));
        let order: Vec<(u32, u16)> = project(&app)
            .tasks
            .iter()
            .map(|t| (t.uid().get(), t.outline_level.get()))
            .collect();
        assert_eq!(order, [(1, 1), (6, 2), (7, 2), (2, 2), (3, 2), (4, 2), (5, 1)]);
        assert_eq!(
            app.document.as_ref().unwrap().session.history().entries.len(),
            entries + 1
        );
    }

    fn plan_unlink_count(p: &Project, sel: &Selection) -> usize {
        plan(StructureOp::Unlink, p, sel).map_or(0, |plan| plan.batch.ops.len())
    }
}
