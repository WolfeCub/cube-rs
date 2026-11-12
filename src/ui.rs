use crate::sensors::{Sensors, now};
use crate::session::{Penalty, Session, Solve, Time};
use crate::timer::Timer;
use crate::tnoodle::Scramble;
use eframe::egui::{
    self, Align2, Button, Color32, FontId, Frame, Image, RichText, Sense, Theme, Vec2,
};
use std::sync::mpsc::Receiver;

const BG: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xea);
const MUTED: Color32 = Color32::from_rgb(0x80, 0x80, 0x88);
const RED: Color32 = Color32::from_rgb(0xef, 0x6b, 0x6b);
const GREEN: Color32 = Color32::from_rgb(0x5c, 0xd6, 0x8a);
const DIM: Color32 = Color32::from_gray(0x40);
/// Space for the status and navigation bars (edge-to-edge on Android 15+).
const TOP_INSET: f32 = 48.0;
const BOTTOM_INSET: f32 = 32.0;
const SCRAMBLE_URI: &str = "bytes://scramble.svg";
const NET_SIZE: Vec2 = Vec2::new(128.0, 96.0);

pub struct App {
    timer: Timer,
    session: Session,
    scrambles: Receiver<Scramble>,
    scramble: Option<Scramble>,
    sensors: Sensors,
    down: bool,
    /// The last solve is on screen, so it can be penalised or discarded.
    last_shown: bool,
}

impl App {
    pub fn new(ctx: &egui::Context, scrambles: Receiver<Scramble>, sensors: Sensors) -> Self {
        egui_extras::install_image_loaders(ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.override_text_color = Some(TEXT);
        visuals.selection.bg_fill = Color32::from_gray(0x3a);
        ctx.set_theme(Theme::Dark);
        ctx.set_visuals_of(Theme::Dark, visuals);

        Self {
            timer: Timer::default(),
            session: Session::default(),
            scrambles,
            scramble: None,
            sensors,
            down: false,
            last_shown: false,
        }
    }

    fn record(&mut self, solve: Option<u64>) {
        if let Some(ms) = solve {
            let scramble = self.scramble.take().map(|s| s.moves).unwrap_or_default();
            self.session.solves.push(Solve {
                ms,
                penalty: Penalty::None,
                scramble,
            });
            self.last_shown = true;
        }
    }

    fn touch(&mut self, down: bool) {
        if down {
            let solve = self.timer.press(now());
            self.record(solve);
        } else {
            self.timer.release(now());
        }
    }

    fn timer_text(&self) -> (String, Color32) {
        let now = now();
        let last = self.session.solves.last().filter(|_| self.last_shown);
        let time = if self.timer.idle() {
            last.map(Solve::time)
        } else {
            self.timer.elapsed_ms(now).map(Time::Ms)
        };
        let color = match self.timer {
            Timer::Holding { .. } if self.timer.ready(now) => GREEN,
            Timer::Holding { .. } => RED,
            _ => TEXT,
        };
        (time.map_or("0.00".into(), |t| t.to_string()), color)
    }

    fn scramble(&self, ui: &mut egui::Ui) {
        ui.add_space(TOP_INSET);
        let (text, color) = match &self.scramble {
            Some(scramble) => (scramble.moves.as_str(), TEXT),
            None => ("Scrambling...", MUTED),
        };
        ui.vertical_centered(|ui| ui.label(RichText::new(text).size(22.0).color(color)));
    }

    /// Penalty and discard buttons for the last solve.
    fn last_solve(&mut self, ui: &mut egui::Ui) {
        let Some(last) = self.session.solves.last_mut().filter(|_| self.last_shown) else {
            return;
        };
        let penalties = [
            (Penalty::None, "OK"),
            (Penalty::PlusTwo, "+2"),
            (Penalty::Dnf, "DNF"),
        ];
        let mut discard = false;
        ui.columns(4, |cols| {
            for (ui, (penalty, label)) in cols.iter_mut().zip(penalties) {
                if button(ui, label, last.penalty == penalty) {
                    last.penalty = penalty;
                }
            }
            discard = button(&mut cols[3], "Discard", false);
        });
        if discard {
            self.session.solves.pop();
            self.last_shown = false;
        }
        ui.add_space(24.0);
    }

    /// The scrambled cube next to the stats.
    fn summary(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Fixed size: by default an image fills the available space, which starts at zero in a
            // self-sizing panel.
            match &self.scramble {
                Some(scramble) => {
                    let image = Image::from_bytes(SCRAMBLE_URI, scramble.svg.clone().into_bytes());
                    ui.add(image.fit_to_exact_size(NET_SIZE));
                }
                None => {
                    ui.allocate_space(NET_SIZE);
                }
            }
            ui.vertical(|ui| self.stats(ui));
        });
        ui.add_space(BOTTOM_INSET);
    }

    fn stats(&self, ui: &mut egui::Ui) {
        let s = &self.session;
        let stats = [
            ("best", s.best()),
            ("ao5", s.average(5)),
            ("ao12", s.average(12)),
        ];
        ui.columns(3, |cols| {
            for (ui, (label, time)) in cols.iter_mut().zip(stats) {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new(label).size(14.0).color(MUTED));
                    ui.label(RichText::new(time.map_or("-".into(), |t| t.to_string())).size(22.0));
                });
            }
        });

        ui.add_space(8.0);
        let recent: Vec<String> = s
            .solves
            .iter()
            .rev()
            .take(5)
            .map(|s| s.time().to_string())
            .collect();
        ui.vertical_centered(|ui| ui.label(RichText::new(recent.join("   ")).color(MUTED)));
    }
}

fn button(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let button = Button::selectable(selected, label).min_size(Vec2::new(72.0, 36.0));
    ui.vertical_centered(|ui| ui.add(button).clicked()).inner
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        if self.scramble.is_none()
            && let Ok(scramble) = self.scrambles.try_recv()
        {
            ui.ctx().forget_image(SCRAMBLE_URI);
            self.scramble = Some(scramble);
        }

        self.sensors.set_active(ui.input(|i| i.focused));
        while let Some(gesture) = self.sensors.gesture() {
            let solve = self.timer.gesture(gesture);
            self.record(solve);
        }

        // Only the timer is shown from the first touch until the solve stops.
        if self.timer.idle() {
            let frame = Frame::NONE
                .fill(BG)
                .inner_margin(egui::Margin::symmetric(24, 0));
            egui::Panel::top("scramble")
                .frame(frame)
                .show_separator_line(false)
                .show(ui, |ui| self.scramble(ui));
            egui::Panel::bottom("stats")
                .frame(frame)
                .show_separator_line(false)
                .show(ui, |ui| {
                    self.last_solve(ui);
                    self.summary(ui);
                });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            // Drag only: egui turns a click-sensing long press into a release after 0.8 s.
            let pad = ui.interact(ui.max_rect(), ui.scope_id().with("pad"), Sense::drag());
            let down = pad.is_pointer_button_down_on();
            if down != self.down {
                self.down = down;
                self.touch(down);
            }
            // Screen center, so the time doesn't move when the panels hide.
            let (text, color) = self.timer_text();
            let center = ui.ctx().content_rect().center();
            ui.painter().text(
                center,
                Align2::CENTER_CENTER,
                text,
                FontId::monospace(80.0),
                color,
            );
            // Lit while a slam would stop the timer.
            let now = now();
            if self.timer.elapsed_ms(now).is_some() {
                let armed = self.timer.slam_armed(now, self.sensors.steady_since());
                let color = if armed { GREEN } else { DIM };
                ui.painter()
                    .circle_filled(center + Vec2::new(0.0, 72.0), 6.0, color);
            }
        });

        if !self.timer.idle() || self.scramble.is_none() {
            ui.ctx().request_repaint();
        }
    }
}
