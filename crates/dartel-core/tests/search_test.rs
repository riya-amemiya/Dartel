use dartel_core::search::{Case, Query, find_all};
use dartel_core::term::{Point, Span, Term};
use dartel_core::vt::Parser;

fn ranges(query: &Query, haystack: &str) -> Vec<(usize, usize)> {
    find_all(query, haystack.as_bytes(), usize::MAX)
        .iter()
        .map(|found| (found.start(), found.end()))
        .collect()
}

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
fn literal_finds_every_occurrence() {
    let query = Query::literal("ab", Case::Sensitive).unwrap();
    assert_eq!(
        ranges(&query, "ab cab\nabab"),
        [(0, 2), (4, 6), (7, 9), (9, 11)]
    );
}

#[test]
fn match_exposes_its_range() {
    let query = Query::literal("b", Case::Sensitive).unwrap();
    assert_eq!(find_all(&query, b"abc", usize::MAX)[0].range(), 1..2);
}

#[test]
fn smart_case_folds_lowercase_queries_only() {
    assert_eq!(
        ranges(&Query::literal("bar", Case::Smart).unwrap(), "bar Bar BAR"),
        [(0, 3), (4, 7), (8, 11)]
    );
    assert_eq!(
        ranges(&Query::literal("Bar", Case::Smart).unwrap(), "bar Bar BAR"),
        [(4, 7)]
    );
}

#[test]
fn explicit_case_modes_override_the_query_text() {
    assert_eq!(
        ranges(
            &Query::literal("Bar", Case::Insensitive).unwrap(),
            "bar BAR"
        ),
        [(0, 3), (4, 7)]
    );
    assert_eq!(
        ranges(&Query::literal("bar", Case::Sensitive).unwrap(), "bar BAR"),
        [(0, 3)]
    );
}

#[test]
fn literal_treats_regex_syntax_as_text() {
    assert_eq!(
        ranges(&Query::literal("a.c", Case::Smart).unwrap(), "abc a.c A.C"),
        [(4, 7), (8, 11)]
    );
}

#[test]
fn regex_matches_patterns() {
    assert_eq!(
        ranges(&Query::regex("fo+", Case::Sensitive).unwrap(), "fo foo f"),
        [(0, 2), (3, 6)]
    );
}

#[test]
fn regex_without_meta_characters_matches_like_a_literal() {
    assert_eq!(
        ranges(
            &Query::regex("foo-bar", Case::Sensitive).unwrap(),
            "foo-bar Foo-Bar"
        ),
        ranges(
            &Query::literal("foo-bar", Case::Sensitive).unwrap(),
            "foo-bar Foo-Bar"
        )
    );
}

#[test]
fn regex_smart_case_ignores_uppercase_inside_escapes() {
    assert_eq!(
        ranges(&Query::regex(r"\Sbc", Case::Smart).unwrap(), "ABC"),
        [(0, 3)]
    );
    assert_eq!(
        ranges(&Query::regex(r"\SBc", Case::Smart).unwrap(), "aBc abc"),
        [(0, 3)]
    );
    assert_eq!(
        ranges(&Query::regex(r"a\\b", Case::Smart).unwrap(), r"A\B"),
        [(0, 3)]
    );
}

#[test]
fn regex_anchors_apply_to_each_line() {
    assert_eq!(
        ranges(
            &Query::regex("^b$", Case::Sensitive).unwrap(),
            "a\nb\nbb\nb"
        ),
        [(2, 3), (7, 8)]
    );
}

#[test]
fn matches_never_span_line_terminators() {
    assert_eq!(
        ranges(
            &Query::regex(r"a\sb", Case::Sensitive).unwrap(),
            "a b\na\nb"
        ),
        [(0, 3)]
    );
}

#[test]
fn empty_matches_are_skipped() {
    assert_eq!(
        ranges(&Query::literal("", Case::Sensitive).unwrap(), "abc"),
        []
    );
    assert_eq!(
        ranges(&Query::regex("x*", Case::Sensitive).unwrap(), "axxb"),
        [(1, 3)]
    );
}

#[test]
fn invalid_regex_is_an_error() {
    assert!(Query::regex("(", Case::Smart).is_err());
}

#[test]
fn limit_caps_the_number_of_matches() {
    let query = Query::literal("a", Case::Sensitive).unwrap();
    assert_eq!(find_all(&query, b"aaaaa", 2).len(), 2);
}

#[test]
fn find_at_starts_from_the_given_offset() {
    let query = Query::literal("ab", Case::Sensitive).unwrap();
    assert_eq!(
        query.find_at(b"abab", 1).map(|found| found.start()),
        Some(2)
    );
    assert_eq!(query.find_at(b"abab", 3), None);
    assert_eq!(query.find_at(b"abab", 9), None);
    assert_eq!(
        Query::regex("a.", Case::Sensitive)
            .unwrap()
            .find_at(b"abab", 9),
        None
    );
}

#[test]
fn multibyte_queries_match_byte_ranges() {
    let query = Query::literal("\u{65E5}\u{672C}", Case::Smart).unwrap();
    assert_eq!(ranges(&query, "a\u{65E5}\u{672C}b"), [(1, 7)]);
}

#[test]
fn large_haystacks_match_a_sequential_scan() {
    let haystack: String = (0..60_000)
        .map(|line| {
            if line % 997 == 0 {
                format!("line {line} holds the Needle here\n")
            } else {
                format!("line {line} holds nothing of note\n")
            }
        })
        .collect();
    assert!(haystack.len() > 1 << 20);
    for query in [
        Query::literal("Needle", Case::Sensitive).unwrap(),
        Query::literal("needle", Case::Smart).unwrap(),
        Query::regex(r"N\w+le here$", Case::Sensitive).unwrap(),
    ] {
        assert_eq!(
            ranges(&query, &haystack)
                .iter()
                .map(|&(start, _)| start)
                .collect::<Vec<_>>(),
            haystack
                .match_indices("Needle")
                .map(|(start, _)| start)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn large_haystacks_respect_the_limit() {
    let haystack = "needle in every line\n".repeat(80_000);
    let query = Query::literal("needle", Case::Sensitive).unwrap();
    assert_eq!(ranges(&query, &haystack)[..3], [(0, 6), (21, 27), (42, 48)]);
    assert_eq!(find_all(&query, haystack.as_bytes(), 5).len(), 5);
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
