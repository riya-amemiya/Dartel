use dartel_term::{
    Attrs, Cell, Color, CursorShape, Modes, Palette, Point, Rgb, Row, Span, Style, Term,
};
use dartel_vt::Parser;

fn term(lines: usize, cols: usize) -> Term {
    Term::new(lines, cols, 100)
}

fn tinted() -> Palette {
    let mut palette = Palette::default();
    palette.set_indexed(1, Rgb::new(1, 2, 3));
    palette.foreground = Rgb::new(4, 5, 6);
    palette.background = Rgb::new(7, 8, 9);
    palette.cursor = Rgb::new(10, 11, 12);
    palette
}

fn feed(term: &mut Term, input: &str) {
    Parser::new().advance(term, input.as_bytes());
}

fn fed(lines: usize, cols: usize, input: &str) -> Term {
    let mut term = term(lines, cols);
    feed(&mut term, input);
    term
}

fn absolute(term: &Term, line: usize) -> String {
    term.text(Span {
        start: Point { line, column: 0 },
        end: Point {
            line,
            column: term.cols() - 1,
        },
    })
}

fn screen(term: &Term) -> Vec<String> {
    (0..term.lines())
        .map(|line| absolute(term, term.grid().history() + line))
        .collect()
}

fn shown(row: &Row) -> String {
    row.cells().iter().filter_map(|cell| cell.ch()).collect()
}

fn cursor(term: &Term) -> (usize, usize) {
    (term.cursor().line, term.cursor().column)
}

fn style_at(term: &Term, line: usize, column: usize) -> Style {
    term.style(term.grid().row(line).cells()[column].style())
}

fn reply(term: &mut Term) -> String {
    String::from_utf8(term.take_reply()).unwrap()
}

fn marks(term: &Term) -> Vec<(bool, bool, bool)> {
    term.grid()
        .rows()
        .map(|row| (row.is_prompt(), row.is_output(), row.is_failed()))
        .collect()
}

#[test]
fn prints_text_and_advances_the_cursor() {
    let term = fed(3, 10, "abc");
    assert_eq!(screen(&term), ["abc", "", ""]);
    assert_eq!(cursor(&term), (0, 3));
}

#[test]
fn defers_the_wrap_until_the_next_character() {
    let mut term = fed(3, 5, "abcde");
    assert_eq!(cursor(&term), (0, 4));
    assert!(!term.grid().row(0).is_wrapped());
    feed(&mut term, "f");
    assert_eq!(screen(&term), ["abcde", "f", ""]);
    assert_eq!(cursor(&term), (1, 1));
    assert!(term.grid().row(0).is_wrapped());
}

#[test]
fn long_ascii_runs_wrap_across_several_lines() {
    let term = fed(4, 4, "abcdefghij");
    assert_eq!(screen(&term), ["abcd", "efgh", "ij", ""]);
    assert_eq!(cursor(&term), (2, 2));
}

#[test]
fn carriage_return_cancels_a_pending_wrap() {
    let term = fed(2, 3, "abc\rx");
    assert_eq!(screen(&term), ["xbc", ""]);
}

#[test]
fn linefeed_at_the_bottom_scrolls_into_history() {
    let term = fed(2, 5, "a\r\nb\r\nc");
    assert_eq!(screen(&term), ["b", "c"]);
    assert_eq!(term.grid().history(), 1);
    assert_eq!(absolute(&term, 0), "a");
}

#[test]
fn history_is_capped_at_its_limit() {
    let mut term = Term::new(2, 5, 3);
    feed(&mut term, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7");
    assert_eq!(term.grid().history(), 3);
    assert_eq!(absolute(&term, 0), "3");
    assert_eq!(screen(&term), ["6", "7"]);
}

#[test]
fn zero_history_limit_discards_scrolled_lines() {
    let mut term = Term::new(2, 5, 0);
    feed(&mut term, "1\r\n2\r\n3");
    assert_eq!(term.grid().history(), 0);
    assert_eq!(screen(&term), ["2", "3"]);
}

#[test]
fn backspace_moves_left_and_stops_at_the_margin() {
    let term = fed(1, 5, "ab\x08\x08\x08x");
    assert_eq!(screen(&term), ["xb"]);
}

#[test]
fn bell_is_reported_once_taken() {
    let mut term = fed(1, 5, "\x07");
    assert!(term.take_bell());
    assert!(!term.take_bell());
}

#[test]
fn cursor_position_uses_one_based_coordinates() {
    let term = fed(3, 10, "\x1b[2;3Hx");
    assert_eq!(screen(&term), ["", "  x", ""]);
}

#[test]
fn cursor_position_clamps_to_the_screen() {
    let term = fed(3, 10, "\x1b[99;99H");
    assert_eq!(cursor(&term), (2, 9));
}

#[test]
fn relative_moves_shift_the_cursor() {
    let term = fed(10, 10, "\x1b[5;5H\x1b[2A\x1b[3D\x1b[B\x1b[2C");
    assert_eq!(cursor(&term), (3, 3));
}

#[test]
fn next_and_previous_line_return_to_the_first_column() {
    let mut term = fed(10, 10, "\x1b[5;5H\x1b[2E");
    assert_eq!(cursor(&term), (6, 0));
    feed(&mut term, "\x1b[5G\x1b[3F");
    assert_eq!(cursor(&term), (3, 0));
}

#[test]
fn absolute_column_and_line_moves_keep_the_other_axis() {
    let term = fed(10, 10, "\x1b[5;5H\x1b[8G\x1b[2d");
    assert_eq!(cursor(&term), (1, 7));
}

#[test]
fn relative_column_and_line_aliases_move_like_the_arrows() {
    let term = fed(10, 10, "\x1b[2a\x1b[3e\x1b[4`");
    assert_eq!(cursor(&term), (3, 3));
}

#[test]
fn erase_display_below_clears_from_the_cursor() {
    let term = fed(3, 5, "abc\r\ndef\r\nghi\x1b[2;2H\x1b[J");
    assert_eq!(screen(&term), ["abc", "d", ""]);
}

#[test]
fn erase_display_above_clears_through_the_cursor() {
    let term = fed(3, 5, "abc\r\ndef\r\nghi\x1b[2;2H\x1b[1J");
    assert_eq!(screen(&term), ["", "  f", "ghi"]);
}

#[test]
fn erase_display_all_clears_the_screen_and_keeps_the_cursor() {
    let term = fed(3, 5, "abc\r\ndef\x1b[2J");
    assert_eq!(screen(&term), ["", "", ""]);
    assert_eq!(cursor(&term), (1, 3));
}

#[test]
fn erase_display_saved_lines_drops_the_history() {
    let term = fed(2, 5, "a\r\nb\r\nc\x1b[3J");
    assert_eq!(term.grid().history(), 0);
    assert_eq!(screen(&term), ["b", "c"]);
}

#[test]
fn erase_line_variants_clear_their_ranges() {
    assert_eq!(screen(&fed(1, 10, "abcdef\x1b[1;3H\x1b[K")), ["ab"]);
    assert_eq!(screen(&fed(1, 10, "abcdef\x1b[1;3H\x1b[1K")), ["   def"]);
    assert_eq!(screen(&fed(1, 10, "abcdef\x1b[1;3H\x1b[2K")), [""]);
}

#[test]
fn erased_cells_take_the_current_background() {
    let term = fed(1, 4, "\x1b[41;1m\x1b[K");
    assert_eq!(
        style_at(&term, 0, 2),
        Style {
            background: Color::indexed(1),
            ..Style::default()
        }
    );
}

#[test]
fn insert_delete_and_erase_characters_edit_the_line() {
    assert_eq!(screen(&fed(1, 10, "abcdef\x1b[1;2H\x1b[2@")), ["a  bcdef"]);
    assert_eq!(screen(&fed(1, 10, "abcdef\x1b[1;2H\x1b[2P")), ["adef"]);
    assert_eq!(screen(&fed(1, 10, "abcdef\x1b[1;2H\x1b[2X")), ["a  def"]);
}

#[test]
fn inserted_characters_push_text_off_the_right_margin() {
    assert_eq!(screen(&fed(1, 5, "abcde\x1b[1;2H\x1b[2@")), ["a  bc"]);
}

#[test]
fn insert_and_delete_lines_shift_the_rows_below() {
    assert_eq!(
        screen(&fed(4, 5, "a\r\nb\r\nc\r\nd\x1b[2;1H\x1b[L")),
        ["a", "", "b", "c"]
    );
    assert_eq!(
        screen(&fed(4, 5, "a\r\nb\r\nc\r\nd\x1b[2;1H\x1b[M")),
        ["a", "c", "d", ""]
    );
}

#[test]
fn deleting_lines_from_the_top_keeps_them_out_of_history() {
    let term = fed(3, 5, "a\r\nb\r\nc\x1b[1;1H\x1b[M");
    assert_eq!(screen(&term), ["b", "c", ""]);
    assert_eq!(term.grid().history(), 0);
}

#[test]
fn scroll_region_confines_linefeed_scrolling() {
    let term = fed(5, 5, "1\r\n2\r\n3\r\n4\r\n5\x1b[2;4r\x1b[4;1H\n");
    assert_eq!(screen(&term), ["1", "3", "4", "", "5"]);
    assert_eq!(term.grid().history(), 0);
}

#[test]
fn scroll_region_from_the_top_keeps_lines_below_it_in_place() {
    let term = fed(4, 5, "1\r\n2\r\n3\r\n4\x1b[1;3r\x1b[3;1H\n");
    assert_eq!(screen(&term), ["2", "3", "", "4"]);
    assert_eq!(term.grid().history(), 1);
    assert_eq!(absolute(&term, 0), "1");
}

#[test]
fn invalid_scroll_region_is_ignored() {
    let term = fed(3, 5, "a\r\nb\r\nc\x1b[3;2r\n");
    assert_eq!(screen(&term), ["b", "c", ""]);
}

#[test]
fn scroll_up_and_down_shift_the_region() {
    assert_eq!(screen(&fed(3, 5, "a\r\nb\r\nc\x1b[S")), ["b", "c", ""]);
    assert_eq!(screen(&fed(3, 5, "a\r\nb\r\nc\x1b[T")), ["", "a", "b"]);
}

#[test]
fn reverse_index_at_the_top_scrolls_down() {
    assert_eq!(screen(&fed(3, 5, "a\r\nb\x1b[1;1H\x1bM")), ["", "a", "b"]);
}

#[test]
fn index_and_next_line_move_down() {
    let term = fed(3, 5, "ab\x1bDc\x1bEd");
    assert_eq!(screen(&term), ["ab", "  c", "d"]);
}

#[test]
fn sgr_sets_attributes_and_indexed_colors() {
    let term = fed(1, 5, "\x1b[1;3;31;44mx");
    let style = style_at(&term, 0, 0);
    assert!(style.attrs.contains(Attrs::BOLD));
    assert!(style.attrs.contains(Attrs::ITALIC));
    assert!(!style.attrs.contains(Attrs::UNDERLINE));
    assert_eq!(style.foreground, Color::indexed(1));
    assert_eq!(style.background, Color::indexed(4));
}

#[test]
fn sgr_reads_semicolon_and_colon_true_color() {
    let term = fed(1, 5, "\x1b[38;2;1;2;3mx\x1b[48:2::4:5:6my\x1b[58:2:7:8:9mz");
    assert_eq!(
        style_at(&term, 0, 0).foreground,
        Color::rgb(Rgb::new(1, 2, 3))
    );
    assert_eq!(
        style_at(&term, 0, 1).background,
        Color::rgb(Rgb::new(4, 5, 6))
    );
    assert_eq!(
        style_at(&term, 0, 2).underline,
        Color::rgb(Rgb::new(7, 8, 9))
    );
}

#[test]
fn sgr_reads_semicolon_and_colon_palette_colors() {
    let term = fed(1, 5, "\x1b[38;5;200mx\x1b[48:5:100my");
    assert_eq!(style_at(&term, 0, 0).foreground, Color::indexed(200));
    assert_eq!(style_at(&term, 0, 1).background, Color::indexed(100));
}

#[test]
fn sgr_continues_after_an_extended_color() {
    let term = fed(1, 5, "\x1b[38;5;9;1mx");
    assert_eq!(style_at(&term, 0, 0).foreground, Color::indexed(9));
    assert!(style_at(&term, 0, 0).attrs.contains(Attrs::BOLD));
}

#[test]
fn sgr_bright_colors_map_to_the_upper_palette() {
    let term = fed(1, 5, "\x1b[91;102mx");
    assert_eq!(style_at(&term, 0, 0).foreground, Color::indexed(9));
    assert_eq!(style_at(&term, 0, 0).background, Color::indexed(10));
}

#[test]
fn sgr_underline_styles_replace_each_other() {
    let term = fed(1, 5, "\x1b[4mx\x1b[4:3my\x1b[21mz\x1b[4:0mw");
    assert!(style_at(&term, 0, 0).attrs.contains(Attrs::UNDERLINE));
    assert!(style_at(&term, 0, 1).attrs.contains(Attrs::CURLY_UNDERLINE));
    assert!(!style_at(&term, 0, 1).attrs.contains(Attrs::UNDERLINE));
    assert!(
        style_at(&term, 0, 2)
            .attrs
            .contains(Attrs::DOUBLE_UNDERLINE)
    );
    assert!(!style_at(&term, 0, 3).attrs.contains(Attrs::ANY_UNDERLINE));
}

#[test]
fn sgr_resets_individual_attributes_and_colors() {
    let term = fed(1, 5, "\x1b[1;2;3;7;9;31;41m\x1b[22;23;27;29;39;49mx");
    assert_eq!(style_at(&term, 0, 0), Style::default());
}

#[test]
fn sgr_zero_and_empty_reset_the_pen() {
    let term = fed(1, 5, "\x1b[1;31mx\x1b[0my\x1b[1mz\x1b[mw");
    assert_eq!(style_at(&term, 0, 1), Style::default());
    assert_eq!(style_at(&term, 0, 3), Style::default());
}

#[test]
fn style_table_survives_more_styles_than_it_can_index() {
    let mut term = term(1, 2);
    let mut parser = Parser::new();
    for value in 0..70_000u32 {
        parser.advance(
            &mut term,
            format!(
                "\x1b[38;2;{};{};{}mx\r",
                value >> 16,
                (value >> 8) & 255,
                value & 255
            )
            .as_bytes(),
        );
    }
    assert_eq!(
        style_at(&term, 0, 0).foreground,
        Color::rgb(Rgb::new(1, 17, 111))
    );
}

#[test]
fn wide_characters_occupy_two_cells() {
    let term = fed(1, 6, "\u{3042}b");
    assert!(term.grid().row(0).cells()[0].is_wide());
    assert!(term.grid().row(0).cells()[1].is_spacer());
    assert_eq!(term.grid().row(0).cells()[2].ch(), Some('b'));
    assert_eq!(cursor(&term), (0, 3));
    assert_eq!(screen(&term), ["\u{3042}b"]);
}

#[test]
fn wide_character_that_does_not_fit_wraps_early() {
    let term = fed(2, 5, "abcd\u{3042}");
    assert_eq!(screen(&term), ["abcd", "\u{3042}"]);
    assert!(term.grid().row(0).is_wrapped());
}

#[test]
fn wide_character_filling_the_last_two_cells_defers_the_wrap() {
    let mut term = fed(2, 4, "ab\u{3042}");
    assert_eq!(cursor(&term), (0, 3));
    feed(&mut term, "c");
    assert_eq!(screen(&term), ["ab\u{3042}", "c"]);
}

#[test]
fn overwriting_half_of_a_wide_character_blanks_the_other_half() {
    let first = fed(1, 6, "\u{3042}\x1b[1;1Hx");
    assert_eq!(screen(&first), ["x"]);
    assert!(!first.grid().row(0).cells()[1].is_spacer());
    let second = fed(1, 6, "\u{3042}\x1b[1;2Hx");
    assert_eq!(screen(&second), [" x"]);
    assert!(!second.grid().row(0).cells()[0].is_wide());
}

#[test]
fn deleting_through_a_wide_character_leaves_no_orphan_half() {
    let term = fed(1, 6, "a\u{3042}b\x1b[1;2H\x1b[P");
    assert_eq!(screen(&term), ["a b"]);
    assert!(
        term.grid()
            .row(0)
            .cells()
            .iter()
            .all(|cell| !cell.is_spacer() && !cell.is_wide())
    );
}

#[test]
fn combining_marks_attach_to_the_previous_cell() {
    let term = fed(1, 6, "e\u{0301}x");
    assert!(term.grid().row(0).cells()[0].is_marked());
    assert_eq!(
        term.grid().row(0).marks(0).collect::<Vec<_>>(),
        ['\u{0301}']
    );
    assert_eq!(screen(&term), ["e\u{0301}x"]);
    assert_eq!(cursor(&term), (0, 2));
}

#[test]
fn combining_marks_attach_to_a_wide_character() {
    let term = fed(1, 6, "\u{304B}\u{3099}");
    assert_eq!(
        term.grid().row(0).marks(0).collect::<Vec<_>>(),
        ['\u{3099}']
    );
    assert_eq!(screen(&term), ["\u{304B}\u{3099}"]);
}

#[test]
fn combining_marks_attach_to_the_last_column_while_the_wrap_is_pending() {
    let term = fed(2, 3, "abe\u{0301}");
    assert_eq!(screen(&term), ["abe\u{0301}", ""]);
}

#[test]
fn combining_marks_without_a_base_are_dropped() {
    let term = fed(1, 6, "\u{0301}a");
    assert_eq!(screen(&term), ["a"]);
}

#[test]
fn overwriting_a_cell_discards_its_combining_marks() {
    let term = fed(1, 6, "e\u{0301}\rx");
    assert_eq!(screen(&term), ["x"]);
    assert_eq!(term.grid().row(0).marks(0).count(), 0);
}

#[test]
fn c1_control_characters_are_not_printed() {
    let term = fed(1, 6, "a\u{0085}b");
    assert_eq!(screen(&term), ["ab"]);
}

#[test]
fn alternate_screen_is_cleared_on_entry_and_restored_on_exit() {
    let mut term = fed(2, 10, "main\x1b[?1049halt");
    assert!(term.modes().contains(Modes::ALTERNATE));
    assert_eq!(screen(&term), ["    alt", ""]);
    feed(&mut term, "\x1b[?1049l");
    assert!(!term.modes().contains(Modes::ALTERNATE));
    assert_eq!(screen(&term), ["main", ""]);
    assert_eq!(cursor(&term), (0, 4));
}

#[test]
fn alternate_screen_keeps_no_history() {
    let term = fed(2, 5, "\x1b[?1049ha\r\nb\r\nc");
    assert_eq!(term.grid().history(), 0);
    assert_eq!(screen(&term), ["b", "c"]);
}

#[test]
fn leaving_the_alternate_screen_twice_keeps_the_cursor() {
    let term = fed(2, 10, "ab\x1b[?1049l");
    assert_eq!(cursor(&term), (0, 2));
}

#[test]
fn legacy_alternate_screen_modes_switch_without_saving_the_cursor() {
    let mut term = fed(2, 10, "main\x1b[?47h\x1b[Hx");
    assert_eq!(screen(&term), ["x", ""]);
    feed(&mut term, "\x1b[?47l");
    assert_eq!(screen(&term), ["main", ""]);
    assert_eq!(cursor(&term), (0, 1));
}

#[test]
fn save_and_restore_cursor_round_trip_position_and_pen() {
    let term = fed(3, 10, "\x1b[2;3H\x1b[31m\x1b7\x1b[H\x1b[0m\x1b8x");
    assert_eq!(screen(&term), ["", "  x", ""]);
    assert_eq!(style_at(&term, 1, 2).foreground, Color::indexed(1));
}

#[test]
fn csi_and_private_mode_cursor_saves_share_the_slot() {
    let term = fed(3, 10, "\x1b[2;3H\x1b[s\x1b[H\x1b[u");
    assert_eq!(cursor(&term), (1, 2));
    let other = fed(3, 10, "\x1b[2;3H\x1b[?1048h\x1b[H\x1b[?1048l");
    assert_eq!(cursor(&other), (1, 2));
}

#[test]
fn status_reports_answer_on_the_reply_channel() {
    let mut term = fed(5, 10, "\x1b[3;4H\x1b[6n\x1b[5n");
    assert_eq!(reply(&mut term), "\x1b[3;4R\x1b[0n");
    assert_eq!(reply(&mut term), "");
}

#[test]
fn device_attributes_identify_the_terminal() {
    let mut term = fed(5, 10, "\x1b[c\x1b[>c");
    assert_eq!(reply(&mut term), "\x1b[?62;4;22c\x1b[>1;10;0c");
}

#[test]
fn private_mode_queries_report_set_reset_and_unknown() {
    let mut term = fed(5, 10, "\x1b[?2026$p\x1b[?2026h\x1b[?2026$p\x1b[?9999$p");
    assert_eq!(
        reply(&mut term),
        "\x1b[?2026;2$y\x1b[?2026;1$y\x1b[?9999;0$y"
    );
}

#[test]
fn text_area_size_is_reported() {
    let mut term = fed(5, 10, "\x1b[18t");
    assert_eq!(reply(&mut term), "\x1b[8;5;10t");
}

#[test]
fn text_area_size_in_pixels_is_reported() {
    let mut term = term(5, 10);
    term.set_cell_size(9, 18);
    feed(&mut term, "\x1b[14t");
    assert_eq!(reply(&mut term), "\x1b[4;90;90t");
}

#[test]
fn cell_size_in_pixels_is_reported() {
    let mut term = term(5, 10);
    term.set_cell_size(9, 18);
    feed(&mut term, "\x1b[16t");
    assert_eq!(reply(&mut term), "\x1b[6;18;9t");
}

#[test]
fn cell_size_has_a_default_before_the_window_sets_it() {
    let mut term = fed(5, 10, "\x1b[16t");
    assert_eq!(reply(&mut term), "\x1b[6;16;8t");
}

#[test]
fn cell_size_of_zero_is_raised_to_one_pixel() {
    let mut term = term(5, 10);
    term.set_cell_size(0, 0);
    feed(&mut term, "\x1b[16t");
    assert_eq!(reply(&mut term), "\x1b[6;1;1t");
}

#[test]
fn unknown_window_report_is_not_answered() {
    let mut term = fed(5, 10, "\x1b[99t");
    assert_eq!(reply(&mut term), "");
}

#[test]
fn title_is_set_from_osc_and_keeps_semicolons() {
    let mut term = fed(1, 5, "\x1b]0;hello\x07");
    assert_eq!(term.title(), "hello");
    assert!(term.take_title_change());
    assert!(!term.take_title_change());
    feed(&mut term, "\x1b]2;a;b\x1b\\");
    assert_eq!(term.title(), "a;b");
}

#[test]
fn clipboard_payload_is_decoded_with_or_without_padding() {
    let mut term = fed(1, 5, "\x1b]52;c;aGVsbG8=\x07");
    assert_eq!(term.take_clipboard().as_deref(), Some("hello"));
    feed(&mut term, "\x1b]52;c;aGVsbG8\x07");
    assert_eq!(term.take_clipboard().as_deref(), Some("hello"));
}

#[test]
fn clipboard_queries_and_invalid_payloads_are_ignored() {
    let mut term = fed(1, 5, "\x1b]52;c;?\x07\x1b]52;c;!!!\x07");
    assert_eq!(term.take_clipboard(), None);
    assert_eq!(reply(&mut term), "");
}

#[test]
fn dynamic_color_queries_reply_with_the_matching_terminator() {
    let mut term = fed(1, 5, "\x1b]11;?\x07\x1b]10;?\x1b\\\x1b]12;?\x07");
    assert_eq!(
        reply(&mut term),
        "\x1b]11;rgb:2121/2727/3333\x07\x1b]10;rgb:c5c5/c8c8/c6c6\x1b\\\x1b]12;rgb:c5c5/c8c8/c6c6\x07"
    );
}

#[test]
fn dynamic_colors_are_set_and_reset() {
    let mut term = fed(
        1,
        5,
        "\x1b]10;#ff0000\x07\x1b]11;rgb:00/ff/00\x07\x1b]12;rgb:0/0/f\x07",
    );
    assert_eq!(term.palette().foreground, Rgb::new(255, 0, 0));
    assert_eq!(term.palette().background, Rgb::new(0, 255, 0));
    assert_eq!(term.palette().cursor, Rgb::new(0, 0, 255));
    feed(&mut term, "\x1b]110\x07\x1b]111\x07\x1b]112\x07");
    assert_eq!(term.palette(), Term::new(1, 5, 0).palette());
}

#[test]
fn invalid_color_specs_leave_the_palette_unchanged() {
    let term = fed(
        1,
        5,
        "\x1b]10;nope\x07\x1b]11;rgb:zz/00/00\x07\x1b]12;rgb:00/00\x07",
    );
    assert_eq!(term.palette(), Term::new(1, 5, 0).palette());
}

#[test]
fn palette_entries_are_set_queried_and_reset() {
    let mut term = fed(
        1,
        5,
        "\x1b]4;1;#010203;2;rgb:ffff/0000/8080\x07\x1b]4;1;?\x07",
    );
    assert_eq!(term.palette().indexed(1), Rgb::new(1, 2, 3));
    assert_eq!(term.palette().indexed(2), Rgb::new(255, 0, 128));
    assert_eq!(reply(&mut term), "\x1b]4;1;rgb:0101/0202/0303\x07");
    feed(&mut term, "\x1b]104;1\x07");
    assert_eq!(
        term.palette().indexed(1),
        Term::new(1, 5, 0).palette().indexed(1)
    );
    assert_eq!(term.palette().indexed(2), Rgb::new(255, 0, 128));
    feed(&mut term, "\x1b]104\x07");
    assert_eq!(term.palette(), Term::new(1, 5, 0).palette());
}

#[test]
fn default_palette_uses_the_fish_theme_magenta_and_bright_red() {
    let term = term(1, 5);
    assert_eq!(term.palette().indexed(5), Rgb::new(0xBD, 0x7B, 0xCD));
    assert_eq!(term.palette().indexed(9), Rgb::new(0xDF, 0x6C, 0x5A));
}

#[test]
fn set_palette_replaces_the_current_colors() {
    let mut term = term(1, 5);
    term.set_palette(tinted());
    assert_eq!(term.palette(), &tinted());
}

#[test]
fn set_palette_becomes_the_target_of_palette_resets() {
    let mut term = term(1, 5);
    term.set_palette(tinted());
    feed(
        &mut term,
        "\x1b]4;1;#000000\x07\x1b]10;#000000\x07\x1b]104\x07\x1b]110\x07",
    );
    assert_eq!(term.palette(), &tinted());
}

#[test]
fn set_palette_survives_a_full_reset() {
    let mut term = term(1, 5);
    term.set_palette(tinted());
    feed(&mut term, "\x1b]4;1;#000000\x07\x1bc");
    assert_eq!(term.palette(), &tinted());
}

#[test]
fn set_palette_damages_every_line() {
    let mut term = term(2, 5);
    term.reset_damage();
    term.set_palette(tinted());
    assert!(term.damage().is_full());
}

#[test]
fn default_palette_follows_the_xterm_cube_and_gray_ramp() {
    let term = term(1, 5);
    assert_eq!(term.palette().indexed(16), Rgb::new(0, 0, 0));
    assert_eq!(term.palette().indexed(21), Rgb::new(0, 0, 255));
    assert_eq!(term.palette().indexed(110), Rgb::new(135, 175, 215));
    assert_eq!(term.palette().indexed(231), Rgb::new(255, 255, 255));
    assert_eq!(term.palette().indexed(232), Rgb::new(8, 8, 8));
    assert_eq!(term.palette().indexed(255), Rgb::new(238, 238, 238));
}

#[test]
fn palette_resolves_default_indexed_and_direct_colors() {
    let term = term(1, 5);
    let fallback = Rgb::new(9, 9, 9);
    assert_eq!(term.palette().resolve(Color::DEFAULT, fallback), fallback);
    assert_eq!(
        term.palette().resolve(Color::indexed(231), fallback),
        Rgb::new(255, 255, 255)
    );
    assert_eq!(
        term.palette()
            .resolve(Color::rgb(Rgb::new(1, 2, 3)), fallback),
        Rgb::new(1, 2, 3)
    );
}

#[test]
fn tabs_advance_to_the_next_stop_and_back() {
    let mut term = fed(1, 20, "a\tb");
    assert_eq!(term.grid().row(0).cells()[8].ch(), Some('b'));
    feed(&mut term, "\x1b[Z");
    assert_eq!(cursor(&term), (0, 8));
    feed(&mut term, "\x1b[2I");
    assert_eq!(cursor(&term), (0, 19));
}

#[test]
fn tab_stops_can_be_set_and_cleared() {
    let mut term = fed(1, 20, "\x1b[1;4H\x1bH\r\t");
    assert_eq!(cursor(&term), (0, 3));
    feed(&mut term, "\x1b[g\r\t");
    assert_eq!(cursor(&term), (0, 8));
    feed(&mut term, "\x1b[3g\r\t");
    assert_eq!(cursor(&term), (0, 19));
}

#[test]
fn dec_special_graphics_translate_through_g0_and_g1() {
    assert_eq!(screen(&fed(1, 5, "\x1b(0q\x1b(Bq")), ["\u{2500}q"]);
    assert_eq!(
        screen(&fed(1, 5, "\x1b)0\x0eqx\x0fq")),
        ["\u{2500}\u{2502}q"]
    );
}

#[test]
fn insert_mode_shifts_existing_text() {
    let mut term = fed(1, 10, "abc\x1b[1;1H\x1b[4hXY");
    assert_eq!(screen(&term), ["XYabc"]);
    feed(&mut term, "\x1b[4lZ");
    assert_eq!(screen(&term), ["XYZbc"]);
}

#[test]
fn origin_mode_addresses_relative_to_the_scroll_region() {
    let mut term = fed(5, 5, "\x1b[2;4r\x1b[?6h\x1b[1;2Hx\x1b[6n");
    assert_eq!(screen(&term), ["", " x", "", "", ""]);
    assert_eq!(reply(&mut term), "\x1b[1;3R");
    feed(&mut term, "\x1b[9;1H");
    assert_eq!(cursor(&term), (3, 0));
}

#[test]
fn disabled_autowrap_overwrites_the_last_column() {
    let term = fed(2, 5, "\x1b[?7labcdefg");
    assert_eq!(screen(&term), ["abcdg", ""]);
    assert_eq!(cursor(&term), (0, 4));
}

#[test]
fn disabled_autowrap_drops_a_wide_character_at_the_margin() {
    let term = fed(2, 5, "\x1b[?7labcd\u{3042}");
    assert_eq!(screen(&term), ["abcd", ""]);
}

#[test]
fn repeat_reprints_the_last_character() {
    assert_eq!(screen(&fed(1, 10, "a\x1b[3b")), ["aaaa"]);
    assert_eq!(screen(&fed(1, 10, "\x1b[3b")), [""]);
}

#[test]
fn newline_mode_adds_a_carriage_return() {
    assert_eq!(screen(&fed(2, 5, "\x1b[20hab\ncd")), ["ab", "cd"]);
    assert_eq!(screen(&fed(2, 5, "ab\ncd")), ["ab", "  cd"]);
}

#[test]
fn private_modes_are_tracked() {
    let mut term = fed(1, 5, "\x1b[?1;1000;1006;2004h\x1b[?25l\x1b=");
    for mode in [
        Modes::APP_CURSOR,
        Modes::MOUSE_PRESS,
        Modes::MOUSE_SGR,
        Modes::BRACKETED_PASTE,
        Modes::APP_KEYPAD,
        Modes::ANY_MOUSE,
    ] {
        assert!(term.modes().contains(mode));
    }
    assert!(!term.cursor().visible);
    feed(&mut term, "\x1b[?1000l\x1b>");
    assert!(!term.modes().contains(Modes::ANY_MOUSE));
    assert!(!term.modes().contains(Modes::APP_KEYPAD));
}

#[test]
fn cursor_style_sets_shape_and_blinking() {
    let mut term = fed(1, 5, "\x1b[4 q");
    assert_eq!(term.cursor().shape, CursorShape::Underline);
    assert!(!term.cursor().blinking);
    feed(&mut term, "\x1b[5 q");
    assert_eq!(term.cursor().shape, CursorShape::Bar);
    assert!(term.cursor().blinking);
    feed(&mut term, "\x1b[2 q\x1b[9 q");
    assert_eq!(term.cursor().shape, CursorShape::Block);
    assert!(!term.cursor().blinking);
}

#[test]
fn full_reset_clears_screen_history_pen_and_title() {
    let mut term = fed(2, 5, "\x1b]0;t\x07a\r\nb\r\nc\x1b[31;1m\x1b[?6h\x1bcx");
    assert_eq!(screen(&term), ["x", ""]);
    assert_eq!(term.grid().history(), 0);
    assert_eq!(style_at(&term, 0, 0), Style::default());
    assert_eq!(term.title(), "");
    assert!(term.take_title_change());
}

#[test]
fn soft_reset_restores_modes_pen_and_region() {
    let mut term = fed(
        3,
        5,
        "\x1b[2;3r\x1b[?6h\x1b[4h\x1b[?25l\x1b[31m\x1b[!p\x1b[1;1Hx",
    );
    assert_eq!(screen(&term), ["x", "", ""]);
    assert_eq!(style_at(&term, 0, 0), Style::default());
    assert!(term.cursor().visible);
    assert!(!term.modes().contains(Modes::INSERT));
    feed(&mut term, "\x1b[3;1H\n");
    assert_eq!(term.grid().history(), 1);
}

#[test]
fn alignment_test_fills_the_screen() {
    let term = fed(2, 3, "\x1b[2;2H\x1b#8");
    assert_eq!(screen(&term), ["EEE", "EEE"]);
    assert_eq!(cursor(&term), (0, 0));
}

#[test]
fn resize_shrinking_lines_pushes_the_top_into_history() {
    let mut term = fed(3, 5, "a\r\nb\r\nc");
    term.resize(2, 5);
    assert_eq!(screen(&term), ["b", "c"]);
    assert_eq!(term.grid().history(), 1);
    assert_eq!(cursor(&term), (1, 1));
    term.resize(3, 5);
    assert_eq!(screen(&term), ["a", "b", "c"]);
    assert_eq!(term.grid().history(), 0);
    assert_eq!(cursor(&term), (2, 1));
}

#[test]
fn resize_shrinking_lines_trims_blank_rows_below_the_cursor_first() {
    let mut term = fed(4, 5, "a\r\nb");
    term.resize(2, 5);
    assert_eq!(screen(&term), ["a", "b"]);
    assert_eq!(term.grid().history(), 0);
}

#[test]
fn resize_growing_lines_appends_blank_rows_without_history() {
    let mut term = fed(2, 5, "a\r\nb");
    term.resize(4, 5);
    assert_eq!(screen(&term), ["a", "b", "", ""]);
    assert_eq!(cursor(&term), (1, 1));
}

#[test]
fn resize_rewraps_long_lines_and_joins_them_again() {
    let mut term = fed(2, 10, "abcdefgh");
    term.resize(2, 4);
    assert_eq!(screen(&term), ["abcd", "efgh"]);
    assert!(term.grid().row(0).is_wrapped());
    assert_eq!(cursor(&term), (1, 3));
    term.resize(2, 10);
    assert_eq!(screen(&term), ["abcdefgh", ""]);
    assert!(!term.grid().row(0).is_wrapped());
    assert_eq!(cursor(&term), (0, 7));
    assert_eq!((term.lines(), term.cols()), (2, 10));
}

#[test]
fn resize_rewraps_history_rows() {
    let mut term = fed(2, 6, "abcdef\r\nx\r\ny");
    term.resize(2, 3);
    assert_eq!(term.grid().history(), 2);
    assert_eq!(absolute(&term, 0), "abc");
    assert_eq!(absolute(&term, 1), "def");
    assert_eq!(screen(&term), ["x", "y"]);
    assert_eq!(cursor(&term), (1, 1));
}

#[test]
fn resize_keeps_blank_rows_below_the_cursor_out_of_the_rewrap() {
    let mut term = fed(4, 6, "abcdef");
    term.resize(4, 3);
    assert_eq!(screen(&term), ["abc", "def", "", ""]);
    assert_eq!(term.grid().history(), 0);
    assert_eq!(cursor(&term), (1, 2));
}

#[test]
fn resize_keeps_combining_marks_with_their_cells() {
    let mut term = fed(2, 6, "abe\u{0301}f");
    term.resize(2, 3);
    assert_eq!(screen(&term), ["abe\u{0301}", "f"]);
    assert_eq!(
        term.grid().row(0).marks(2).collect::<Vec<_>>(),
        ['\u{0301}']
    );
}

#[test]
fn resize_truncates_the_alternate_screen_and_rewraps_the_primary_one() {
    let mut term = fed(2, 6, "abcdef\x1b[?1049h\x1b[HABCDEF");
    term.resize(2, 3);
    assert_eq!(screen(&term), ["ABC", ""]);
    feed(&mut term, "\x1b[?1049l");
    assert_eq!(screen(&term), ["abc", "def"]);
}

#[test]
fn resize_moves_a_wide_character_that_no_longer_fits_to_the_next_row() {
    let mut term = fed(2, 6, "ab\u{3042}c");
    term.resize(2, 3);
    assert_eq!(screen(&term), ["ab", "\u{3042}c"]);
    assert!(!term.grid().row(0).cells()[2].is_wide());
    assert!(term.grid().row(1).cells()[0].is_wide());
    assert_eq!(cursor(&term), (1, 2));
}

#[test]
fn resize_cutting_a_wide_character_on_the_alternate_screen_blanks_its_first_half() {
    let mut term = fed(1, 6, "\x1b[?1049h\x1b[Hab\u{3042}");
    term.resize(1, 3);
    assert_eq!(screen(&term), ["ab"]);
    assert!(!term.grid().row(0).cells()[2].is_wide());
}

#[test]
fn resize_resets_the_scroll_region_and_keeps_the_alternate_screen_usable() {
    let mut term = fed(4, 5, "\x1b[2;3r\x1b[?1049h");
    term.resize(3, 6);
    feed(&mut term, "\x1b[3;1Hx\ny\x1b[?1049l");
    assert_eq!(term.grid().history(), 0);
    assert_eq!((term.lines(), term.cols()), (3, 6));
    assert_eq!(screen(&term), ["", "", ""]);
}

#[test]
fn resize_clamps_degenerate_sizes() {
    let mut term = term(3, 5);
    term.resize(0, 0);
    assert_eq!((term.lines(), term.cols()), (1, 2));
}

#[test]
fn display_offset_shows_history_and_stays_anchored_during_output() {
    let mut term = fed(2, 5, "1\r\n2\r\n3\r\n4");
    term.scroll_display(1);
    assert_eq!(term.display_offset(), 1);
    assert_eq!(shown(term.visible_row(0)), "2");
    assert_eq!(shown(term.visible_row(1)), "3");
    feed(&mut term, "\r\n5");
    assert_eq!(term.display_offset(), 2);
    assert_eq!(shown(term.visible_row(0)), "2");
    term.scroll_display(100);
    assert_eq!(term.display_offset(), 3);
    term.scroll_display(-1);
    assert_eq!(term.display_offset(), 2);
    term.reset_display();
    assert_eq!(term.display_offset(), 0);
    assert_eq!(shown(term.visible_row(1)), "5");
}

#[test]
fn cursor_view_follows_the_display_offset() {
    let mut term = fed(2, 5, "1\r\n2\r\n3\r\n4\x1b[1;1H");
    term.scroll_display(1);
    assert_eq!(cursor(&term), (1, 0));
    assert!(term.cursor().visible);
    term.scroll_display(1);
    assert!(!term.cursor().visible);
}

#[test]
fn damage_tracks_touched_lines_and_full_redraws() {
    let mut term = term(3, 5);
    assert!(term.damage().is_full());
    term.reset_damage();
    assert!((0..3).all(|line| !term.damage().line(line)));
    feed(&mut term, "\x1b[2;1Ha");
    assert!(!term.damage().is_full());
    assert!(!term.damage().line(0));
    assert!(term.damage().line(1));
    term.reset_damage();
    feed(&mut term, "\x1b[3;1H\n");
    assert!(term.damage().is_full());
    assert!(term.damage().line(0));
}

#[test]
fn text_joins_wrapped_rows_and_breaks_unwrapped_ones() {
    let term = fed(4, 4, "abcdef\r\ngh");
    assert_eq!(
        term.text(Span {
            start: Point { line: 0, column: 1 },
            end: Point { line: 2, column: 0 }
        }),
        "bcdef\ng"
    );
}

#[test]
fn directory_is_unknown_before_osc_7() {
    assert_eq!(term(1, 5).directory(), None);
}

#[test]
fn osc_7_records_the_host_and_the_directory() {
    let term = fed(1, 5, "\x1b]7;file://box/home/u/src\x07");
    assert_eq!(term.directory(), Some(("box", "/home/u/src")));
}

#[test]
fn osc_7_accepts_an_empty_host() {
    let term = fed(1, 5, "\x1b]7;file:///tmp\x1b\\");
    assert_eq!(term.directory(), Some(("", "/tmp")));
}

#[test]
fn osc_7_decodes_percent_escapes() {
    let term = fed(1, 5, "\x1b]7;file://box/a%20b/%E3%81%82%3Bc\x07");
    assert_eq!(term.directory(), Some(("box", "/a b/\u{3042};c")));
}

#[test]
fn osc_7_keeps_a_malformed_percent_escape() {
    let term = fed(1, 5, "\x1b]7;file://box/a%2/b%zz/c%\x07");
    assert_eq!(term.directory(), Some(("box", "/a%2/b%zz/c%")));
}

#[test]
fn osc_7_keeps_a_semicolon_in_the_path() {
    let term = fed(1, 5, "\x1b]7;file://box/a;b\x07");
    assert_eq!(term.directory(), Some(("box", "/a;b")));
}

#[test]
fn osc_7_ignores_other_schemes_and_a_missing_path() {
    let term = fed(
        1,
        5,
        "\x1b]7;file://box/first\x07\x1b]7;https://box/x\x07\x1b]7;file://box\x07",
    );
    assert_eq!(term.directory(), Some(("box", "/first")));
}

#[test]
fn osc_133_marks_the_prompt_and_output_rows() {
    let term = fed(
        4,
        10,
        "\x1b]133;A\x07$ ls\r\n\x1b]133;C\x07out\r\n\x1b]133;D;0\x07",
    );
    assert_eq!(
        marks(&term),
        [
            (true, false, false),
            (false, true, false),
            (false, false, false),
            (false, false, false)
        ]
    );
}

#[test]
fn osc_133_d_with_a_nonzero_code_marks_the_prompt_as_failed() {
    let term = fed(
        4,
        10,
        "\x1b]133;A\x07$ x\r\n\x1b]133;C\x07err\r\n\x1b]133;D;127\x07",
    );
    assert_eq!(marks(&term)[0], (true, false, true));
}

#[test]
fn osc_133_d_without_a_numeric_code_marks_nothing() {
    let term = fed(
        4,
        10,
        "\x1b]133;A\x07$ x\r\n\x1b]133;D\x07\x1b]133;D;abc\x07",
    );
    assert_eq!(marks(&term)[0], (true, false, false));
}

#[test]
fn osc_133_d_without_a_prompt_marks_nothing() {
    let mut term = term(2, 10);
    term.reset_damage();
    feed(&mut term, "\x1b]133;D;1\x07");
    assert_eq!(marks(&term), [(false, false, false); 2]);
    assert!(!term.damage().is_full());
}

#[test]
fn osc_133_failure_damages_the_screen() {
    let mut term = fed(2, 10, "\x1b]133;A\x07$ x\r\n");
    term.reset_damage();
    feed(&mut term, "\x1b]133;D;1\x07");
    assert!(term.damage().is_full());
}

#[test]
fn osc_133_failure_marks_the_nearest_prompt_above() {
    let term = fed(
        4,
        10,
        "\x1b]133;A\x07$ a\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ b\r\n\x1b]133;D;1\x07",
    );
    assert_eq!(
        marks(&term)[..2],
        [(true, false, false), (true, false, true)]
    );
}

#[test]
fn osc_133_is_ignored_on_the_alternate_screen() {
    let term = fed(2, 10, "\x1b[?1049h\x1b]133;A\x07\x1b]133;C\x07");
    assert_eq!(marks(&term), [(false, false, false); 2]);
}

#[test]
fn osc_133_ignores_other_marks() {
    let term = fed(2, 10, "\x1b]133;B\x07\x1b]133;\x07");
    assert_eq!(marks(&term), [(false, false, false); 2]);
}

#[test]
fn shell_marks_scroll_into_history_with_their_rows() {
    let term = fed(2, 10, "\x1b]133;A\x07$ a\r\nb\r\nc");
    assert_eq!(
        marks(&term),
        [
            (true, false, false),
            (false, false, false),
            (false, false, false)
        ]
    );
}

#[test]
fn shell_marks_are_cleared_with_the_screen() {
    let term = fed(2, 10, "\x1b]133;A\x07$ a\x1b[2J");
    assert_eq!(marks(&term), [(false, false, false); 2]);
}

#[test]
fn shell_marks_do_not_follow_a_recycled_row() {
    let mut term = Term::new(2, 10, 1);
    feed(&mut term, "\x1b]133;A\x07a\r\nb\r\nc\r\nd");
    assert_eq!(marks(&term), [(false, false, false); 3]);
}

#[test]
fn shell_marks_stay_on_the_first_row_after_a_reflow() {
    let mut term = fed(3, 10, "x\r\n\x1b]133;A\x07abcdefgh\r\ny");
    term.resize(3, 4);
    assert_eq!(
        marks(&term).iter().map(|mark| mark.0).collect::<Vec<_>>(),
        [false, true, false, false]
    );
}

#[test]
fn osc_8_links_the_following_cells() {
    let term = fed(1, 10, "a\x1b]8;;https://e.test/x\x07bc\x1b]8;;\x07d");
    let links: Vec<_> = (0..4)
        .map(|column| term.link(style_at(&term, 0, column).link))
        .collect();
    assert_eq!(
        links,
        [
            None,
            Some("https://e.test/x"),
            Some("https://e.test/x"),
            None
        ]
    );
}

#[test]
fn osc_8_reuses_the_id_of_a_repeated_uri() {
    let term = fed(1, 10, "\x1b]8;;u1\x07a\x1b]8;;u2\x07b\x1b]8;;u1\x07c");
    assert_eq!(
        [0, 1, 2].map(|column| style_at(&term, 0, column).link),
        [1, 2, 1]
    );
}

#[test]
fn osc_8_keeps_semicolons_in_the_uri_and_ignores_the_parameters() {
    let term = fed(1, 10, "\x1b]8;id=7;https://e.test/a;b\x1b\\x");
    assert_eq!(
        term.link(style_at(&term, 0, 0).link),
        Some("https://e.test/a;b")
    );
}

#[test]
fn sgr_reset_keeps_the_link() {
    let term = fed(1, 10, "\x1b]8;;u\x07\x1b[1ma\x1b[0mb\x1b[mc");
    assert_eq!(
        [0, 1, 2].map(|column| style_at(&term, 0, column).link),
        [1, 1, 1]
    );
    assert_eq!(style_at(&term, 0, 1).attrs, Attrs::default());
    assert_eq!(style_at(&term, 0, 2).attrs, Attrs::default());
}

#[test]
fn osc_8_ignores_a_uri_over_the_size_limit() {
    let term = fed(1, 10, &format!("\x1b]8;;{}\x07a", "u".repeat(4097)));
    assert_eq!(style_at(&term, 0, 0).link, 0);
}

#[test]
fn osc_8_stops_adding_links_when_the_table_is_full() {
    let mut term = term(1, 10);
    for index in 0..=u16::MAX {
        feed(&mut term, &format!("\x1b]8;;u{index}\x07"));
    }
    feed(&mut term, "a\x1b]8;;u0\x07b");
    assert_eq!(style_at(&term, 0, 0).link, 0);
    assert_eq!(style_at(&term, 0, 1).link, 1);
}

#[test]
fn link_of_an_unknown_id_is_absent() {
    let term = fed(1, 10, "\x1b]8;;u\x07a");
    assert_eq!(term.link(0), None);
    assert_eq!(term.link(2), None);
}

#[test]
fn full_reset_forgets_the_links() {
    let term = fed(1, 10, "\x1b]8;;u\x07a\x1bc");
    assert_eq!(term.link(1), None);
}

#[test]
fn keyboard_flags_start_at_zero() {
    assert_eq!(term(1, 5).keyboard(), 0);
}

#[test]
fn keyboard_push_sets_the_flags_and_pop_restores_them() {
    let mut term = fed(1, 5, "\x1b[>1u\x1b[>5u");
    assert_eq!(term.keyboard(), 5);
    feed(&mut term, "\x1b[<u");
    assert_eq!(term.keyboard(), 1);
    feed(&mut term, "\x1b[<5u");
    assert_eq!(term.keyboard(), 0);
}

#[test]
fn keyboard_push_without_flags_pushes_zero() {
    let term = fed(1, 5, "\x1b[>3u\x1b[>u");
    assert_eq!(term.keyboard(), 0);
}

#[test]
fn keyboard_set_replaces_adds_and_removes_flags() {
    let mut term = fed(1, 5, "\x1b[=5u");
    assert_eq!(term.keyboard(), 5);
    feed(&mut term, "\x1b[=2;2u");
    assert_eq!(term.keyboard(), 7);
    feed(&mut term, "\x1b[=4;3u");
    assert_eq!(term.keyboard(), 3);
    feed(&mut term, "\x1b[=8;1u");
    assert_eq!(term.keyboard(), 8);
    feed(&mut term, "\x1b[=1;9u");
    assert_eq!(term.keyboard(), 8);
}

#[test]
fn keyboard_set_changes_only_the_top_of_the_stack() {
    let term = fed(1, 5, "\x1b[>1u\x1b[>2u\x1b[=8u\x1b[<u");
    assert_eq!(term.keyboard(), 1);
}

#[test]
fn keyboard_flags_outside_the_known_bits_are_dropped() {
    let term = fed(1, 5, "\x1b[>255u");
    assert_eq!(term.keyboard(), 31);
}

#[test]
fn keyboard_query_reports_the_flags() {
    let mut term = fed(1, 5, "\x1b[>9u\x1b[?u");
    assert_eq!(reply(&mut term), "\x1b[?9u");
}

#[test]
fn keyboard_stack_drops_the_oldest_entry_when_full() {
    let mut term = term(1, 5);
    for flags in 1..=17 {
        feed(&mut term, &format!("\x1b[>{flags}u"));
    }
    feed(&mut term, "\x1b[<15u");
    assert_eq!(term.keyboard(), 2);
    feed(&mut term, "\x1b[<u");
    assert_eq!(term.keyboard(), 0);
}

#[test]
fn keyboard_flags_are_separate_on_the_alternate_screen() {
    let mut term = fed(1, 5, "\x1b[>1u\x1b[?1049h");
    assert_eq!(term.keyboard(), 0);
    feed(&mut term, "\x1b[>8u\x1b[?1049l");
    assert_eq!(term.keyboard(), 1);
    feed(&mut term, "\x1b[?1049h");
    assert_eq!(term.keyboard(), 8);
}

#[test]
fn full_reset_clears_the_keyboard_flags() {
    let term = fed(1, 5, "\x1b[>1u\x1bc");
    assert_eq!(term.keyboard(), 0);
}

#[test]
fn recycled_row_keeps_nothing_of_its_earlier_content() {
    let mut term = Term::new(2, 6, 1);
    feed(
        &mut term,
        "\x1b[41mab\u{3042}e\u{301}\x1b[K\x1b[m\r\n\r\n\r\n",
    );
    assert!(
        term.grid()
            .rows()
            .all(|row| row.cells().iter().all(|&cell| cell == Cell::default()))
    );
    assert!(term.grid().rows().all(|row| row.marks(4).next().is_none()));
}

#[test]
fn text_written_after_erasing_to_the_end_of_the_line_stays() {
    let term = fed(1, 8, "abcdef\x1b[1;3H\x1b[K\x1b[1;7Hz");
    assert_eq!(screen(&term), ["ab    z"]);
}

#[test]
fn line_edits_give_the_same_screen_wherever_the_history_starts() {
    for scrolled in 0..12 {
        let mut term = Term::new(4, 3, 2);
        feed(&mut term, &"x\r\n".repeat(scrolled));
        feed(
            &mut term,
            "\x1b[2J\x1b[H1\r\n2\r\n3\r\n4\x1b[2;1H\x1b[L\x1b[3;1H\x1b[M\x1b[1;1H\x1bM\x1b[2S",
        );
        assert_eq!(screen(&term), ["", "3", "", ""], "scrolled {scrolled}");
    }
}

#[test]
fn resize_keeps_the_row_order_after_the_history_wrapped_around() {
    let mut term = Term::new(2, 4, 2);
    feed(&mut term, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7");
    term.resize(3, 6);
    assert_eq!(term.grid().history(), 1);
    assert_eq!(absolute(&term, 0), "4");
    assert_eq!(screen(&term), ["5", "6", "7"]);
}

#[test]
fn erasing_saved_lines_keeps_the_screen_after_the_history_wrapped_around() {
    let mut term = Term::new(2, 4, 2);
    feed(&mut term, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\x1b[3J\r\n8");
    assert_eq!(term.grid().history(), 1);
    assert_eq!(absolute(&term, 0), "6");
    assert_eq!(screen(&term), ["7", "8"]);
}

#[test]
fn scroll_region_from_the_top_keeps_lines_below_it_once_the_history_is_full() {
    let mut term = Term::new(3, 4, 1);
    feed(&mut term, "1\r\n2\r\n3\r\n4\r\n5\x1b[1;2r\x1b[2;1H\n\n");
    assert_eq!(screen(&term), ["", "", "5"]);
    assert_eq!(term.grid().history(), 1);
    assert_eq!(absolute(&term, 0), "4");
}

#[test]
fn alternate_screen_scrolls_its_rows_in_order() {
    let term = fed(3, 4, "\x1b[?1049h1\r\n2\r\n3\r\n4\r\n5");
    assert_eq!(screen(&term), ["3", "4", "5"]);
    assert_eq!(term.grid().history(), 0);
}

#[test]
fn long_runs_of_wide_characters_wrap_across_several_lines() {
    let term = fed(12, 6, &"\u{3042}".repeat(35));
    assert_eq!(screen(&term)[..11], ["\u{3042}\u{3042}\u{3042}"; 11]);
    assert_eq!(screen(&term)[11], "\u{3042}\u{3042}");
    assert_eq!(cursor(&term), (11, 4));
}

#[test]
fn wide_characters_after_narrow_text_keep_their_columns() {
    let term = fed(1, 10, "a\u{3042}\u{E9}\u{4E2D}b");
    assert_eq!(screen(&term), ["a\u{3042}\u{E9}\u{4E2D}b"]);
    assert_eq!(cursor(&term), (0, 7));
    assert!(term.grid().row(0).cells()[1].is_wide());
    assert!(term.grid().row(0).cells()[2].is_spacer());
    assert!(term.grid().row(0).cells()[4].is_wide());
}

#[test]
fn row_takes_forty_bytes() {
    assert_eq!(std::mem::size_of::<Row>(), 40);
}

#[test]
fn used_covers_the_written_cells_of_a_row() {
    let term = fed(2, 10, "abc");
    assert_eq!(term.grid().row(0).used(), 3);
    assert_eq!(term.grid().row(1).used(), 0);
}

#[test]
fn used_covers_a_row_erased_with_a_background_color() {
    assert_eq!(fed(2, 10, "\x1b[44m\x1b[K").grid().row(0).used(), 10);
}

#[test]
fn cells_after_used_are_default_cells() {
    let term = fed(2, 10, "\x1b[44mab\x1b[0m\x1b[5C\x1b[41mc\x1b[1K");
    let row = term.grid().row(0);
    assert!(row.used() < 10);
    assert!(
        row.cells()[row.used()..]
            .iter()
            .all(|cell| *cell == Cell::default())
    );
}
