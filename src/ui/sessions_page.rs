//! The sessions page: open, add, rename and delete sessions.

use super::MUTED;
use super::widgets::{button, title};
use crate::sessions::Sessions;
use eframe::egui::{self, Button, RichText, ScrollArea, TextEdit, Vec2};
use std::mem;

/// The sessions page's state while it's open.
#[derive(Default)]
pub struct SessionsPage {
    /// A name for a new or renamed session.
    name: String,
    /// Delete was tapped once, and asks to be tapped again.
    confirm_delete: bool,
}

impl SessionsPage {
    pub fn show(&mut self, ui: &mut egui::Ui, sessions: &mut Sessions) {
        title(ui, "Sessions");
        ui.add_space(24.0);

        // At the top, so the keyboard doesn't cover it.
        ui.label(RichText::new("Name for a new or renamed session").color(MUTED));
        ui.add(TextEdit::singleline(&mut self.name).desired_width(f32::INFINITY));
        ui.add_space(8.0);
        let typed = self.name.trim().to_owned();
        let available = sessions.available(&typed);
        let deletable = sessions.names().len() > 1;
        let delete = if self.confirm_delete {
            "Sure?"
        } else {
            "Delete"
        };
        let mut tap = None;
        ui.columns(3, |cols| {
            if enabled_button(&mut cols[0], available, "New") {
                tap = Some(Tap::New);
            }
            if enabled_button(&mut cols[1], available, "Rename") {
                tap = Some(Tap::Rename);
            }
            if enabled_button(&mut cols[2], deletable, delete) {
                tap = Some(Tap::Delete);
            }
        });

        ui.add_space(16.0);
        ScrollArea::vertical().show(ui, |ui| {
            for name in sessions.names() {
                let size = Vec2::new(ui.available_width(), 36.0);
                let row = Button::selectable(name == sessions.name(), name).min_size(size);
                if ui.add(row).clicked() {
                    tap = Some(Tap::Open(name.clone()));
                }
            }
        });

        let Some(tap) = tap else { return };
        // Any other tap cancels a delete.
        let confirmed = mem::take(&mut self.confirm_delete);
        match tap {
            Tap::New => {
                sessions.switch(&typed);
                self.name.clear();
            }
            Tap::Rename => {
                sessions.rename(&typed);
                self.name.clear();
            }
            Tap::Delete if confirmed => sessions.delete(),
            Tap::Delete => self.confirm_delete = true,
            Tap::Open(name) => sessions.switch(&name),
        }
    }
}

enum Tap {
    New,
    Rename,
    Delete,
    Open(String),
}

fn enabled_button(ui: &mut egui::Ui, enabled: bool, label: &str) -> bool {
    ui.add_enabled_ui(enabled, |ui| button(ui, label, false))
        .inner
}
