//! Widgets shared by the pages.

use super::{BG, CHIP, MUTED, TEXT, TOP_INSET};
use eframe::egui::{
    self, Button, FontId, Frame, Margin, Painter, Pos2, Rect, RichText, Sense, Stroke, StrokeKind,
    Vec2, pos2, vec2,
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

/// Labelled values side by side, each label small above its value.
pub fn stats(ui: &mut egui::Ui, stats: &[(&str, String)]) {
    ui.columns(stats.len(), |cols| {
        for (ui, (label, value)) in cols.iter_mut().zip(stats) {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(*label).size(14.0).color(MUTED));
                ui.label(RichText::new(value).size(22.0));
            });
        }
    });
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

/// An icon's width in a chip.
const ICON: f32 = 14.0;

/// Tappable text `font` points high in a rounded box, after `icon` if given.
pub fn chip(
    ui: &mut egui::Ui,
    text: &str,
    font: f32,
    icon: Option<fn(&Painter, Pos2)>,
) -> egui::Response {
    let text = ui
        .painter()
        .layout_no_wrap(text.into(), FontId::proportional(font), TEXT);
    let pad = font * vec2(0.875, 0.4375);
    let icon_width = icon.map_or(0.0, |_| ICON + pad.y);
    let size = text.size() + vec2(2.0 * pad.x + icon_width, 2.0 * pad.y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, rect.height() / 2.0, CHIP);
    let left = rect.left() + pad.x;
    if let Some(icon) = icon {
        icon(painter, pos2(left + ICON / 2.0, rect.center().y));
    }
    let text_at = pos2(left + icon_width, rect.center().y - text.size().y / 2.0);
    painter.galley(text_at, text, TEXT);
    response
}

/// Two overlapping squares, for sessions: the front one hides the back one's corner.
pub fn sessions_icon(painter: &Painter, c: Pos2) {
    let stroke = Stroke::new(1.5, MUTED);
    let square = |offset| Rect::from_center_size(c + Vec2::splat(offset), Vec2::splat(9.0));
    painter.rect_stroke(square(-2.0), 2.0, stroke, StrokeKind::Middle);
    painter.rect_filled(square(2.0), 2.0, CHIP);
    painter.rect_stroke(square(2.0), 2.0, stroke, StrokeKind::Middle);
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
