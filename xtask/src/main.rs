//! Tâches de maintenance du dépôt (motif cargo-xtask), lancées par
//! `cargo xtask <tâche>` grâce à l'alias de `.cargo/config.toml`.
//!
//! Tâches :
//! - `layers` : vérifie la règle 9 (couches strictes). Aucune crate du
//!   workspace, hors exemptions, ne doit dépendre, directement ou
//!   transitivement, d'egui, eframe, winit ou rfd.
//! - `corpus fetch` : télécharge les fichiers du corpus public
//!   (`tests/corpus/manifest.toml`), contrôle taille et SHA-256.
//! - `schema fetch` : extrait le schéma MSPDI `mspdi_pj15.xsd` du MSI du SDK
//!   de Project 2013 (`tests/schema/`, sha256 vérifiés).
//! - `panics` : complète clippy pour la règle 10 (ne jamais planter). Refuse
//!   hors du code de test les `allow`/`expect` qui lèvent un interdit de la
//!   règle 10 et la famille `assert!`, qu'aucune lint clippy ne refuse.
//! - `fonts [--check]` : écrit (ou vérifie) la table des avances d'Inter de
//!   `plannercraft-render` depuis `assets/fonts/` (H-R11).
//! - `text-ids` : chaque `TextId` de l'interface est traduit en français et
//!   en anglais et utilisé dans le code livré (ADR-0016 D14).
//! - `icon [--check]` : écrit (ou vérifie) l'icône de l'application en PNG
//!   (`assets/icon/`, 16 à 1 024 px), dessin déterministe.
//! - `icons [--check]` : écrit (ou vérifie) les polylignes des icônes du
//!   ruban depuis les SVG Lucide de `assets/icons/lucide/` (H-UI-77).
//! - `release-check <tag>` : tag = version du workspace = première section
//!   de `CHANGELOG.md` (C-REL-01) ; `release-notes <tag>` : notes de la
//!   release (section du CHANGELOG et lien vers le guide d'installation).
//! - `rules` : vérifie la règle 4. Chaque fiche `R-xxx` active de
//!   `docs/scheduling-rules.md` a une ligne « Tests » et au moins une
//!   fonction `fn rxxx_` dans `tests/` ou `crates/`.

mod corpus;
mod fonts;
mod icon;
mod icons;
mod layers;
mod panics;
mod release;
mod rules;
mod schema;
mod text_ids;

use std::process::ExitCode;

const USAGE: &str = "usage : cargo xtask <tâche>

tâches :
  layers   vérifie les couches (règle 9)
  panics   vérifie les interdits de la règle 10 que clippy ne couvre pas
  fonts [--check]
           écrit (ou vérifie) la table des avances d'Inter (H-R11)
  icon [--check]
           écrit (ou vérifie) l'icône de l'application (assets/icon, PNG)
  icons [--check]
           écrit (ou vérifie) les icônes du ruban (SVG Lucide → polylignes)
  text-ids vérifie que chaque texte de l'interface est traduit (FR, EN) et utilisé
  rules    vérifie que chaque règle R-xxx a ses tests (règle 4)
  release-check <tag>
           vérifie tag = version du workspace = CHANGELOG.md (C-REL-01)
  release-notes <tag>
           imprime les notes de la release (section du CHANGELOG)
  corpus fetch
           télécharge le corpus public (tests/corpus/files, sha256 vérifié)
  schema fetch
           extrait mspdi_pj15.xsd du SDK de Project 2013 (tests/schema, sha256 vérifiés)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("layers") => layers::run(),
        Some("panics") => panics::run(),
        Some("rules") => rules::run(),
        Some("text-ids") => text_ids::run(),
        Some("icon") => icon::run(args.get(1..).unwrap_or_default()),
        Some("icons") => icons::run(args.get(1..).unwrap_or_default()),
        Some("release-check") => release::run_check(args.get(1..).unwrap_or_default()),
        Some("release-notes") => release::run_notes(args.get(1..).unwrap_or_default()),
        Some("fonts") => fonts::run(args.get(1..).unwrap_or_default()),
        Some("corpus") => corpus::run(args.get(1..).unwrap_or_default()),
        Some("schema") => schema::run(args.get(1..).unwrap_or_default()),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
