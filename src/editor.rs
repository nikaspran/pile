use crop::Rope;
use eframe::egui;

use crate::model::{Document, EditTransaction, Selection};
use crate::search::SearchMatch;

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
                let row_rect = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), y),
                    egui::vec2(content_width, row_height),
                );

                if line_index == caret_line {
                    painter.rect_filled(
                        row_rect,
                        0.0,
                        ui.visuals().selection.bg_fill.gamma_multiply(0.25),
                    );
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

fn handle_input(ui: &egui::Ui, document: &mut Document, view_state: &mut EditorViewState) -> bool {
    let events = ui.input(|input| input.events.clone());
    let mut changed = false;
    let mut had_typing_event = false;

    for event in events {
        match event {
            egui::Event::Paste(text) if !text.is_empty() => {
                changed |= replace_selection_with(document, &text);
                view_state.preferred_column = None;
                had_typing_event = true;
            }
            egui::Event::Text(text) if !text.is_empty() && text != "\n" && text != "\r" => {
                changed |= replace_selection_with(document, &text);
                view_state.preferred_column = None;
                had_typing_event = true;
            }
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command && !modifiers.shift && !modifiers.alt => match key {
                egui::Key::Z => {
                    changed |= document.undo();
                    view_state.preferred_column = None;
                }
                egui::Key::J => {
                    changed |= join_selected_lines(document);
                    view_state.preferred_column = None;
                }
                _ => {}
            },
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command && modifiers.shift && !modifiers.alt => match key {
                egui::Key::Z => {
                    changed |= document.redo();
                    view_state.preferred_column = None;
                }
                egui::Key::D => {
                    changed |= duplicate_selected_lines(document);
                    view_state.preferred_column = None;
                }
                egui::Key::K => {
                    changed |= delete_selected_lines(document);
                    view_state.preferred_column = None;
                }
                egui::Key::S => {
                    changed |= sort_selected_lines(document);
                    view_state.preferred_column = None;
                }
                egui::Key::R => {
                    changed |= reverse_selected_lines(document);
                    view_state.preferred_column = None;
                }
                egui::Key::T => {
                    changed |= trim_trailing_whitespace(document);
                    view_state.preferred_column = None;
                }
                _ => {}
            },
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command && modifiers.alt && !modifiers.shift => match key {
                egui::Key::ArrowUp => {
                    changed |= move_selected_lines_up(document);
                    view_state.preferred_column = None;
                }
                egui::Key::ArrowDown => {
                    changed |= move_selected_lines_down(document);
                    view_state.preferred_column = None;
                }
                _ => {}
            },
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.command => {
                let extend = modifiers.shift;
                let word = modifiers.alt || modifiers.ctrl;
                let plain = !modifiers.shift && !modifiers.alt && !modifiers.ctrl;
                let indentation = !modifiers.alt && !modifiers.ctrl;
                match key {
                    egui::Key::Backspace if plain => {
                        changed |= backspace(document);
                        view_state.preferred_column = None;
                        had_typing_event = true;
                    }
                    egui::Key::Delete if plain => {
                        changed |= delete(document);
                        view_state.preferred_column = None;
                        had_typing_event = true;
                    }
                    egui::Key::Enter if plain => {
                        changed |= insert_newline_with_auto_indent(document);
                        view_state.preferred_column = None;
                        had_typing_event = true;
                    }
                    egui::Key::Tab if indentation => {
                        changed |= if modifiers.shift {
                            outdent_selection(document)
                        } else {
                            indent_selection(document)
                        };
                        view_state.preferred_column = None;
                    }
                    egui::Key::ArrowLeft => {
                        if word {
                            move_word_left(document, extend);
                        } else {
                            move_left(document, extend);
                        }
                        view_state.preferred_column = None;
                    }
                    egui::Key::ArrowRight => {
                        if word {
                            move_word_right(document, extend);
                        } else {
                            move_right(document, extend);
                        }
                        view_state.preferred_column = None;
                    }
                    egui::Key::ArrowUp => {
                        if word {
                            move_paragraph_up(document, extend);
                            view_state.preferred_column = None;
                        } else {
                            move_vertical(document, view_state, -1, extend);
                        }
                    }
                    egui::Key::ArrowDown => {
                        if word {
                            move_paragraph_down(document, extend);
                            view_state.preferred_column = None;
                        } else {
                            move_vertical(document, view_state, 1, extend);
                        }
                    }
                    egui::Key::Home => {
                        if word {
                            move_document_start(document, extend);
                        } else {
                            move_home(document, extend);
                        }
                        view_state.preferred_column = None;
                    }
                    egui::Key::End => {
                        if word {
                            move_document_end(document, extend);
                        } else {
                            move_end(document, extend);
                        }
                        view_state.preferred_column = None;
                    }
                    egui::Key::PageUp => {
                        move_page(document, view_state, -1, extend);
                    }
                    egui::Key::PageDown => {
                        move_page(document, view_state, 1, extend);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if changed || had_typing_event {
        document.commit_undo_group();
    } else {
        document.discard_undo_group();
    }

    changed
}

pub fn replace_selection_with(document: &mut Document, text: &str) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);

    if start == end && text.is_empty() {
        return false;
    }

    let deleted_text = document.rope.byte_slice(start..end).to_string();
    document.begin_undo_group();
    document.push_undo(EditTransaction {
        start,
        end,
        deleted_text,
        inserted_text: text.to_owned(),
        selections_before: vec![selection],
    });

    if start != end {
        document.rope.delete(start..end);
    }
    if !text.is_empty() {
        document.rope.insert(start, text);
    }

    let caret = start + text.len();
    set_primary_selection(document, Selection::caret(caret));
    document.revision += 1;
    true
}

pub fn insert_newline_with_auto_indent(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, _) = selection_range(selection);
    let line = line_index_of_byte(&document.rope, start);
    let line_start = byte_of_visual_line(&document.rope, line);
    let indent = leading_whitespace(&document.rope, line_start, start);
    let text = format!("\n{indent}");

    replace_selection_with(document, &text)
}

pub fn indent_selection(document: &mut Document) -> bool {
    change_line_indentation(document, IndentChange::Indent)
}

pub fn outdent_selection(document: &mut Document) -> bool {
    change_line_indentation(document, IndentChange::Outdent)
}

pub fn duplicate_selected_lines(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        let before_text = String::new();
        let before_selection = primary_selection(document);
        document.rope.insert(0, "\n");
        set_primary_selection(document, Selection::caret(1));
        document.commit_and_start_new_undo_group();
        document.push_undo(EditTransaction {
            start: 0,
            end: 0,
            deleted_text: before_text,
            inserted_text: "\n".to_owned(),
            selections_before: vec![before_selection],
        });
        document.commit_undo_group();
        document.revision += 1;
        return true;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (line_start, line_end) =
        selected_full_line_bounds(&document.rope, selection_start, selection_end);
    let text = document.rope.byte_slice(line_start..line_end).to_string();
    if text.is_empty() {
        return false;
    }

    let prefix = if text.ends_with('\n') { "" } else { "\n" };
    let insert_at = line_end;
    let original_text = document.text();

    document.rope.insert(insert_at, &format!("{prefix}{text}"));

    let duplicate_start = insert_at + prefix.len();
    let anchor = duplicate_start + selection.anchor.saturating_sub(line_start);
    let head = duplicate_start + selection.head.saturating_sub(line_start);
    let new_selection = Selection {
        anchor: anchor.min(duplicate_start + text.len()),
        head: head.min(duplicate_start + text.len()),
    };

    document.commit_and_start_new_undo_group();
    document.push_undo(EditTransaction {
        start: 0,
        end: original_text.len(),
        deleted_text: original_text,
        inserted_text: document.text(),
        selections_before: vec![selection],
    });
    document.commit_undo_group();

    set_primary_selection(document, new_selection);
    document.revision += 1;
    true
}

pub fn delete_selected_lines(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (mut delete_start, delete_end) =
        selected_full_line_bounds(&document.rope, selection_start, selection_end);

    if delete_start == delete_end {
        return false;
    }

    if delete_end == document.rope.byte_len() && delete_start > 0 {
        delete_start = previous_line_break_offset(&document.rope, delete_start)
            .map_or(delete_start, |offset| offset + 1);
        if delete_start > 0 {
            delete_start -= 1;
        }
    }

    let original_text = document.text();

    document.rope.delete(delete_start..delete_end);
    let caret = delete_start.min(document.rope.byte_len());
    let new_text = document.text();

    document.commit_and_start_new_undo_group();
    document.push_undo(EditTransaction {
        start: 0,
        end: original_text.len(),
        deleted_text: original_text,
        inserted_text: new_text,
        selections_before: vec![selection],
    });
    document.commit_undo_group();

    set_primary_selection(document, Selection::caret(caret));
    document.revision += 1;
    true
}

pub fn move_selected_lines_up(document: &mut Document) -> bool {
    move_selected_lines(document, LineMoveDirection::Up)
}

pub fn move_selected_lines_down(document: &mut Document) -> bool {
    move_selected_lines(document, LineMoveDirection::Down)
}

pub fn join_selected_lines(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (first_line, selected_last_line) =
        selected_line_range(&document.rope, selection_start, selection_end);
    let last_line = if first_line == selected_last_line {
        first_line + 1
    } else {
        selected_last_line
    };

    if last_line >= document.rope.line_len() {
        return false;
    }

    let start = byte_of_visual_line(&document.rope, first_line);
    let end = if last_line + 1 < document.rope.line_len() {
        byte_of_visual_line(&document.rope, last_line + 1)
    } else {
        document.rope.byte_len()
    };
    let original = document.rope.byte_slice(start..end).to_string();
    let (body, suffix) = original
        .strip_suffix('\n')
        .map_or((original.as_str(), ""), |body| (body, "\n"));
    let mut joined = join_lines_text(body);
    joined.push_str(suffix);
    if joined == original {
        return false;
    }

    document.rope.delete(start..end);
    document.rope.insert(start, &joined);
    let caret = start + joined.len() - suffix.len();

    document.commit_and_start_new_undo_group();
    document.push_undo(EditTransaction {
        start,
        end,
        deleted_text: original,
        inserted_text: joined.clone(),
        selections_before: vec![selection],
    });
    document.commit_undo_group();

    set_primary_selection(document, Selection::caret(caret));
    document.revision += 1;
    true
}

pub fn sort_selected_lines(document: &mut Document) -> bool {
    transform_selected_lines(document, |lines| {
        lines.sort_by(|left, right| left.cmp(right))
    })
}

pub fn reverse_selected_lines(document: &mut Document) -> bool {
    transform_selected_lines(document, |lines| {
        lines.reverse();
    })
}

pub fn trim_trailing_whitespace(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (start, end) = selected_full_line_bounds(&document.rope, selection_start, selection_end);

    let original = document.rope.byte_slice(start..end).to_string();
    let has_trailing_newline = original.ends_with('\n');
    let body = original.strip_suffix('\n').unwrap_or(&original);

    let lines: Vec<&str> = body.split('\n').collect();
    let trimmed_lines: Vec<String> = lines.iter().map(|line| line.trim_end().to_string()).collect();

    let changed = lines
        .iter()
        .zip(trimmed_lines.iter())
        .any(|(orig, trimmed)| *orig != trimmed);

    if !changed {
        return false;
    }

    let mut replacement = trimmed_lines.join("\n");
    if has_trailing_newline {
        replacement.push('\n');
    }

    document.rope.delete(start..end);
    document.rope.insert(start, &replacement);

    document.commit_and_start_new_undo_group();
    document.push_undo(EditTransaction {
        start,
        end,
        deleted_text: original,
        inserted_text: replacement.clone(),
        selections_before: vec![selection],
    });
    document.commit_undo_group();

    set_primary_selection(
        document,
        Selection {
            anchor: start,
            head: start + replacement.len(),
        },
    );
    document.revision += 1;
    true
}

fn transform_selected_lines<F>(document: &mut Document, transform: F) -> bool
where
    F: FnOnce(&mut Vec<String>),
{
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (first_line, last_line) =
        selected_line_range(&document.rope, selection_start, selection_end);
    if first_line == last_line {
        return false;
    }

    let (start, end) = selected_full_line_bounds(&document.rope, selection_start, selection_end);
    let original = document.rope.byte_slice(start..end).to_string();
    let has_trailing_newline = original.ends_with('\n');
    let body = original.strip_suffix('\n').unwrap_or(&original);
    let mut lines = body.split('\n').map(ToOwned::to_owned).collect::<Vec<_>>();
    if lines.len() <= 1 {
        return false;
    }

    transform(&mut lines);

    let mut replacement = lines.join("\n");
    if has_trailing_newline {
        replacement.push('\n');
    }
    if replacement == original {
        return false;
    }

    document.rope.delete(start..end);
    document.rope.insert(start, &replacement);

    document.commit_and_start_new_undo_group();
    document.push_undo(EditTransaction {
        start,
        end,
        deleted_text: original,
        inserted_text: replacement.clone(),
        selections_before: vec![selection],
    });
    document.commit_undo_group();

    set_primary_selection(
        document,
        Selection {
            anchor: start,
            head: start + replacement.len(),
        },
    );
    document.revision += 1;
    true
}

fn join_lines_text(text: &str) -> String {
    let mut lines = text.split('\n');
    let Some(first) = lines.next() else {
        return String::new();
    };

    let mut joined = first.trim_end_matches([' ', '\t']).to_owned();
    for line in lines {
        let next = line.trim_start_matches([' ', '\t']);
        if joined.is_empty()
            || next.is_empty()
            || joined.chars().next_back().is_some_and(char::is_whitespace)
        {
            joined.push_str(next);
        } else {
            joined.push(' ');
            joined.push_str(next);
        }
    }

    joined
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LineMoveDirection {
    Up,
    Down,
}

fn move_selected_lines(document: &mut Document, direction: LineMoveDirection) -> bool {
    clamp_primary_selection(document);
    if document.rope.byte_len() == 0 {
        return false;
    }

    let selection = primary_selection(document);
    let (selection_start, selection_end) = selection_range(selection);
    let (first_line, last_line) =
        selected_line_range(&document.rope, selection_start, selection_end);
    let (selected_start, selected_end) =
        selected_full_line_bounds(&document.rope, selection_start, selection_end);

    let original_text = document.text();

    match direction {
        LineMoveDirection::Up => {
            if first_line == 0 {
                return false;
            }

            let previous_start = byte_of_visual_line(&document.rope, first_line - 1);
            let previous_text = document
                .rope
                .byte_slice(previous_start..selected_start)
                .to_string();
            let selected_text = document
                .rope
                .byte_slice(selected_start..selected_end)
                .to_string();
            let replacement = swap_line_text_up(&selected_text, &previous_text);
            let shift = selected_start - previous_start;

            document.rope.delete(previous_start..selected_end);
            document.rope.insert(previous_start, &replacement);
            set_primary_selection(
                document,
                Selection {
                    anchor: selection.anchor.saturating_sub(shift),
                    head: selection.head.saturating_sub(shift),
                },
            );
        }
        LineMoveDirection::Down => {
            let next_line = last_line + 1;
            if next_line >= document.rope.line_len() {
                return false;
            }

            let next_end = if next_line + 1 < document.rope.line_len() {
                byte_of_visual_line(&document.rope, next_line + 1)
            } else {
                document.rope.byte_len()
            };
            let selected_text = document
                .rope
                .byte_slice(selected_start..selected_end)
                .to_string();
            let next_text = document.rope.byte_slice(selected_end..next_end).to_string();
            let shift = moved_down_selection_shift(&selected_text, &next_text);
            let replacement = swap_line_text_down(&selected_text, &next_text);

            document.rope.delete(selected_start..next_end);
            document.rope.insert(selected_start, &replacement);
            set_primary_selection(
                document,
                Selection {
                    anchor: selection.anchor + shift,
                    head: selection.head + shift,
                },
            );
        }
    }

    document.commit_and_start_new_undo_group();
    document.push_undo(EditTransaction {
        start: 0,
        end: original_text.len(),
        deleted_text: original_text,
        inserted_text: document.text(),
        selections_before: vec![selection],
    });
    document.commit_undo_group();

    document.revision += 1;
    true
}

fn swap_line_text_up(selected_text: &str, previous_text: &str) -> String {
    if !selected_text.ends_with('\n') && previous_text.ends_with('\n') {
        let previous_without_break = previous_text.strip_suffix('\n').unwrap_or(previous_text);
        format!("{selected_text}\n{previous_without_break}")
    } else {
        format!("{selected_text}{previous_text}")
    }
}

fn swap_line_text_down(selected_text: &str, next_text: &str) -> String {
    if selected_text.ends_with('\n') && !next_text.ends_with('\n') {
        let selected_without_break = selected_text.strip_suffix('\n').unwrap_or(selected_text);
        format!("{next_text}\n{selected_without_break}")
    } else {
        format!("{next_text}{selected_text}")
    }
}

fn moved_down_selection_shift(selected_text: &str, next_text: &str) -> usize {
    if selected_text.ends_with('\n') && !next_text.ends_with('\n') {
        next_text.len() + 1
    } else {
        next_text.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IndentChange {
    Indent,
    Outdent,
}

fn change_line_indentation(document: &mut Document, change: IndentChange) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (sel_start, sel_end) = selection_range(selection);
    let (first_line, last_line) = selected_line_range(&document.rope, sel_start, sel_end);

    match change {
        IndentChange::Indent => {
            let line_starts = (first_line..=last_line)
                .map(|line| byte_of_visual_line(&document.rope, line))
                .collect::<Vec<_>>();
            if line_starts.is_empty() {
                return false;
            }

            let original_text = document.text();
            let mut adjusted_selection = selection;
            let mut shift = 0;
            for line_start in line_starts {
                let insert_at = line_start + shift;
                document.rope.insert(insert_at, "    ");
                adjusted_selection.anchor =
                    adjust_offset_after_insert(adjusted_selection.anchor, insert_at, 4);
                adjusted_selection.head =
                    adjust_offset_after_insert(adjusted_selection.head, insert_at, 4);
                shift += 4;
            }

            document.commit_and_start_new_undo_group();
            document.push_undo(EditTransaction {
                start: 0,
                end: original_text.len(),
                deleted_text: original_text,
                inserted_text: document.text(),
                selections_before: vec![selection],
            });
            document.commit_undo_group();

            set_primary_selection(document, adjusted_selection);
            document.revision += 1;
            true
        }
        IndentChange::Outdent => {
            let deletions = (first_line..=last_line)
                .filter_map(|line| outdent_range_for_line(&document.rope, line))
                .collect::<Vec<_>>();
            if deletions.is_empty() {
                return false;
            }

            let original_text = document.text();
            let mut adjusted_selection = selection;
            for (delete_start, delete_end) in deletions.into_iter().rev() {
                document.rope.delete(delete_start..delete_end);
                adjusted_selection.anchor =
                    adjust_offset_after_delete(adjusted_selection.anchor, delete_start, delete_end);
                adjusted_selection.head =
                    adjust_offset_after_delete(adjusted_selection.head, delete_start, delete_end);
            }

            document.commit_and_start_new_undo_group();
            document.push_undo(EditTransaction {
                start: 0,
                end: original_text.len(),
                deleted_text: original_text,
                inserted_text: document.text(),
                selections_before: vec![selection],
            });
            document.commit_undo_group();

            set_primary_selection(document, adjusted_selection);
            document.revision += 1;
            true
        }
    }
}

fn selected_line_range(rope: &Rope, start: usize, end: usize) -> (usize, usize) {
    let first_line = line_index_of_byte(rope, start);
    let mut last_line = line_index_of_byte(rope, end);
    if end > start && end == byte_of_visual_line(rope, last_line) {
        last_line = last_line.saturating_sub(1);
    }
    (first_line, last_line.max(first_line))
}

fn selected_full_line_bounds(rope: &Rope, start: usize, end: usize) -> (usize, usize) {
    let (first_line, last_line) = selected_line_range(rope, start, end);
    let line_start = byte_of_visual_line(rope, first_line);
    let next_line = last_line + 1;
    let line_end = if next_line < rope.line_len() {
        byte_of_visual_line(rope, next_line)
    } else {
        rope.byte_len()
    };
    (line_start, line_end)
}

fn previous_line_break_offset(rope: &Rope, before: usize) -> Option<usize> {
    let before = before.min(rope.byte_len());
    rope.byte_slice(..before)
        .chars()
        .rev()
        .scan(before, |offset, char| {
            *offset -= char.len_utf8();
            Some((*offset, char))
        })
        .find_map(|(offset, char)| (char == '\n').then_some(offset))
}

fn leading_whitespace(rope: &Rope, line_start: usize, limit: usize) -> String {
    let mut indent = String::new();
    for char in rope.byte_slice(line_start..limit).chars() {
        if char == ' ' || char == '\t' {
            indent.push(char);
        } else {
            break;
        }
    }
    indent
}

fn outdent_range_for_line(rope: &Rope, line_index: usize) -> Option<(usize, usize)> {
    let line_start = byte_of_visual_line(rope, line_index);
    let (_, line_end) = visual_line_bounds(rope, line_index);
    let mut delete_end = line_start;
    let mut spaces = 0;

    for char in rope.byte_slice(line_start..line_end).chars() {
        if char == '\t' {
            return Some((line_start, line_start + char.len_utf8()));
        }
        if char != ' ' || spaces == 4 {
            break;
        }
        spaces += 1;
        delete_end += char.len_utf8();
    }

    (delete_end > line_start).then_some((line_start, delete_end))
}

fn adjust_offset_after_insert(offset: usize, insert_at: usize, inserted_len: usize) -> usize {
    if offset >= insert_at {
        offset + inserted_len
    } else {
        offset
    }
}

fn adjust_offset_after_delete(offset: usize, delete_start: usize, delete_end: usize) -> usize {
    if offset <= delete_start {
        offset
    } else if offset >= delete_end {
        offset - (delete_end - delete_start)
    } else {
        delete_start
    }
}

pub fn replace_match(
    document: &mut Document,
    search_match: SearchMatch,
    replacement: &str,
) -> usize {
    let SearchMatch { start, end } = search_match;
    if end > document.rope.byte_len() || start > end {
        return start.min(document.rope.byte_len());
    }

    let deleted_text = document.rope.byte_slice(start..end).to_string();
    let selection_before = primary_selection(document);

    if start != end {
        document.rope.delete(start..end);
    }
    if !replacement.is_empty() {
        document.rope.insert(start, replacement);
    }

    let caret = start + replacement.len();

    document.commit_and_start_new_undo_group();
    document.push_undo(EditTransaction {
        start,
        end,
        deleted_text,
        inserted_text: replacement.to_owned(),
        selections_before: vec![selection_before],
    });
    document.commit_undo_group();

    set_primary_selection(document, Selection::caret(caret));
    document.revision += 1;
    caret
}

pub fn replace_all_matches(
    document: &mut Document,
    matches: &[SearchMatch],
    replacement: &str,
) -> usize {
    if matches.is_empty() {
        return 0;
    }

    let rope_len = document.rope.byte_len();
    let original_text = document.text();
    let selection_before = primary_selection(document);
    let mut count = 0;
    for search_match in matches.iter().rev() {
        let SearchMatch { start, end } = *search_match;
        if end > rope_len || start > end {
            continue;
        }
        if start != end {
            document.rope.delete(start..end);
        }
        if !replacement.is_empty() {
            document.rope.insert(start, replacement);
        }
        count += 1;
    }

    if count > 0 {
        let first_start = matches.first().map(|m| m.start).unwrap_or(0);
        let caret = first_start + replacement.len();
        let caret = caret.min(document.rope.byte_len());

        document.commit_and_start_new_undo_group();
        document.push_undo(EditTransaction {
            start: 0,
            end: original_text.len(),
            deleted_text: original_text,
            inserted_text: document.text(),
            selections_before: vec![selection_before],
        });
        document.commit_undo_group();

        set_primary_selection(document, Selection::caret(caret));
        document.revision += 1;
    }

    count
}

pub fn backspace(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);
    if start != end {
        return replace_selection_with(document, "");
    }
    if start == 0 {
        return false;
    }

    let delete_start = previous_char_boundary(&document.rope, start);
    let deleted_text = document.rope.byte_slice(delete_start..start).to_string();

    document.begin_undo_group();
    document.push_undo(EditTransaction {
        start: delete_start,
        end: start,
        deleted_text,
        inserted_text: String::new(),
        selections_before: vec![selection],
    });

    document.rope.delete(delete_start..start);
    set_primary_selection(document, Selection::caret(delete_start));
    document.revision += 1;
    true
}

pub fn delete(document: &mut Document) -> bool {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let (start, end) = selection_range(selection);
    if start != end {
        return replace_selection_with(document, "");
    }
    if start >= document.rope.byte_len() {
        return false;
    }

    let delete_end = next_char_boundary(&document.rope, start);
    let deleted_text = document.rope.byte_slice(start..delete_end).to_string();

    document.begin_undo_group();
    document.push_undo(EditTransaction {
        start,
        end: delete_end,
        deleted_text,
        inserted_text: String::new(),
        selections_before: vec![selection],
    });

    document.rope.delete(start..delete_end);
    set_primary_selection(document, Selection::caret(start));
    document.revision += 1;
    true
}

pub fn move_left(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let target = if !extend && selection.anchor != selection.head {
        selection_range(selection).0
    } else {
        previous_char_boundary(&document.rope, selection.head)
    };
    apply_motion(document, target, extend);
}

pub fn move_right(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let target = if !extend && selection.anchor != selection.head {
        selection_range(selection).1
    } else {
        next_char_boundary(&document.rope, selection.head)
    };
    apply_motion(document, target, extend);
}

pub fn move_word_left(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let from = if !extend && selection.anchor != selection.head {
        selection_range(selection).0
    } else {
        selection.head
    };
    let target = previous_word_boundary(&document.rope, from);
    apply_motion(document, target, extend);
}

pub fn move_word_right(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let from = if !extend && selection.anchor != selection.head {
        selection_range(selection).1
    } else {
        selection.head
    };
    let target = next_word_boundary(&document.rope, from);
    apply_motion(document, target, extend);
}

pub fn move_home(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let line = line_index_of_byte(&document.rope, primary_selection(document).head);
    let target = byte_of_visual_line(&document.rope, line);
    apply_motion(document, target, extend);
}

pub fn move_end(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let line = line_index_of_byte(&document.rope, primary_selection(document).head);
    let (_, end) = visual_line_bounds(&document.rope, line);
    apply_motion(document, end, extend);
}

pub fn move_vertical(
    document: &mut Document,
    view_state: &mut EditorViewState,
    delta: isize,
    extend: bool,
) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let current_line = line_index_of_byte(&document.rope, selection.head);
    let line_count = visual_line_count(&document.rope);
    let target_line = (current_line as isize + delta).clamp(0, line_count as isize - 1) as usize;
    let column = view_state
        .preferred_column
        .unwrap_or_else(|| column_of_byte(&document.rope, selection.head));
    view_state.preferred_column = Some(column);
    let target = byte_for_line_column(&document.rope, target_line, column);
    apply_motion(document, target, extend);
}

pub fn move_document_start(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    apply_motion(document, 0, extend);
}

pub fn move_document_end(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    apply_motion(document, document.rope.byte_len(), extend);
}

pub fn move_paragraph_up(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let head = primary_selection(document).head;
    let current_line = line_index_of_byte(&document.rope, head);
    let target = (0..current_line)
        .rev()
        .find(|line| is_blank_line(&document.rope, *line))
        .map(|line| byte_of_visual_line(&document.rope, line))
        .unwrap_or(0);
    apply_motion(document, target, extend);
}

pub fn move_paragraph_down(document: &mut Document, extend: bool) {
    clamp_primary_selection(document);
    let head = primary_selection(document).head;
    let current_line = line_index_of_byte(&document.rope, head);
    let line_count = visual_line_count(&document.rope);
    let target = ((current_line + 1)..line_count)
        .find(|line| is_blank_line(&document.rope, *line))
        .map(|line| byte_of_visual_line(&document.rope, line))
        .unwrap_or_else(|| document.rope.byte_len());
    apply_motion(document, target, extend);
}

pub fn move_page(
    document: &mut Document,
    view_state: &mut EditorViewState,
    delta_pages: isize,
    extend: bool,
) {
    clamp_primary_selection(document);
    let selection = primary_selection(document);
    let current_line = line_index_of_byte(&document.rope, selection.head);
    let line_count = visual_line_count(&document.rope);
    let visible_rows = view_state.visible_rows.unwrap_or(1).max(1);
    let step = visible_rows.saturating_sub(1).max(1) as isize;
    let target_line =
        (current_line as isize + delta_pages * step).clamp(0, line_count as isize - 1) as usize;
    let column = view_state
        .preferred_column
        .unwrap_or_else(|| column_of_byte(&document.rope, selection.head));
    view_state.preferred_column = Some(column);
    let target = byte_for_line_column(&document.rope, target_line, column);
    apply_motion(document, target, extend);
}

fn is_blank_line(rope: &Rope, line_index: usize) -> bool {
    if line_index >= rope.line_len() {
        return true;
    }
    rope.line(line_index).chars().all(|c| c.is_whitespace())
}

fn apply_motion(document: &mut Document, target: usize, extend: bool) {
    let selection = primary_selection(document);
    let new = if extend {
        Selection {
            anchor: selection.anchor,
            head: target,
        }
    } else {
        Selection::caret(target)
    };
    set_primary_selection(document, new);
}

pub fn set_primary_selection(document: &mut Document, selection: Selection) {
    let selection = clamp_selection_to_rope(&document.rope, selection);
    if let Some(primary) = document.selections.first_mut() {
        *primary = selection;
    } else {
        document.selections.push(selection);
    }
}

pub fn primary_selection(document: &Document) -> Selection {
    document
        .selections
        .first()
        .copied()
        .unwrap_or_else(|| Selection::caret(0))
}

pub fn visual_line_count(rope: &Rope) -> usize {
    let base = rope.line_len().max(1);
    if has_trailing_newline(rope) {
        base + 1
    } else {
        base
    }
}

pub fn decimal_digits(value: usize) -> usize {
    value
        .checked_ilog10()
        .map_or(1, |digits| digits as usize + 1)
}

fn clamp_primary_selection(document: &mut Document) {
    let selection = primary_selection(document);
    set_primary_selection(document, selection);
}

fn clamp_selection_to_rope(rope: &Rope, selection: Selection) -> Selection {
    Selection {
        anchor: clamp_to_char_boundary(rope, selection.anchor.min(rope.byte_len())),
        head: clamp_to_char_boundary(rope, selection.head.min(rope.byte_len())),
    }
}

fn clamp_to_char_boundary(rope: &Rope, mut offset: usize) -> usize {
    while offset > 0 && !rope.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn selection_range(selection: Selection) -> (usize, usize) {
    if selection.anchor <= selection.head {
        (selection.anchor, selection.head)
    } else {
        (selection.head, selection.anchor)
    }
}

pub fn word_at_selection(rope: &Rope, selection: Selection) -> Option<(usize, usize)> {
    let (start, end) = selection_range(selection);
    if start != end {
        return Some((start, end));
    }

    let offset = start;
    if offset >= rope.byte_len() {
        return None;
    }

    let char_at_caret = rope.byte_slice(offset..).chars().next();
    let Some(char_at_caret) = char_at_caret else {
        return None;
    };

    if classify_char(char_at_caret) == CharClass::NonWord {
        return None;
    }

    let mut word_start = offset;
    let mut search_offset = offset;
    loop {
        if search_offset == 0 {
            break;
        }
        let prev_char = rope.byte_slice(..search_offset).chars().next_back();
        match prev_char {
            Some(c) if classify_char(c) == CharClass::Word => {
                search_offset -= c.len_utf8();
                word_start = search_offset;
            }
            _ => break,
        }
    }

    let mut word_end = offset;
    let mut chars_after = rope.byte_slice(offset..).chars();
    if let Some(c) = chars_after.next() {
        word_end += c.len_utf8();
    }
    for c in chars_after {
        if classify_char(c) == CharClass::Word {
            word_end += c.len_utf8();
        } else {
            break;
        }
    }

    if word_start < word_end {
        Some((word_start, word_end))
    } else {
        None
    }
}

fn previous_char_boundary(rope: &Rope, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let offset = clamp_to_char_boundary(rope, offset.min(rope.byte_len()));
    rope.byte_slice(..offset)
        .chars()
        .next_back()
        .map_or(0, |char| offset - char.len_utf8())
}

fn next_char_boundary(rope: &Rope, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(rope, offset.min(rope.byte_len()));
    if offset >= rope.byte_len() {
        return rope.byte_len();
    }
    rope.byte_slice(offset..)
        .chars()
        .next()
        .map_or(rope.byte_len(), |char| offset + char.len_utf8())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CharClass {
    Word,
    NonWord,
}

fn classify_char(c: char) -> CharClass {
    if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::NonWord
    }
}

fn next_word_boundary(rope: &Rope, start: usize) -> usize {
    let total = rope.byte_len();
    let mut offset = clamp_to_char_boundary(rope, start.min(total));
    if offset >= total {
        return total;
    }

    let mut chars = rope.byte_slice(offset..).chars();
    let lead = loop {
        match chars.next() {
            Some(c) if c.is_whitespace() => offset += c.len_utf8(),
            Some(c) => break c,
            None => return total,
        }
    };
    let class = classify_char(lead);
    offset += lead.len_utf8();
    for c in chars {
        if c.is_whitespace() || classify_char(c) != class {
            break;
        }
        offset += c.len_utf8();
    }
    offset
}

fn previous_word_boundary(rope: &Rope, start: usize) -> usize {
    let mut offset = clamp_to_char_boundary(rope, start.min(rope.byte_len()));
    if offset == 0 {
        return 0;
    }

    let mut chars = rope.byte_slice(..offset).chars();
    let lead = loop {
        match chars.next_back() {
            Some(c) if c.is_whitespace() => offset -= c.len_utf8(),
            Some(c) => break c,
            None => return 0,
        }
    };
    let class = classify_char(lead);
    offset -= lead.len_utf8();
    while let Some(c) = chars.next_back() {
        if c.is_whitespace() || classify_char(c) != class {
            break;
        }
        offset -= c.len_utf8();
    }
    offset
}

fn has_trailing_newline(rope: &Rope) -> bool {
    rope.byte_len() > 0 && rope.byte(rope.byte_len() - 1) == b'\n'
}

fn byte_of_visual_line(rope: &Rope, line_index: usize) -> usize {
    if rope.line_len() == 0 || line_index >= rope.line_len() {
        rope.byte_len()
    } else {
        rope.byte_of_line(line_index)
    }
}

fn visual_line_bounds(rope: &Rope, line_index: usize) -> (usize, usize) {
    if rope.line_len() == 0 || line_index >= rope.line_len() {
        let end = rope.byte_len();
        return (end, end);
    }

    let start = rope.byte_of_line(line_index);
    let end = start + rope.line(line_index).byte_len();
    (start, end)
}

fn visual_line_text(rope: &Rope, line_index: usize) -> String {
    if rope.line_len() == 0 || line_index >= rope.line_len() {
        String::new()
    } else {
        rope.line(line_index).to_string()
    }
}

fn line_index_of_byte(rope: &Rope, offset: usize) -> usize {
    let offset = offset.min(rope.byte_len());
    let line = if rope.byte_len() == 0 {
        0
    } else {
        rope.line_of_byte(offset)
    };
    line.min(visual_line_count(rope).saturating_sub(1))
}

fn column_of_byte(rope: &Rope, offset: usize) -> usize {
    let offset = clamp_to_char_boundary(rope, offset.min(rope.byte_len()));
    let line = line_index_of_byte(rope, offset);
    let line_start = byte_of_visual_line(rope, line);
    rope.byte_slice(line_start..offset).chars().count()
}

fn offset_at_pointer(
    rope: &Rope,
    pos: egui::Pos2,
    rect: egui::Rect,
    text_origin_x: f32,
    row_height: f32,
    char_width: f32,
    line_count: usize,
) -> usize {
    let line = ((pos.y - rect.top()).max(0.0) / row_height) as usize;
    let line = line.min(line_count.saturating_sub(1));
    let column = ((pos.x - (rect.left() + text_origin_x)) / char_width)
        .round()
        .max(0.0) as usize;
    byte_for_line_column(rope, line, column)
}

fn byte_for_line_column(rope: &Rope, line_index: usize, column: usize) -> usize {
    let (start, end) = visual_line_bounds(rope, line_index);
    let mut offset = start;
    for (current_column, char) in rope.byte_slice(start..end).chars().enumerate() {
        if current_column >= column {
            break;
        }
        offset += char.len_utf8();
    }
    offset
}

fn caret_position(
    rope: &Rope,
    offset: usize,
    text_left: f32,
    content_top: f32,
    row_height: f32,
    char_width: f32,
) -> egui::Pos2 {
    let line = line_index_of_byte(rope, offset);
    let column = column_of_byte(rope, offset);
    egui::pos2(
        text_left + column as f32 * char_width,
        content_top + line as f32 * row_height,
    )
}

fn monospace_char_width(ui: &egui::Ui, font_id: egui::FontId) -> f32 {
    ui.painter()
        .layout_no_wrap("m".to_owned(), font_id, ui.visuals().text_color())
        .size()
        .x
        .max(1.0)
}

fn paint_selection_for_line(
    painter: &egui::Painter,
    document: &Document,
    selection: Selection,
    line_index: usize,
    text_pos: egui::Pos2,
    row_height: f32,
    char_width: f32,
    color: egui::Color32,
) {
    let (selection_start, selection_end) = selection_range(selection);
    if selection_start == selection_end {
        return;
    }

    let (line_start, line_end) = visual_line_bounds(&document.rope, line_index);
    let start = selection_start.max(line_start);
    let end = selection_end.min(line_end);
    if start >= end {
        return;
    }

    let start_column = document.rope.byte_slice(line_start..start).chars().count();
    let end_column = document.rope.byte_slice(line_start..end).chars().count();
    let min = egui::pos2(text_pos.x + start_column as f32 * char_width, text_pos.y);
    let max = egui::pos2(
        text_pos.x + end_column as f32 * char_width,
        text_pos.y + row_height,
    );
    painter.rect_filled(egui::Rect::from_min_max(min, max), 0.0, color);
}

fn paint_search_highlights_for_line(
    painter: &egui::Painter,
    rope: &Rope,
    highlights: &[SearchHighlight],
    line_index: usize,
    current_only: bool,
    text_pos: egui::Pos2,
    row_height: f32,
    char_width: f32,
    visuals: &egui::Visuals,
) {
    let normal_fill = visuals.warn_fg_color.gamma_multiply(0.30);
    let current_fill = visuals.warn_fg_color.gamma_multiply(0.55);
    let current_stroke = egui::Stroke::new(1.0, visuals.warn_fg_color);

    for highlight in highlights {
        if highlight.is_current != current_only {
            continue;
        }

        let Some(span) = highlight_columns_for_line(rope, *highlight, line_index) else {
            continue;
        };

        let min = egui::pos2(
            text_pos.x + span.start_column as f32 * char_width,
            text_pos.y,
        );
        let max = egui::pos2(
            text_pos.x + span.end_column as f32 * char_width,
            text_pos.y + row_height,
        );
        let rect = egui::Rect::from_min_max(min, max);
        if highlight.is_current {
            painter.rect_filled(rect, 0.0, current_fill);
            painter.rect_stroke(rect, 0.0, current_stroke, egui::StrokeKind::Inside);
        } else {
            painter.rect_filled(rect, 0.0, normal_fill);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HighlightColumns {
    start_column: usize,
    end_column: usize,
}

fn highlight_columns_for_line(
    rope: &Rope,
    highlight: SearchHighlight,
    line_index: usize,
) -> Option<HighlightColumns> {
    if highlight.start >= highlight.end {
        return None;
    }

    let (line_start, line_end) = visual_line_bounds(rope, line_index);
    let start = highlight.start.max(line_start).min(line_end);
    let end = highlight.end.min(line_end).max(line_start);
    if start >= end {
        return None;
    }

    Some(HighlightColumns {
        start_column: rope.byte_slice(line_start..start).chars().count(),
        end_column: rope.byte_slice(line_start..end).chars().count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> Document {
        let mut document = Document::new_untitled(1);
        document.replace_text(text);
        document.selections = vec![Selection::caret(0)];
        document.revision = 0;
        document
    }

    #[test]
    fn typing_and_paste_insert_at_caret() {
        let mut document = document("");

        assert!(replace_selection_with(&mut document, "hi"));
        assert_eq!(document.text(), "hi");
        assert_eq!(primary_selection(&document), Selection::caret(2));

        assert!(replace_selection_with(&mut document, "\nthere"));
        assert_eq!(document.text(), "hi\nthere");
        assert_eq!(primary_selection(&document), Selection::caret(8));
    }

    #[test]
    fn typing_replaces_selected_range() {
        let mut document = document("hello world");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 6,
                head: 11,
            },
        );

        assert!(replace_selection_with(&mut document, "pile"));

        assert_eq!(document.text(), "hello pile");
        assert_eq!(primary_selection(&document), Selection::caret(10));
    }

    #[test]
    fn newline_preserves_current_line_indent() {
        let mut document = document("fn main() {\n    let value = 1;");
        let end = document.rope.byte_len();
        set_primary_selection(&mut document, Selection::caret(end));

        assert!(insert_newline_with_auto_indent(&mut document));

        assert_eq!(document.text(), "fn main() {\n    let value = 1;\n    ");
        assert_eq!(
            primary_selection(&document),
            Selection::caret(document.rope.byte_len())
        );
    }

    #[test]
    fn newline_replaces_selection_and_uses_selection_start_indent() {
        let mut document = document("    first selected\n    second selected");
        let start = "    fi".len();
        let end = "    first selected\n    second sele".len();
        set_primary_selection(
            &mut document,
            Selection {
                anchor: start,
                head: end,
            },
        );

        assert!(insert_newline_with_auto_indent(&mut document));

        assert_eq!(document.text(), "    fi\n    cted");
        assert_eq!(
            primary_selection(&document),
            Selection::caret("    fi\n    ".len())
        );
    }

    #[test]
    fn indent_at_caret_indents_current_line() {
        let mut document = document("alpha\nbeta");
        set_primary_selection(&mut document, Selection::caret("alpha\nbe".len()));

        assert!(indent_selection(&mut document));

        assert_eq!(document.text(), "alpha\n    beta");
        assert_eq!(
            primary_selection(&document),
            Selection::caret("alpha\n    be".len())
        );
    }

    #[test]
    fn indent_selection_indents_touched_lines() {
        let mut document = document("one\ntwo\nthree");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 1,
                head: "one\ntwo".len(),
            },
        );

        assert!(indent_selection(&mut document));

        assert_eq!(document.text(), "    one\n    two\nthree");
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 5,
                head: "    one\n    two".len()
            }
        );
    }

    #[test]
    fn indent_selection_excludes_line_at_selection_end_boundary() {
        let mut document = document("one\ntwo\nthree");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 0,
                head: "one\ntwo\n".len(),
            },
        );

        assert!(indent_selection(&mut document));

        assert_eq!(document.text(), "    one\n    two\nthree");
    }

    #[test]
    fn outdent_selection_removes_tabs_or_up_to_four_spaces() {
        let mut document = document("    one\n  two\n\tthree\nfour");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 0,
                head: "    one\n  two\n\tthree".len(),
            },
        );

        assert!(outdent_selection(&mut document));

        assert_eq!(document.text(), "one\ntwo\nthree\nfour");
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 0,
                head: "one\ntwo\nthree".len()
            }
        );
    }

    #[test]
    fn outdent_without_leading_whitespace_is_noop() {
        let mut document = document("alpha\nbeta");
        let revision_before = document.revision;
        set_primary_selection(&mut document, Selection::caret(2));

        assert!(!outdent_selection(&mut document));

        assert_eq!(document.text(), "alpha\nbeta");
        assert_eq!(document.revision, revision_before);
    }

    #[test]
    fn duplicate_line_at_caret_copies_current_line_below() {
        let mut document = document("one\ntwo");
        set_primary_selection(&mut document, Selection::caret("one\nt".len()));

        assert!(duplicate_selected_lines(&mut document));

        assert_eq!(document.text(), "one\ntwo\ntwo");
        assert_eq!(
            primary_selection(&document),
            Selection::caret("one\ntwo\nt".len())
        );
    }

    #[test]
    fn duplicate_selected_lines_preserves_selection_shape() {
        let mut document = document("one\ntwo\nthree");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 1,
                head: "one\ntw".len(),
            },
        );

        assert!(duplicate_selected_lines(&mut document));

        assert_eq!(document.text(), "one\ntwo\none\ntwo\nthree");
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: "one\ntwo\n".len() + 1,
                head: "one\ntwo\none\ntw".len()
            }
        );
    }

    #[test]
    fn duplicate_empty_document_creates_blank_line() {
        let mut document = document("");

        assert!(duplicate_selected_lines(&mut document));

        assert_eq!(document.text(), "\n");
        assert_eq!(primary_selection(&document), Selection::caret(1));
    }

    #[test]
    fn delete_line_at_caret_removes_current_line() {
        let mut document = document("one\ntwo\nthree");
        set_primary_selection(&mut document, Selection::caret("one\nt".len()));

        assert!(delete_selected_lines(&mut document));

        assert_eq!(document.text(), "one\nthree");
        assert_eq!(
            primary_selection(&document),
            Selection::caret("one\n".len())
        );
    }

    #[test]
    fn delete_selected_lines_removes_touched_lines() {
        let mut document = document("one\ntwo\nthree\nfour");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: "one\n".len(),
                head: "one\ntwo\nthr".len(),
            },
        );

        assert!(delete_selected_lines(&mut document));

        assert_eq!(document.text(), "one\nfour");
        assert_eq!(
            primary_selection(&document),
            Selection::caret("one\n".len())
        );
    }

    #[test]
    fn delete_last_line_removes_preceding_line_break() {
        let mut document = document("one\ntwo");
        set_primary_selection(&mut document, Selection::caret("one\nt".len()));

        assert!(delete_selected_lines(&mut document));

        assert_eq!(document.text(), "one");
        assert_eq!(primary_selection(&document), Selection::caret("one".len()));
    }

    #[test]
    fn delete_line_excludes_line_at_selection_end_boundary() {
        let mut document = document("one\ntwo\nthree");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 0,
                head: "one\ntwo\n".len(),
            },
        );

        assert!(delete_selected_lines(&mut document));

        assert_eq!(document.text(), "three");
        assert_eq!(primary_selection(&document), Selection::caret(0));
    }

    #[test]
    fn move_line_up_swaps_with_previous_line() {
        let mut document = document("one\ntwo\nthree");
        set_primary_selection(&mut document, Selection::caret("one\ntw".len()));

        assert!(move_selected_lines_up(&mut document));

        assert_eq!(document.text(), "two\none\nthree");
        assert_eq!(primary_selection(&document), Selection::caret("tw".len()));
    }

    #[test]
    fn move_line_up_at_document_start_is_noop() {
        let mut document = document("one\ntwo");
        let revision_before = document.revision;
        set_primary_selection(&mut document, Selection::caret(1));

        assert!(!move_selected_lines_up(&mut document));

        assert_eq!(document.text(), "one\ntwo");
        assert_eq!(document.revision, revision_before);
        assert_eq!(primary_selection(&document), Selection::caret(1));
    }

    #[test]
    fn move_last_line_up_preserves_line_break_between_lines() {
        let mut document = document("one\ntwo");
        set_primary_selection(&mut document, Selection::caret("one\ntw".len()));

        assert!(move_selected_lines_up(&mut document));

        assert_eq!(document.text(), "two\none");
        assert_eq!(primary_selection(&document), Selection::caret("tw".len()));
    }

    #[test]
    fn move_selected_lines_up_preserves_selection_shape() {
        let mut document = document("zero\none\ntwo\nthree");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: "zero\no".len(),
                head: "zero\none\ntw".len(),
            },
        );

        assert!(move_selected_lines_up(&mut document));

        assert_eq!(document.text(), "one\ntwo\nzero\nthree");
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: "o".len(),
                head: "one\ntw".len()
            }
        );
    }

    #[test]
    fn move_line_down_swaps_with_next_line() {
        let mut document = document("one\ntwo\nthree");
        set_primary_selection(&mut document, Selection::caret("on".len()));

        assert!(move_selected_lines_down(&mut document));

        assert_eq!(document.text(), "two\none\nthree");
        assert_eq!(
            primary_selection(&document),
            Selection::caret("two\non".len())
        );
    }

    #[test]
    fn move_line_down_at_document_end_is_noop() {
        let mut document = document("one\ntwo");
        let revision_before = document.revision;
        set_primary_selection(&mut document, Selection::caret("one\nt".len()));

        assert!(!move_selected_lines_down(&mut document));

        assert_eq!(document.text(), "one\ntwo");
        assert_eq!(document.revision, revision_before);
        assert_eq!(
            primary_selection(&document),
            Selection::caret("one\nt".len())
        );
    }

    #[test]
    fn move_line_down_over_last_line_preserves_line_break_between_lines() {
        let mut document = document("one\ntwo");
        set_primary_selection(&mut document, Selection::caret("on".len()));

        assert!(move_selected_lines_down(&mut document));

        assert_eq!(document.text(), "two\none");
        assert_eq!(
            primary_selection(&document),
            Selection::caret("two\non".len())
        );
    }

    #[test]
    fn move_selected_lines_down_preserves_selection_shape() {
        let mut document = document("zero\none\ntwo\nthree");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: "zero\no".len(),
                head: "zero\none\ntw".len(),
            },
        );

        assert!(move_selected_lines_down(&mut document));

        assert_eq!(document.text(), "zero\nthree\none\ntwo");
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: "zero\nthree\no".len(),
                head: "zero\nthree\none\ntw".len()
            }
        );
    }

    #[test]
    fn join_line_at_caret_merges_with_next_line() {
        let mut document = document("one\n  two\nthree");
        set_primary_selection(&mut document, Selection::caret("on".len()));

        assert!(join_selected_lines(&mut document));

        assert_eq!(document.text(), "one two\nthree");
        assert_eq!(
            primary_selection(&document),
            Selection::caret("one two".len())
        );
    }

    #[test]
    fn join_selected_lines_merges_all_touched_lines() {
        let mut document = document("one\n  two\n\tthree\nfour");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 1,
                head: "one\n  two\n\tthr".len(),
            },
        );

        assert!(join_selected_lines(&mut document));

        assert_eq!(document.text(), "one two three\nfour");
        assert_eq!(
            primary_selection(&document),
            Selection::caret("one two three".len())
        );
    }

    #[test]
    fn join_line_avoids_extra_space_for_empty_sides() {
        let mut document = document("one\n\n  two");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 0,
                head: "one\n\n  tw".len(),
            },
        );

        assert!(join_selected_lines(&mut document));

        assert_eq!(document.text(), "one two");
        assert_eq!(
            primary_selection(&document),
            Selection::caret("one two".len())
        );
    }

    #[test]
    fn join_line_trims_trailing_horizontal_whitespace() {
        let mut document = document("one   \n\t two");
        set_primary_selection(&mut document, Selection::caret(1));

        assert!(join_selected_lines(&mut document));

        assert_eq!(document.text(), "one two");
    }

    #[test]
    fn join_last_line_is_noop() {
        let mut document = document("one\ntwo");
        let revision_before = document.revision;
        set_primary_selection(&mut document, Selection::caret("one\nt".len()));

        assert!(!join_selected_lines(&mut document));

        assert_eq!(document.text(), "one\ntwo");
        assert_eq!(document.revision, revision_before);
        assert_eq!(
            primary_selection(&document),
            Selection::caret("one\nt".len())
        );
    }

    #[test]
    fn sort_selected_lines_orders_touched_lines() {
        let mut document = document("gamma\nalpha\nbeta\nomega");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 1,
                head: "gamma\nalpha\nbe".len(),
            },
        );

        assert!(sort_selected_lines(&mut document));

        assert_eq!(document.text(), "alpha\nbeta\ngamma\nomega");
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 0,
                head: "alpha\nbeta\ngamma\n".len()
            }
        );
    }

    #[test]
    fn sort_selected_lines_excludes_line_at_selection_end_boundary() {
        let mut document = document("b\na\nc");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 0,
                head: "b\na\n".len(),
            },
        );

        assert!(sort_selected_lines(&mut document));

        assert_eq!(document.text(), "a\nb\nc");
    }

    #[test]
    fn sort_selected_lines_without_trailing_newline_keeps_none() {
        let mut document = document("b\na");
        let end = document.rope.byte_len();
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 0,
                head: end,
            },
        );

        assert!(sort_selected_lines(&mut document));

        assert_eq!(document.text(), "a\nb");
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 0,
                head: "a\nb".len()
            }
        );
    }

    #[test]
    fn sort_selected_lines_noops_for_single_or_already_sorted_lines() {
        let mut document = document("a\nb\nc");
        let revision_before = document.revision;
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 0,
                head: "a\nb\n".len(),
            },
        );

        assert!(!sort_selected_lines(&mut document));

        assert_eq!(document.text(), "a\nb\nc");
        assert_eq!(document.revision, revision_before);

        set_primary_selection(&mut document, Selection::caret(1));
        assert!(!sort_selected_lines(&mut document));
    }

    #[test]
    fn backspace_and_delete_handle_boundaries_and_lines() {
        let mut document = document("ab\ncd");
        set_primary_selection(&mut document, Selection::caret(3));

        assert!(backspace(&mut document));
        assert_eq!(document.text(), "abcd");
        assert_eq!(primary_selection(&document), Selection::caret(2));

        assert!(delete(&mut document));
        assert_eq!(document.text(), "abd");
        assert_eq!(primary_selection(&document), Selection::caret(2));

        set_primary_selection(&mut document, Selection::caret(0));
        assert!(!backspace(&mut document));
        let end = document.rope.byte_len();
        set_primary_selection(&mut document, Selection::caret(end));
        assert!(!delete(&mut document));
    }

    #[test]
    fn movement_respects_multibyte_char_boundaries() {
        let mut document = document("aé日");
        let end = document.rope.byte_len();
        set_primary_selection(&mut document, Selection::caret(end));

        move_left(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(3));
        move_left(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(1));
        move_right(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(3));
    }

    #[test]
    fn vertical_and_line_boundary_movement_tracks_columns() {
        let mut document = document("abc\nde\nfghi");
        let mut view_state = EditorViewState::default();
        set_primary_selection(&mut document, Selection::caret(3));

        move_vertical(&mut document, &mut view_state, 1, false);
        assert_eq!(primary_selection(&document), Selection::caret(6));

        move_vertical(&mut document, &mut view_state, 1, false);
        assert_eq!(primary_selection(&document), Selection::caret(10));

        move_home(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(7));

        move_end(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(11));
    }

    #[test]
    fn shift_arrow_extends_selection() {
        let mut document = document("hello world");
        set_primary_selection(&mut document, Selection::caret(3));

        move_right(&mut document, true);
        move_right(&mut document, true);
        assert_eq!(
            primary_selection(&document),
            Selection { anchor: 3, head: 5 }
        );

        move_left(&mut document, true);
        assert_eq!(
            primary_selection(&document),
            Selection { anchor: 3, head: 4 }
        );
    }

    #[test]
    fn shift_home_end_extend_to_line_bounds() {
        let mut document = document("hello world");
        set_primary_selection(&mut document, Selection::caret(6));

        move_home(&mut document, true);
        assert_eq!(
            primary_selection(&document),
            Selection { anchor: 6, head: 0 }
        );

        set_primary_selection(&mut document, Selection::caret(6));
        move_end(&mut document, true);
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 6,
                head: 11
            }
        );
    }

    #[test]
    fn shift_vertical_preserves_anchor_and_preferred_column() {
        let mut document = document("abcd\nef\nghij");
        let mut view_state = EditorViewState::default();
        set_primary_selection(&mut document, Selection::caret(3));

        move_vertical(&mut document, &mut view_state, 1, true);
        assert_eq!(
            primary_selection(&document),
            Selection { anchor: 3, head: 7 }
        );
        assert_eq!(view_state.preferred_column, Some(3));

        move_vertical(&mut document, &mut view_state, 1, true);
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 3,
                head: 11
            }
        );
    }

    #[test]
    fn move_word_right_skips_whitespace_then_word() {
        let mut document = document("  foo bar");
        set_primary_selection(&mut document, Selection::caret(0));

        move_word_right(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(5));

        move_word_right(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(9));
    }

    #[test]
    fn move_word_right_stops_at_punctuation() {
        let mut document = document("foo, bar");
        set_primary_selection(&mut document, Selection::caret(0));

        move_word_right(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(3));

        move_word_right(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(4));

        move_word_right(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(8));
    }

    #[test]
    fn move_word_left_symmetric() {
        let mut document = document("  foo bar");
        let end = document.rope.byte_len();
        set_primary_selection(&mut document, Selection::caret(end));

        move_word_left(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(6));

        move_word_left(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(2));

        move_word_left(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(0));
    }

    #[test]
    fn move_word_right_unicode_lands_on_char_boundary() {
        let text = "héllo wörld";
        let mut document = document(text);
        set_primary_selection(&mut document, Selection::caret(0));

        move_word_right(&mut document, false);
        let after_first = "héllo".len();
        assert_eq!(primary_selection(&document), Selection::caret(after_first));
        assert!(document.rope.is_char_boundary(after_first));

        move_word_right(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(text.len()));
    }

    #[test]
    fn move_word_extends_selection() {
        let mut document = document("foo bar baz");
        set_primary_selection(&mut document, Selection::caret(0));

        move_word_right(&mut document, true);
        assert_eq!(
            primary_selection(&document),
            Selection { anchor: 0, head: 3 }
        );

        move_word_right(&mut document, true);
        assert_eq!(
            primary_selection(&document),
            Selection { anchor: 0, head: 7 }
        );
    }

    #[test]
    fn word_motion_at_document_edges_is_noop() {
        let mut document = document("foo");
        set_primary_selection(&mut document, Selection::caret(0));
        move_word_left(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(0));

        let end = document.rope.byte_len();
        set_primary_selection(&mut document, Selection::caret(end));
        move_word_right(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(end));
    }

    #[test]
    fn visual_lines_include_empty_document_and_trailing_newline() {
        assert_eq!(visual_line_count(&Rope::from("")), 1);
        assert_eq!(visual_line_count(&Rope::from("a\n")), 2);
        assert_eq!(byte_for_line_column(&Rope::from("a\n"), 1, 0), 2);
    }

    #[test]
    fn search_highlight_columns_clip_to_visual_line() {
        let rope = Rope::from("abc\ndef");

        assert_eq!(
            highlight_columns_for_line(
                &rope,
                SearchHighlight {
                    start: 1,
                    end: 3,
                    is_current: false,
                },
                0,
            ),
            Some(HighlightColumns {
                start_column: 1,
                end_column: 3,
            })
        );
        assert_eq!(
            highlight_columns_for_line(
                &rope,
                SearchHighlight {
                    start: 1,
                    end: 3,
                    is_current: false,
                },
                1,
            ),
            None
        );
    }

    #[test]
    fn search_highlight_columns_split_multiline_matches() {
        let rope = Rope::from("abc\ndef");
        let highlight = SearchHighlight {
            start: 2,
            end: 6,
            is_current: true,
        };

        assert_eq!(
            highlight_columns_for_line(&rope, highlight, 0),
            Some(HighlightColumns {
                start_column: 2,
                end_column: 3,
            })
        );
        assert_eq!(
            highlight_columns_for_line(&rope, highlight, 1),
            Some(HighlightColumns {
                start_column: 0,
                end_column: 2,
            })
        );
    }

    #[test]
    fn search_highlight_columns_count_multibyte_characters() {
        let rope = Rope::from("aé日z");
        let highlight = SearchHighlight {
            start: 1,
            end: "aé日".len(),
            is_current: false,
        };

        assert_eq!(
            highlight_columns_for_line(&rope, highlight, 0),
            Some(HighlightColumns {
                start_column: 1,
                end_column: 3,
            })
        );
    }

    #[test]
    fn search_selection_is_clamped_to_valid_byte_offsets() {
        let mut document = document("aé日");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 2,
                head: 999,
            },
        );

        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 1,
                head: document.rope.byte_len(),
            }
        );
    }

    #[test]
    fn offset_at_pointer_maps_clicks_to_byte_offsets() {
        let document = document("abc\nhello\nworld");
        let rope = &document.rope;
        let line_count = visual_line_count(rope);
        let row_height = 10.0;
        let char_width = 8.0;
        let text_origin_x = 20.0;
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 200.0));

        let pointer_at = |x: f32, y: f32| {
            offset_at_pointer(
                rope,
                egui::pos2(x, y),
                rect,
                text_origin_x,
                row_height,
                char_width,
                line_count,
            )
        };

        assert_eq!(pointer_at(text_origin_x - 50.0, 0.0), 0);
        assert_eq!(pointer_at(text_origin_x + char_width * 2.0, 0.0), 2);
        assert_eq!(
            pointer_at(text_origin_x + char_width * 100.0, row_height * 1.5),
            "abc\nhello".len()
        );
        assert_eq!(
            pointer_at(text_origin_x, row_height * 50.0),
            "abc\nhello\n".len()
        );
    }

    #[test]
    fn document_boundary_motion_jumps_to_doc_ends() {
        let text = "abc\ndef\nghi";
        let mut document = document(text);
        set_primary_selection(&mut document, Selection::caret(5));

        move_document_start(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(0));

        move_document_end(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(text.len()));
    }

    #[test]
    fn document_boundary_motion_extends_selection() {
        let text = "abc\ndef\nghi";
        let mut document = document(text);
        set_primary_selection(&mut document, Selection::caret(5));

        move_document_end(&mut document, true);
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 5,
                head: text.len()
            }
        );

        move_document_start(&mut document, true);
        assert_eq!(
            primary_selection(&document),
            Selection { anchor: 5, head: 0 }
        );
    }

    #[test]
    fn paragraph_motion_jumps_blank_line_boundaries() {
        // line indices: 0:"first" 1:"more" 2:"" 3:"second" 4:"two" 5:"" 6:"third"
        let text = "first\nmore\n\nsecond\ntwo\n\nthird";
        let mut document = document(text);

        // From caret on line 0, paragraph_down lands on the blank between "more" and "second".
        set_primary_selection(&mut document, Selection::caret(2));
        move_paragraph_down(&mut document, false);
        let blank_one = "first\nmore\n".len();
        assert_eq!(primary_selection(&document), Selection::caret(blank_one));

        // Next paragraph_down lands on the blank before "third".
        move_paragraph_down(&mut document, false);
        let blank_two = "first\nmore\n\nsecond\ntwo\n".len();
        assert_eq!(primary_selection(&document), Selection::caret(blank_two));

        // Past the last blank, paragraph_down clamps to EOF.
        move_paragraph_down(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(text.len()));

        // From EOF, paragraph_up walks back through blanks.
        move_paragraph_up(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(blank_two));

        move_paragraph_up(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(blank_one));

        // No earlier blank — clamps to doc start.
        move_paragraph_up(&mut document, false);
        assert_eq!(primary_selection(&document), Selection::caret(0));
    }

    #[test]
    fn paragraph_motion_extends_selection() {
        let text = "first\n\nsecond";
        let mut document = document(text);
        set_primary_selection(&mut document, Selection::caret(0));

        move_paragraph_down(&mut document, true);
        let blank = "first\n".len();
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 0,
                head: blank
            }
        );
    }

    #[test]
    fn page_motion_steps_by_visible_rows_minus_one() {
        let text = "l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7";
        let mut document = document(text);
        set_primary_selection(&mut document, Selection::caret(0));
        let mut view_state = EditorViewState {
            visible_rows: Some(5),
            ..Default::default()
        };

        move_page(&mut document, &mut view_state, 1, false);
        // Step is 4 → land on line 4 column 0.
        assert_eq!(
            primary_selection(&document),
            Selection::caret("l0\nl1\nl2\nl3\n".len())
        );

        move_page(&mut document, &mut view_state, 1, false);
        assert_eq!(
            primary_selection(&document),
            Selection::caret("l0\nl1\nl2\nl3\nl4\nl5\nl6\n".len())
        );

        // Past EOF clamps to last line.
        move_page(&mut document, &mut view_state, 1, false);
        assert_eq!(
            primary_selection(&document),
            Selection::caret("l0\nl1\nl2\nl3\nl4\nl5\nl6\n".len())
        );

        move_page(&mut document, &mut view_state, -1, false);
        assert_eq!(
            primary_selection(&document),
            Selection::caret("l0\nl1\nl2\n".len())
        );
    }

    #[test]
    fn page_motion_preserves_preferred_column() {
        let text = "abcdefgh\nx\nlong line\n12345678";
        let mut document = document(text);
        // Start at column 6 of line 0.
        set_primary_selection(&mut document, Selection::caret(6));
        let mut view_state = EditorViewState {
            visible_rows: Some(3),
            ..Default::default()
        };

        // Step = 2; lands on line 2 ("long line"), column 6.
        move_page(&mut document, &mut view_state, 1, false);
        assert_eq!(
            primary_selection(&document),
            Selection::caret("abcdefgh\nx\nlong l".len())
        );
        assert_eq!(view_state.preferred_column, Some(6));

        // Step back 2; lands on line 0 column 6 with preferred column intact.
        move_page(&mut document, &mut view_state, -1, false);
        assert_eq!(primary_selection(&document), Selection::caret(6));
        assert_eq!(view_state.preferred_column, Some(6));
    }

    #[test]
    fn page_motion_extends_selection() {
        let text = "a\nb\nc\nd\ne";
        let mut document = document(text);
        set_primary_selection(&mut document, Selection::caret(0));
        let mut view_state = EditorViewState {
            visible_rows: Some(3),
            ..Default::default()
        };

        move_page(&mut document, &mut view_state, 1, true);
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 0,
                head: "a\nb\n".len()
            }
        );
    }

    #[test]
    fn replace_match_replaces_range_and_moves_caret() {
        let mut document = document("hello world hello");
        let revision_before = document.revision;

        let caret = replace_match(&mut document, SearchMatch { start: 6, end: 11 }, "earth");

        assert_eq!(document.text(), "hello earth hello");
        assert_eq!(caret, 11);
        assert_eq!(primary_selection(&document), Selection::caret(11));
        assert_eq!(document.revision, revision_before + 1);
    }

    #[test]
    fn replace_match_handles_empty_replacement() {
        let mut document = document("delete me here");

        let caret = replace_match(&mut document, SearchMatch { start: 7, end: 9 }, "");

        assert_eq!(document.text(), "delete  here");
        assert_eq!(caret, 7);
        assert_eq!(primary_selection(&document), Selection::caret(7));
    }

    #[test]
    fn replace_all_matches_applies_in_reverse_order() {
        let mut document = document("foo bar foo bar foo");
        let revision_before = document.revision;

        let count = replace_all_matches(
            &mut document,
            &[
                SearchMatch { start: 0, end: 3 },
                SearchMatch { start: 8, end: 11 },
                SearchMatch { start: 16, end: 19 },
            ],
            "qux",
        );

        assert_eq!(count, 3);
        assert_eq!(document.text(), "qux bar qux bar qux");
        assert_eq!(primary_selection(&document), Selection::caret(3));
        assert_eq!(document.revision, revision_before + 1);
    }

    #[test]
    fn replace_all_matches_handles_replacement_containing_query() {
        let mut document = document("a a a");

        let count = replace_all_matches(
            &mut document,
            &[
                SearchMatch { start: 0, end: 1 },
                SearchMatch { start: 2, end: 3 },
                SearchMatch { start: 4, end: 5 },
            ],
            "aa",
        );

        assert_eq!(count, 3);
        assert_eq!(document.text(), "aa aa aa");
    }

    #[test]
    fn replace_all_matches_handles_multibyte_text() {
        let mut document = document("aé日 aé日");

        let count = replace_all_matches(
            &mut document,
            &[
                SearchMatch { start: 1, end: 6 },
                SearchMatch { start: 8, end: 13 },
            ],
            "x",
        );

        assert_eq!(count, 2);
        assert_eq!(document.text(), "ax ax");
    }

    #[test]
    fn replace_all_matches_no_op_for_empty_input() {
        let mut document = document("untouched");
        let revision_before = document.revision;

        let count = replace_all_matches(&mut document, &[], "x");

        assert_eq!(count, 0);
        assert_eq!(document.text(), "untouched");
        assert_eq!(document.revision, revision_before);
    }

    #[test]
    fn undo_restores_text_after_typing() {
        let mut document = document("");
        replace_selection_with(&mut document, "hello");
        document.commit_undo_group();

        assert!(document.undo());
        assert_eq!(document.text(), "");
        assert_eq!(primary_selection(&document), Selection::caret(0));
    }

    #[test]
    fn undo_restores_text_after_backspace() {
        let mut document = document("hello");
        set_primary_selection(&mut document, Selection::caret(5));
        backspace(&mut document);
        document.commit_undo_group();

        assert!(document.undo());
        assert_eq!(document.text(), "hello");
        assert_eq!(primary_selection(&document), Selection::caret(5));
    }

    #[test]
    fn undo_restores_text_after_delete() {
        let mut document = document("hello");
        set_primary_selection(&mut document, Selection::caret(0));
        delete(&mut document);
        document.commit_undo_group();

        assert!(document.undo());
        assert_eq!(document.text(), "hello");
        assert_eq!(primary_selection(&document), Selection::caret(0));
    }

    #[test]
    fn undo_restores_text_after_delete_line() {
        let mut document = document("one\ntwo\nthree");
        set_primary_selection(&mut document, Selection::caret("one\nt".len()));
        delete_selected_lines(&mut document);
        document.commit_undo_group();

        assert!(document.undo());
        assert_eq!(document.text(), "one\ntwo\nthree");
    }

    #[test]
    fn undo_restores_text_after_duplicate_line() {
        let mut document = document("one\ntwo");
        set_primary_selection(&mut document, Selection::caret("one\nt".len()));
        duplicate_selected_lines(&mut document);
        document.commit_undo_group();

        assert!(document.undo());
        assert_eq!(document.text(), "one\ntwo");
    }

    #[test]
    fn undo_restores_text_after_indent() {
        let mut document = document("alpha\nbeta");
        set_primary_selection(&mut document, Selection::caret("alpha\nbe".len()));
        indent_selection(&mut document);
        document.commit_undo_group();

        assert!(document.undo());
        assert_eq!(document.text(), "alpha\nbeta");
    }

    #[test]
    fn undo_restores_text_after_sort_lines() {
        let mut document = document("gamma\nalpha\nbeta\nomega");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 1,
                head: "gamma\nalpha\nbe".len(),
            },
        );
        sort_selected_lines(&mut document);
        document.commit_undo_group();

        assert!(document.undo());
        assert_eq!(document.text(), "gamma\nalpha\nbeta\nomega");
    }

    #[test]
    fn redo_reapplies_undone_edit() {
        let mut document = document("");
        replace_selection_with(&mut document, "hello");
        document.commit_undo_group();

        assert!(document.undo());
        assert_eq!(document.text(), "");

        assert!(document.redo());
        assert_eq!(document.text(), "hello");
    }

    #[test]
    fn redo_is_cleared_after_new_edit() {
        let mut document = document("");
        replace_selection_with(&mut document, "hello");
        document.commit_undo_group();

        assert!(document.undo());
        assert!(document.can_redo());

        replace_selection_with(&mut document, "world");
        document.commit_undo_group();
        assert!(!document.can_redo());
    }

    #[test]
    fn undo_noop_when_empty() {
        let mut document = document("hello");
        assert!(!document.undo());
        assert_eq!(document.text(), "hello");
    }

    #[test]
    fn redo_noop_when_empty() {
        let mut document = document("hello");
        assert!(!document.redo());
        assert_eq!(document.text(), "hello");
    }

    #[test]
    fn undo_replaces_selection_text_and_restores_selection() {
        let mut document = document("hello world");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 6,
                head: 11,
            },
        );
        replace_selection_with(&mut document, "pile");
        document.commit_undo_group();

        assert_eq!(document.text(), "hello pile");

        assert!(document.undo());
        assert_eq!(document.text(), "hello world");
        assert_eq!(
            primary_selection(&document),
            Selection {
                anchor: 6,
                head: 11,
            }
        );
    }

    #[test]
    fn multiple_undo_steps_chain_correctly() {
        let mut document = document("");
        replace_selection_with(&mut document, "a");
        document.commit_undo_group();
        replace_selection_with(&mut document, "b");
        document.commit_undo_group();
        replace_selection_with(&mut document, "c");
        document.commit_undo_group();

        assert_eq!(document.text(), "abc");

        assert!(document.undo());
        assert_eq!(document.text(), "ab");

        assert!(document.undo());
        assert_eq!(document.text(), "a");

        assert!(document.undo());
        assert_eq!(document.text(), "");

        assert!(!document.undo());
    }

    #[test]
    fn replace_all_can_be_undone() {
        let mut document = document("foo bar foo");
        replace_all_matches(
            &mut document,
            &[
                SearchMatch { start: 0, end: 3 },
                SearchMatch { start: 8, end: 11 },
            ],
            "baz",
        );
        document.commit_undo_group();

        assert_eq!(document.text(), "baz bar baz");

        assert!(document.undo());
        assert_eq!(document.text(), "foo bar foo");
    }

    #[test]
    fn replace_match_can_be_undone() {
        let mut document = document("hello world");
        replace_match(&mut document, SearchMatch { start: 6, end: 11 }, "earth");
        document.commit_undo_group();

        assert_eq!(document.text(), "hello earth");

        assert!(document.undo());
        assert_eq!(document.text(), "hello world");
    }

    #[test]
    fn move_line_up_can_be_undone() {
        let mut document = document("one\ntwo\nthree");
        set_primary_selection(&mut document, Selection::caret("one\ntw".len()));
        move_selected_lines_up(&mut document);
        document.commit_undo_group();

        assert_eq!(document.text(), "two\none\nthree");

        assert!(document.undo());
        assert_eq!(document.text(), "one\ntwo\nthree");
    }

    #[test]
    fn join_lines_can_be_undone() {
        let mut document = document("one\n  two\nthree");
        set_primary_selection(&mut document, Selection::caret("on".len()));
        join_selected_lines(&mut document);
        document.commit_undo_group();

        assert_eq!(document.text(), "one two\nthree");

        assert!(document.undo());
        assert_eq!(document.text(), "one\n  two\nthree");
    }

    #[test]
    fn outdent_can_be_undone() {
        let mut document = document("    one\n  two\n\tthree\nfour");
        set_primary_selection(
            &mut document,
            Selection {
                anchor: 0,
                head: "    one\n  two\n\tthree".len(),
            },
        );
        outdent_selection(&mut document);
        document.commit_undo_group();

        assert_eq!(document.text(), "one\ntwo\nthree\nfour");

        assert!(document.undo());
        assert_eq!(document.text(), "    one\n  two\n\tthree\nfour");
    }

    #[test]
    fn word_at_selection_finds_word_under_caret() {
        let rope = Rope::from("hello world");
        let sel = Selection::caret(3); // inside "hello"
        let result = word_at_selection(&rope, sel);
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn word_at_selection_uses_existing_selection() {
        let rope = Rope::from("hello world");
        let sel = Selection {
            anchor: 0,
            head: 5,
        };
        let result = word_at_selection(&rope, sel);
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn word_at_selection_returns_none_for_whitespace() {
        let rope = Rope::from("hello world");
        let sel = Selection::caret(5); // space between words
        let result = word_at_selection(&rope, sel);
        assert_eq!(result, None);
    }

    #[test]
    fn word_at_selection_handles_non_alphanumeric_word() {
        let rope = Rope::from("hello_world test");
        let sel = Selection::caret(8); // inside "hello_world"
        let result = word_at_selection(&rope, sel);
        assert_eq!(result, Some((0, 11)));
    }
}
