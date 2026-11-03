//! `cargo xtask panics` : interdits de la règle 10 que clippy ne couvre pas.
//!
//! Clippy refuse `unwrap()`, `panic!`, l'indexation… (voir
//! `[workspace.lints.clippy]`), mais un `#[allow(clippy::unwrap_used)]` les
//! lève localement, et aucune lint ne refuse `assert!`. Ce contrôle parcourt
//! l'arbre de modules de chaque cible livrée (bibliothèques et binaires des
//! membres du workspace, hors [`EXEMPT_PACKAGES`]) et refuse, hors du code de
//! test :
//! - un `allow`, `expect` ou `cfg_attr` qui nomme une lint de
//!   [`GUARDED_LINTS`] ;
//! - un appel de [`ASSERT_MACROS`].
//!
//! Est du code de test : un élément sous `#[cfg(test)]` (ou `cfg(all(test,
//! …))`), une fonction `#[test]` (ou `#[…::test]`), un fichier de module
//! déclaré par un `mod` sous `#[cfg(test)]`. Les dossiers `tests/`,
//! `benches/` et `examples/` ne sont pas parcourus (cibles non livrées).
//!
//! Limites : les appels écrits dans le corps d'une autre macro (`macro_rules!`,
//! arguments de `format!`…) ne sont pas analysés ; un `#[path]` dans un module
//! en ligne est résolu de façon simplifiée. Un module introuvable fait échouer
//! le contrôle plutôt que de passer en silence.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cargo_metadata::{CargoOpt, MetadataCommand};
use proc_macro2::{TokenStream, TokenTree};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Attribute, ImplItem, Item, ItemMod, Macro, Meta, Token, TraitItem};

/// Paquets du workspace exemptés : outils de test et de maintenance, jamais
/// livrés (`plannercraft-tests` est entièrement du code de test).
const EXEMPT_PACKAGES: &[&str] = &["plannercraft-tests", "xtask"];

/// Lints (et groupes qui les contiennent) qu'aucun `allow` ne doit lever hors
/// des tests.
const GUARDED_LINTS: &[&str] = &[
    // Interdits de la règle 10 (`[workspace.lints.clippy]`).
    "unwrap_used",
    "expect_used",
    "panic",
    "unreachable",
    "todo",
    "unimplemented",
    "indexing_slicing",
    "string_slice",
    "arithmetic_side_effects",
    // Gardes de plannercraft-core (attributs de crate et `clippy.toml`).
    "float_arithmetic",
    "disallowed_methods",
    "disallowed_types",
    "print_stdout",
    "print_stderr",
    "dbg_macro",
    // Groupes qui contiennent les lints ci-dessus.
    "restriction",
    "style",
    "all",
];

/// Macros qui paniquent quand leur condition est fausse.
const ASSERT_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
];

pub fn run() -> ExitCode {
    let metadata = match MetadataCommand::new()
        .features(CargoOpt::AllFeatures)
        .no_deps()
        .exec()
    {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("xtask panics : `cargo metadata` a échoué : {error}");
            return ExitCode::FAILURE;
        }
    };
    let root = metadata.workspace_root.as_std_path();
    let mut roots = Vec::new();
    for package in metadata.workspace_packages() {
        if EXEMPT_PACKAGES.contains(&package.name.as_str()) {
            continue;
        }
        for target in &package.targets {
            let shipped =
                !(target.is_test() || target.is_bench() || target.is_example() || target.is_custom_build());
            if shipped {
                roots.push(target.src_path.clone().into_std_path_buf());
            }
        }
    }

    match check_crates(&roots) {
        Ok(report) if report.findings.is_empty() => {
            println!(
                "xtask panics : OK ({} fichiers vérifiés, exemptés : {})",
                report.files,
                EXEMPT_PACKAGES.join(", ")
            );
            ExitCode::SUCCESS
        }
        Ok(report) => {
            for finding in &report.findings {
                let file = finding.file.strip_prefix(root).unwrap_or(&finding.file);
                eprintln!(
                    "xtask panics : {}:{} : {}",
                    file.display(),
                    finding.line,
                    finding.message
                );
            }
            eprintln!(
                "xtask panics : {} violation(s) de la règle 10 hors des tests",
                report.findings.len()
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("xtask panics : {error}");
            ExitCode::FAILURE
        }
    }
}

/// Résultat du contrôle sur un ensemble de crates.
#[derive(Debug, Default)]
struct Report {
    files: usize,
    findings: Vec<Finding>,
}

/// Violation trouvée dans un fichier source.
#[derive(Debug, PartialEq, Eq)]
struct Finding {
    file: PathBuf,
    line: usize,
    message: String,
}

/// Fichier de module à analyser, avec le dossier où chercher ses sous-modules.
#[derive(Debug, PartialEq, Eq)]
struct ModuleFile {
    file: PathBuf,
    module_dir: PathBuf,
}

/// Parcourt l'arbre de modules de chaque racine de crate.
fn check_crates(roots: &[PathBuf]) -> Result<Report, String> {
    let mut report = Report::default();
    let mut seen = BTreeSet::new();
    let mut pending: Vec<ModuleFile> = roots
        .iter()
        .map(|file| ModuleFile {
            file: file.clone(),
            module_dir: file.parent().map(Path::to_path_buf).unwrap_or_default(),
        })
        .collect();
    while let Some(module) = pending.pop() {
        if !seen.insert(module.file.clone()) {
            continue;
        }
        let source = std::fs::read_to_string(&module.file)
            .map_err(|error| format!("{} : lecture impossible : {error}", module.file.display()))?;
        let scan = scan_source(&source, &module.file, &module.module_dir)
            .map_err(|error| format!("{} : analyse impossible : {error}", module.file.display()))?;
        report.files = report.files.saturating_add(1);
        report.findings.extend(scan.findings);
        for child in scan.children {
            pending.push(resolve_child(child)?);
        }
    }
    report
        .findings
        .sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    Ok(report)
}

/// Sous-module déclaré par `mod nom;`, avant résolution sur le disque.
#[derive(Debug, PartialEq, Eq)]
enum Child {
    /// `mod nom;` : `<dossier>/nom.rs` ou `<dossier>/nom/mod.rs`.
    Named { dir: PathBuf, name: String },
    /// `#[path = "…"] mod nom;` : chemin relatif au dossier donné.
    Path { file: PathBuf },
}

fn resolve_child(child: Child) -> Result<ModuleFile, String> {
    match child {
        Child::Named { dir, name } => {
            let flat = dir.join(format!("{name}.rs"));
            let nested = dir.join(&name).join("mod.rs");
            let module_dir = dir.join(&name);
            if flat.is_file() {
                Ok(ModuleFile {
                    file: flat,
                    module_dir,
                })
            } else if nested.is_file() {
                Ok(ModuleFile {
                    file: nested,
                    module_dir,
                })
            } else {
                Err(format!(
                    "module `{name}` introuvable ({} ou {})",
                    flat.display(),
                    nested.display()
                ))
            }
        }
        Child::Path { file } => {
            if file.is_file() {
                let module_dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
                Ok(ModuleFile { file, module_dir })
            } else {
                Err(format!("module introuvable : {}", file.display()))
            }
        }
    }
}

/// Résultat de l'analyse d'un fichier.
#[derive(Debug, Default)]
struct Scan {
    findings: Vec<Finding>,
    children: Vec<Child>,
}

/// Analyse un fichier source : violations et sous-modules hors test.
///
/// `module_dir` est le dossier des sous-modules de ce fichier (`src/` pour
/// `lib.rs`, `src/a/` pour `src/a.rs`).
fn scan_source(source: &str, file: &Path, module_dir: &Path) -> Result<Scan, syn::Error> {
    let ast = syn::parse_file(source)?;
    let mut scanner = Scanner {
        file,
        file_dir: file.parent().map(Path::to_path_buf).unwrap_or_default(),
        dirs: vec![module_dir.to_path_buf()],
        scan: Scan::default(),
    };
    if !is_test_only(&ast.attrs) {
        scanner.visit_file(&ast);
    }
    Ok(scanner.scan)
}

struct Scanner<'a> {
    file: &'a Path,
    /// Dossier du fichier, base des `#[path]` de premier niveau.
    file_dir: PathBuf,
    /// Pile des dossiers de sous-modules (un niveau par module en ligne).
    dirs: Vec<PathBuf>,
    scan: Scan,
}

impl Scanner<'_> {
    fn report(&mut self, line: usize, message: String) {
        self.scan.findings.push(Finding {
            file: self.file.to_path_buf(),
            line,
            message,
        });
    }

    fn current_dir(&self) -> PathBuf {
        self.dirs.last().cloned().unwrap_or_default()
    }
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !item_attrs(item).is_some_and(|attrs| is_test_only(attrs)) {
            visit::visit_item(self, item);
        }
    }

    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        let attrs = match item {
            ImplItem::Const(item) => &item.attrs,
            ImplItem::Fn(item) => &item.attrs,
            ImplItem::Type(item) => &item.attrs,
            ImplItem::Macro(item) => &item.attrs,
            _ => return visit::visit_impl_item(self, item),
        };
        if !is_test_only(attrs) {
            visit::visit_impl_item(self, item);
        }
    }

    fn visit_trait_item(&mut self, item: &'ast TraitItem) {
        let attrs = match item {
            TraitItem::Const(item) => &item.attrs,
            TraitItem::Fn(item) => &item.attrs,
            TraitItem::Type(item) => &item.attrs,
            TraitItem::Macro(item) => &item.attrs,
            _ => return visit::visit_trait_item(self, item),
        };
        if !is_test_only(attrs) {
            visit::visit_trait_item(self, item);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        let name = item.ident.to_string();
        let path_attr = item.attrs.iter().find_map(path_attribute);
        if item.content.is_none() {
            // `mod nom;` : le fichier sera analysé à son tour.
            for attr in &item.attrs {
                self.visit_attribute(attr);
            }
            let child = match path_attr {
                Some(path) => {
                    // Premier niveau : relatif au dossier du fichier ; dans un
                    // module en ligne : relatif au dossier de ce module.
                    let base = if self.dirs.len() > 1 {
                        self.current_dir()
                    } else {
                        self.file_dir.clone()
                    };
                    Child::Path {
                        file: base.join(path),
                    }
                }
                None => Child::Named {
                    dir: self.current_dir(),
                    name,
                },
            };
            self.scan.children.push(child);
            return;
        }
        let dir = match path_attr {
            Some(path) => self.current_dir().join(path),
            None => self.current_dir().join(&name),
        };
        self.dirs.push(dir);
        visit::visit_item_mod(self, item);
        self.dirs.pop();
    }

    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        let Some(kind) = attr
            .path()
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
        else {
            return;
        };
        if !matches!(kind.as_str(), "allow" | "expect" | "cfg_attr") {
            return;
        }
        let Meta::List(list) = &attr.meta else {
            return;
        };
        for lint in clippy_lints(list.tokens.clone()) {
            if GUARDED_LINTS.contains(&lint.as_str()) {
                let line = attr.pound_token.span.start().line;
                self.report(
                    line,
                    format!("`{kind}(clippy::{lint})` lève un interdit de la règle 10 hors des tests"),
                );
            }
        }
    }

    fn visit_macro(&mut self, mac: &'ast Macro) {
        if let Some(segment) = mac.path.segments.last() {
            let name = segment.ident.to_string();
            if ASSERT_MACROS.contains(&name.as_str()) {
                let line = segment.ident.span().start().line;
                self.report(
                    line,
                    format!("`{name}!` panique si sa condition est fausse : renvoyer une erreur (règle 10)"),
                );
            }
        }
        visit::visit_macro(self, mac);
    }
}

/// Attributs d'un élément, quand il en a.
fn item_attrs(item: &Item) -> Option<&Vec<Attribute>> {
    Some(match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => return None,
    })
}

/// Vrai si les attributs réservent l'élément aux tests : `#[cfg(test)]`,
/// `#[cfg(all(test, …))]`, `#[test]` ou `#[…::test]`.
fn is_test_only(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();
        if path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "test")
        {
            return true;
        }
        if !path.is_ident("cfg") {
            return false;
        }
        attr.parse_args::<Meta>()
            .is_ok_and(|meta| cfg_requires_test(&meta))
    })
}

/// Vrai si le prédicat `cfg` n'est vrai que sous `test`.
fn cfg_requires_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if list.path.is_ident("all") => list
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .is_ok_and(|items| items.iter().any(cfg_requires_test)),
        _ => false,
    }
}

/// Valeur d'un attribut `#[path = "…"]`.
fn path_attribute(attr: &Attribute) -> Option<PathBuf> {
    if !attr.path().is_ident("path") {
        return None;
    }
    let Meta::NameValue(name_value) = &attr.meta else {
        return None;
    };
    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(value),
        ..
    }) = &name_value.value
    else {
        return None;
    };
    Some(PathBuf::from(value.value()))
}

/// Noms `xxx` des `clippy::xxx` d'un flux de jetons, groupes compris. Les
/// littéraux (dont `reason = "…"`) sont ignorés.
fn clippy_lints(tokens: TokenStream) -> Vec<String> {
    let mut flat = Vec::new();
    flatten(tokens, &mut flat);
    flat.windows(4)
        .filter_map(|window| match window {
            [
                TokenTree::Ident(tool),
                TokenTree::Punct(first),
                TokenTree::Punct(second),
                TokenTree::Ident(lint),
            ] if tool == "clippy" && first.as_char() == ':' && second.as_char() == ':' => {
                Some(lint.to_string())
            }
            _ => None,
        })
        .collect()
}

fn flatten(tokens: TokenStream, out: &mut Vec<TokenTree>) {
    for token in tokens {
        match token {
            TokenTree::Group(group) => flatten(group.stream(), out),
            other => out.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        reason = "tests : les interdits de la règle 10 ne s'appliquent pas, un échec doit paniquer"
    )]

    use std::path::{Path, PathBuf};

    use super::{Child, scan_source};

    /// Lignes signalées dans une source analysée comme `src/lib.rs`.
    fn lines(source: &str) -> Vec<usize> {
        scan_source(source, Path::new("src/lib.rs"), Path::new("src"))
            .unwrap()
            .findings
            .iter()
            .map(|finding| finding.line)
            .collect()
    }

    #[test]
    fn guarded_allow_outside_tests_is_reported() {
        let source = r#"
fn f() {}
#[allow(clippy::unwrap_used, reason = "non")]
fn g() {}
#[expect(clippy::indexing_slicing)]
fn h() {}
#[cfg_attr(feature = "x", allow(clippy::panic))]
fn i() {}
#[allow(clippy::restriction)]
fn j() {}
"#;
        assert_eq!(lines(source), vec![3, 5, 7, 9]);
    }

    #[test]
    fn inner_allow_of_the_crate_is_reported() {
        assert_eq!(lines("#![allow(clippy::arithmetic_side_effects)]\n"), vec![1]);
    }

    #[test]
    fn harmless_allow_and_lint_names_in_strings_are_ignored() {
        let source = r#"
#[allow(clippy::too_many_lines, reason = "clippy::unwrap_used dans une chaîne")]
#[allow(clippy::panic_in_result_fn, reason = "nom proche, autre lint")]
fn f() {}
"#;
        assert_eq!(lines(source), Vec::<usize>::new());
    }

    #[test]
    fn assert_macros_outside_tests_are_reported() {
        let source = "
fn f(x: u8) {
    assert!(x > 0);
    debug_assert_eq!(x, 1);
    std::assert_ne!(x, 2);
    println!(\"{x}\");
}
";
        assert_eq!(lines(source), vec![3, 4, 5]);
    }

    #[test]
    fn test_code_is_skipped() {
        let source = r#"
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]
    #[test]
    fn t() { assert!(true); }
}
#[test]
fn t() { assert_eq!(1, 1); }
#[cfg(all(test, unix))]
fn helper() { assert!(true); }
struct S;
impl S {
    #[cfg(test)]
    fn helper() { assert!(true); }
}
"#;
        assert_eq!(lines(source), Vec::<usize>::new());
    }

    #[test]
    fn non_test_cfg_is_checked() {
        let source = "
#[cfg(not(test))]
fn f() { assert!(true); }
#[cfg(any(test, unix))]
fn g() { assert!(true); }
";
        assert_eq!(lines(source), vec![3, 5]);
    }

    #[test]
    fn whole_test_file_is_skipped() {
        assert_eq!(
            lines("#![cfg(test)]\nfn f() { assert!(true); }\n"),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn child_modules_are_listed_except_test_ones() {
        let source = r#"
mod model;
#[cfg(test)]
mod tests;
mod inline {
    mod nested;
}
#[path = "other/place.rs"]
mod moved;
"#;
        let scan = scan_source(source, Path::new("src/lib.rs"), Path::new("src")).unwrap();
        assert_eq!(
            scan.children,
            vec![
                Child::Named {
                    dir: PathBuf::from("src"),
                    name: "model".to_owned()
                },
                Child::Named {
                    dir: PathBuf::from("src/inline"),
                    name: "nested".to_owned()
                },
                Child::Path {
                    file: PathBuf::from("src/other/place.rs")
                },
            ]
        );
    }
}
