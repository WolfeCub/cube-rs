mod sessions_page;
mod settings_page;
mod solves_page;
mod timer_page;
mod widgets;

use crate::sensors::Sensors;
use crate::sessions::Sessions;
use crate::settings::Settings;
use crate::timer::Timer;
use crate::tnoodle::{Drawer, Scramble};
use crate::trace::Tracer;
use eframe::egui::{self, Color32, Theme};
use sessions_page::SessionsPage;
use solves_page::SolvesPage;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

const BG: Color32 = Color32::from_rgb(0x12, 0x12, 0x14);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xea);
const MUTED: Color32 = Color32::from_rgb(0x80, 0x80, 0x88);
const RED: Color32 = Color32::from_rgb(0xef, 0x6b, 0x6b);
const GREEN: Color32 = Color32::from_rgb(0x5c, 0xd6, 0x8a);
const DIM: Color32 = Color32::from_gray(0x40);
const CHIP: Color32 = Color32::from_gray(0x26);
/// Space for the status and navigation bars (edge-to-edge on Android 15+).
const TOP_INSET: f32 = 48.0;
const BOTTOM_INSET: f32 = 32.0;

pub struct App {
    timer: Timer,
    sessions: Sessions,
    scrambles: Receiver<Scramble>,
    scramble: Option<Scramble>,
    drawer: Drawer,
    sensors: Sensors,
    down: bool,
    /// The last solve is on screen, so it can be penalised or discarded.
    last_shown: bool,
    tracer: Tracer,
    /// The stop questions haven't been answered for the last solve.
    ask_stop: bool,
    settings: Settings,
    settings_path: PathBuf,
    page: Page,
}

/// What's on screen. Back goes from the others to the timer.
enum Page {
    Timer,
    Settings,
    Sessions(SessionsPage),
    Solves(SolvesPage),
}

impl App {
    /// Settings and sessions are saved in `data`.
    pub fn new(
        ctx: &egui::Context,
        scrambles: Receiver<Scramble>,
        drawer: Drawer,
        sensors: Sensors,
        tracer: Tracer,
        data: PathBuf,
    ) -> Self {
        egui_extras::install_image_loaders(ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.override_text_color = Some(TEXT);
        visuals.selection.bg_fill = Color32::from_gray(0x3a);
        ctx.set_theme(Theme::Dark);
        ctx.set_visuals_of(Theme::Dark, visuals);

        let settings_path = data.join("settings.json");
        let settings = Settings::load(&settings_path);
        let app = Self {
            timer: Timer::default(),
            sessions: Sessions::open(data.join("sessions"), &settings.session),
            scrambles,
            scramble: None,
            drawer,
            sensors,
            down: false,
            last_shown: false,
            tracer,
            ask_stop: false,
            settings,
            settings_path,
            page: Page::Timer,
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

    /// Applies and saves changed settings. A changed session hides the last solve, which may be
    /// another session's.
    fn settings_changed(&mut self, before: &Settings) {
        if self.settings.session != self.sessions.name() {
            self.settings.session = self.sessions.name().into();
            self.last_shown = false;
        }
        if self.settings != *before {
            self.apply_settings();
            self.settings.save(&self.settings_path);
        }
    }

    /// Back: to the timer from another page. Does nothing on the timer.
    fn back(&mut self, ctx: &egui::Context) {
        if !matches!(self.page, Page::Timer) {
            self.page = Page::Timer;
            // Frees the solves page's scramble images.
            ctx.forget_all_images();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.next_scramble(ui.ctx());

        let used = self.settings.slam || self.settings.tracing;
        self.sensors.set_active(used && ui.input(|i| i.focused));
        while let Some(slam) = self.sensors.slam() {
            if self.settings.slam {
                let solve = self.timer.slam(slam);
                self.record(solve);
            }
        }

        if ui.input(|i| i.key_pressed(egui::Key::BrowserBack)) {
            self.back(ui.ctx());
        }
        let before = self.settings.clone();
        let panel = egui::CentralPanel::default().frame(widgets::page_frame());
        match &mut self.page {
            Page::Timer => self.timer_page(ui),
            Page::Settings => {
                panel.show(ui, |ui| settings_page::show(ui, &mut self.settings));
            }
            Page::Sessions(page) => {
                panel.show(ui, |ui| page.show(ui, &mut self.sessions));
            }
            Page::Solves(page) => {
                panel.show(ui, |ui| page.show(ui, &self.sessions, &self.drawer));
            }
        }
        self.settings_changed(&before);

        self.tracer.timer(self.timer);

        if !self.timer.idle() || self.scramble.is_none() {
            ui.ctx().request_repaint();
        }
    }
}
