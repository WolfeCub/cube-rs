//! The solves page: every solve of the open session, newest first, with its scramble drawn.

use super::widgets::title;
use super::{DIM, MUTED, TEXT};
use crate::session::{Penalty, Solve};
use crate::sessions::Sessions;
use crate::tnoodle::Drawer;
use eframe::egui::{
    self, Align2, FontId, Image, Rect, RichText, ScrollArea, Sense, Stroke, Vec2, load::Bytes,
    pos2, vec2,
};
use std::collections::HashMap;
use std::sync::Arc;

const ROW_HEIGHT: f32 = 92.0;
/// tnoodle's 130 by 98 net.
const NET_SIZE: Vec2 = Vec2::new(104.0, 78.0);
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The page's scramble images while it's open, by scramble: `None` while being drawn.
#[derive(Default)]
pub struct SolvesPage {
    svgs: HashMap<String, Option<Arc<[u8]>>>,
}

impl SolvesPage {
    pub fn show(&mut self, ui: &mut egui::Ui, sessions: &Sessions, drawer: &Drawer) {
        while let Some(drawn) = drawer.try_recv() {
            self.svgs
                .insert(drawn.moves, Some(drawn.svg.into_bytes().into()));
        }

        let solves = sessions.current().solves();
        title(ui, "Solves");
        ui.vertical_centered(|ui| ui.label(RichText::new(sessions.name()).color(MUTED)));
        ui.add_space(16.0);

        // Rows of equal height, so only the visible ones are laid out.
        ScrollArea::vertical().show_rows(ui, ROW_HEIGHT, solves.len(), |ui, rows| {
            for row in rows {
                let n = solves.len() - row;
                self.row(ui, n, &solves[n - 1], drawer);
            }
        });

        if self.svgs.values().any(Option::is_none) {
            ui.ctx().request_repaint();
        }
    }

    /// Solve number `n`: its scramble drawn, its time, when it was done and its scramble.
    fn row(&mut self, ui: &mut egui::Ui, n: usize, solve: &Solve, drawer: &Drawer) {
        let size = vec2(ui.available_width(), ROW_HEIGHT);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let net = Rect::from_min_size(rect.left_center() - vec2(0.0, NET_SIZE.y / 2.0), NET_SIZE);
        let svg = self.svgs.entry(solve.scramble.clone()).or_insert_with(|| {
            drawer.request(&solve.scramble);
            None
        });
        if let Some(svg) = svg.as_ref().filter(|svg| !svg.is_empty()) {
            let uri = format!("bytes://{}.svg", solve.scramble);
            Image::from_bytes(uri, Bytes::Shared(svg.clone())).paint_at(ui, net);
        }

        let painter = ui.painter();
        let left = net.right() + 16.0;
        let top = rect.top() + 8.0;
        let time = match solve.penalty {
            Penalty::PlusTwo => format!("{}+", solve.time()),
            _ => solve.time().to_string(),
        };
        let font = |size| FontId::proportional(size);
        painter.text(pos2(left, top), Align2::LEFT_TOP, time, font(22.0), TEXT);
        let when = format!("#{n} · {}", local_time(solve.at));
        let right = pos2(rect.right(), top + 4.0);
        painter.text(right, Align2::RIGHT_TOP, when, font(14.0), MUTED);

        // Two lines, centered between the time and the date.
        let center = (left + rect.right()) / 2.0;
        let mut y = top + 30.0;
        for half in halves(&solve.scramble) {
            let line = painter.layout_no_wrap(half, font(14.0), MUTED);
            let height = line.size().y;
            painter.galley(pos2(center - line.size().x / 2.0, y), line, MUTED);
            y += height;
        }
        painter.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, DIM));
    }
}

/// `moves` split in the middle, the first half the longer.
fn halves(moves: &str) -> [String; 2] {
    let moves: Vec<&str> = moves.split_whitespace().collect();
    let (first, second) = moves.split_at(moves.len().div_ceil(2));
    [first.join(" "), second.join(" ")]
}

/// `Nov 14, 14:02` in the phone's time zone.
fn local_time(unix_secs: u64) -> String {
    let t = unix_secs as libc::time_t;
    // SAFETY: localtime_r only writes the given `tm`.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return String::new();
    }
    let month = MONTHS[tm.tm_mon as usize];
    format!("{month} {}, {:02}:{:02}", tm.tm_mday, tm.tm_hour, tm.tm_min)
}
