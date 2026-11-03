//! Accès typé aux champs d'un élément retenu : premier gagnant, doublon
//! signalé, valeur illisible signalée et rendue absente (l'appelant applique
//! le défaut).

use std::borrow::Cow;

use plannercraft_core::{Moment, Subject, TimeOfDay};

use crate::diag::{ImportCode, Sink};
use crate::scalar::{
    DateTimeError, IsoDuration, parse_bool, parse_datetime, parse_int, parse_iso_duration, parse_time,
};
use crate::xml::Element;

/// Champs d'un élément, indexés par nom local.
pub(crate) struct Fields<'e> {
    /// Enfants triés par nom, ordre du document conservé entre homonymes
    /// (tri stable) : recherche par dichotomie, le premier gagne.
    items: Vec<&'e Element<'e>>,
    /// Chemin de l'élément porteur (« Tasks/Task »), vide pour `Project`.
    path: Cow<'static, str>,
    /// Élément du projet concerné.
    subject: Option<Subject>,
    /// Position de l'élément porteur (diagnostic d'un champ absent).
    at: u64,
}

impl<'e> Fields<'e> {
    pub(crate) fn new(
        children: impl IntoIterator<Item = &'e Element<'e>>,
        path: impl Into<Cow<'static, str>>,
        subject: Option<Subject>,
        at: u64,
    ) -> Self {
        let mut items: Vec<&'e Element<'e>> = children.into_iter().collect();
        items.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            items,
            path: path.into(),
            subject,
            at,
        }
    }

    /// Précise l'élément du projet concerné (connu après lecture de l'UID).
    pub(crate) const fn set_subject(&mut self, subject: Option<Subject>) {
        self.subject = subject;
    }

    /// Élément du projet concerné.
    pub(crate) const fn subject(&self) -> Option<Subject> {
        self.subject
    }

    /// Chemin d'un champ (« Tasks/Task/UID »).
    pub(crate) fn path_of(&self, name: &str) -> String {
        if self.path.is_empty() {
            name.to_owned()
        } else {
            format!("{}/{name}", self.path)
        }
    }

    /// Position de l'élément porteur.
    pub(crate) const fn at(&self) -> u64 {
        self.at
    }

    /// Premier élément de ce nom ; un doublon est signalé (le premier gagne).
    pub(crate) fn get(&self, name: &str, sink: &mut Sink<'_>) -> Option<&'e Element<'e>> {
        let i = self.items.partition_point(|e| e.name.as_ref() < name);
        let e = *self.items.get(i).filter(|e| e.name == name)?;
        let duplicated = self
            .items
            .get(i.saturating_add(1))
            .is_some_and(|next| next.name == name);
        if duplicated {
            sink.push(
                ImportCode::Duplicate,
                self.subject,
                e.at,
                &self.path_of(name),
                None,
            );
        }
        Some(e)
    }

    /// Vrai si le champ est présent (même vide).
    pub(crate) fn has(&self, name: &str) -> bool {
        let i = self.items.partition_point(|e| e.name.as_ref() < name);
        self.items.get(i).is_some_and(|e| e.name == name)
    }

    /// Texte brut d'un champ ; `None` si absent.
    pub(crate) fn raw(&self, name: &str, sink: &mut Sink<'_>) -> Option<&'e str> {
        self.get(name, sink).map(|e| e.text.as_ref())
    }

    /// Texte rogné d'un champ scalaire ; un élément vide vaut absent.
    pub(crate) fn scalar(&self, name: &str, sink: &mut Sink<'_>) -> Option<(&'e str, u64)> {
        let e = self.get(name, sink)?;
        let t = e.text.trim();
        (!t.is_empty()).then_some((t, e.at))
    }

    /// Signale une valeur illisible.
    pub(crate) fn invalid(&self, name: &str, at: u64, value: &str, sink: &mut Sink<'_>) {
        sink.push(
            ImportCode::InvalidValue,
            self.subject,
            at,
            &self.path_of(name),
            Some(value),
        );
    }

    /// Signale un champ, avec un code donné.
    pub(crate) fn report(
        &self,
        code: ImportCode,
        name: &str,
        at: u64,
        value: Option<&str>,
        sink: &mut Sink<'_>,
    ) {
        sink.push(code, self.subject, at, &self.path_of(name), value);
    }

    /// Signale un champ obligatoire absent.
    pub(crate) fn missing(&self, name: &str, sink: &mut Sink<'_>) {
        sink.push(
            ImportCode::MissingRequired,
            self.subject,
            self.at,
            &self.path_of(name),
            None,
        );
    }

    /// Booléen (R-076) ; absent → `None` ; illisible → diagnostic, `None`.
    pub(crate) fn bool(&self, name: &str, sink: &mut Sink<'_>) -> Option<bool> {
        let (t, at) = self.scalar(name, sink)?;
        let v = parse_bool(t);
        if v.is_none() {
            self.invalid(name, at, t, sink);
        }
        v
    }

    /// Entier ; absent → `None` ; illisible → diagnostic, `None`.
    pub(crate) fn int(&self, name: &str, sink: &mut Sink<'_>) -> Option<(i64, u64)> {
        let (t, at) = self.scalar(name, sink)?;
        let v = parse_int(t);
        if v.is_none() {
            self.invalid(name, at, t, sink);
        }
        v.map(|v| (v, at))
    }

    /// Date (R-075) ; secondes tronquées et fuseau ignoré signalés ;
    /// illisible ou hors domaine → diagnostic, `None`.
    pub(crate) fn datetime(&self, name: &str, sink: &mut Sink<'_>) -> Option<Moment> {
        let (t, at) = self.scalar(name, sink)?;
        match parse_datetime(t) {
            Ok(p) => {
                if p.rounded {
                    self.report(ImportCode::Rounded, name, at, Some(t), sink);
                }
                if p.zone_ignored {
                    self.report(ImportCode::Inconsistent, name, at, Some(t), sink);
                }
                Some(p.moment)
            }
            Err(DateTimeError::Syntax | DateTimeError::OutOfDomain) => {
                self.invalid(name, at, t, sink);
                None
            }
        }
    }

    /// Heure `hh:mm:ss` ; secondes tronquées signalées.
    pub(crate) fn time(&self, name: &str, sink: &mut Sink<'_>) -> Option<TimeOfDay> {
        let (t, at) = self.scalar(name, sink)?;
        let Some((v, rounded)) = parse_time(t) else {
            self.invalid(name, at, t, sink);
            return None;
        };
        if rounded {
            self.report(ImportCode::Rounded, name, at, Some(t), sink);
        }
        Some(v)
    }

    /// Durée ISO 8601 (R-070), signée ; arrondi signalé ; illisible →
    /// diagnostic, `None`.
    pub(crate) fn duration(&self, name: &str, sink: &mut Sink<'_>) -> Option<(IsoDuration, u64)> {
        let (t, at) = self.scalar(name, sink)?;
        let Ok(d) = parse_iso_duration(t) else {
            self.invalid(name, at, t, sink);
            return None;
        };
        if d.rounded {
            self.report(ImportCode::Rounded, name, at, Some(t), sink);
        }
        Some((d, at))
    }
}
