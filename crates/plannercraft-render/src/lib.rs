//! Rendu du diagramme de Gantt en SVG.
//!
//! Sert à l'IA (via MCP), aux tests et à l'export. Ne dépend que de
//! `plannercraft-core` (et de serde pour le style) : aucune interface graphique ici. Le SVG
//! (`svg`, `svg_styled`) ne lit que la scène de `geometry::layout`. Déterministe : coordonnées
//! entières en 1/16 px, aucune police système, sortie bornée et rapport de
//! troncature (ADR-0012 D10). PNG reporté en J7.
//!
//! Hypothèses d'affichage H-R : `docs/features/render.md`. J8 (ADR-0016) :
//! style et modèles (`style`), géométrie partagée (`geometry`), pagination
//! (`paginate`), largeurs d'Inter (H-R11). ADR-0017 : géométrie de la
//! chronologie (`timeline`, H-R16), imprimée en tête de la première page
//! (`paginate::paginate_timeline`) et du SVG (`svg_styled_timeline`).
//! Finitions P3 (ADR-0022 D4) : regroupement des tâches (`group`, H-R17).

mod axis;
pub mod geometry;
pub mod group;
mod metrics;
mod metrics_inter;
pub mod paginate;
mod request;
mod scene;
pub mod style;
mod svg;
pub mod timeline;

pub use metrics::{FontRef, FontWeight, InterMetrics, TextMeasure};
pub use request::{GanttRequest, Lang, RangeSpec, RenderLimits, RenderReport, RowFilter, Theme, Zoom};
pub use style::{GroupBy, PlanStyle, Rgb, TemplateId, contrast_ratio, parse_doc_style, resolve, template};
pub use svg::{svg, svg_styled, svg_styled_timeline};
