//! `cargo xtask text-ids` : chaque texte de l'interface est traduit et
//! utilisé (ADR-0016 D14).
//!
//! Lit la liste `text_ids! { Variante => "clé", … }` de
//! `crates/plannercraft-ui-egui/src/i18n.rs` et échoue si :
//! - une clé manque dans `locales/fr.ftl` ou `locales/en.ftl` (ou si un
//!   message de ces fichiers n'est pas dans la liste) ;
//! - une variante n'est écrite `TextId::Variante` nulle part dans le code
//!   livré de l'interface (`crates/plannercraft-ui-egui/src`, `apps/plannercraft/src`,
//!   `apps/plannercraft-web/src`), hors de la liste elle-même.
//!
//! Exception : les clés `error-*` sont lues par leur code de refus
//! (`error_text_id`, clé construite à l'exécution) ; leur couverture est
//! vérifiée par les tests de `i18n.rs`. De même, les clés `diag-<code>`
//! (diagnostics et erreurs d'ouverture traduits par code, N5 et N9, J8
//! lot L14) sont lues par `diag_text.rs` ; le contrôle exige en plus qu'à
//! chaque code des fonctions `code()` du moteur et du lecteur MSPDI
//! ([`DIAG_SOURCES`]) corresponde sa clé (un code nouveau sans message
//! échoue ici, avant le repli sur le texte français à l'exécution).
//!
//! Limites : recherche textuelle (un `TextId::X` dans un commentaire ou un
//! module de test du code livré compte comme un usage) ; un alias
//! (`use TextId as T`) n'est pas suivi : le code du dépôt n'en a pas.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Liste des identifiants, relative à la racine du dépôt.
const I18N: &str = "crates/plannercraft-ui-egui/src/i18n.rs";
/// Fichiers de traduction, relatifs à la racine.
const LOCALES: [&str; 2] = [
    "crates/plannercraft-ui-egui/locales/fr.ftl",
    "crates/plannercraft-ui-egui/locales/en.ftl",
];
/// Code livré parcouru, relatif à la racine.
const SOURCE_DIRS: [&str; 3] = [
    "crates/plannercraft-ui-egui/src",
    "apps/plannercraft/src",
    "apps/plannercraft-web/src",
];
/// Préfixe des clés lues par leur code de refus.
const DYNAMIC_PREFIX: &str = "error-";
/// Fichiers dont la fonction `code()` énumère des codes de diagnostic
/// traduits par une clé `diag-<code>` (relatifs à la racine).
const DIAG_SOURCES: [&str; 3] = [
    "crates/plannercraft-core/src/diagnostic.rs",
    "crates/plannercraft-mspdi/src/diag.rs",
    "crates/plannercraft-mspdi/src/error.rs",
];

pub fn run() -> ExitCode {
    match check(&repo_root()) {
        Ok(count) => {
            println!(
                "xtask text-ids : OK ({count} textes, traduits en français et en anglais, tous utilisés)"
            );
            ExitCode::SUCCESS
        }
        Err(problems) => {
            for p in &problems {
                eprintln!("xtask text-ids : {p}");
            }
            eprintln!("xtask text-ids : {} problème(s)", problems.len());
            ExitCode::FAILURE
        }
    }
}

/// Vérifie le dépôt de racine `root` ; rend le nombre de textes.
fn check(root: &Path) -> Result<usize, Vec<String>> {
    let i18n =
        std::fs::read_to_string(root.join(I18N)).map_err(|e| vec![format!("{I18N} illisible : {e}")])?;
    let (ids, outside) =
        parse_text_ids(&i18n).ok_or_else(|| vec![format!("{I18N} : liste `text_ids!` introuvable")])?;
    let mut problems = Vec::new();
    let keys: BTreeSet<&str> = ids.iter().map(|(_, k)| k.as_str()).collect();
    if keys.len() != ids.len() {
        problems.push("clé en double dans `text_ids!`".to_owned());
    }
    for locale in LOCALES {
        let text = match std::fs::read_to_string(root.join(locale)) {
            Ok(t) => t,
            Err(e) => {
                problems.push(format!("{locale} illisible : {e}"));
                continue;
            }
        };
        let declared = ftl_keys(&text);
        for key in keys.difference(&declared) {
            problems.push(format!("{locale} : clé « {key} » manquante"));
        }
        for key in declared.difference(&keys) {
            problems.push(format!("{locale} : message « {key} » absent de `text_ids!`"));
        }
    }
    let mut diag_keys = BTreeSet::new();
    for source in DIAG_SOURCES {
        match std::fs::read_to_string(root.join(source)) {
            Ok(text) => {
                let codes = diag_codes(&text);
                if codes.is_empty() {
                    problems.push(format!(
                        "{source} : aucun code de diagnostic trouvé dans `fn code`"
                    ));
                }
                diag_keys.extend(
                    codes
                        .iter()
                        .map(|code| format!("diag-{}", code.replace('.', "-"))),
                );
            }
            Err(e) => problems.push(format!("{source} illisible : {e}")),
        }
    }
    for key in &diag_keys {
        if !keys.contains(key.as_str()) {
            problems.push(format!(
                "clé « {key} » absente de `text_ids!` (code de diagnostic sans message)"
            ));
        }
    }
    let mut used = used_variants(&outside);
    for dir in SOURCE_DIRS {
        let mut files = Vec::new();
        if let Err(e) = collect_rs(&root.join(dir), &mut files) {
            problems.push(format!("{dir} : {e}"));
        }
        for file in files {
            if file.ends_with("i18n.rs") && file.starts_with(root.join(SOURCE_DIRS[0])) {
                continue;
            }
            match std::fs::read_to_string(&file) {
                Ok(text) => used.extend(used_variants(&text)),
                Err(e) => problems.push(format!("{} illisible : {e}", file.display())),
            }
        }
    }
    for (variant, key) in &ids {
        if !key.starts_with(DYNAMIC_PREFIX) && !diag_keys.contains(key) && !used.contains(variant) {
            problems.push(format!("`TextId::{variant}` (« {key} ») utilisé nulle part"));
        }
    }
    if problems.is_empty() {
        Ok(ids.len())
    } else {
        Err(problems)
    }
}

/// Paires (variante, clé) de l'invocation `text_ids! { … }` (ligne qui
/// commence par `text_ids! {`, jusqu'à la première `}` en colonne 0), et le
/// texte du fichier hors de cette liste.
fn parse_text_ids(src: &str) -> Option<(Vec<(String, String)>, String)> {
    let mut ids = Vec::new();
    let mut outside = String::new();
    let mut inside = false;
    let mut found = false;
    for line in src.lines() {
        if !inside && line.starts_with("text_ids! {") {
            inside = true;
            found = true;
            continue;
        }
        if inside {
            if line.starts_with('}') {
                inside = false;
                continue;
            }
            if let Some((variant, rest)) = line.trim().split_once("=>") {
                let key = rest.trim().trim_end_matches(',').trim().trim_matches('"');
                ids.push((variant.trim().to_owned(), key.to_owned()));
            }
            continue;
        }
        outside.push_str(line);
        outside.push('\n');
    }
    (found && !inside).then_some((ids, outside))
}

/// Codes de diagnostic (`"calendar.unknown"`, `"mspdi.error.xml"`) écrits
/// en chaînes littérales dans le corps des fonctions `fn code(` d'un
/// fichier (jusqu'à la première ligne `    }` qui suit).
fn diag_codes(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = src;
    while let Some(i) = rest.find("fn code(") {
        let body = rest.get(i..).unwrap_or_default();
        let end = body.find("\n    }\n").unwrap_or(body.len());
        let body_text = body.get(..end).unwrap_or_default();
        for literal in body_text.split('"').skip(1).step_by(2) {
            let parts: Vec<&str> = literal.split('.').collect();
            let well_formed = parts.len() >= 2
                && parts.iter().all(|p| {
                    !p.is_empty()
                        && p.chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                });
            if well_formed {
                out.insert(literal.to_owned());
            }
        }
        rest = body.get(end..).unwrap_or_default();
    }
    out
}

/// Clés des messages d'un fichier fluent (ni termes `-x`, ni attributs,
/// ni commentaires, ni lignes de suite indentées).
fn ftl_keys(text: &str) -> BTreeSet<&str> {
    text.lines()
        .filter_map(|line| {
            let (key, _) = line.split_once(" =")?;
            let first = key.chars().next()?;
            (first.is_ascii_alphabetic()
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .then_some(key)
        })
        .collect()
}

/// Variantes écrites `TextId::Variante` dans `text`.
fn used_variants(text: &str) -> BTreeSet<String> {
    const MARK: &str = "TextId::";
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(i) = rest.find(MARK) {
        rest = rest.get(i.saturating_add(MARK.len())..).unwrap_or_default();
        let ident: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if ident.starts_with(|c: char| c.is_ascii_uppercase()) {
            out.insert(ident);
        }
    }
    out
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_rs(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn repo_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    /// Le dépôt passe le contrôle (chaque texte traduit et utilisé).
    #[test]
    fn text_ids_all_used() {
        let result = check(&repo_root());
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn text_ids_parse_list_keys_and_uses() {
        let src = "macro_rules! text_ids { () => {} }\ntext_ids! {\n    // Section.\n    A => \"a-key\",\n    Bb => \"bb\",\n}\nfn f() { TextId::A; }\n";
        let (ids, outside) = parse_text_ids(src).unwrap();
        assert_eq!(
            ids,
            [
                ("A".to_owned(), "a-key".to_owned()),
                ("Bb".to_owned(), "bb".to_owned())
            ]
        );
        assert!(outside.contains("TextId::A;") && !outside.contains("a-key"));
        assert!(parse_text_ids("text_ids! {\n A => \"a\",\n").is_none());

        let ftl = "# c\na-key = x\n    .attr = y\n-term = z\nbb = { $n ->\n   *[other] n\n}\n";
        assert_eq!(ftl_keys(ftl), BTreeSet::from(["a-key", "bb"]));

        let src = "impl X {\n    pub fn code(&self) -> String {\n        match self {\n            Self::A => \"a.b_c\".to_owned(),\n            Self::C(d) => format!(\"core.{}\", d),\n            Self::D => \"Pas un code\".to_owned(),\n        }\n    }\n    fn other() { \"x.y\" }\n}\n";
        assert_eq!(diag_codes(src), BTreeSet::from(["a.b_c".to_owned()]));

        let used = used_variants("TextId::A TextId::AbC(x) TextId::ALL TextId::lower");
        assert_eq!(
            used,
            BTreeSet::from(["A".to_owned(), "AbC".to_owned(), "ALL".to_owned()])
        );
    }
}
