use eframe::egui;
use std::time::Instant;

use crate::model::{Document, Selection};

mod geometry;
mod input;
mod line_ops;
mod motion;
mod multicursor;
mod ops;
mod replace;

use geometry::*;
pub use geometry::{
    decimal_digits, primary_selection, select_line_at_offset, select_word_at_offset,
    set_primary_selection, visual_line_count, word_at_selection,
};
use input::handle_input;
use line_ops::*;
pub use line_ops::{
    delete_selected_lines, duplicate_selected_lines, indent_selection, join_selected_lines,
    move_selected_lines_down, move_selected_lines_up, normalize_whitespace, outdent_selection,
    reverse_selected_lines, sort_selected_lines, toggle_comments, trim_trailing_whitespace,
};
pub use motion::*;
pub use multicursor::{
    add_all_matches, add_next_match, clear_secondary_cursors, delete_all,
    replace_selection_all, split_selection_into_lines,
};
use ops::*;
pub use ops::{convert_case_all_selections, convert_case_selection, CaseType};
pub use replace::{replace_all_matches, replace_match};

const LINE_GUTTER_MIN_WIDTH: f32 = 44.0;
const LINE_GUTTER_PADDING: f32 = 10.0;
const EDITOR_MIN_WIDTH: f32 = 320.0;
const TRIPLE_CLICK_DURATION: f32 = 0.4;

#[derive(Clone, Debug, Default)]
pub struct EditorViewState {
    preferred_column: Option<usize>,
    visible_rows: Option<usize>,
    last_click_time: Option<Instant>,
    click_count: u32,
    /// Set to true when Alt+click or Alt+drag is used for column selection
    pub column_selection: bool,
    /// The anchor column for column selection
    pub column_selection_anchor_col: Option<usize>,
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
            let click = response.clicked();
            let drag_started = response.drag_started();
            if click || drag_started {
                response.request_focus();
                if let Some(pointer_position) = response.interact_pointer_pos() {
                    let now = Instant::now();
                    let is_multi = view_state
                        .last_click_time
                        .is_some_and(|t| now.duration_since(t).as_secs_f32() < TRIPLE_CLICK_DURATION);
                    view_state.last_click_time = Some(now);
                    if is_multi {
                        view_state.click_count = (view_state.click_count % 3) + 1;
                    } else {
                        view_state.click_count = 1;
                    }

                    let offset = offset_at_pointer(
                        &document.rope,
                        pointer_position,
                        rect,
                        text_origin_x,
                        row_height,
                        char_width,
                        line_count,
                    );

                    let column = column_of_byte(&document.rope, offset);
                    let is_alt = ui.input(|i| i.modifiers.alt);

                    if is_alt {
                        // Column (rectangular) selection
                        if view_state.column_selection_anchor_col.is_none() {
                            view_state.column_selection_anchor_col = Some(column);
                        }
                        view_state.column_selection = true;
                        let anchor_col = view_state.column_selection_anchor_col.unwrap_or(column);
                        let start_line = line_index_of_byte(&document.rope, offset);
                        create_column_selection(document, anchor_col, column, start_line, start_line);
                    } else if view_state.click_count == 3 {
                        set_primary_selection(document, select_line_at_offset(&document.rope, offset));
                        view_state.column_selection = false;
                        view_state.column_selection_anchor_col = None;
                    } else if view_state.click_count == 2 {
                        set_primary_selection(document, select_word_at_offset(&document.rope, offset));
                        view_state.column_selection = false;
                        view_state.column_selection_anchor_col = None;
                    } else {
                        set_primary_selection(document, Selection::caret(offset));
                        view_state.column_selection = false;
                        view_state.column_selection_anchor_col = None;
                    }
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
                    let column = column_of_byte(&document.rope, offset);
                    let line = line_index_of_byte(&document.rope, offset);
                    let is_alt = ui.input(|i| i.modifiers.alt);

                    if is_alt || view_state.column_selection {
                        // Column (rectangular) selection
                        if view_state.column_selection_anchor_col.is_none() {
                            let anchor = primary_selection(document).anchor;
                            view_state.column_selection_anchor_col =
                                Some(column_of_byte(&document.rope, anchor));
                        }
                        view_state.column_selection = true;
                        let anchor_col = view_state.column_selection_anchor_col.unwrap_or(column);
                        let anchor_line = line_index_of_byte(
                            &document.rope,
                            primary_selection(document).anchor,
                        );
                        create_column_selection(document, anchor_col, column, anchor_line, line);
                    } else {
                        let mut selection = primary_selection(document);
                        if selection.anchor == selection.head {
                            selection.anchor = offset;
                        }
                        selection.head = offset;
                        set_primary_selection(document, selection);
                    }
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
            let primary = primary_selection(document);
            let caret_line = line_index_of_byte(&document.rope, primary.head);

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

                // Draw search highlights first (behind selections)
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

                // Draw all selections with different colors for primary vs secondary
                for (i, sel) in document.selections.iter().enumerate() {
                    let is_primary = i == 0;
                    let color = if is_primary {
                        ui.visuals().selection.bg_fill
                    } else {
                        ui.visuals().selection.bg_fill.gamma_multiply(0.6)
                    };
                    paint_selection_for_line(
                        &painter,
                        document,
                        *sel,
                        line_index,
                        text_pos,
                        row_height,
                        char_width,
                        color,
                    );
                }

                // Draw extra selections (from search) with yet another color
                for sel in extra_selections {
                    paint_selection_for_line(
                        &painter,
                        document,
                        *sel,
                        line_index,
                        text_pos,
                        row_height,
                        char_width,
                        ui.visuals().selection.bg_fill.gamma_multiply(0.4),
                    );
                }

                // Draw search highlights on top for current match
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

            // Draw carets for all selections
            for (i, sel) in document.selections.iter().enumerate() {
                let caret_pos = caret_position(
                    &document.rope,
                    sel.head,
                    rect.left() + text_origin_x,
                    rect.top(),
                    row_height,
                    char_width,
                );
                let caret_rect =
                    egui::Rect::from_min_size(caret_pos, egui::vec2(1.0, row_height));

                let is_primary = i == 0;
                let stroke_width = if is_primary { 1.5 } else { 1.0 };
                let color = if response.has_focus() {
                    ui.visuals().text_color()
                } else {
                    ui.visuals().text_color().gamma_multiply(0.5)
                };

                if is_primary && reveal_selection.is_some() {
                    ui.scroll_to_rect(caret_rect.expand(24.0), Some(egui::Align::Center));
                }

                if response.has_focus() || !is_primary {
                    painter.line_segment(
                        [caret_rect.left_top(), caret_rect.left_bottom()],
                        egui::Stroke::new(stroke_width, color),
                    );
                }
            }
        });

    EditorResponse { changed }
}

#[cfg(test)]
mod tests;
