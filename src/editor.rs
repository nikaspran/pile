use crop::Rope;
use eframe::egui;

use crate::model::{Document, Selection};

mod geometry;
mod input;
mod line_ops;
mod motion;
mod ops;
mod replace;

use geometry::*;
pub use geometry::{
    decimal_digits, primary_selection, set_primary_selection, visual_line_count, word_at_selection,
};
use input::handle_input;
use line_ops::*;
use motion::*;
use ops::*;
pub use replace::{replace_all_matches, replace_match};

const LINE_GUTTER_MIN_WIDTH: f32 = 44.0;
const LINE_GUTTER_PADDING: f32 = 10.0;
const EDITOR_MIN_WIDTH: f32 = 320.0;

#[derive(Debug, Default)]
pub struct EditorViewState {
    preferred_column: Option<usize>,
    visible_rows: Option<usize>,
}

#[derive(Debug)]
pub struct EditorResponse {
    pub changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchHighlight {
    pub start: usize,
    pub end: usize,
    pub is_current: bool,
}

pub fn show_editor(
    ui: &mut egui::Ui,
    document: &mut Document,
    view_state: &mut EditorViewState,
    focus_pending: &mut bool,
    reveal_selection: Option<Selection>,
    search_highlights: &[SearchHighlight],
    extra_selections: &[Selection],
) -> EditorResponse {
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    clamp_primary_selection(document);

    if let Some(selection) = reveal_selection {
        set_primary_selection(document, selection);
        *focus_pending = true;
    }

    let line_count = visual_line_count(&document.rope);
    let line_digits = decimal_digits(line_count);
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let gutter_width =
        (line_digits as f32 * 8.0 + LINE_GUTTER_PADDING * 2.0).max(LINE_GUTTER_MIN_WIDTH);
    let available_width = ui.available_width().max(EDITOR_MIN_WIDTH);
    let text_origin_x = gutter_width + LINE_GUTTER_PADDING;
    let content_width = available_width.max(text_origin_x + EDITOR_MIN_WIDTH);
    let content_height = (line_count as f32 * row_height).max(ui.available_height());
    let font_id = egui::TextStyle::Monospace.resolve(ui.style());
    let char_width = monospace_char_width(ui, font_id.clone());

    let mut changed = false;

    egui::ScrollArea::both()
        .id_salt("editor-scroll")
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(content_width, content_height),
                egui::Sense::click_and_drag(),
            );

            if *focus_pending {
                response.request_focus();
                *focus_pending = false;
            }
            if response.drag_started() || response.clicked() {
                response.request_focus();
                if let Some(pointer_position) = response.interact_pointer_pos() {
                    let offset = offset_at_pointer(
                        &document.rope,
                        pointer_position,
                        rect,
                        text_origin_x,
                        row_height,
                        char_width,
                        line_count,
                    );
                    set_primary_selection(document, Selection::caret(offset));
                    view_state.preferred_column = None;
                }
            } else if response.dragged() {
                if let Some(pointer_position) = response.interact_pointer_pos() {
                    let offset = offset_at_pointer(
                        &document.rope,
                        pointer_position,
                        rect,
                        text_origin_x,
                        row_height,
                        char_width,
                        line_count,
                    );
                    let mut selection = primary_selection(document);
                    selection.head = offset;
                    set_primary_selection(document, selection);
                    view_state.preferred_column = None;

                    let viewport_top_abs = rect.top() + viewport.min.y;
                    let viewport_bottom_abs = rect.top() + viewport.max.y;
                    if pointer_position.y < viewport_top_abs
                        || pointer_position.y > viewport_bottom_abs
                    {
                        let scroll_rect = egui::Rect::from_min_size(
                            egui::pos2(pointer_position.x, pointer_position.y - row_height * 0.5),
                            egui::vec2(1.0, row_height),
                        );
                        ui.scroll_to_rect(scroll_rect, None);
                    }
                }
            }

            let viewport_rows = ((viewport.max.y - viewport.min.y) / row_height)
                .floor()
                .max(1.0) as usize;
            view_state.visible_rows = Some(viewport_rows);

            if response.has_focus() {
                ui.memory_mut(|memory| {
                    memory.set_focus_lock_filter(
                        response.id,
                        egui::EventFilter {
                            tab: true,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: false,
                        },
                    );
                });
                changed |= handle_input(ui, document, view_state);
                if changed {
                    ui.memory_mut(|memory| memory.request_focus(response.id));
                }
            }

            let painter = ui.painter_at(rect);
            let first_line = (viewport.min.y / row_height).floor().max(0.0) as usize;
            let last_line = ((viewport.max.y / row_height).ceil() as usize + 1).min(line_count);
            let selection = primary_selection(document);
            let caret_line = line_index_of_byte(&document.rope, selection.head);

            for line_index in first_line..last_line {
                let y = rect.top() + line_index as f32 * row_height;
                if line_index == caret_line {
                    let line_highlight_color = if ui.visuals().dark_mode {
                        egui::Color32::from_rgba_premultiplied(255, 255, 255, 12)
                    } else {
                        egui::Color32::from_rgba_premultiplied(0, 0, 0, 12)
                    };
                    let gutter_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.left(), y),
                        egui::vec2(gutter_width, row_height),
                    );
                    painter.rect_filled(gutter_rect, 0.0, line_highlight_color);
                }

                let line_number_pos = egui::pos2(rect.left() + LINE_GUTTER_PADDING, y);
                painter.text(
                    line_number_pos,
                    egui::Align2::LEFT_TOP,
                    (line_index + 1).to_string(),
                    font_id.clone(),
                    ui.visuals().weak_text_color(),
                );

                let text_pos = egui::pos2(rect.left() + text_origin_x, y);
                paint_search_highlights_for_line(
                    &painter,
                    &document.rope,
                    search_highlights,
                    line_index,
                    false,
                    text_pos,
                    row_height,
                    char_width,
                    ui.visuals(),
                );
                paint_selection_for_line(
                    &painter,
                    document,
                    selection,
                    line_index,
                    text_pos,
                    row_height,
                    char_width,
                    ui.visuals().selection.bg_fill,
                );
                for sel in extra_selections {
                    paint_selection_for_line(
                        &painter,
                        document,
                        *sel,
                        line_index,
                        text_pos,
                        row_height,
                        char_width,
                        ui.visuals().selection.bg_fill.gamma_multiply(0.6),
                    );
                }
                paint_search_highlights_for_line(
                    &painter,
                    &document.rope,
                    search_highlights,
                    line_index,
                    true,
                    text_pos,
                    row_height,
                    char_width,
                    ui.visuals(),
                );

                let line_text = visual_line_text(&document.rope, line_index);
                painter.text(
                    text_pos,
                    egui::Align2::LEFT_TOP,
                    line_text,
                    font_id.clone(),
                    ui.visuals().text_color(),
                );
            }

            let caret_position = caret_position(
                &document.rope,
                selection.head,
                rect.left() + text_origin_x,
                rect.top(),
                row_height,
                char_width,
            );
            let current_caret_rect =
                egui::Rect::from_min_size(caret_position, egui::vec2(1.0, row_height));

            if reveal_selection.is_some() {
                ui.scroll_to_rect(current_caret_rect.expand(24.0), Some(egui::Align::Center));
            }

            if response.has_focus() {
                painter.line_segment(
                    [
                        current_caret_rect.left_top(),
                        current_caret_rect.left_bottom(),
                    ],
                    egui::Stroke::new(1.5, ui.visuals().text_color()),
                );
            }
        });

    EditorResponse { changed }
}

#[cfg(test)]
mod tests;
