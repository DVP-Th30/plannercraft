//! Ordre topologique stable et cycles (spécification J2 § 3.6, R-035,
//! R-065).
//!
//! 1. Chemin rapide : si tout arc va vers l'avant de la numérotation
//!    canonique, l'ordre est l'identité.
//! 2. Sinon, Kahn sur un tas-min de l'indice canonique : l'ordre ne dépend
//!    ni de l'ordre de stockage des liens ni d'une table de hachage.
//! 3. S'il reste des nœuds, Tarjan itératif (pile explicite) sur eux ; dans
//!    chaque composante fortement connexe d'au moins deux nœuds, les arcs de
//!    lien internes qui remontent (`from ≥ to`) sont coupés, puis Kahn est
//!    relancé une fois. Preuve d'arrêt : un cycle reste dans une composante
//!    et contient un arc qui remonte ; les arcs de hiérarchie vont tous vers
//!    l'avant, c'est donc un lien, coupé ; après la coupure, plus de cycle.

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};

use super::network::{Network, NodeIx, ix, pos};
use crate::diagnostic::{Diagnostic, Diagnostics};
use crate::ids::TaskUid;

/// Kahn sur les arcs non coupés ; rend l'ordre (éventuellement incomplet
/// s'il reste un cycle).
fn kahn(net: &Network<'_>) -> Option<Vec<NodeIx>> {
    let n = net.nodes.len();
    let mut indeg = vec![0u32; n];
    for (k, a) in net.arcs.iter().enumerate() {
        if net.is_cut(ix(k)?) {
            continue;
        }
        let d = indeg.get_mut(pos(a.to)?)?;
        *d = d.checked_add(1)?;
    }
    let mut heap: BinaryHeap<Reverse<NodeIx>> = BinaryHeap::new();
    for (v, d) in indeg.iter().enumerate() {
        if *d == 0 {
            heap.push(Reverse(ix(v)?));
        }
    }
    let mut order = Vec::with_capacity(n);
    while let Some(Reverse(v)) = heap.pop() {
        order.push(v);
        for &a in net.outgoing(v)? {
            if net.is_cut(a) {
                continue;
            }
            let w = net.arc(a)?.to;
            let d = indeg.get_mut(pos(w)?)?;
            *d = d.checked_sub(1)?;
            if *d == 0 {
                heap.push(Reverse(w));
            }
        }
    }
    Some(order)
}

/// Composantes fortement connexes d'au moins deux nœuds, restreintes aux
/// nœuds `remaining` et aux arcs non coupés (Tarjan itératif).
fn strong_components(net: &Network<'_>, remaining: &[bool]) -> Option<Vec<Vec<NodeIx>>> {
    const UNVISITED: u32 = u32::MAX;
    let n = net.nodes.len();
    let mut index = vec![UNVISITED; n];
    let mut low = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<NodeIx> = Vec::new();
    // Pile d'appels : (nœud, prochain arc sortant à examiner).
    let mut calls: Vec<(NodeIx, usize)> = Vec::new();
    let mut counter: u32 = 0;
    let mut components = Vec::new();
    for root in 0..n {
        if !*remaining.get(root)? || *index.get(root)? != UNVISITED {
            continue;
        }
        let root = ix(root)?;
        visit(
            root,
            &mut index,
            &mut low,
            &mut on_stack,
            &mut stack,
            &mut counter,
        )?;
        calls.push((root, 0));
        while let Some(&(v, next)) = calls.last() {
            let out = net.outgoing(v)?;
            if let Some(&a) = out.get(next) {
                calls.last_mut()?.1 = next.checked_add(1)?;
                if net.is_cut(a) {
                    continue;
                }
                let w = net.arc(a)?.to;
                let wi = pos(w)?;
                if !*remaining.get(wi)? {
                    continue;
                }
                if *index.get(wi)? == UNVISITED {
                    visit(w, &mut index, &mut low, &mut on_stack, &mut stack, &mut counter)?;
                    calls.push((w, 0));
                } else if *on_stack.get(wi)? {
                    let iw = *index.get(wi)?;
                    let lv = low.get_mut(pos(v)?)?;
                    *lv = (*lv).min(iw);
                }
                continue;
            }
            calls.pop();
            let lv = *low.get(pos(v)?)?;
            if let Some(&(parent, _)) = calls.last() {
                let lp = low.get_mut(pos(parent)?)?;
                *lp = (*lp).min(lv);
            }
            if lv == *index.get(pos(v)?)? {
                let mut comp = Vec::new();
                loop {
                    let w = stack.pop()?;
                    *on_stack.get_mut(pos(w)?)? = false;
                    comp.push(w);
                    if w == v {
                        break;
                    }
                }
                if comp.len() >= 2 {
                    comp.sort_unstable();
                    components.push(comp);
                }
            }
        }
    }
    Some(components)
}

/// Premier passage d'un nœud dans Tarjan.
fn visit(
    v: NodeIx,
    index: &mut [u32],
    low: &mut [u32],
    on_stack: &mut [bool],
    stack: &mut Vec<NodeIx>,
    counter: &mut u32,
) -> Option<()> {
    let i = pos(v)?;
    *index.get_mut(i)? = *counter;
    *low.get_mut(i)? = *counter;
    *on_stack.get_mut(i)? = true;
    stack.push(v);
    *counter = counter.checked_add(1)?;
    Some(())
}

/// Ordre topologique complet du réseau (R-035) ; coupe les cycles (R-065)
/// et les signale. `None` si un invariant est violé.
pub(super) fn order(net: &mut Network<'_>, diags: &mut Diagnostics) -> Option<Vec<NodeIx>> {
    if net.arcs.iter().all(|a| a.from < a.to) {
        return (0..net.node_count()?).map(Some).collect();
    }
    let first = kahn(net)?;
    if first.len() == net.nodes.len() {
        return Some(first);
    }
    let mut remaining = vec![true; net.nodes.len()];
    for v in &first {
        *remaining.get_mut(pos(*v)?)? = false;
    }
    let components = strong_components(net, &remaining)?;
    let mut comp_of: Vec<Option<usize>> = vec![None; net.nodes.len()];
    for (c, comp) in components.iter().enumerate() {
        for v in comp {
            *comp_of.get_mut(pos(*v)?)? = Some(c);
        }
    }
    let mut ignored: BTreeSet<(TaskUid, TaskUid)> = BTreeSet::new();
    for (k, a) in net.arcs.iter().enumerate() {
        let Some(link) = a.link else { continue };
        let cf = *comp_of.get(pos(a.from)?)?;
        if cf.is_some() && cf == *comp_of.get(pos(a.to)?)? && a.from >= a.to {
            *net.cut.get_mut(k)? = true;
            ignored.insert((link.pred, link.succ));
        }
    }
    for comp in &components {
        let mut tasks: Vec<TaskUid> = comp
            .iter()
            .map(|v| net.node(*v).map(|n| n.uid))
            .collect::<Option<Vec<TaskUid>>>()?;
        tasks.sort_unstable();
        tasks.dedup();
        diags.push(Diagnostic::LinkCycle { tasks });
    }
    for (predecessor, successor) in ignored {
        diags.push(Diagnostic::LinkIgnoredInCycle {
            predecessor,
            successor,
        });
    }
    let second = kahn(net)?;
    (second.len() == net.nodes.len()).then_some(second)
}
