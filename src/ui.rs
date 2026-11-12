use crate::sensors::{Sensors, now};
use crate::session::{Penalty, Session, Solve, Time};
use crate::settings::{HOLD_MS, SENSITIVITIES, Settings};
use crate::timer::Timer;
use crate::tnoodle::Scramble;
use crate::trace::Tracer;
use eframe::egui::{
    self, Align2, Button, Color32, FontId, Frame, Image, RichText, Sense, Stroke, Theme, Vec2,
};
use std::path::PathBuf;
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
/// Questions after each solve while slam detection is being tuned: a caption, then answers
/// as (saved, shown).
const STOP_QUESTIONS: [(&str, &[(&str, &str)]); 2] = [
    (
        "How did it stop?",
        &[("right", "Right"), ("missed", "Missed slam")],
    ),
    (
        "Stopped early by",
        &[
            ("bump", "Bump"),
            ("buzz", "Buzz"),
            ("nudge", "Nudge"),
            ("other", "Other"),
        ],
    ),
];

pub struct App {
    timer: Timer,
    session: Session,
    scrambles: Receiver<Scramble>,
    scramble: Option<Scramble>,
    sensors: Sensors,
    down: bool,
    /// The last solve is on screen, so it can be penalised or discarded.
    last_shown: bool,
    tracer: Tracer,
    /// `STOP_QUESTIONS` hasn't been answered for the last solve.
    ask_stop: bool,
    settings: Settings,
    /// Where settings are saved, if anywhere.
    settings_path: Option<PathBuf>,
    /// The settings page is open.
    show_settings: bool,
}

impl App {
    pub fn new(
        ctx: &egui::Context,
        scrambles: Receiver<Scramble>,
        sensors: Sensors,
        tracer: Tracer,
        settings_path: Option<PathBuf>,
    ) -> Self {
        egui_extras::install_image_loaders(ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.override_text_color = Some(TEXT);
        visuals.selection.bg_fill = Color32::from_gray(0x3a);
        ctx.set_theme(Theme::Dark);
        ctx.set_visuals_of(Theme::Dark, visuals);

        let app = Self {
            timer: Timer::default(),
            session: Session::default(),
            scrambles,
            scramble: None,
            sensors,
            down: false,
            last_shown: false,
            tracer,
            ask_stop: false,
            settings: settings_path
                .as_deref()
                .map_or_else(Settings::default, Settings::load),
            settings_path,
            show_settings: false,
        };
        app.apply_settings();
        app
    }

    /// Passes the settings that live outside the UI on to the sensors and the tracer.
    fn apply_settings(&self) {
        self.sensors
            .set_slam_vertical(self.settings.slam_vertical());
        self.tracer.set_enabled(self.settings.tracing);
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
            self.ask_stop = true;
        }
    }

    fn touch(&mut self, down: bool) {
        if down {
            let solve = self.timer.press(now());
            self.record(solve);
        } else {
            self.timer.release(now(), self.settings.hold_ns());
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
            Timer::Holding { .. } if self.timer.ready(now, self.settings.hold_ns()) => GREEN,
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

    /// Asks how the last solve stopped, in a box above the time, until answered. The answer is
    /// saved with the solve's trace.
    fn stop_questions(&mut self, ctx: &egui::Context) {
        let asking = self.settings.questions && self.settings.tracing && self.ask_stop;
        if !(asking && self.last_shown && self.timer.idle()) {
            return;
        }
        let screen = ctx.content_rect();
        // Fixed width: columns in a self-sizing area would start at zero.
        let width = (screen.width() - 72.0).min(380.0);
        let frame = Frame::NONE
            .fill(BG)
            .stroke(Stroke::new(1.0, MUTED))
            .corner_radius(8)
            .inner_margin(12);
        egui::Area::new(egui::Id::unique("stop questions"))
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(screen.center() - Vec2::new(0.0, 64.0))
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    ui.set_width(width);
                    for (caption, answers) in STOP_QUESTIONS {
                        ui.vertical_centered(|ui| ui.label(RichText::new(caption).color(MUTED)));
                        ui.columns(answers.len(), |cols| {
                            for (ui, &(answer, label)) in cols.iter_mut().zip(answers) {
                                if button(ui, label, false) {
                                    self.tracer.answer(answer);
                                    self.ask_stop = false;
                                }
                            }
                        });
                        ui.add_space(4.0);
                    }
                });
            });
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

    /// A cog in the top right corner, in the status bar's space above the scramble, that opens
    /// the settings. It floats over the panels so the layout doesn't move.
    fn cog(&mut self, ctx: &egui::Context) {
        let center = ctx.content_rect().right_top() + Vec2::new(-24.0, TOP_INSET / 2.0);
        egui::Area::new(egui::Id::unique("cog"))
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos(center)
            .show(ctx, |ui| {
                let (rect, response) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                let (c, painter) = (rect.center(), ui.painter());
                for i in 0..8 {
                    let tooth = Vec2::angled(i as f32 * std::f32::consts::TAU / 8.0);
                    painter
                        .line_segment([c + 5.0 * tooth, c + 10.0 * tooth], Stroke::new(3.5, MUTED));
                }
                painter.circle_filled(c, 7.5, MUTED);
                painter.circle_filled(c, 3.0, BG);
                if response.clicked() {
                    self.show_settings = true;
                }
            });
    }

    /// The settings page. Changes apply and save right away.
    fn settings(&mut self, ui: &mut egui::Ui) {
        let before = self.settings;
        let s = &mut self.settings;
        let on_off = [(false, "Off"), (true, "On")];
        ui.add_space(TOP_INSET);
        ui.vertical_centered(|ui| ui.label(RichText::new("Settings").size(28.0)));
        ui.add_space(24.0);

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

        ui.add_space(8.0);
        if button(ui, "Done", false) {
            self.show_settings = false;
        }
        if self.settings != before {
            self.apply_settings();
            if let Some(path) = &self.settings_path {
                self.settings.save(path);
            }
        }
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

/// A caption over a row of buttons, the one matching `value` selected. A click sets `value`.
fn choice<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    caption: &str,
    value: &mut T,
    options: &[(T, &str)],
) {
    ui.label(RichText::new(caption).color(MUTED));
    ui.columns(options.len(), |cols| {
        for (ui, &(option, label)) in cols.iter_mut().zip(options) {
            let size = Vec2::new(ui.available_width(), 36.0);
            let button = Button::selectable(*value == option, label).min_size(size);
            if ui.add(button).clicked() {
                *value = option;
            }
        }
    });
    ui.add_space(16.0);
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        if self.scramble.is_none()
            && let Ok(scramble) = self.scrambles.try_recv()
        {
            ui.ctx().forget_image(SCRAMBLE_URI);
            self.scramble = Some(scramble);
        }

        let used = self.settings.slam || self.settings.tracing;
        self.sensors.set_active(used && ui.input(|i| i.focused));
        while let Some(slam) = self.sensors.slam() {
            if self.settings.slam {
                let solve = self.timer.slam(slam);
                self.record(solve);
            }
        }

        if self.show_settings {
            let frame = Frame::NONE
                .fill(BG)
                .inner_margin(egui::Margin::symmetric(24, 0));
            egui::CentralPanel::default()
                .frame(frame)
                .show(ui, |ui| self.settings(ui));
        } else {
            self.timer_screen(ui);
        }

        self.tracer.timer(self.timer);

        if !self.timer.idle() || self.scramble.is_none() {
            ui.ctx().request_repaint();
        }
    }
}

impl App {
    fn timer_screen(&mut self, ui: &mut egui::Ui) {
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
            self.cog(&ui.ctx().clone());
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
            if self.settings.slam && self.timer.elapsed_ms(now).is_some() {
                let armed = self.timer.slam_armed(now);
                let color = if armed { GREEN } else { DIM };
                ui.painter()
                    .circle_filled(center + Vec2::new(0.0, 72.0), 6.0, color);
            }
        });
        self.stop_questions(&ui.ctx().clone());
    }
}
