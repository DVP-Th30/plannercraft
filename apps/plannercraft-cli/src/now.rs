//! « Maintenant » et langue de la CLI : seule lecture de l'horloge et de
//! l'environnement (règle 5 : le reste du code reçoit ces valeurs).
//!
//! Priorité de « maintenant » : `--now` > `PLANNERCRAFT_NOW` > horloge locale.
//! Priorité de la langue : `--lang` > `LANG` (`fr…` ou `en…`) > français.

use plannercraft_automation::text::Language;
use plannercraft_core::Moment;

/// Variable d'environnement qui fixe « maintenant » (scripts, tests).
pub const NOW_ENV: &str = "PLANNERCRAFT_NOW";

/// « Maintenant » d'après l'option, la variable et l'horloge.
///
/// # Errors
///
/// Message d'usage si l'option ou la variable n'est pas une date
/// `AAAA-MM-JJTHH:MM` valide.
pub fn resolve_now(flag: Option<&str>) -> Result<Moment, String> {
    resolve_now_with(flag, std::env::var(NOW_ENV).ok().as_deref(), clock)
}

/// Même règle, sources injectées (tests unitaires).
fn resolve_now_with(
    flag: Option<&str>,
    env: Option<&str>,
    clock: impl FnOnce() -> Moment,
) -> Result<Moment, String> {
    if let Some(s) = flag {
        return s.parse().map_err(|e| format!("--now « {s} » : {e}"));
    }
    match env.map(str::trim) {
        Some(s) if !s.is_empty() => s.parse().map_err(|e| format!("{NOW_ENV} « {s} » : {e}")),
        _ => Ok(clock()),
    }
}

/// Horloge locale tronquée à la minute ; hors du domaine des dates du cœur,
/// repli sur un instant fixe, signalé (même règle que l'application
/// desktop).
fn clock() -> Moment {
    match Moment::from_naive_floor(chrono::Local::now().naive_local()) {
        Ok((moment, _truncated)) => moment,
        Err(error) => {
            crate::cmd::warn(&format!(
                "date système inutilisable ({error}), repli sur {}",
                Moment::MIN
            ));
            Moment::MIN
        }
    }
}

/// Langue d'après la variable `LANG` (`fr_FR.UTF-8` → français,
/// `en_US.UTF-8` → anglais), français à défaut.
pub fn env_language() -> Language {
    language_from_env(std::env::var("LANG").ok().as_deref())
}

fn language_from_env(lang: Option<&str>) -> Language {
    match lang.map(str::to_ascii_lowercase) {
        Some(l) if l.starts_with("en") => Language::En,
        _ => Language::Fr,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, reason = "tests")]

    use super::*;

    fn m(s: &str) -> Moment {
        s.parse().unwrap()
    }

    #[test]
    fn now_flag_beats_env_beats_clock() {
        let clock = || m("2030-01-01T08:00");
        let got = resolve_now_with(Some("2026-10-19T08:00"), Some("2027-01-04T08:00"), clock);
        assert_eq!(got, Ok(m("2026-10-19T08:00")));
        let got = resolve_now_with(None, Some("2027-01-04T08:00"), clock);
        assert_eq!(got, Ok(m("2027-01-04T08:00")));
        assert_eq!(resolve_now_with(None, None, clock), Ok(m("2030-01-01T08:00")));
        assert_eq!(
            resolve_now_with(None, Some("  "), clock),
            Ok(m("2030-01-01T08:00"))
        );
    }

    #[test]
    fn now_invalid_is_an_error() {
        let clock = || Moment::MIN;
        assert!(resolve_now_with(Some("demain"), None, clock).is_err());
        assert!(resolve_now_with(None, Some("2026-13-01T08:00"), clock).is_err());
    }

    #[test]
    fn language_from_lang_variable() {
        assert_eq!(language_from_env(Some("en_US.UTF-8")), Language::En);
        assert_eq!(language_from_env(Some("fr_FR.UTF-8")), Language::Fr);
        assert_eq!(language_from_env(Some("C")), Language::Fr);
        assert_eq!(language_from_env(None), Language::Fr);
    }
}
