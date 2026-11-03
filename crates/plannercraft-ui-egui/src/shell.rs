//! Enveloppe eframe : seul type qui implémente `eframe::App`.
//!
//! Lit l'état d'interface au démarrage et l'écrit à la sauvegarde, en JSON
//! (clé [`STATE_KEY`], à défaut l'ancienne clé de `planrs` :
//! [`crate::state::LEGACY_STATE_KEY`]). La mémoire interne d'egui n'est jamais persistée :
//! notre état est la seule source. Le stockage n'existe que si l'application
//! active la feature `persistence` d'eframe (desktop et web).
//!
//! Les entrées brutes passent par [`sanitize_raw_input`] : un pilote qui
//! émet une molette, un zoom ou une position non finis ferait échouer une
//! assertion d'egui avant tout code PlannerCraft (relecture A, M1).

use crate::app::{App, Env};
use crate::state::{STATE_KEY, UiState, stored_state};

/// Application eframe de PlannerCraft, commune au desktop et au web.
pub struct PlannerCraftApp {
    /// Application pure.
    app: App,
}

impl PlannerCraftApp {
    /// Crée l'application : état relu depuis le stockage d'eframe s'il
    /// existe (lecture tolérante, avertissement affiché une fois).
    #[must_use]
    pub fn new(cc: &eframe::CreationContext<'_>, env: Env) -> Self {
        let stored = cc
            .storage
            .and_then(|storage| stored_state(|key| storage.get_string(key)));
        let (state, warning) = stored.map_or_else(
            || (UiState::default(), None),
            |json| UiState::from_json_lenient(&json),
        );
        let mut app = App::new(state, env);
        if let Some(warning) = warning {
            app.report_state_load(&warning);
        }
        Self { app }
    }

    /// Application pure enveloppée.
    #[must_use]
    pub const fn app(&self) -> &App {
        &self.app
    }

    /// Application pure enveloppée, modifiable (observateur du document).
    pub const fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }
}

/// Retire des entrées brutes tout événement de pointeur, de molette, de zoom
/// ou de rotation portant une composante non finie (NaN, infini).
pub fn sanitize_raw_input(raw: &mut egui::RawInput) {
    raw.events.retain(|event| match event {
        egui::Event::PointerMoved(p) => p.is_finite(),
        egui::Event::MouseMoved(v) => v.is_finite(),
        egui::Event::MouseWheel { delta, .. } => delta.is_finite(),
        egui::Event::Zoom(factor) | egui::Event::Rotate(factor) => factor.is_finite(),
        egui::Event::Touch { pos, force, .. } => pos.is_finite() && force.is_none_or(f32::is_finite),
        _ => true,
    });
}

impl eframe::App for PlannerCraftApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        sanitize_raw_input(raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::ui(ui, &mut self.app);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        match self.app.state.to_json() {
            Ok(json) => storage.set_string(STATE_KEY, json),
            Err(error) => log::warn!("état d'interface non sauvegardé : {error}"),
        }
    }

    fn persist_egui_memory(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "tests : un échec doit s'arrêter net et lisiblement"
)]
mod tests {
    use egui::{Event, Modifiers, MouseWheelSource, MouseWheelUnit, RawInput, TouchPhase, pos2, vec2};

    use super::sanitize_raw_input;

    fn wheel(x: f32, y: f32) -> Event {
        Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: vec2(x, y),
            phase: TouchPhase::Move,
            source: MouseWheelSource::Wheel,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn sanitize_drops_non_finite_events() {
        let mut raw = RawInput {
            events: vec![
                wheel(f32::NAN, 0.0),
                wheel(0.0, -40.0),
                wheel(f32::INFINITY, 0.0),
                Event::Zoom(f32::NAN),
                Event::Zoom(1.1),
                Event::PointerMoved(pos2(f32::NEG_INFINITY, 3.0)),
                Event::PointerMoved(pos2(10.0, 3.0)),
                Event::MouseMoved(vec2(f32::NAN, 0.0)),
                Event::Rotate(f32::INFINITY),
            ],
            ..RawInput::default()
        };
        sanitize_raw_input(&mut raw);
        assert_eq!(raw.events.len(), 3, "{:?}", raw.events);
        assert_eq!(raw.events[0], wheel(0.0, -40.0));
        assert_eq!(raw.events[1], Event::Zoom(1.1));
        assert_eq!(raw.events[2], Event::PointerMoved(pos2(10.0, 3.0)));
    }

    /// Une molette non finie filtrée ne fait plus échouer egui.
    #[test]
    fn sanitized_nan_wheel_runs_frame() {
        let ctx = egui::Context::default();
        let mut raw = RawInput {
            events: vec![
                Event::PointerMoved(pos2(10.0, 10.0)),
                wheel(f32::NAN, f32::INFINITY),
            ],
            ..RawInput::default()
        };
        sanitize_raw_input(&mut raw);
        let mut output = ctx.run_ui(raw, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| ui.add_space(1.0));
        });
        output.textures_delta.clear();
    }
}
