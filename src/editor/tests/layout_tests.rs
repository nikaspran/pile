use super::*;

/// Helper to create a TextLayoutPipeline for high-DPI testing with custom dimensions
fn layout_for_dpi(
    text: &str,
    char_width: f32,
    row_height: f32,
    dpi_scale: f32,
) -> (TextLayoutPipeline, Rope) {
    let rope = Rope::from(text);
    let line_count = visual_line_count(&rope);
    let font_id = egui::FontId::monospace(14.0 * dpi_scale);
    let gutter_width = 44.0 * dpi_scale;
    let text_origin_x = gutter_width + 10.0 * dpi_scale;
    let content_width =
        (text_origin_x + 400.0 * dpi_scale).max(text_origin_x + EDITOR_MIN_WIDTH * dpi_scale);
    let content_height = (line_count as f32 * row_height).max(200.0 * dpi_scale);

    let pipeline = TextLayoutPipeline::for_test(
        row_height,
        char_width,
        font_id,
        gutter_width,
        text_origin_x,
        content_width,
        content_height,
        line_count,
    );
    (pipeline, rope)
}

#[test]
fn high_dpi_char_width_scales_correctly() {
    // Simulate 1x DPI
    let (layout_1x, rope) = layout_for_dpi("hello", 8.0, 16.0, 1.0);
    let rect_1x = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));

    // At position 0, should map to offset 0
    let offset_1x = layout_1x.offset_at_pointer(
        &rope,
        egui::pos2(layout_1x.text_origin_x + 0.0, 0.0),
        rect_1x,
    );
    assert_eq!(offset_1x, 0);

    // At column 3, should map to offset of 'l' (3rd char)
    let offset_3x = layout_1x.offset_at_pointer(
        &rope,
        egui::pos2(layout_1x.text_origin_x + 3.0 * 8.0, 0.0),
        rect_1x,
    );
    assert_eq!(offset_3x, "hel".len());

    // Simulate 2x DPI (Retina/HiDPI)
    let (layout_2x, _) = layout_for_dpi("hello", 16.0, 32.0, 2.0);
    let rect_2x = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1600.0, 1200.0));

    // At column 3 with 2x DPI, should still map to same text offset
    let offset_2x = layout_2x.offset_at_pointer(
        &rope,
        egui::pos2(layout_2x.text_origin_x + 3.0 * 16.0, 0.0),
        rect_2x,
    );
    assert_eq!(offset_2x, "hel".len());
}

#[test]
fn high_dpi_caret_position_scales_correctly() {
    let rope = Rope::from("hello");
    let font_id = egui::FontId::monospace(14.0);

    // 1x DPI
    let layout_1x =
        TextLayoutPipeline::for_test(16.0, 8.0, font_id.clone(), 44.0, 54.0, 800.0, 600.0, 1);
    let pos_1x = layout_1x.caret_position(&rope, 3, 0.0);
    assert_eq!(pos_1x.x, 54.0 + 3.0 * 8.0); // text_origin_x + column * char_width

    // 2x DPI - all dimensions doubled
    let layout_2x =
        TextLayoutPipeline::for_test(32.0, 16.0, font_id, 88.0, 108.0, 1600.0, 1200.0, 1);
    let pos_2x = layout_2x.caret_position(&rope, 3, 0.0);
    assert_eq!(pos_2x.x, 108.0 + 3.0 * 16.0);
}

#[test]
fn high_dpi_multiline_layout_scales() {
    let text = "line1\nline2\nline3";
    let rope = Rope::from(text);
    let line_count = visual_line_count(&rope);

    // 1x DPI
    let layout_1x = TextLayoutPipeline::for_test(
        16.0,
        8.0,
        egui::FontId::monospace(14.0),
        44.0,
        54.0,
        800.0,
        600.0,
        line_count,
    );

    // Line 1 should be at y = 0, Line 2 at y = 16, Line 3 at y = 32
    assert_eq!(layout_1x.line_y(0, 0.0), 0.0);
    assert_eq!(layout_1x.line_y(1, 0.0), 16.0);
    assert_eq!(layout_1x.line_y(2, 0.0), 32.0);

    // 2x DPI
    let layout_2x = TextLayoutPipeline::for_test(
        32.0,
        16.0,
        egui::FontId::monospace(28.0),
        88.0,
        108.0,
        1600.0,
        1200.0,
        line_count,
    );

    // Line 1 should be at y = 0, Line 2 at y = 32, Line 3 at y = 64
    assert_eq!(layout_2x.line_y(0, 0.0), 0.0);
    assert_eq!(layout_2x.line_y(1, 0.0), 32.0);
    assert_eq!(layout_2x.line_y(2, 0.0), 64.0);
}

#[test]
fn high_dpi_visible_line_range_scales() {
    let rope = Rope::from("line1\nline2\nline3\nline4\nline5");
    let line_count = visual_line_count(&rope);

    // 1x DPI: row_height = 16, viewport from y=16 to y=64
    // first_line = (16/16).floor() = 1
    // last_line = (64/16).ceil() + 1 = 4 + 1 = 5, but min(5, 5) = 5
    let layout_1x = TextLayoutPipeline::for_test(
        16.0,
        8.0,
        egui::FontId::monospace(14.0),
        44.0,
        54.0,
        800.0,
        600.0,
        line_count,
    );
    let viewport_1x = egui::Rect::from_min_max(egui::pos2(0.0, 16.0), egui::pos2(800.0, 64.0));
    let (first, last) = layout_1x.visible_line_range(&viewport_1x);
    assert_eq!(first, 1); // starts at y=16, which is line 1
    assert_eq!(last, 5); // (64/16).ceil()+1 = 5

    // 2x DPI: row_height = 32, viewport from y=32 to y=128
    // first_line = (32/32).floor() = 1
    // last_line = (128/32).ceil() + 1 = 4 + 1 = 5, but min(5, 5) = 5
    let layout_2x = TextLayoutPipeline::for_test(
        32.0,
        16.0,
        egui::FontId::monospace(28.0),
        88.0,
        108.0,
        1600.0,
        1200.0,
        line_count,
    );
    let viewport_2x = egui::Rect::from_min_max(egui::pos2(0.0, 32.0), egui::pos2(1600.0, 128.0));
    let (first_2x, last_2x) = layout_2x.visible_line_range(&viewport_2x);
    assert_eq!(first_2x, 1);
    assert_eq!(last_2x, 5);
}

#[test]
fn longest_visual_line_chars_tracks_unwrapped_scroll_width() {
    let rope = Rope::from("short\nthis line is much longer\nmid");

    assert_eq!(
        crate::editor::layout::longest_visual_line_chars(&rope),
        "this line is much longer".chars().count()
    );
}

#[test]
fn font_fallback_renders_cjk_characters() {
    // CJK characters may fall back to a different font
    let text = "Hello 世界";
    let rope = Rope::from(text);

    let layout = TextLayoutPipeline::for_test(
        16.0,
        8.0,
        egui::FontId::monospace(14.0),
        44.0,
        54.0,
        800.0,
        600.0,
        1,
    );

    // The layout should handle CJK characters without panicking
    // CJK characters might have different display widths, but our layout uses fixed char_width
    let line_text = layout.wrapped_line_text(&rope, 0);
    assert_eq!(line_text, "Hello 世界");

    // Test offset calculation with multibyte characters
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));

    // Click at column 6 (after "Hello ")
    let offset = layout.offset_at_pointer(
        &rope,
        egui::pos2(layout.text_origin_x + 6.0 * 8.0, 0.0),
        rect,
    );
    // "Hello ".len() = 6 bytes (ASCII), but we need to account for the CJK chars
    // Actually, offset_at_pointer uses column calculation, so column 6 should be at byte offset 6
    assert_eq!(offset, 6);
}

#[test]
fn font_fallback_renders_emoji() {
    // Emoji characters may fall back to a different font
    let text = "Click 😀 here";
    let rope = Rope::from(text);

    let layout = TextLayoutPipeline::for_test(
        16.0,
        8.0,
        egui::FontId::monospace(14.0),
        44.0,
        54.0,
        800.0,
        600.0,
        1,
    );

    // The layout should handle emoji without panicking
    let line_text = layout.wrapped_line_text(&rope, 0);
    assert_eq!(line_text, "Click 😀 here");

    // Test that we can calculate caret position for emoji
    // "Click ".len() = 6, "😀".len() = 4 bytes
    let pos = layout.caret_position(&rope, 10, 0.0); // After "Click 😀" (6 + 4 = 10 bytes)
    assert!(pos.x > layout.text_origin_x);
}

#[test]
fn font_fallback_renders_mixed_scripts() {
    // Mix of Latin, CJK, Arabic, and Cyrillic
    let text = "Hello 世界 مرحبا Привет";
    let rope = Rope::from(text);

    let layout = TextLayoutPipeline::for_test(
        16.0,
        8.0,
        egui::FontId::monospace(14.0),
        44.0,
        54.0,
        800.0,
        600.0,
        1,
    );

    // Should handle all character types without panicking
    let line_text = layout.wrapped_line_text(&rope, 0);
    assert_eq!(line_text, "Hello 世界 مرحبا Привет");

    // Test grapheme handling with mixed scripts
    let mut doc = document(text);
    set_primary_selection(&mut doc, Selection::caret(6)); // At first space

    // Move right should respect grapheme boundaries
    move_right(&mut doc, false);
    // After " " (space), should be at start of "世" (byte offset 6 + 1 for space = 7... wait)
    // "Hello " = 6 bytes (each char is 1 byte for ASCII), so position 6 is after space
    // Actually "Hello " is 'H','e','l','l','o',' ' = 6 bytes
    // "世" is 3 bytes, "界" is 3 bytes
    // So after "Hello " at byte 6, move_right goes to byte 6 (start of "世")
    // Wait, the cursor is already at byte 6. Let me re-read...
    // set_primary_selection with Selection::caret(6) puts cursor after "Hello "
    // move_right should move to after "世" which is byte 9
    assert_eq!(primary_selection(&doc).head, "Hello 世".len()); // 6 + 3 = 9
}

#[test]
fn font_fallback_test_char_width_consistency() {
    // Characters from different scripts should use the same char_width in our layout
    // This test ensures our fixed-width assumption is documented
    let layout = TextLayoutPipeline::for_test(
        16.0,
        8.0,
        egui::FontId::monospace(14.0),
        44.0,
        54.0,
        800.0,
        600.0,
        1,
    );

    // All characters should use the same width for column calculation
    // This is a design decision - we use fixed width based on 'm' character
    assert_eq!(layout.char_width, 8.0);

    // Test that column_x is consistent
    assert_eq!(layout.column_x(0), layout.text_origin_x);
    assert_eq!(layout.column_x(5), layout.text_origin_x + 5.0 * 8.0);
    assert_eq!(layout.column_x(10), layout.text_origin_x + 10.0 * 8.0);
}

#[test]
fn high_dpi_gutter_width_scales() {
    // Test that gutter width scales with DPI
    let rope = Rope::from("line1\nline2");
    let line_count = visual_line_count(&rope);

    // 1x DPI
    let layout_1x = TextLayoutPipeline::for_test(
        16.0,
        8.0,
        egui::FontId::monospace(14.0),
        44.0,
        54.0,
        800.0,
        600.0,
        line_count,
    );
    assert_eq!(layout_1x.gutter_width, 44.0);
    assert_eq!(layout_1x.text_origin_x, 54.0); // gutter + padding

    // 2x DPI
    let layout_2x = TextLayoutPipeline::for_test(
        32.0,
        16.0,
        egui::FontId::monospace(28.0),
        88.0,
        108.0,
        1600.0,
        1200.0,
        line_count,
    );
    assert_eq!(layout_2x.gutter_width, 88.0);
    assert_eq!(layout_2x.text_origin_x, 108.0);
}

#[test]
fn high_dpi_content_size_scales() {
    let rope = Rope::from("line1\nline2\nline3");
    let line_count = visual_line_count(&rope);

    // 1x DPI - content_height is max(available_height, line_count * row_height)
    let layout_1x = TextLayoutPipeline::for_test(
        16.0,
        8.0,
        egui::FontId::monospace(14.0),
        44.0,
        54.0,
        800.0,
        600.0,
        line_count,
    );
    let size_1x = layout_1x.content_size();
    // content_height should be max(600.0, 3 * 16.0) = 600.0
    assert_eq!(size_1x.y, 600.0);

    // 2x DPI
    let layout_2x = TextLayoutPipeline::for_test(
        32.0,
        16.0,
        egui::FontId::monospace(28.0),
        88.0,
        108.0,
        1600.0,
        1200.0,
        line_count,
    );
    let size_2x = layout_2x.content_size();
    assert_eq!(size_2x.y, 1200.0);
}
