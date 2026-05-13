use eframe::egui;

use crate::model::Document;

use super::{
    CaseType, EditorViewState, add_all_matches, add_next_match, backspace_all,
    backspace_with_pair_deletion, backspace_word, backspace_word_all, clear_secondary_cursors,
    contract_selection_by_bracket_pair, contract_selection_by_indent_block,
    contract_selection_by_line, contract_selection_by_word, convert_case_all_selections,
    convert_case_selection, delete, delete_all, delete_selected_lines, delete_word,
    delete_word_all, expand_selection_by_bracket_pair, expand_selection_by_indent_block,
    expand_selection_by_line, expand_selection_by_word, indent_selection, insert_char_with_pairing,
    insert_newline_with_auto_indent, join_selected_lines, move_document_end, move_document_start,
    move_end, move_home, move_left, move_page, move_paragraph_down, move_paragraph_up, move_right,
    move_selected_lines_down, move_selected_lines_up, move_vertical, move_word_left,
    move_word_right, outdent_selection, replace_selection_all, replace_selection_with,
    reverse_selected_lines, sort_selected_lines, split_selection_into_lines, toggle_comments,
    trim_trailing_whitespace,
};

pub(super) fn handle_input(
    ui: &egui::Ui,
    document: &mut Document,
    view_state: &mut EditorViewState,
) -> bool {
    let events = ui.input(|input| input.events.clone());
    let mut changed = false;
    let mut had_typing_event = false;

    for event in events {
        match event {
            egui::Event::Paste(text) if !text.is_empty() => {
                if document.selections.len() > 1 {
                    changed |= replace_selection_all(document, &text);
                } else {
                    changed |= replace_selection_with(document, &text);
                }
                view_state.preferred_column = None;
                had_typing_event = true;
            }
            egui::Event::Text(text) if !text.is_empty() && text != "\n" && text != "\r" => {
                if document.selections.len() > 1 {
                    changed |= replace_selection_all(document, &text);
                } else if text.chars().count() == 1 {
                    let ch = text.chars().next().unwrap();
                    changed |= insert_char_with_pairing(document, ch);
                } else {
                    changed |= replace_selection_with(document, &text);
                }
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
                egui::Key::D => {
                    add_next_match(document);
                    changed = true;
                    view_state.preferred_column = None;
                }
                egui::Key::J => {
                    changed |= join_selected_lines(document);
                    view_state.preferred_column = None;
                }
                egui::Key::Slash => {
                    // Toggle comments using detected language
                    let comment_prefix = document
                        .detect_syntax()
                        .and_then(|d| d.language.comment_prefix())
                        .unwrap_or("//");
                    changed |= toggle_comments(document, comment_prefix);
                    view_state.preferred_column = None;
                }
                egui::Key::ArrowLeft => {
                    move_word_left(document, false);
                    changed = true;
                    view_state.preferred_column = None;
                }
                egui::Key::ArrowRight => {
                    move_word_right(document, false);
                    changed = true;
                    view_state.preferred_column = None;
                }
                egui::Key::ArrowUp => {
                    move_document_start(document, false);
                    changed = true;
                    view_state.preferred_column = None;
                }
                egui::Key::ArrowDown => {
                    move_document_end(document, false);
                    changed = true;
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
                    add_all_matches(document);
                    changed = true;
                    view_state.preferred_column = None;
                }
                egui::Key::K => {
                    changed |= delete_selected_lines(document);
                    view_state.preferred_column = None;
                }
                egui::Key::L => {
                    split_selection_into_lines(document);
                    changed = true;
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
                egui::Key::ArrowLeft => {
                    move_word_left(document, true);
                    changed = true;
                    view_state.preferred_column = None;
                }
                egui::Key::ArrowRight => {
                    move_word_right(document, true);
                    changed = true;
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
            } if modifiers.command && modifiers.ctrl => match key {
                egui::Key::U => {
                    if document.selections.len() > 1 {
                        changed |= convert_case_all_selections(document, CaseType::Upper);
                    } else {
                        changed |= convert_case_selection(document, CaseType::Upper);
                    }
                    view_state.preferred_column = None;
                }
                egui::Key::L => {
                    if document.selections.len() > 1 {
                        changed |= convert_case_all_selections(document, CaseType::Lower);
                    } else {
                        changed |= convert_case_selection(document, CaseType::Lower);
                    }
                    view_state.preferred_column = None;
                }
                egui::Key::T => {
                    if document.selections.len() > 1 {
                        changed |= convert_case_all_selections(document, CaseType::Title);
                    } else {
                        changed |= convert_case_selection(document, CaseType::Title);
                    }
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
                        changed |= if document.selections.len() > 1 {
                            backspace_all(document)
                        } else {
                            backspace_with_pair_deletion(document)
                        };
                        view_state.preferred_column = None;
                        had_typing_event = true;
                    }
                    egui::Key::Backspace if word && !extend => {
                        changed |= if document.selections.len() > 1 {
                            backspace_word_all(document)
                        } else {
                            backspace_word(document)
                        };
                        view_state.preferred_column = None;
                        had_typing_event = true;
                    }
                    egui::Key::Delete if plain => {
                        changed |= if document.selections.len() > 1 {
                            delete_all(document)
                        } else {
                            delete(document)
                        };
                        view_state.preferred_column = None;
                        had_typing_event = true;
                    }
                    egui::Key::Delete if word && !extend => {
                        changed |= if document.selections.len() > 1 {
                            delete_word_all(document)
                        } else {
                            delete_word(document)
                        };
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
                    egui::Key::Escape => {
                        clear_secondary_cursors(document);
                        changed = true;
                    }
                    egui::Key::ArrowLeft => {
                        if word {
                            move_word_left(document, extend);
                        } else {
                            move_left(document, extend);
                        }
                        changed = true;
                        view_state.preferred_column = None;
                    }
                    egui::Key::ArrowRight => {
                        if word {
                            move_word_right(document, extend);
                        } else {
                            move_right(document, extend);
                        }
                        changed = true;
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
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.alt && !modifiers.command => {
                let contract = modifiers.shift;
                match key {
                    egui::Key::W => {
                        if contract {
                            contract_selection_by_word(document);
                        } else {
                            expand_selection_by_word(document);
                        }
                        view_state.preferred_column = None;
                    }
                    egui::Key::L => {
                        if contract {
                            contract_selection_by_line(document);
                        } else {
                            expand_selection_by_line(document);
                        }
                        view_state.preferred_column = None;
                    }
                    egui::Key::B => {
                        if contract {
                            contract_selection_by_bracket_pair(document);
                        } else {
                            expand_selection_by_bracket_pair(document);
                        }
                        view_state.preferred_column = None;
                    }
                    egui::Key::I => {
                        if contract {
                            contract_selection_by_indent_block(document);
                        } else {
                            expand_selection_by_indent_block(document);
                        }
                        view_state.preferred_column = None;
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
