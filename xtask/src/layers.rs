//! `cargo xtask layers` : contrôle de la règle 9 (couches strictes).

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::process::ExitCode;

use cargo_metadata::{CargoOpt, Metadata, MetadataCommand};

/// Crates du workspace autorisées à dépendre d'une bibliothèque d'interface.
///
/// La CLI (`plannercraft-cli`) n'y figure pas exprès : c'est un binaire headless,
/// et `plannercraft mcp` parle à l'application ouverte par un canal (ADR-0002),
/// pas en liant egui.
const GUI_ALLOWED: &[&str] = &["plannercraft-ui-egui", "plannercraft", "plannercraft-web"];

/// Paquets d'interface interdits sous `plannercraft-ui-egui` (règle 9).
const GUI_PACKAGES: &[&str] = &["egui", "eframe", "winit", "rfd"];

/// Préfixes de la famille egui (`egui-winit`, `egui_kittest`…), interdits aussi.
const GUI_PREFIXES: &[&str] = &["egui-", "egui_"];

/// Couche du rendu (J8, ADR-0016 : `plannercraft-render`, `plannercraft-pdf`) : en plus
/// des paquets d'interface, elle ne dépend pas de `plannercraft-automation` (les
/// textes viennent de `BarTextSource`, implémenté au-dessus).
const RENDER_LAYER: &[&str] = &["plannercraft-render", "plannercraft-pdf"];

/// Paquets interdits à la couche du rendu, en plus des paquets d'interface.
const RENDER_FORBIDDEN: &[&str] = &["plannercraft-automation"];

pub fn run() -> ExitCode {
    // Toutes les features : une dépendance optionnelle (`dep:egui` derrière une
    // feature) doit être vue même si aucune feature par défaut ne l'active.
    let metadata = match MetadataCommand::new().features(CargoOpt::AllFeatures).exec() {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("xtask layers : `cargo metadata` a échoué : {error}");
            return ExitCode::FAILURE;
        }
    };
    let graph = Graph::from_metadata(&metadata);
    let violations = graph.violations();
    if violations.is_empty() {
        println!(
            "xtask layers : OK ({} crates vérifiées, exemptées : {})",
            graph.members.len(),
            GUI_ALLOWED.join(", ")
        );
        return ExitCode::SUCCESS;
    }
    for violation in &violations {
        eprintln!(
            "xtask layers : `{}` dépend de `{}` (chemin : {})",
            violation.member,
            violation.forbidden,
            violation.path.join(" -> ")
        );
    }
    eprintln!(
        "xtask layers : {} violation(s) de la règle 9 (couches strictes)",
        violations.len()
    );
    ExitCode::FAILURE
}

/// Dépendance interdite trouvée depuis une crate du workspace.
#[derive(Debug, PartialEq, Eq)]
struct Violation {
    member: String,
    forbidden: String,
    /// Chaîne de noms de paquets, du membre au paquet interdit inclus.
    path: Vec<String>,
}

/// Graphe de dépendances résolu, indexé par identifiant de paquet.
///
/// L'identifiant (et non le nom) distingue les versions d'une même crate :
/// avec un index par nom, les arêtes d'une version écraseraient celles de
/// l'autre. Les noms ne servent qu'à l'affichage et à reconnaître les paquets
/// d'interface.
///
/// Les arêtes viennent de la résolution de `cargo metadata` avec toutes les
/// features : toutes les sortes de dépendances (normales, build, dev des
/// membres) et toutes les plateformes. Les features y sont unifiées sur tout
/// le workspace : le contrôle peut produire un faux positif, pas un faux
/// négatif.
#[derive(Debug, Default)]
struct Graph {
    /// Identifiant de paquet -> nom.
    names: BTreeMap<String, String>,
    /// Identifiants des membres du workspace.
    members: BTreeSet<String>,
    /// Identifiant de paquet -> identifiants de ses dépendances directes.
    edges: BTreeMap<String, BTreeSet<String>>,
}

impl Graph {
    fn from_metadata(metadata: &Metadata) -> Self {
        let names = metadata
            .packages
            .iter()
            .map(|package| (package.id.repr.clone(), package.name.as_str().to_owned()))
            .collect();
        let members = metadata
            .workspace_members
            .iter()
            .map(|id| id.repr.clone())
            .collect();
        let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        if let Some(resolve) = &metadata.resolve {
            for node in &resolve.nodes {
                edges
                    .entry(node.id.repr.clone())
                    .or_default()
                    .extend(node.deps.iter().map(|dep| dep.pkg.repr.clone()));
            }
        }
        Self {
            names,
            members,
            edges,
        }
    }

    /// Nom d'un paquet ; son identifiant brut s'il est inconnu.
    fn name<'a>(&'a self, id: &'a str) -> &'a str {
        self.names.get(id).map_or(id, String::as_str)
    }

    /// Liste les membres non exemptés qui atteignent un paquet d'interface.
    fn violations(&self) -> Vec<Violation> {
        self.members
            .iter()
            .filter(|member| !GUI_ALLOWED.contains(&self.name(member)))
            .flat_map(|member| self.forbidden_paths_from(member))
            .collect()
    }

    /// Parcours en largeur depuis `member` ; un chemin par paquet interdit atteint.
    fn forbidden_paths_from(&self, member: &str) -> Vec<Violation> {
        let render = RENDER_LAYER.contains(&self.name(member));
        let forbidden = |name: &str| is_gui_package(name) || (render && RENDER_FORBIDDEN.contains(&name));
        let mut parent: BTreeMap<&str, &str> = BTreeMap::new();
        let mut seen: BTreeSet<&str> = BTreeSet::from([member]);
        let mut queue: VecDeque<&str> = VecDeque::from([member]);
        let mut found = Vec::new();
        while let Some(current) = queue.pop_front() {
            if current != member && forbidden(self.name(current)) {
                found.push(Violation {
                    member: self.name(member).to_owned(),
                    forbidden: self.name(current).to_owned(),
                    path: self.rebuild_path(&parent, current),
                });
                // Inutile de descendre sous un paquet déjà signalé.
                continue;
            }
            for dep in self.edges.get(current).into_iter().flatten() {
                if seen.insert(dep.as_str()) {
                    parent.insert(dep.as_str(), current);
                    queue.push_back(dep.as_str());
                }
            }
        }
        found
    }

    /// Chemin de noms du membre jusqu'à `end`, à partir des parents du parcours.
    fn rebuild_path(&self, parent: &BTreeMap<&str, &str>, end: &str) -> Vec<String> {
        let mut path = vec![self.name(end).to_owned()];
        let mut current = end;
        while let Some(previous) = parent.get(current) {
            path.push(self.name(previous).to_owned());
            current = previous;
        }
        path.reverse();
        path
    }
}

fn is_gui_package(name: &str) -> bool {
    GUI_PACKAGES.contains(&name) || GUI_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use super::{Graph, Violation};

    /// Construit un graphe. Un identifiant `nom@version` distingue deux
    /// versions d'une même crate ; le nom est la partie avant `@`.
    fn graph(members: &[&str], edges: &[(&str, &[&str])]) -> Graph {
        let mut ids: Vec<&str> = members.to_vec();
        for (from, to) in edges {
            ids.push(from);
            ids.extend(to.iter());
        }
        Graph {
            names: ids
                .iter()
                .map(|id| ((*id).to_owned(), id.split('@').next().unwrap().to_owned()))
                .collect(),
            members: members.iter().map(|id| (*id).to_owned()).collect(),
            edges: edges
                .iter()
                .map(|(from, to)| ((*from).to_owned(), to.iter().map(|id| (*id).to_owned()).collect()))
                .collect(),
        }
    }

    fn names(path: &[&str]) -> Vec<String> {
        path.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn clean_graph_has_no_violation() {
        let graph = graph(
            &["plannercraft-core", "plannercraft-ui-egui"],
            &[
                ("plannercraft-core", &["chrono"]),
                ("plannercraft-ui-egui", &["plannercraft-core", "egui"]),
            ],
        );
        assert_eq!(graph.violations(), Vec::new());
    }

    #[test]
    fn transitive_gui_dependency_is_reported_with_its_path() {
        let graph = graph(
            &["plannercraft-core", "plannercraft-render"],
            &[
                ("plannercraft-render", &["plannercraft-core", "helper"]),
                ("helper", &["egui-winit"]),
                ("egui-winit", &["winit"]),
            ],
        );
        assert_eq!(
            graph.violations(),
            vec![Violation {
                member: "plannercraft-render".to_owned(),
                forbidden: "egui-winit".to_owned(),
                path: names(&["plannercraft-render", "helper", "egui-winit"]),
            }]
        );
    }

    #[test]
    fn non_exempt_member_depending_on_ui_crate_is_reported() {
        let graph = graph(
            &["plannercraft-automation", "plannercraft-ui-egui"],
            &[
                ("plannercraft-automation", &["plannercraft-ui-egui"]),
                ("plannercraft-ui-egui", &["rfd"]),
            ],
        );
        let violations = graph.violations();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].forbidden, "rfd");
    }

    /// Deux versions de `helper` : seule la 2 mène à winit. Le membre qui
    /// dépend de la 2 est signalé, quel que soit l'ordre de lecture des nœuds
    /// (un index par nom gardait les arêtes de la dernière version lue).
    #[test]
    fn render_layer_must_not_reach_automation() {
        let graph = graph(
            &[
                "plannercraft-pdf",
                "plannercraft-render",
                "plannercraft-automation",
                "plannercraft-mcp",
            ],
            &[
                (
                    "plannercraft-pdf",
                    &["plannercraft-render", "plannercraft-automation"],
                ),
                ("plannercraft-render", &["plannercraft-core"]),
                (
                    "plannercraft-automation",
                    &["plannercraft-render", "plannercraft-pdf"],
                ),
                ("plannercraft-mcp", &["plannercraft-automation"]),
            ],
        );
        let violations = graph.violations();
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].member, "plannercraft-pdf");
        assert_eq!(violations[0].forbidden, "plannercraft-automation");
    }

    #[test]
    fn duplicate_versions_keep_their_own_edges() {
        let graph = graph(
            &["plannercraft-render", "plannercraft-mspdi"],
            &[
                ("plannercraft-render", &["helper@2"]),
                ("plannercraft-mspdi", &["helper@1"]),
                ("helper@2", &["winit"]),
                ("helper@1", &["smallvec"]),
            ],
        );
        assert_eq!(
            graph.violations(),
            vec![Violation {
                member: "plannercraft-render".to_owned(),
                forbidden: "winit".to_owned(),
                path: names(&["plannercraft-render", "helper", "winit"]),
            }]
        );
    }
}
