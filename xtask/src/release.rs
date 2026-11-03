//! `cargo xtask release-check <tag>` et `cargo xtask release-notes <tag>`
//! (J8, lot L1 ; contrat C-REL-01, `docs/features/release.md`).
//!
//! - `release-check` : le tag `vX.Y.Z[-pre]` (semver, sans métadonnée de
//!   build) égale la version du workspace (`[workspace.package] version` du
//!   `Cargo.toml` racine) et la première section publiée de `CHANGELOG.md`
//!   (format Keep a Changelog : titres `## [X.Y.Z] - …`). Une section
//!   « Non publié » (ou « Unreleased ») en tête est admise vide seulement :
//!   non vide, son contenu manquerait aux notes de la version taguée.
//! - Tag d'essai `vX.Y.Z-test.N` (J8, lot L16 : release complète en
//!   brouillon, jamais publique) : il éprouve la version du workspace qu'il
//!   prépare, sans la renommer. Accepté quand `X.Y.Z` égale le noyau de la
//!   version du workspace (`1.0.0-test.1` pour `1.0.0-rc.1`) ; CHANGELOG et
//!   notes sont alors ceux de la version du workspace, comme les noms des
//!   artefacts (`desktop-build.yml`).
//! - `release-notes` : imprime sur la sortie standard le corps de la section
//!   de la version du tag, suivi du lien vers `docs/user/install.md` à ce
//!   tag (dépôt lu dans `[workspace.package] repository`). Le workflow
//!   `release.yml` le passe à `gh release create --notes-file`.
//!
//! Lecture seule, sans réseau ni horloge : mêmes fichiers, même verdict.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cargo_metadata::semver::Version;

/// Titres de la section des changements non publiés.
const UNRELEASED: [&str; 2] = ["Non publié", "Unreleased"];

/// Une section `## [label] …` de `CHANGELOG.md` et son corps (lignes
/// jusqu'au titre de niveau 2 suivant, références de liens exclues).
#[derive(Debug, PartialEq, Eq)]
struct Section {
    label: String,
    body: String,
}

impl Section {
    fn is_unreleased(&self) -> bool {
        UNRELEASED
            .iter()
            .any(|name| self.label.eq_ignore_ascii_case(name))
    }

    fn is_blank(&self) -> bool {
        self.body.trim().is_empty()
    }
}

/// Sections de niveau 2 d'un CHANGELOG, dans l'ordre du fichier.
fn sections(changelog: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for line in changelog.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            let label = rest
                .trim()
                .strip_prefix('[')
                .and_then(|inner| inner.split_once(']'))
                .map_or_else(|| rest.trim().to_owned(), |(label, _)| label.trim().to_owned());
            out.push(Section {
                label,
                body: String::new(),
            });
        } else if let Some(current) = out.last_mut() {
            // Définitions de liens de fin de fichier (`[0.1.0]: https://…`) :
            // hors du corps.
            let is_link_ref = line.starts_with('[') && line.contains("]: ");
            if !is_link_ref {
                current.body.push_str(line);
                current.body.push('\n');
            }
        }
    }
    for section in &mut out {
        let trimmed = section.body.trim().to_owned();
        section.body = trimmed;
    }
    out
}

/// Version d'un tag `vX.Y.Z[-pre]`.
fn tag_version(tag: &str) -> Result<Version, String> {
    let Some(raw) = tag.strip_prefix('v') else {
        return Err(format!("tag « {tag} » : forme attendue vX.Y.Z ou vX.Y.Z-pré"));
    };
    let version =
        Version::parse(raw).map_err(|error| format!("tag « {tag} » : version SemVer invalide ({error})"))?;
    if !version.build.is_empty() {
        return Err(format!("tag « {tag} » : métadonnée de build (+…) refusée"));
    }
    Ok(version)
}

/// Vrai pour un tag d'essai `vX.Y.Z-test.N` (brouillon de `release.yml`).
fn is_test_tag(version: &Version) -> bool {
    version.pre.as_str().starts_with("test.")
}

/// Version publiée que vérifie `tag` : la sienne, ou, pour un tag d'essai,
/// celle du workspace dont il partage le noyau `X.Y.Z`.
fn checked_version(tag: &str, cargo_toml: &str) -> Result<Version, String> {
    let version = tag_version(tag)?;
    if !is_test_tag(&version) {
        return Ok(version);
    }
    let raw = workspace_package(cargo_toml, "version")?;
    let workspace = Version::parse(&raw)
        .map_err(|error| format!("version du workspace {raw} : SemVer invalide ({error})"))?;
    let core = |v: &Version| (v.major, v.minor, v.patch);
    if core(&workspace) == core(&version) {
        Ok(workspace)
    } else {
        Err(format!(
            "tag d'essai {tag} : noyau {}.{}.{} ≠ version du workspace {workspace}",
            version.major, version.minor, version.patch
        ))
    }
}

/// Valeur d'une clé de `[workspace.package]` du `Cargo.toml` racine.
fn workspace_package(cargo_toml: &str, key: &str) -> Result<String, String> {
    let table: toml::Table =
        toml::from_str(cargo_toml).map_err(|error| format!("Cargo.toml illisible : {error}"))?;
    table
        .get("workspace")
        .and_then(|w| w.get("package"))
        .and_then(|p| p.get(key))
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("Cargo.toml : [workspace.package] {key} absent"))
}

/// C-REL-01 : rend la version si tag, workspace et CHANGELOG concordent,
/// sinon la liste des écarts (tous, pas seulement le premier).
fn release_check(tag: &str, cargo_toml: &str, changelog: &str) -> Result<Version, Vec<String>> {
    let mut errors = Vec::new();
    let version = match checked_version(tag, cargo_toml) {
        Ok(version) => Some(version),
        Err(error) => {
            errors.push(error);
            None
        }
    };
    let expected = version.as_ref().map(ToString::to_string);
    match workspace_package(cargo_toml, "version") {
        Ok(workspace) => {
            if let Some(expected) = &expected
                && &workspace != expected
            {
                errors.push(format!(
                    "version du workspace {workspace} ≠ tag {tag} (attendu {expected})"
                ));
            }
        }
        Err(error) => errors.push(error),
    }
    let all = sections(changelog);
    let mut released = all.iter();
    let mut first = released.next();
    if let Some(head) = first
        && head.is_unreleased()
    {
        if !head.is_blank() {
            errors.push(format!(
                "CHANGELOG.md : section « {} » non vide ; la renommer en [{}] avant le tag",
                head.label,
                expected.as_deref().unwrap_or("X.Y.Z")
            ));
        }
        first = released.next();
    }
    match first {
        None => errors.push("CHANGELOG.md : aucune section de version (## [X.Y.Z] - …)".to_owned()),
        Some(section) => {
            if let Some(expected) = &expected
                && &section.label != expected
            {
                errors.push(format!(
                    "CHANGELOG.md : première section [{}] ≠ tag {tag} (attendu [{expected}])",
                    section.label
                ));
            }
            if section.is_blank() {
                errors.push(format!("CHANGELOG.md : section [{}] vide", section.label));
            }
        }
    }
    match version {
        Some(version) if errors.is_empty() => Ok(version),
        _ => Err(errors),
    }
}

/// Notes de la release du tag : corps de sa section et lien vers le guide
/// d'installation à ce tag.
fn release_notes(tag: &str, cargo_toml: &str, changelog: &str) -> Result<String, String> {
    let version = checked_version(tag, cargo_toml)?.to_string();
    let repository = workspace_package(cargo_toml, "repository")?;
    let section = sections(changelog)
        .into_iter()
        .find(|section| section.label == version)
        .ok_or_else(|| format!("CHANGELOG.md : section [{version}] absente"))?;
    if section.is_blank() {
        return Err(format!("CHANGELOG.md : section [{version}] vide"));
    }
    let repository = repository.trim_end_matches('/');
    Ok(format!(
        "{}\n\n---\n\nInstallation (macOS, Windows, Linux, web) et vérification des empreintes : \
         {repository}/blob/{tag}/docs/user/install.md\n",
        section.body
    ))
}

/// `Cargo.toml` et `CHANGELOG.md` de la racine du dépôt.
fn read_inputs() -> Result<(String, String), String> {
    let root = repo_root();
    let read = |name: &str| {
        std::fs::read_to_string(root.join(name)).map_err(|error| format!("{name} illisible : {error}"))
    };
    Ok((read("Cargo.toml")?, read("CHANGELOG.md")?))
}

/// `cargo xtask release-check <tag>`.
pub fn run_check(args: &[String]) -> ExitCode {
    let [tag] = args else {
        eprintln!("usage : cargo xtask release-check <vX.Y.Z[-pré]>");
        return ExitCode::FAILURE;
    };
    let (cargo_toml, changelog) = match read_inputs() {
        Ok(inputs) => inputs,
        Err(error) => {
            eprintln!("xtask release-check : {error}");
            return ExitCode::FAILURE;
        }
    };
    match release_check(tag, &cargo_toml, &changelog) {
        Ok(version) => {
            println!("xtask release-check : OK ({tag} = workspace {version} = CHANGELOG [{version}])");
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in errors {
                eprintln!("xtask release-check : {error}");
            }
            ExitCode::FAILURE
        }
    }
}

/// `cargo xtask release-notes <tag>`.
pub fn run_notes(args: &[String]) -> ExitCode {
    let [tag] = args else {
        eprintln!("usage : cargo xtask release-notes <vX.Y.Z[-pré]>");
        return ExitCode::FAILURE;
    };
    let notes = read_inputs().and_then(|(cargo_toml, changelog)| release_notes(tag, &cargo_toml, &changelog));
    match notes {
        Ok(notes) => {
            print!("{notes}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("xtask release-notes : {error}");
            ExitCode::FAILURE
        }
    }
}

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]

    use super::*;

    const CARGO: &str = r#"
[workspace]
members = []

[workspace.package]
version = "1.0.0-rc.1"
repository = "https://github.com/exemple/plannercraft"
"#;

    const CHANGELOG: &str = "# Journal des modifications

## [Non publié]

## [1.0.0-rc.1] - 2026-11-02

### Ajouté
- Export PDF.

## [0.1.0] - J1 à J7

### Ajouté
- Moteur.

[1.0.0-rc.1]: https://github.com/exemple/plannercraft/releases/tag/v1.0.0-rc.1
";

    fn errors(tag: &str, cargo: &str, changelog: &str) -> Vec<String> {
        release_check(tag, cargo, changelog).unwrap_err()
    }

    #[test]
    fn release_check_accepts_tag_version_changelog() {
        let version = release_check("v1.0.0-rc.1", CARGO, CHANGELOG).unwrap();
        assert_eq!(version.to_string(), "1.0.0-rc.1");
        // Sans section « Non publié » : même verdict.
        let without = CHANGELOG.replace("## [Non publié]\n\n", "");
        assert!(release_check("v1.0.0-rc.1", CARGO, &without).is_ok());
    }

    /// Tag d'essai (L16) : vérifie la version du workspace qu'il prépare ;
    /// noyau différent refusé ; notes de la version du workspace.
    #[test]
    fn release_check_test_tag_checks_workspace_version() {
        let version = release_check("v1.0.0-test.1", CARGO, CHANGELOG).unwrap();
        assert_eq!(version.to_string(), "1.0.0-rc.1");
        let found = errors("v1.0.1-test.1", CARGO, CHANGELOG);
        assert!(
            found
                .iter()
                .any(|e| e.contains("noyau 1.0.1 ≠ version du workspace 1.0.0-rc.1")),
            "{found:?}"
        );
        let notes = release_notes("v1.0.0-test.2", CARGO, CHANGELOG).unwrap();
        assert!(notes.starts_with("### Ajouté\n- Export PDF."), "{notes}");
        assert!(
            notes.contains("/blob/v1.0.0-test.2/docs/user/install.md"),
            "{notes}"
        );
        // Une préversion ordinaire reste comparée telle quelle.
        let found = errors("v1.0.0-rc.2", CARGO, CHANGELOG);
        assert!(
            found
                .iter()
                .any(|e| e.contains("version du workspace 1.0.0-rc.1")),
            "{found:?}"
        );
    }

    #[test]
    fn release_check_rejects_tag_not_matching_workspace() {
        let found = errors("v1.0.0", CARGO, CHANGELOG);
        assert!(
            found
                .iter()
                .any(|e| e.contains("version du workspace 1.0.0-rc.1")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|e| e.contains("première section [1.0.0-rc.1]")),
            "{found:?}"
        );
    }

    #[test]
    fn release_check_rejects_changelog_not_matching() {
        let cargo = CARGO.replace("1.0.0-rc.1", "0.1.0");
        let found = errors("v0.1.0", &cargo, CHANGELOG);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("première section [1.0.0-rc.1] ≠ tag v0.1.0"),
            "{found:?}"
        );
    }

    #[test]
    fn release_check_rejects_unreleased_with_content() {
        let changelog = CHANGELOG.replace("## [Non publié]\n", "## [Non publié]\n\n- Oubli.\n");
        let found = errors("v1.0.0-rc.1", CARGO, &changelog);
        assert!(found[0].contains("« Non publié » non vide"), "{found:?}");
        let english = CHANGELOG.replace("## [Non publié]\n", "## [Unreleased]\n- x\n");
        assert_eq!(errors("v1.0.0-rc.1", CARGO, &english).len(), 1);
    }

    #[test]
    fn release_check_rejects_malformed_tags() {
        for tag in ["1.0.0-rc.1", "v1.0", "vX", "v1.0.0+build.5", "ui-preview-1", ""] {
            let found = errors(tag, CARGO, CHANGELOG);
            assert!(found.iter().any(|e| e.contains("tag")), "{tag} : {found:?}");
        }
    }

    #[test]
    fn release_check_rejects_empty_or_missing_sections() {
        let empty = "# Journal\n\n## [1.0.0-rc.1] - 2026-11-02\n\n## [0.1.0]\n- x\n";
        let found = errors("v1.0.0-rc.1", CARGO, empty);
        assert!(found[0].contains("section [1.0.0-rc.1] vide"), "{found:?}");
        let none = "# Journal\n\n## [Non publié]\n";
        let found = errors("v1.0.0-rc.1", CARGO, none);
        assert!(found[0].contains("aucune section de version"), "{found:?}");
        let found = errors("v1.0.0-rc.1", "[package]\nname = \"x\"\n", CHANGELOG);
        assert!(
            found[0].contains("[workspace.package] version absent"),
            "{found:?}"
        );
    }

    #[test]
    fn release_notes_extracts_section() {
        let notes = release_notes("v1.0.0-rc.1", CARGO, CHANGELOG).unwrap();
        assert!(notes.starts_with("### Ajouté\n- Export PDF.\n\n---\n"), "{notes}");
        assert!(!notes.contains("Moteur"), "{notes}");
        assert!(
            !notes.contains("releases/tag"),
            "référence de lien incluse : {notes}"
        );
        assert!(
            notes.contains("https://github.com/exemple/plannercraft/blob/v1.0.0-rc.1/docs/user/install.md"),
            "{notes}"
        );
        // Section plus ancienne : trouvée aussi ; section absente : erreur.
        assert!(
            release_notes("v0.1.0", CARGO, CHANGELOG)
                .unwrap()
                .contains("Moteur")
        );
        assert!(
            release_notes("v2.0.0", CARGO, CHANGELOG)
                .unwrap_err()
                .contains("[2.0.0] absente")
        );
    }

    /// Le CHANGELOG du dépôt se lit : une section « Non publié » en tête,
    /// puis une première version égale à celle du workspace.
    #[test]
    fn release_check_repo_changelog_structure() {
        let (cargo_toml, changelog) = read_inputs().unwrap();
        let all = sections(&changelog);
        assert!(all[0].is_unreleased(), "{:?}", all[0].label);
        let workspace = workspace_package(&cargo_toml, "version").unwrap();
        assert_eq!(all[1].label, workspace);
        assert!(!all[1].is_blank());
        let notes = release_notes(&format!("v{workspace}"), &cargo_toml, &changelog).unwrap();
        assert!(notes.contains("docs/user/install.md"), "{notes}");
    }
}
