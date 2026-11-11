use crate::timer::{Penalty, Session, Solve, Time, Timer};
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense};
use std::sync::mpsc::Receiver;
use std::time::Instant;

/// Space for the status and navigation bars (edge-to-edge on Android 15+).
const INSET: f32 = 40.0;

pub struct App {
    timer: Timer,
    session: Session,
    scrambles: Receiver<String>,
    scramble: Option<String>,
    epoch: Instant,
    down: bool,
}

impl App {
    pub fn new(scrambles: Receiver<String>) -> Self {
        Self {
            timer: Timer::default(),
            session: Session::default(),
            scrambles,
            scramble: None,
            epoch: Instant::now(),
            down: false,
        }
    }

    fn now(&self) -> u64 {
        self.epoch.elapsed().as_nanos() as u64
    }

    fn touch(&mut self, down: bool) {
        let now = self.now();
        if !down {
            self.timer.release(now);
        } else if let Some(ms) = self.timer.press(now) {
            let scramble = self.scramble.take().unwrap_or_default();
            self.session.solves.push(Solve {
                ms,
                penalty: Penalty::None,
                scramble,
            });
        }
    }

    fn timer_text(&self) -> (String, Color32) {
        let now = self.now();
        match self.timer {
            Timer::Idle => match self.session.solves.last() {
                Some(solve) => (solve.time().to_string(), Color32::WHITE),
                None => ("0.00".into(), Color32::WHITE),
            },
            Timer::Holding { .. } if self.timer.ready(now) => ("0.00".into(), Color32::GREEN),
            Timer::Holding { .. } => ("0.00".into(), Color32::RED),
            Timer::Running { .. } => (
                Time::Ms(self.timer.elapsed_ms(now).unwrap()).to_string(),
                Color32::WHITE,
            ),
        }
    }

    fn stats(&mut self, ui: &mut egui::Ui) {
        let fmt = |t: Option<Time>| t.map_or("-".into(), |t| t.to_string());
        let s = &self.session;
        ui.label(format!(
            "best {}   ao5 {}   ao12 {}",
            fmt(s.best()),
            fmt(s.average(5)),
            fmt(s.average(12))
        ));

        let recent: Vec<String> = s
            .solves
            .iter()
            .rev()
            .take(12)
            .map(|s| s.time().to_string())
            .collect();
        ui.label(recent.join("  "));

        if let Some(last) = self.session.solves.last_mut() {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut last.penalty, Penalty::None, "OK");
                ui.selectable_value(&mut last.penalty, Penalty::PlusTwo, "+2");
                ui.selectable_value(&mut last.penalty, Penalty::Dnf, "DNF");
            });
        }
        ui.add_space(INSET);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        if self.scramble.is_none() {
            self.scramble = self.scrambles.try_recv().ok();
        }

        // Only the timer is shown while running, so any touch stops it.
        if !matches!(self.timer, Timer::Running { .. }) {
            egui::Panel::top("scramble").show(ui, |ui| {
                ui.add_space(INSET);
                let text = self.scramble.as_deref().unwrap_or("Scrambling...");
                ui.label(RichText::new(text).size(24.0));
            });
            egui::Panel::bottom("stats").show(ui, |ui| self.stats(ui));
        }

        egui::CentralPanel::default().show(ui, |ui| {
            let rect = ui.max_rect();
            let down = ui
                .interact(rect, ui.scope_id().with("pad"), Sense::click_and_drag())
                .is_pointer_button_down_on();
            if down != self.down {
                self.down = down;
                self.touch(down);
            }
            let (text, color) = self.timer_text();
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                FontId::monospace(80.0),
                color,
            );
        });

        if self.timer != Timer::Idle || self.scramble.is_none() {
            ui.ctx().request_repaint();
        }
    }
}
