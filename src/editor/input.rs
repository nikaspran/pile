use eframe::egui;

use crate::command::{Command, EDITOR_KEY_COMMANDS, command_for_key_event};
use crate::model::Document;

use super::{
    CaseType, EditorViewState, add_all_matches, add_cursor_vertical, add_next_match, backspace_all,
    backspace_with_pair_deletion, backspace_word, backspace_word_all, clear_secondary_cursors,
    contract_selection_by_bracket_pair, contract_selection_by_indent_block,
    contract_selection_by_line, contract_selection_by_word, convert_case_all_selections,
    convert_case_selection, delete, delete_all, delete_selected_lines, delete_word,
    delete_word_all, duplicate_selected_lines, expand_selection_by_bracket_pair,
    expand_selection_by_indent_block, expand_selection_by_line, expand_selection_by_word,
    indent_selection, insert_char_with_pairing, insert_newline_with_auto_indent,
    join_selected_lines, move_document_end, move_document_start, move_end, move_home, move_left,
    move_page, move_paragraph_down, move_paragraph_up, move_right, move_selected_lines_down,
    move_selected_lines_up, move_vertical, move_word_left, move_word_right, outdent_selection,
    replace_selection_all, replace_selection_with, reverse_selected_lines, sort_selected_lines,
    split_selection_into_lines, toggle_comments, trim_trailing_whitespace,
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
            } => {
                if let Some(command) = command_for_key_event(key, modifiers, EDITOR_KEY_COMMANDS) {
                    let (command_changed, command_was_typing) =
                        dispatch_editor_key_command(command, document, view_state);
                    changed |= command_changed;
                    had_typing_event |= command_was_typing;
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

fn dispatch_editor_key_command(
    command: Command,
    document: &mut Document,
    view_state: &mut EditorViewState,
) -> (bool, bool) {
    use Command::*;

    match command {
        Undo => {
            view_state.preferred_column = None;
            (document.undo(), false)
        }
        Redo => {
            view_state.preferred_column = None;
            (document.redo(), false)
        }
        Backspace => {
            view_state.preferred_column = None;
            let changed = if document.selections.len() > 1 {
                backspace_all(document)
            } else {
                backspace_with_pair_deletion(document)
            };
            (changed, true)
        }
        DeleteForward => {
            view_state.preferred_column = None;
            let changed = if document.selections.len() > 1 {
                delete_all(document)
            } else {
                delete(document)
            };
            (changed, true)
        }
        BackspaceWord => {
            view_state.preferred_column = None;
            let changed = if document.selections.len() > 1 {
                backspace_word_all(document)
            } else {
                backspace_word(document)
            };
            (changed, true)
        }
        DeleteWordForward => {
            view_state.preferred_column = None;
            let changed = if document.selections.len() > 1 {
                delete_word_all(document)
            } else {
                delete_word(document)
            };
            (changed, true)
        }
        InsertNewline => {
            view_state.preferred_column = None;
            (insert_newline_with_auto_indent(document), true)
        }
        Indent => {
            view_state.preferred_column = None;
            (indent_selection(document), false)
        }
        Outdent => {
            view_state.preferred_column = None;
            (outdent_selection(document), false)
        }
        ClearSecondaryCursors => {
            view_state.preferred_column = None;
            (clear_secondary_cursors(document), false)
        }
        MoveLeft => {
            move_left(document, false);
            view_state.preferred_column = None;
            (true, false)
        }
        MoveRight => {
            move_right(document, false);
            view_state.preferred_column = None;
            (true, false)
        }
        MoveWordLeft => {
            move_word_left(document, false);
            view_state.preferred_column = None;
            (true, false)
        }
        MoveWordRight => {
            move_word_right(document, false);
            view_state.preferred_column = None;
            (true, false)
        }
        MoveUp => {
            move_vertical(document, view_state, -1, false);
            (true, false)
        }
        MoveDown => {
            move_vertical(document, view_state, 1, false);
            (true, false)
        }
        MoveDocumentStart => {
            move_document_start(document, false);
            view_state.preferred_column = None;
            (true, false)
        }
        MoveDocumentEnd => {
            move_document_end(document, false);
            view_state.preferred_column = None;
            (true, false)
        }
        MoveLineStart => {
            move_home(document, false);
            view_state.preferred_column = None;
            (true, false)
        }
        MoveLineEnd => {
            move_end(document, false);
            view_state.preferred_column = None;
            (true, false)
        }
        MoveParagraphUp => {
            move_paragraph_up(document, false);
            view_state.preferred_column = None;
            (true, false)
        }
        MoveParagraphDown => {
            move_paragraph_down(document, false);
            view_state.preferred_column = None;
            (true, false)
        }
        PageUp => {
            move_page(document, view_state, -1, false);
            (true, false)
        }
        PageDown => {
            move_page(document, view_state, 1, false);
            (true, false)
        }
        SelectLeft => {
            move_left(document, true);
            view_state.preferred_column = None;
            (true, false)
        }
        SelectRight => {
            move_right(document, true);
            view_state.preferred_column = None;
            (true, false)
        }
        SelectWordLeft => {
            move_word_left(document, true);
            view_state.preferred_column = None;
            (true, false)
        }
        SelectWordRight => {
            move_word_right(document, true);
            view_state.preferred_column = None;
            (true, false)
        }
        SelectUp => {
            move_vertical(document, view_state, -1, true);
            (true, false)
        }
        SelectDown => {
            move_vertical(document, view_state, 1, true);
            (true, false)
        }
        SelectDocumentStart => {
            move_document_start(document, true);
            view_state.preferred_column = None;
            (true, false)
        }
        SelectDocumentEnd => {
            move_document_end(document, true);
            view_state.preferred_column = None;
            (true, false)
        }
        SelectLineStart => {
            move_home(document, true);
            view_state.preferred_column = None;
            (true, false)
        }
        SelectLineEnd => {
            move_end(document, true);
            view_state.preferred_column = None;
            (true, false)
        }
        SelectParagraphUp => {
            move_paragraph_up(document, true);
            view_state.preferred_column = None;
            (true, false)
        }
        SelectParagraphDown => {
            move_paragraph_down(document, true);
            view_state.preferred_column = None;
            (true, false)
        }
        SelectPageUp => {
            move_page(document, view_state, -1, true);
            (true, false)
        }
        SelectPageDown => {
            move_page(document, view_state, 1, true);
            (true, false)
        }
        ExpandWord => {
            expand_selection_by_word(document);
            view_state.preferred_column = None;
            (true, false)
        }
        ContractWord => {
            contract_selection_by_word(document);
            view_state.preferred_column = None;
            (true, false)
        }
        ExpandLine => {
            expand_selection_by_line(document);
            view_state.preferred_column = None;
            (true, false)
        }
        ContractLine => {
            contract_selection_by_line(document);
            view_state.preferred_column = None;
            (true, false)
        }
        ExpandBracketPair => {
            expand_selection_by_bracket_pair(document);
            view_state.preferred_column = None;
            (true, false)
        }
        ContractBracketPair => {
            contract_selection_by_bracket_pair(document);
            view_state.preferred_column = None;
            (true, false)
        }
        ExpandIndentBlock => {
            expand_selection_by_indent_block(document);
            view_state.preferred_column = None;
            (true, false)
        }
        ContractIndentBlock => {
            contract_selection_by_indent_block(document);
            view_state.preferred_column = None;
            (true, false)
        }
        DuplicateLines => {
            view_state.preferred_column = None;
            (duplicate_selected_lines(document), false)
        }
        DeleteLines => {
            view_state.preferred_column = None;
            (delete_selected_lines(document), false)
        }
        MoveLinesUp => {
            view_state.preferred_column = None;
            (move_selected_lines_up(document), false)
        }
        MoveLinesDown => {
            view_state.preferred_column = None;
            (move_selected_lines_down(document), false)
        }
        JoinLines => {
            view_state.preferred_column = None;
            (join_selected_lines(document), false)
        }
        SortLines => {
            view_state.preferred_column = None;
            (sort_selected_lines(document), false)
        }
        ReverseLines => {
            view_state.preferred_column = None;
            (reverse_selected_lines(document), false)
        }
        TrimTrailingWhitespace => {
            view_state.preferred_column = None;
            (trim_trailing_whitespace(document), false)
        }
        AddNextMatch => {
            add_next_match(document);
            view_state.preferred_column = None;
            (true, false)
        }
        AddAllMatches => {
            add_all_matches(document);
            view_state.preferred_column = None;
            (true, false)
        }
        SplitSelectionIntoLines => {
            split_selection_into_lines(document);
            view_state.preferred_column = None;
            (true, false)
        }
        AddCursorAbove => {
            view_state.preferred_column = None;
            (add_cursor_vertical(document, -1), false)
        }
        AddCursorBelow => {
            view_state.preferred_column = None;
            (add_cursor_vertical(document, 1), false)
        }
        ToggleComments => {
            let comment_prefix = document
                .detect_syntax()
                .and_then(|d| d.language.comment_prefix())
                .unwrap_or("//");
            view_state.preferred_column = None;
            (toggle_comments(document, comment_prefix), false)
        }
        UpperCase => {
            view_state.preferred_column = None;
            let changed = if document.selections.len() > 1 {
                convert_case_all_selections(document, CaseType::Upper)
            } else {
                convert_case_selection(document, CaseType::Upper)
            };
            (changed, false)
        }
        LowerCase => {
            view_state.preferred_column = None;
            let changed = if document.selections.len() > 1 {
                convert_case_all_selections(document, CaseType::Lower)
            } else {
                convert_case_selection(document, CaseType::Lower)
            };
            (changed, false)
        }
        TitleCase => {
            view_state.preferred_column = None;
            let changed = if document.selections.len() > 1 {
                convert_case_all_selections(document, CaseType::Title)
            } else {
                convert_case_selection(document, CaseType::Title)
            };
            (changed, false)
        }
        _ => (false, false),
    }
}
