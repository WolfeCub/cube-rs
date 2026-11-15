//! The stats page: the open session's averages, current and best, and a graph of its times.

use super::widgets::{stats, title};
use super::{DIM, GREEN, MUTED, TEXT};
use crate::session::{Penalty, Session, Time};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, RichText, ScrollArea, Sense, Stroke};
use eframe::egui::{pos2, vec2};
use std::iter;

/// The WCA averages listed, by size, each once there are enough solves.
const AVERAGES: [usize; 5] = [5, 12, 50, 100, 1000];
/// The WCA averages drawn as lines on the graph, with their colors.
const LINES: [(usize, Color32); 2] = [(5, TEXT), (12, GREEN)];
const GRAPH_HEIGHT: f32 = 240.0;
/// Room left of the graph for its time labels.
const LABEL_WIDTH: f32 = 48.0;

/// The stats, worked out when the page opens: the session can't change while it's open.
pub struct StatsPage {
    solves: usize,
    dnfs: usize,
    mean: Option<Time>,
    /// A name, the current time and the best, for singles and each WCA average.
    rows: Vec<(String, Time, Time)>,
    /// Each solve's time in ms, `None` for DNFs.
    singles: Vec<Option<u64>>,
    /// Each `LINES` average at each solve, `None` before there are enough solves and for DNFs.
    lines: Vec<Vec<Option<u64>>>,
}

impl StatsPage {
    pub fn new(session: &Session) -> Self {
        let solves = session.solves();
        let singles: Vec<Option<u64>> = solves.iter().map(|s| s.time().ms()).collect();
        let mut rows = Vec::new();
        if let (Some(last), Some(best)) = (solves.last(), session.best()) {
            rows.push(("single".into(), last.time(), best));
        }
        rows.extend(AVERAGES.iter().filter_map(|&n| {
            let row = (session.wca_average(n)?, session.best_wca_average(n)?);
            Some((format!("ao{n}"), row.0, row.1))
        }));
        let lines = LINES
            .iter()
            .map(|&(n, _)| {
                let averages = session.wca_averages(n).map(Time::ms);
                let line = iter::repeat_n(None, n - 1).chain(averages);
                line.take(solves.len()).collect()
            })
            .collect();
        Self {
            solves: solves.len(),
            dnfs: solves.iter().filter(|s| s.penalty == Penalty::Dnf).count(),
            mean: session.mean(),
            rows,
            singles,
            lines,
        }
    }

    pub fn show(&self, ui: &mut egui::Ui, session: &str) {
        title(ui, "Stats");
        ui.vertical_centered(|ui| ui.label(RichText::new(session).color(MUTED)));
        ui.add_space(16.0);
        ScrollArea::vertical().show(ui, |ui| {
            let mean = self.mean.map_or("-".into(), |t| t.to_string());
            let summary = [
                ("solves", self.solves.to_string()),
                ("mean", mean),
                ("DNFs", self.dnfs.to_string()),
            ];
            stats(ui, &summary);
            ui.add_space(24.0);
            self.table(ui);
            ui.add_space(24.0);
            self.graph(ui);
        });
    }

    /// Current and best of each row.
    fn table(&self, ui: &mut egui::Ui) {
        let header = |text: &str| RichText::new(text).size(14.0).color(MUTED);
        row(ui, [header(""), header("current"), header("best")]);
        for (name, current, best) in &self.rows {
            let time = |t: &Time| RichText::new(t.to_string()).size(20.0);
            let name = RichText::new(name).size(18.0).color(MUTED);
            row(ui, [name, time(current), time(best)]);
        }
    }

    /// Each solve's time as a dot and the `LINES` averages as lines, oldest on the left.
    fn graph(&self, ui: &mut egui::Ui) {
        let finished = self.singles.iter().flatten();
        let (Some(&lo), Some(&hi)) = (finished.clone().min(), finished.max()) else {
            return;
        };
        if self.singles.len() < 2 {
            return;
        }
        ui.label(RichText::new("Times").size(20.0));
        ui.add_space(4.0);
        // What each mark is, drawn as it is on the graph. Lines only once they have data.
        ui.horizontal(|ui| {
            legend(ui, MUTED, None, "single");
            for (line, &(n, color)) in self.lines.iter().zip(&LINES) {
                if line.iter().any(Option::is_some) {
                    ui.add_space(12.0);
                    legend(ui, color, Some(2.0), &format!("ao{n}"));
                }
            }
        });

        let size = vec2(ui.available_width(), GRAPH_HEIGHT);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let plot = rect
            .with_min_x(rect.left() + LABEL_WIDTH)
            .shrink2(vec2(4.0, 8.0));
        let span = (hi - lo).max(1) as f32;
        let last = (self.singles.len() - 1) as f32;
        let at = |i: usize, ms: u64| -> Pos2 {
            let x = plot.left() + plot.width() * i as f32 / last;
            pos2(x, plot.bottom() - plot.height() * (ms - lo) as f32 / span)
        };

        let painter = ui.painter();
        for ms in [lo, (lo + hi) / 2, hi] {
            let y = at(0, ms).y;
            painter.hline(plot.x_range(), y, Stroke::new(1.0, DIM));
            let label = Time::Ms(ms).to_string();
            let font = FontId::proportional(12.0);
            painter.text(
                pos2(rect.left(), y),
                Align2::LEFT_CENTER,
                label,
                font,
                MUTED,
            );
        }
        for (i, ms) in self.singles.iter().enumerate() {
            if let Some(ms) = *ms {
                painter.circle_filled(at(i, ms), 2.5, MUTED);
            }
        }
        // Broken at DNF averages. A point with no neighbour is a dot, as a line needs two.
        for (line, (_, color)) in self.lines.iter().zip(LINES) {
            let point = |i: usize| line.get(i).copied().flatten();
            for i in 0..line.len() {
                let Some(ms) = point(i) else { continue };
                if let Some(next) = point(i + 1) {
                    painter.line_segment([at(i, ms), at(i + 1, next)], Stroke::new(2.0, color));
                } else if i.checked_sub(1).and_then(point).is_none() {
                    painter.circle_filled(at(i, ms), 3.0, color);
                }
            }
        }
    }
}

/// A legend entry: a dot, or a line `width` thick, then `label`.
fn legend(ui: &mut egui::Ui, color: Color32, width: Option<f32>, label: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(16.0, 14.0), Sense::hover());
    let (c, painter) = (rect.center(), ui.painter());
    if let Some(width) = width {
        painter.hline(rect.x_range(), c.y, Stroke::new(width, color));
    } else {
        painter.circle_filled(c, 2.5, color);
    }
    ui.label(RichText::new(label).size(14.0).color(MUTED));
}

/// Three cells side by side, each centered in a third of the width.
fn row(ui: &mut egui::Ui, cells: [RichText; 3]) {
    ui.columns(3, |cols| {
        for (ui, cell) in cols.iter_mut().zip(cells) {
            ui.vertical_centered(|ui| ui.label(cell));
        }
    });
}
