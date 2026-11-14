//! Widgets shared by the pages.

use super::{BG, CHIP, MUTED, TEXT, TOP_INSET};
use eframe::egui::{
    self, Button, FontId, Frame, Margin, RichText, Sense, Stroke, Vec2, pos2, vec2,
};

/// A page's frame: the background and side margins.
pub fn page_frame() -> Frame {
    Frame::NONE.fill(BG).inner_margin(Margin::symmetric(24, 0))
}

/// A page's title, below the status bar.
pub fn title(ui: &mut egui::Ui, text: &str) {
    ui.add_space(TOP_INSET);
    ui.vertical_centered(|ui| ui.label(RichText::new(text).size(28.0)));
}

pub fn button(ui: &mut egui::Ui, label: &str, selected: bool) -> bool {
    let button = Button::selectable(selected, label).min_size(Vec2::new(72.0, 36.0));
    ui.vertical_centered(|ui| ui.add(button).clicked()).inner
}

/// A caption over a row of buttons, the one matching `value` selected. A click sets `value`.
pub fn choice<T: PartialEq + Copy>(
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

pub enum Chevron {
    /// Picks from a list.
    Down,
    /// Opens a page.
    Right,
}

/// Tappable text in a rounded box, with a chevron after it.
pub fn chip(ui: &mut egui::Ui, text: &str, chevron: Chevron) -> egui::Response {
    let text = ui
        .painter()
        .layout_no_wrap(text.into(), FontId::proportional(16.0), TEXT);
    let pad = vec2(14.0, 7.0);
    let size = text.size() + vec2(3.0 * pad.x, 2.0 * pad.y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, rect.height() / 2.0, CHIP);
    let text_at = rect.left_center() + vec2(pad.x, -text.size().y / 2.0);
    painter.galley(text_at, text, TEXT);
    let c = pos2(rect.right() - pad.x, rect.center().y);
    let points = match chevron {
        Chevron::Down => [vec2(-4.0, -2.0), vec2(0.0, 2.0), vec2(4.0, -2.0)],
        Chevron::Right => [vec2(-2.0, -4.0), vec2(2.0, 0.0), vec2(-2.0, 4.0)],
    };
    painter.line(points.map(|p| c + p).to_vec(), Stroke::new(2.0, MUTED));
    response
}

/// A tappable cog.
pub fn cog(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
    let (c, painter) = (rect.center(), ui.painter());
    for i in 0..8 {
        let tooth = Vec2::angled(i as f32 * std::f32::consts::TAU / 8.0);
        painter.line_segment([c + 5.0 * tooth, c + 10.0 * tooth], Stroke::new(3.5, MUTED));
    }
    painter.circle_filled(c, 7.5, MUTED);
    painter.circle_filled(c, 3.0, BG);
    response
}
