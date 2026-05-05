use crop::Rope;
use eframe::egui;
use std::time::{Duration, Instant};

use crate::model::{Document, Selection};
use crate::syntax_highlighting::{highlight_color, highlight_name};

pub mod geometry;
mod input;
pub mod layout;
mod line_ops;
pub mod minimap;
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
use layout::TextLayoutPipeline;
pub use line_ops::{
    delete_selected_lines, duplicate_selected_lines, indent_selection, join_selected_lines,
    move_selected_lines_down, move_selected_lines_up, normalize_whitespace, outdent_selection,
    reverse_selected_lines, sort_selected_lines, toggle_comments, trim_trailing_whitespace,
};
use motion::find_matching_bracket_at;
pub use motion::*;
pub use multicursor::{
    add_all_matches, add_next_match, clear_secondary_cursors, delete_all, replace_selection_all,
    split_selection_into_lines,
};
use ops::*;
pub use ops::{CaseType, convert_case_all_selections, convert_case_selection};
pub use replace::{replace_all_matches, replace_match};

const LINE_GUTTER_MIN_WIDTH: f32 = 44.0;
const LINE_GUTTER_PADDING: f32 = 10.0;
const EDITOR_MIN_WIDTH: f32 = 320.0;
const TRIPLE_CLICK_DURATION: f32 = 0.4;
const SMOOTH_SCROLL_DURATION: f32 = 0.15; // seconds
const LARGE_FILE_LINE_COUNT: usize = 50000;
const LARGE_FILE_BYTE_SIZE: usize = 5_000_000;

/// Smooth scroll animation state.
#[derive(Clone, Debug)]
pub struct ScrollAnimation {
    start_y: f32,
    target_y: f32,
    start_time: Instant,
    duration: Duration,
}

impl ScrollAnimation {
    pub fn new(start_y: f32, target_y: f32) -> Self {
        Self {
            start_y,
            target_y,
            start_time: Instant::now(),
            duration: Duration::from_secs_f32(SMOOTH_SCROLL_DURATION),
        }
    }

    pub fn current_value(&self) -> f32 {
        let elapsed = self.start_time.elapsed();
        if elapsed >= self.duration {
            return self.target_y;
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        // Ease-out cubic
        let t = 1.0 - (1.0 - t).powi(3);
        self.start_y + (self.target_y - self.start_y) * t
    }

    pub fn is_done(&self) -> bool {
        self.start_time.elapsed() >= self.duration
    }
}

#[derive(Clone, Debug, Default)]
pub struct EditorViewState {
    pub preferred_column: Option<usize>,
    pub visible_rows: Option<usize>,
    pub last_click_time: Option<Instant>,
    pub click_count: u32,
    /// Set to true when Alt+click or Alt+drag is used for column selection
    pub column_selection: bool,
    /// The anchor column for column selection
    pub column_selection_anchor_col: Option<usize>,
    /// Active smooth scroll animation, if any.
    pub scroll_animation: Option<ScrollAnimation>,
    /// Cached layout pipeline, invalidated on revision/width/wrap/ruler changes.
    pub cached_layout: Option<(
        u64,
        f32,
        crate::settings::WrapMode,
        Vec<usize>,
        TextLayoutPipeline,
    )>,
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

/// Renders bracket matching highlights for the given line.
/// Highlights both the bracket at `bracket_offset` and its matching pair at `match_offset`.
fn paint_bracket_highlight_for_line(
    painter: &egui::Painter,
    rope: &Rope,
    bracket_offset: usize,
    match_offset: usize,
    line_index: usize,
    text_pos: egui::Pos2,
    row_height: f32,
    char_width: f32,
    color: egui::Color32,
) {
    let (line_start, line_end) = visual_line_bounds(rope, line_index);

    // Helper to paint a single bracket highlight
    let paint_bracket = |offset: usize| {
        if offset < line_start || offset >= line_end {
            return;
        }
        let col = rope.byte_slice(line_start..offset).chars().count();
        let x = text_pos.x + col as f32 * char_width;
        let rect = egui::Rect::from_min_size(
            egui::pos2(x - 1.0, text_pos.y),
            egui::vec2(char_width + 2.0, row_height),
        );
        painter.rect_stroke(
            rect,
            egui::CornerRadius::same(2),
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
    };

    paint_bracket(bracket_offset);
    if match_offset != bracket_offset {
        paint_bracket(match_offset);
    }
}

pub fn show_editor(
    ui: &mut egui::Ui,
    document: &mut Document,
    view_state: &mut EditorViewState,
    focus_pending: &mut bool,
    reveal_selection: Option<Selection>,
    search_highlights: &[SearchHighlight],
    extra_selections: &[Selection],
    wrap_mode: crate::settings::WrapMode,
    rulers: &[usize],
    show_visible_whitespace: bool,
    show_indentation_guides: bool,
    theme: crate::settings::Theme,
) -> EditorResponse {
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
    clamp_primary_selection(document);

    if let Some(selection) = reveal_selection {
        set_primary_selection(document, selection);
        *focus_pending = true;
    }

    let available_width = ui.available_width().max(EDITOR_MIN_WIDTH);
    let available_height = ui.available_height();

    // Use cached layout if inputs haven't changed, otherwise rebuild and cache.
    let mut need_rebuild = true;
    if let Some((rev, w, cached_wrap, cached_rulers, _)) = &view_state.cached_layout {
        if *rev == document.revision
            && (*w - available_width).abs() < 1.0
            && *cached_wrap == wrap_mode
            && *cached_rulers == rulers
        {
            need_rebuild = false;
        }
    }

    if need_rebuild {
        let pipeline = TextLayoutPipeline::new(
            ui,
            &document.rope,
            available_width,
            available_height,
            wrap_mode,
            rulers,
        );
        view_state.cached_layout = Some((
            document.revision,
            available_width,
            wrap_mode,
            rulers.to_vec(),
            pipeline.clone(),
        ));
    }

    let layout = view_state
        .cached_layout
        .as_ref()
        .map(|(_, _, _, _, pl)| pl.clone())
        .unwrap_or_else(|| {
            TextLayoutPipeline::new(
                ui,
                &document.rope,
                available_width,
                available_height,
                wrap_mode,
                rulers,
            )
        });

    let mut changed = false;

    // Use animated scroll value if animating, otherwise use stored document scroll
    let scroll_y = if let Some(anim) = &view_state.scroll_animation {
        anim.current_value()
    } else {
        document.scroll.y
    };
    let scroll_offset = egui::Vec2::new(document.scroll.x, scroll_y);

    let output = egui::ScrollArea::both()
        .id_salt("editor-scroll")
        .scroll_offset(scroll_offset)
        .auto_shrink([false, false])
        .show_viewport(ui, |ui, viewport| {
            let (rect, response) =
                ui.allocate_exact_size(layout.content_size(), egui::Sense::click_and_drag());

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
                    let is_multi = view_state.last_click_time.is_some_and(|t| {
                        now.duration_since(t).as_secs_f32() < TRIPLE_CLICK_DURATION
                    });
                    view_state.last_click_time = Some(now);
                    if is_multi {
                        view_state.click_count = (view_state.click_count % 3) + 1;
                    } else {
                        view_state.click_count = 1;
                    }

                    let offset = layout.offset_at_pointer(&document.rope, pointer_position, rect);
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
                        create_column_selection(
                            document, anchor_col, column, start_line, start_line,
                        );
                    } else if view_state.click_count == 3 {
                        set_primary_selection(
                            document,
                            select_line_at_offset(&document.rope, offset),
                        );
                        view_state.column_selection = false;
                        view_state.column_selection_anchor_col = None;
                    } else if view_state.click_count == 2 {
                        set_primary_selection(
                            document,
                            select_word_at_offset(&document.rope, offset),
                        );
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
                    let offset = layout.offset_at_pointer(&document.rope, pointer_position, rect);
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
                        let anchor_line =
                            line_index_of_byte(&document.rope, primary_selection(document).anchor);
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
                            egui::pos2(
                                pointer_position.x,
                                pointer_position.y - layout.row_height * 0.5,
                            ),
                            egui::vec2(1.0, layout.row_height),
                        );
                        ui.scroll_to_rect(scroll_rect, None);
                    }
                }
            }

            let viewport_rows = layout.visible_row_count(&viewport);
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
            let (first_line, last_line) = layout.visible_line_range(&viewport);
            let primary = primary_selection(document);
            let caret_line = line_index_of_byte(&document.rope, primary.head);

            // Calculate visible byte range for cache keying
            let visible_start = layout.wrapped_line_byte_start(&document.rope, first_line);
            let visible_end = if last_line < layout.line_count {
                layout.wrapped_line_byte_start(&document.rope, last_line)
            } else {
                document.rope.byte_len()
            };

            // Detect large files for performance guards
            let is_large_file = layout.line_count > LARGE_FILE_LINE_COUNT
                || document.rope.byte_len() > LARGE_FILE_BYTE_SIZE;

            // Precompute bookmarked lines for this render pass (skip for large files)
            let bookmarked_lines: std::collections::HashSet<usize> = if !is_large_file {
                document
                    .bookmarks
                    .iter()
                    .map(|&bm| line_index_of_byte(&document.rope, bm))
                    .collect()
            } else {
                std::collections::HashSet::new()
            };

            // Precompute bracket match for the primary cursor (skip for large files)
            let bracket_match = if !is_large_file {
                find_matching_bracket_at(&document.rope, primary.head)
            } else {
                None
            };
            let bracket_highlight_color = theme.bracket_highlight();

            for line_index in first_line..last_line {
                let y = layout.line_y(line_index, rect.top());
                if line_index == caret_line {
                    let line_highlight_color = theme.current_line_highlight();
                    let full_line_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.left(), y),
                        egui::vec2(rect.width(), layout.row_height),
                    );
                    painter.rect_filled(full_line_rect, 0.0, line_highlight_color);
                }

                // Draw bracket matching highlights (skip for large files)
                if !is_large_file {
                    if let Some((bracket_offset, match_offset)) = bracket_match {
                        paint_bracket_highlight_for_line(
                            &painter,
                            &document.rope,
                            bracket_offset,
                            match_offset,
                            line_index,
                            egui::pos2(rect.left() + layout.text_origin_x, y),
                            layout.row_height,
                            layout.char_width,
                            bracket_highlight_color,
                        );
                    }
                }

                // Draw bookmark indicator (skip for large files)
                if !is_large_file && bookmarked_lines.contains(&line_index) {
                    let bookmark_color = theme.bookmark();
                    let icon_pos = egui::pos2(rect.left() + 2.0, y + layout.row_height * 0.5 - 6.0);
                    painter.text(
                        icon_pos,
                        egui::Align2::LEFT_CENTER,
                        "🔖",
                        layout.font_id.clone(),
                        bookmark_color,
                    );
                }

                let line_number_pos = egui::pos2(rect.left() + LINE_GUTTER_PADDING, y);
                painter.text(
                    line_number_pos,
                    egui::Align2::LEFT_TOP,
                    (line_index + 1).to_string(),
                    layout.font_id.clone(),
                    ui.visuals().weak_text_color(),
                );

                // Draw indentation guides (skip for large files)
                if !is_large_file && show_indentation_guides {
                    let indent_level = line_indent_level(&document.rope, line_index);
                    if indent_level > 0 {
                        let guide_color = theme.indent_guide();
                        let tab_width = document.tab_width;
                        for col in (tab_width..=indent_level).step_by(tab_width) {
                            let x = rect.left() + layout.text_origin_x + layout.column_x(col);
                            painter.line_segment(
                                [egui::pos2(x, y), egui::pos2(x, y + layout.row_height)],
                                egui::Stroke::new(1.0, guide_color),
                            );
                        }
                    }
                }

                let text_pos = egui::pos2(rect.left() + layout.text_origin_x, y);

                // Draw search highlights first (behind selections)
                paint_search_highlights_for_line(
                    &painter,
                    &document.rope,
                    search_highlights,
                    line_index,
                    false,
                    text_pos,
                    layout.row_height,
                    layout.char_width,
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
                        layout.row_height,
                        layout.char_width,
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
                        layout.row_height,
                        layout.char_width,
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
                    layout.row_height,
                    layout.char_width,
                    ui.visuals(),
                );

                let line_text = layout.wrapped_line_text(&document.rope, line_index);
                let line_start_byte = layout.wrapped_line_byte_start(&document.rope, line_index);

                // Get syntax highlight spans for this line
                let highlight_spans: Vec<(usize, usize, egui::Color32)> = if !is_large_file {
                    if let Some(detection) = document.detect_syntax() {
                        if detection.language != crate::syntax::LanguageId::PlainText {
                            let text = document.text();
                            let spans = document.syntax_state.highlight(
                                &text,
                                detection.language,
                                document.revision,
                                visible_start,
                                visible_end,
                            );
                            let line_end_byte = line_start_byte + line_text.len();
                            spans
                                .iter()
                                .filter_map(|span| {
                                    let start = span.start.max(line_start_byte);
                                    let end = span.end.min(line_end_byte);
                                    if start < end {
                                        let color =
                                            highlight_color(&highlight_name(span.highlight), theme);
                                        Some((
                                            start - line_start_byte,
                                            end - line_start_byte,
                                            color,
                                        ))
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                if show_visible_whitespace {
                    let whitespace_color = ui.visuals().weak_text_color();
                    let mut x_offset = 0.0;
                    let mut span_idx = 0;

                    for (i, ch) in line_text.chars().enumerate() {
                        let byte_pos = line_text.char_indices().nth(i).map(|(b, _)| b).unwrap_or(0);
                        let (display_ch, mut color) = match ch {
                            ' ' => ('·', whitespace_color),
                            '\t' => ('→', whitespace_color),
                            _ => (ch, ui.visuals().text_color()),
                        };

                        // Check if this character is within a highlight span
                        while span_idx < highlight_spans.len()
                            && highlight_spans[span_idx].1 <= byte_pos
                        {
                            span_idx += 1;
                        }
                        if span_idx < highlight_spans.len()
                            && byte_pos >= highlight_spans[span_idx].0
                            && byte_pos < highlight_spans[span_idx].1
                        {
                            if ch != ' ' && ch != '\t' {
                                color = highlight_spans[span_idx].2;
                            }
                        }

                        painter.text(
                            text_pos + egui::vec2(x_offset, 0.0),
                            egui::Align2::LEFT_TOP,
                            display_ch.to_string(),
                            layout.font_id.clone(),
                            color,
                        );
                        x_offset += layout.char_width;
                    }
                } else {
                    // Render with syntax highlighting
                    if highlight_spans.is_empty() {
                        // No highlights: render entire line with default color
                        painter.text(
                            text_pos,
                            egui::Align2::LEFT_TOP,
                            line_text,
                            layout.font_id.clone(),
                            ui.visuals().text_color(),
                        );
                    } else {
                        // Render line piece-by-piece with different colors
                        let mut x_offset = 0.0;
                        let mut last_byte = 0;
                        let default_color = ui.visuals().text_color();

                        // Sort spans by start position
                        let mut sorted_spans = highlight_spans.clone();
                        sorted_spans.sort_by_key(|(start, _, _)| *start);

                        for (span_start, span_end, color) in sorted_spans {
                            // Render unhighlighted text before this span
                            if span_start > last_byte {
                                let text_segment =
                                    &line_text[last_byte..span_start.min(line_text.len())];
                                if !text_segment.is_empty() {
                                    painter.text(
                                        text_pos + egui::vec2(x_offset, 0.0),
                                        egui::Align2::LEFT_TOP,
                                        text_segment,
                                        layout.font_id.clone(),
                                        default_color,
                                    );
                                    x_offset +=
                                        layout.char_width * text_segment.chars().count() as f32;
                                }
                            }

                            // Render highlighted text
                            let segment_start = span_start.min(line_text.len());
                            let segment_end = span_end.min(line_text.len());
                            if segment_start < segment_end {
                                let text_segment = &line_text[segment_start..segment_end];
                                if !text_segment.is_empty() {
                                    painter.text(
                                        text_pos + egui::vec2(x_offset, 0.0),
                                        egui::Align2::LEFT_TOP,
                                        text_segment,
                                        layout.font_id.clone(),
                                        color,
                                    );
                                    x_offset +=
                                        layout.char_width * text_segment.chars().count() as f32;
                                }
                            }

                            last_byte = segment_end;
                        }

                        // Render remaining unhighlighted text
                        if last_byte < line_text.len() {
                            let text_segment = &line_text[last_byte..];
                            if !text_segment.is_empty() {
                                painter.text(
                                    text_pos + egui::vec2(x_offset, 0.0),
                                    egui::Align2::LEFT_TOP,
                                    text_segment,
                                    layout.font_id.clone(),
                                    default_color,
                                );
                            }
                        }
                    }
                }
            }

            // Handle reveal_selection with smooth scroll animation
            if reveal_selection.is_some() {
                let primary = primary_selection(document);
                let caret_pos = layout.caret_position(&document.rope, primary.head, rect.top());
                let target_y = caret_pos.y - viewport.height() * 0.5 + layout.row_height * 0.5;
                let target_y = target_y.max(0.0);
                if (target_y - document.scroll.y).abs() > 1.0 {
                    view_state.scroll_animation =
                        Some(ScrollAnimation::new(document.scroll.y, target_y));
                }
            }

            // Draw carets for all selections
            for (i, sel) in document.selections.iter().enumerate() {
                let caret_pos = layout.caret_position(&document.rope, sel.head, rect.top());
                let caret_rect =
                    egui::Rect::from_min_size(caret_pos, egui::vec2(1.0, layout.row_height));

                let is_primary = i == 0;
                let stroke_width = if is_primary { 1.5 } else { 1.0 };
                let color = if response.has_focus() {
                    ui.visuals().text_color()
                } else {
                    ui.visuals().text_color().gamma_multiply(0.5)
                };

                if is_primary && reveal_selection.is_some() {
                    // Smooth scroll is handled above via animation
                }

                if response.has_focus() || !is_primary {
                    painter.line_segment(
                        [caret_rect.left_top(), caret_rect.left_bottom()],
                        egui::Stroke::new(stroke_width, color),
                    );
                }
            }
        });

    // Smooth scroll: advance animation and save animated value
    if let Some(anim) = &mut view_state.scroll_animation {
        if anim.is_done() {
            document.scroll.y = anim.target_y;
            view_state.scroll_animation = None;
        } else {
            document.scroll.y = anim.current_value();
        }
    }

    // Save scroll offset back to document for persistence (x always, y if no animation)
    document.scroll.x = output.state.offset.x;
    if view_state.scroll_animation.is_none() {
        document.scroll.y = output.state.offset.y;
    }

    EditorResponse { changed }
}

#[cfg(test)]
mod tests;
