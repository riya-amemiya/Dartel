use dartel_search::{Case, Query};
use dartel_term::{Point, Span, Term};
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
