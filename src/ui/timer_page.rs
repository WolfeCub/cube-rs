//! The timer page: the scramble, the time, the last solve and the stats.

use super::sessions_page::SessionsPage;
use super::solves_page::SolvesPage;
use super::widgets::{Chevron, button, chip, cog, page_frame};
use super::{App, BG, BOTTOM_INSET, DIM, GREEN, MUTED, Page, RED, TEXT, TOP_INSET};
use crate::session::{Penalty, Solve, Time};
use crate::sys;
use crate::timer::Timer;
use eframe::egui::{self, Align2, Color32, FontId, Frame, Image, RichText, Sense, Stroke, Vec2};

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

impl App {
    pub(super) fn timer_page(&mut self, ui: &mut egui::Ui) {
        // Only the timer is shown from the first touch until the solve stops.
        if self.timer.idle() {
            egui::Panel::top("scramble")
                .frame(page_frame())
                .show_separator_line(false)
                .show(ui, |ui| self.scramble(ui));
            egui::Panel::bottom("stats")
                .frame(page_frame())
                .show_separator_line(false)
                .show(ui, |ui| {
                    self.last_solve(ui);
                    self.summary(ui);
                });
            self.corners(&ui.ctx().clone());
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
            let now = sys::boottime();
            if self.settings.slam && self.timer.elapsed_ms(now).is_some() {
                let armed = self.timer.slam_armed(now);
                let color = if armed { GREEN } else { DIM };
                ui.painter()
                    .circle_filled(center + Vec2::new(0.0, 72.0), 6.0, color);
            }
        });
        self.stop_questions(&ui.ctx().clone());
    }

    /// Takes the next scramble once the last one is used.
    pub(super) fn next_scramble(&mut self, ctx: &egui::Context) {
        if self.scramble.is_none()
            && let Ok(scramble) = self.scrambles.try_recv()
        {
            ctx.forget_image(SCRAMBLE_URI);
            self.scramble = Some(scramble);
        }
    }

    pub(super) fn record(&mut self, solve: Option<u64>) {
        if let Some(ms) = solve {
            let scramble = self.scramble.take().map(|s| s.moves).unwrap_or_default();
            self.sessions.current_mut().push(Solve::new(ms, scramble));
            self.last_shown = true;
            self.ask_stop = true;
        }
    }

    /// The last solve, while it's on screen.
    fn shown(&self) -> Option<&Solve> {
        let last = self.sessions.current().solves().last();
        last.filter(|_| self.last_shown)
    }

    fn touch(&mut self, down: bool) {
        // A solve can't start until its scramble is ready.
        if down && self.timer.idle() && self.scramble.is_none() {
            return;
        }
        if down {
            let solve = self.timer.press(sys::boottime());
            self.record(solve);
        } else {
            self.timer.release(sys::boottime(), self.settings.hold_ns());
        }
    }

    fn timer_text(&self) -> (String, Color32) {
        let now = sys::boottime();
        let time = if self.timer.idle() {
            self.shown().map(Solve::time)
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
        ui.add_space(TOP_INSET + 12.0);
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
        let Some(current) = self.shown().map(|s| s.penalty) else {
            return;
        };
        let penalties = [
            (Penalty::None, "OK"),
            (Penalty::PlusTwo, "+2"),
            (Penalty::Dnf, "DNF"),
        ];
        let (mut chosen, mut discard) = (current, false);
        ui.columns(4, |cols| {
            for (ui, (penalty, label)) in cols.iter_mut().zip(penalties) {
                if button(ui, label, current == penalty) {
                    chosen = penalty;
                }
            }
            discard = button(&mut cols[3], "Discard", false);
        });
        let session = self.sessions.current_mut();
        if discard {
            session.pop();
            self.last_shown = false;
        } else if chosen != current {
            session.set_penalty(chosen);
        }
        ui.add_space(24.0);
    }

    /// The scrambled cube next to the stats.
    fn summary(&mut self, ui: &mut egui::Ui) {
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
            ui.vertical(|ui| {
                self.stats(ui);
                self.solves_chip(ui);
            });
        });
        ui.add_space(BOTTOM_INSET);
    }

    fn stats(&self, ui: &mut egui::Ui) {
        let s = self.sessions.current();
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
    }

    /// The number of solves, opening the solves page.
    fn solves_chip(&mut self, ui: &mut egui::Ui) {
        let n = self.sessions.current().solves().len();
        if n == 0 {
            return;
        }
        ui.add_space(8.0);
        let label = if n == 1 {
            "1 solve".into()
        } else {
            format!("{n} solves")
        };
        let solves = ui.vertical_centered(|ui| chip(ui, &label, Chevron::Right));
        if solves.inner.clicked() {
            self.page = Page::Solves(SolvesPage::default());
        }
    }

    /// The session's name in the top left corner, opening the sessions page, and a cog in the top
    /// right, opening the settings. They sit in the status bar's space above the scramble,
    /// floating over the panels so the layout doesn't move.
    fn corners(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let left = screen.left_top() + Vec2::new(16.0, TOP_INSET / 2.0);
        egui::Area::new(egui::Id::unique("sessions"))
            .pivot(Align2::LEFT_CENTER)
            .fixed_pos(left)
            .show(ctx, |ui| {
                if chip(ui, self.sessions.name(), Chevron::Down).clicked() {
                    self.page = Page::Sessions(SessionsPage::default());
                }
            });

        let right = screen.right_top() + Vec2::new(-24.0, TOP_INSET / 2.0);
        egui::Area::new(egui::Id::unique("cog"))
            .pivot(Align2::CENTER_CENTER)
            .fixed_pos(right)
            .show(ctx, |ui| {
                if cog(ui).clicked() {
                    self.page = Page::Settings;
                }
            });
    }
}
