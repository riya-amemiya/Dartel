use dartel_search::{Case, Query};
use dartel_term::{Point, Span, Target, Term, path_candidate};
use dartel_vt::Parser;

fn fed(lines: usize, cols: usize, input: &str) -> Term {
    let mut term = Term::new(lines, cols, 100);
    Parser::new().advance(&mut term, input.as_bytes());
    term
}

fn span(start: (usize, usize), end: (usize, usize)) -> Span {
    Span {
        start: Point {
            line: start.0,
            column: start.1,
        },
        end: Point {
            line: end.0,
            column: end.1,
        },
    }
}

#[test]
fn term_search_reports_screen_positions() {
    let term = fed(3, 10, "foo bar\r\nBar foo");
    assert_eq!(
        term.search(&Query::literal("bar", Case::Smart).unwrap(), usize::MAX),
        [span((0, 4), (0, 6)), span((1, 0), (1, 2))]
    );
}

#[test]
fn term_search_covers_history_lines() {
    let term = fed(2, 10, "first\r\nsecond\r\nthird\r\nfourth");
    assert_eq!(
        term.search(&Query::literal("first", Case::Smart).unwrap(), usize::MAX),
        [span((0, 0), (0, 4))]
    );
    assert_eq!(
        term.search(&Query::literal("fourth", Case::Smart).unwrap(), usize::MAX),
        [span((3, 0), (3, 5))]
    );
}

#[test]
fn term_search_follows_wrapped_rows() {
    let term = fed(3, 5, "abcdefgh\r\ndefg");
    assert_eq!(
        term.search(&Query::literal("defg", Case::Smart).unwrap(), usize::MAX),
        [span((0, 3), (1, 1)), span((2, 0), (2, 3))]
    );
}

#[test]
fn term_search_does_not_join_unwrapped_rows() {
    let term = fed(3, 5, "abc\r\ndef");
    assert_eq!(
        term.search(&Query::literal("cd", Case::Smart).unwrap(), usize::MAX),
        []
    );
}

#[test]
fn term_search_maps_wide_and_combined_cells_to_columns() {
    let term = fed(2, 12, "\u{3042}\u{3044}x\u{3046}e\u{0301}y");
    assert_eq!(
        term.search(&Query::literal("x", Case::Smart).unwrap(), usize::MAX),
        [span((0, 4), (0, 4))]
    );
    assert_eq!(
        term.search(
            &Query::literal("\u{3044}", Case::Smart).unwrap(),
            usize::MAX
        ),
        [span((0, 2), (0, 2))]
    );
    assert_eq!(
        term.search(&Query::literal("y", Case::Smart).unwrap(), usize::MAX),
        [span((0, 8), (0, 8))]
    );
    assert_eq!(
        term.search(
            &Query::literal("e\u{0301}", Case::Smart).unwrap(),
            usize::MAX
        ),
        [span((0, 7), (0, 7))]
    );
}

#[test]
fn term_search_treats_skipped_cells_as_spaces() {
    let term = fed(1, 10, "a\x1b[1;4Hb");
    assert_eq!(
        term.search(&Query::literal("a  b", Case::Smart).unwrap(), usize::MAX),
        [span((0, 0), (0, 3))]
    );
}

#[test]
fn term_search_honors_the_limit() {
    let term = fed(1, 10, "aaaa");
    assert_eq!(
        term.search(&Query::literal("a", Case::Smart).unwrap(), 2)
            .len(),
        2
    );
}

fn at(line: usize, column: usize) -> Point {
    Point { line, column }
}

#[test]
fn word_at_selects_the_word_under_the_point() {
    let term = fed(1, 20, "ls src/main.rs now");
    assert_eq!(term.word_at(at(0, 5)), Some(span((0, 3), (0, 13))));
}

#[test]
fn word_at_stops_at_separators() {
    let term = fed(1, 20, "(a,b) \"c d\"");
    assert_eq!(term.word_at(at(0, 1)), Some(span((0, 1), (0, 1))));
    assert_eq!(term.word_at(at(0, 3)), Some(span((0, 3), (0, 3))));
    assert_eq!(term.word_at(at(0, 7)), Some(span((0, 7), (0, 7))));
}

#[test]
fn word_at_is_absent_on_blanks_and_separators() {
    let term = fed(1, 10, "a (b");
    for column in [1, 2, 6] {
        assert_eq!(term.word_at(at(0, column)), None);
    }
}

#[test]
fn word_at_follows_a_wrapped_row() {
    let term = fed(3, 5, "ab cdefghij k");
    assert_eq!(term.word_at(at(1, 2)), Some(span((0, 3), (2, 0))));
}

#[test]
fn word_at_does_not_cross_a_line_break() {
    let term = fed(2, 5, "abcde\r\nfgh");
    assert_eq!(term.word_at(at(0, 2)), Some(span((0, 0), (0, 4))));
    assert_eq!(term.word_at(at(1, 0)), Some(span((1, 0), (1, 2))));
}

#[test]
fn word_at_covers_wide_characters_from_either_cell() {
    let term = fed(1, 10, "a \u{3042}\u{3044} b");
    for column in 2..=5 {
        assert_eq!(term.word_at(at(0, column)), Some(span((0, 2), (0, 5))));
    }
}

#[test]
fn word_at_clamps_a_point_outside_the_grid() {
    let term = fed(1, 5, "abcde");
    assert_eq!(term.word_at(at(9, 9)), Some(span((0, 0), (0, 4))));
}

#[test]
fn line_at_covers_the_whole_wrapped_line() {
    let term = fed(4, 5, "x\r\nabcdefgh\r\ny");
    assert_eq!(term.line_at(at(2, 1)), span((1, 0), (2, 4)));
    assert_eq!(term.line_at(at(1, 4)), span((1, 0), (2, 4)));
    assert_eq!(term.line_at(at(0, 0)), span((0, 0), (0, 4)));
    assert_eq!(term.line_at(at(9, 0)), span((3, 0), (3, 4)));
}

#[test]
fn target_at_prefers_the_osc_8_link() {
    let term = fed(1, 20, "\x1b]8;;https://e.test/x\x07docs\x1b]8;;\x07");
    assert_eq!(
        term.target_at(at(0, 2)),
        Some(Target::Url("https://e.test/x".to_owned()))
    );
}

#[test]
fn target_at_uses_the_wide_cell_for_a_spacer() {
    let term = fed(1, 10, "\x1b]8;;mailto:a@e.test\x07\u{3042}\x1b]8;;\x07");
    assert_eq!(
        term.target_at(at(0, 1)),
        Some(Target::Url("mailto:a@e.test".to_owned()))
    );
}

#[test]
fn target_at_accepts_the_listed_link_schemes_in_any_case() {
    for uri in [
        "http://e.test",
        "HTTPS://e.test",
        "ftp://e.test",
        "File:///tmp/a",
        "mailto:a@e.test",
    ] {
        let term = fed(1, 10, &format!("\x1b]8;;{uri}\x07x\x1b]8;;\x07"));
        assert_eq!(term.target_at(at(0, 0)), Some(Target::Url(uri.to_owned())));
    }
}

#[test]
fn target_at_ignores_a_link_with_another_scheme() {
    for uri in [
        "javascript:alert(1)",
        "x-app://run",
        "ht",
        "\u{3042}\u{3044}",
    ] {
        let term = fed(1, 10, &format!("\x1b]8;;{uri}\x07x\x1b]8;;\x07"));
        assert_eq!(term.target_at(at(0, 0)), None);
    }
}

#[test]
fn target_at_finds_a_url_in_the_text() {
    let term = fed(1, 40, "see https://e.test/a?b=1, ok");
    assert_eq!(
        term.target_at(at(0, 10)),
        Some(Target::Url("https://e.test/a?b=1".to_owned()))
    );
}

#[test]
fn target_at_finds_a_url_across_a_wrapped_row() {
    let term = fed(3, 10, "https://e.test/abc/def end");
    assert_eq!(
        term.target_at(at(1, 3)),
        Some(Target::Url("https://e.test/abc/def".to_owned()))
    );
}

#[test]
fn target_at_returns_a_path_candidate_for_other_words() {
    let term = fed(1, 40, "error at src/main.rs:12:5: bad");
    assert_eq!(
        term.target_at(at(0, 12)),
        Some(Target::Path("src/main.rs".to_owned()))
    );
}

#[test]
fn target_at_is_absent_on_a_blank() {
    let term = fed(1, 10, "a b");
    assert_eq!(term.target_at(at(0, 1)), None);
    assert_eq!(term.target_at(at(0, 8)), None);
}

#[test]
fn target_at_is_absent_when_the_word_is_only_punctuation() {
    let term = fed(1, 10, "a ... b");
    assert_eq!(term.target_at(at(0, 3)), None);
}

#[test]
fn path_candidate_drops_the_line_and_the_column() {
    assert_eq!(path_candidate("src/main.rs:12:5"), "src/main.rs");
    assert_eq!(path_candidate("src/main.rs:12"), "src/main.rs");
    assert_eq!(path_candidate("src/main.rs:12:5:"), "src/main.rs");
}

#[test]
fn path_candidate_drops_trailing_punctuation() {
    assert_eq!(path_candidate("notes.txt."), "notes.txt");
    assert_eq!(path_candidate("notes.txt,"), "notes.txt");
}

#[test]
fn path_candidate_keeps_other_colons() {
    assert_eq!(path_candidate("a:b"), "a:b");
    assert_eq!(path_candidate("a:1:2:3"), "a:1");
    assert_eq!(path_candidate("plain"), "plain");
}

#[test]
fn prompt_before_and_after_find_the_nearest_prompt_rows() {
    let term = fed(
        6,
        10,
        "\x1b]133;A\x07p1\r\nout\r\n\x1b]133;A\x07p2\r\nout\r\n\x1b]133;A\x07p3",
    );
    assert_eq!(term.prompt_before(4), Some(2));
    assert_eq!(term.prompt_before(3), Some(2));
    assert_eq!(term.prompt_before(0), None);
    assert_eq!(term.prompt_before(99), Some(4));
    assert_eq!(term.prompt_after(0), Some(2));
    assert_eq!(term.prompt_after(2), Some(4));
    assert_eq!(term.prompt_after(4), None);
}

#[test]
fn last_output_spans_the_rows_between_the_output_mark_and_the_next_prompt() {
    let term = fed(
        6,
        10,
        "\x1b]133;A\x07$ ls\r\n\x1b]133;C\x07one\r\ntwo\r\n\x1b]133;D;0\x07\x1b]133;A\x07$ ",
    );
    assert_eq!(term.last_output(), Some(span((1, 0), (2, 9))));
    assert_eq!(term.text(span((1, 0), (2, 9))), "one\ntwo");
}

#[test]
fn last_output_reaches_the_cursor_while_a_command_runs() {
    let term = fed(6, 10, "\x1b]133;A\x07$ ls\r\n\x1b]133;C\x07one\r\ntw");
    assert_eq!(term.last_output(), Some(span((1, 0), (2, 9))));
}

#[test]
fn last_output_is_absent_without_an_output_mark() {
    assert_eq!(fed(2, 10, "\x1b]133;A\x07$ ").last_output(), None);
}

#[test]
fn last_output_is_absent_when_the_command_printed_nothing() {
    let term = fed(
        4,
        10,
        "\x1b]133;A\x07$ cd\r\n\x1b]133;C\x07\x1b]133;D;0\x07\x1b]133;A\x07$ ",
    );
    assert_eq!(term.last_output(), None);
}
