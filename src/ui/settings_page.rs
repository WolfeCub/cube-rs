//! The settings page.

use super::widgets::{choice, title};
use crate::settings::{HOLD_MS, SENSITIVITIES, Settings};
use eframe::egui;

pub fn show(ui: &mut egui::Ui, s: &mut Settings) {
    let on_off = [(false, "Off"), (true, "On")];
    title(ui, "Settings");
    ui.add_space(24.0);

    choice(ui, "15 s inspection", &mut s.inspection, &on_off);
    choice(ui, "Slam to stop", &mut s.slam, &on_off);
    ui.add_enabled_ui(s.slam, |ui| {
        let caption = format!("Slam sensitivity: {}", SENSITIVITIES[s.sensitivity].0);
        let labels = ["1", "2", "3", "4", "5"];
        let notches: Vec<_> = labels.into_iter().enumerate().collect();
        choice(ui, &caption, &mut s.sensitivity, &notches);
    });
    let holds = HOLD_MS.map(|ms| (ms, format!("{} s", ms as f32 / 1000.0)));
    let holds: Vec<_> = holds.iter().map(|(ms, l)| (*ms, l.as_str())).collect();
    choice(ui, "Hold to start", &mut s.hold_ms, &holds);
    choice(ui, "Save a trace of each solve", &mut s.tracing, &on_off);
    ui.add_enabled_ui(s.tracing, |ui| {
        choice(ui, "Ask how each solve stopped", &mut s.questions, &on_off);
    });
}
