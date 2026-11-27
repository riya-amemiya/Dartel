use dartel_search::{Case, Query, find_all};

fn ranges(query: &Query, haystack: &str) -> Vec<(usize, usize)> {
    find_all(query, haystack.as_bytes(), usize::MAX)
        .iter()
        .map(|found| (found.start(), found.end()))
        .collect()
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
