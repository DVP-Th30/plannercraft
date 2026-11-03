//! Lectures typées de l'interface (J7, ADR-0015 D6) : la même fonction que
//! la branche correspondante de `Workspace::call`, sans passer par du JSON
//! à chaque image. Chaque [`Query::id`] est une entrée du registre, de
//! genre `Read` ou `Analysis` (vérifié par test) : l'interface ne lit rien
//! que le MCP ne puisse lire.

use plannercraft_automation::Session;
use plannercraft_automation::analysis::{self, VarianceView};
use plannercraft_automation::command::params::{ExplainParams, UsageParams, VarianceParams};
use plannercraft_automation::error::CommandError;
use plannercraft_automation::explain::{self, ExplainView};
use plannercraft_automation::view::{self, UsageView};

/// Lecture demandée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    /// Utilisation des ressources (`analysis.resource_usage`).
    Usage(UsageParams),
    /// Explication d'une date (`analysis.explain_date`).
    Explain(ExplainParams),
    /// Écarts à une ligne de base (`analysis.baseline_variance`).
    Variance(VarianceParams),
}

/// Résultat d'une lecture.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(
    clippy::large_enum_variant,
    reason = "une valeur par lecture, gardée en cache une fois par génération : la taille ne pèse pas"
)]
pub enum QueryValue {
    /// Utilisation des ressources.
    Usage(UsageView),
    /// Explication d'une date.
    Explain(ExplainView),
    /// Écarts à une ligne de base.
    Variance(VarianceView),
}

impl Query {
    /// Id du registre de la lecture.
    #[must_use]
    pub const fn id(&self) -> &'static str {
        match self {
            Self::Usage(_) => "analysis.resource_usage",
            Self::Explain(_) => "analysis.explain_date",
            Self::Variance(_) => "analysis.baseline_variance",
        }
    }

    /// Exécute la lecture sur la session `s`.
    ///
    /// # Errors
    ///
    /// Le refus de la vue (`limit.exceeded`, `task.not_found`…).
    pub fn run(&self, s: &Session) -> Result<QueryValue, CommandError> {
        match self {
            Self::Usage(p) => view::resource_usage(s, p).map(QueryValue::Usage),
            Self::Explain(p) => explain::explain_date(s, p).map(QueryValue::Explain),
            Self::Variance(p) => analysis::baseline_variance(s, p).map(QueryValue::Variance),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use plannercraft_automation::command::TaskRef;
    use plannercraft_automation::command::params::{ExplainParams, UsageParams, VarianceParams};
    use plannercraft_automation::registry::{Kind, lookup};
    use plannercraft_automation::{AnchorSpec, NewProject, Session};
    use plannercraft_core::TaskUid;

    use super::{Query, QueryValue};

    fn queries() -> [Query; 3] {
        [
            Query::Usage(serde_json::from_value::<UsageParams>(serde_json::json!({})).unwrap()),
            Query::Explain(ExplainParams {
                task: TaskRef::Uid(TaskUid::new(1)),
                side: None,
                depth: Some(1),
            }),
            Query::Variance(VarianceParams::default()),
        ]
    }

    #[test]
    fn queries_are_registered() {
        for q in queries() {
            let spec = lookup(q.id()).unwrap();
            assert!(spec.is_listed(), "{}", q.id());
            assert!(matches!(spec.kind, Kind::Read | Kind::Analysis), "{}", q.id());
        }
    }

    #[test]
    fn queries_run_like_the_registry() {
        let init = NewProject {
            name: "P".to_owned(),
            anchor: AnchorSpec::Start("2026-10-19T08:00".parse().unwrap()),
            lang: None,
        };
        let s = Session::new_project(init, "2026-10-19T08:00".parse().unwrap()).unwrap();
        let [usage, explain, variance] = queries();
        assert!(matches!(usage.run(&s), Ok(QueryValue::Usage(_))));
        // Tâche 1 absente : refus de la vue, pas de panique.
        assert_eq!(explain.run(&s).unwrap_err().code, "task.not_found");
        assert!(matches!(variance.run(&s), Ok(QueryValue::Variance(_))));
    }
}
