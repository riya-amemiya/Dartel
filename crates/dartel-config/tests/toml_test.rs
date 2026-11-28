use std::borrow::Cow;

use dartel_config::toml::{Error, Table, Value, parse};

fn value<'t, 'a>(table: &'t Table<'a>, path: &[&str]) -> &'t Value<'a> {
    path[1..]
        .iter()
        .fold(table.get(path[0]).unwrap(), |value, key| match value {
            Value::Table(table) => table.get(key).unwrap(),
            other => panic!("{other:?} is not a table"),
        })
}

fn string(text: &str) -> Value<'_> {
    Value::String(Cow::Borrowed(text))
}

fn error(input: &str) -> (usize, String) {
    let error = parse(input).unwrap_err();
    (error.line, error.message)
}

fn rejected(line: usize, message: &str) -> (usize, String) {
    (line, message.to_owned())
}

#[test]
fn empty_input_is_an_empty_table() {
    assert_eq!(parse("").unwrap().entries(), []);
}

#[test]
fn bare_key_takes_a_value() {
    let document = parse("name = \"dartel\"").unwrap();
    assert_eq!(value(&document, &["name"]), &string("dartel"));
}

#[test]
fn missing_key_is_absent() {
    assert_eq!(parse("name = 1").unwrap().get("other"), None);
}

#[test]
fn comments_and_blank_lines_are_skipped() {
    let document = parse("# first\n\n  # second\na = 1\n\n").unwrap();
    assert_eq!(document.entries().len(), 1);
}

#[test]
fn comment_follows_a_value() {
    let document = parse("a = 1 # note\nb = 2").unwrap();
    assert_eq!(value(&document, &["b"]), &Value::Integer(2));
}

#[test]
fn crlf_line_endings_are_accepted() {
    let document = parse("a = 1\r\n# note\r\nb = 2\r\n").unwrap();
    assert_eq!(value(&document, &["b"]), &Value::Integer(2));
}

#[test]
fn bare_keys_accept_digits_dashes_and_underscores() {
    let document = parse("a-b_1 = true").unwrap();
    assert_eq!(value(&document, &["a-b_1"]), &Value::Boolean(true));
}

#[test]
fn basic_quoted_key_keeps_its_characters() {
    let document = parse("\"a b\\t\" = 1").unwrap();
    assert_eq!(value(&document, &["a b\t"]), &Value::Integer(1));
}

#[test]
fn literal_quoted_key_keeps_its_dots() {
    let document = parse("'c.d' = 2").unwrap();
    assert_eq!(value(&document, &["c.d"]), &Value::Integer(2));
}

#[test]
fn dotted_keys_build_nested_tables() {
    let document = parse("a.b.c = 1\na.b.d = 2").unwrap();
    assert_eq!(value(&document, &["a", "b", "c"]), &Value::Integer(1));
    assert_eq!(value(&document, &["a", "b", "d"]), &Value::Integer(2));
}

#[test]
fn spaces_around_the_dots_of_a_key_are_ignored() {
    let document = parse("a . b = 1").unwrap();
    assert_eq!(value(&document, &["a", "b"]), &Value::Integer(1));
}

#[test]
fn table_header_groups_the_following_keys() {
    let document = parse("[font]\nsize = 13\n[window]\nsize = 4").unwrap();
    assert_eq!(value(&document, &["font", "size"]), &Value::Integer(13));
    assert_eq!(value(&document, &["window", "size"]), &Value::Integer(4));
}

#[test]
fn dotted_table_header_creates_the_parents() {
    let document = parse("[ colors . normal ]\nred = 1").unwrap();
    assert_eq!(
        value(&document, &["colors", "normal", "red"]),
        &Value::Integer(1)
    );
}

#[test]
fn parent_table_is_defined_after_its_child() {
    let document = parse("[a.b]\nx = 1\n[a]\ny = 2").unwrap();
    assert_eq!(value(&document, &["a", "b", "x"]), &Value::Integer(1));
    assert_eq!(value(&document, &["a", "y"]), &Value::Integer(2));
}

#[test]
fn dotted_key_extends_a_parent_made_by_a_child_header() {
    let document = parse("[a.b.c]\n[a]\nb.x = 1").unwrap();
    assert_eq!(value(&document, &["a", "b", "x"]), &Value::Integer(1));
}

#[test]
fn sub_table_is_added_under_a_table_made_by_dotted_keys() {
    let document =
        parse("[fruit]\napple.color = \"red\"\n[fruit.apple.texture]\nsmooth = true").unwrap();
    assert_eq!(
        value(&document, &["fruit", "apple", "texture", "smooth"]),
        &Value::Boolean(true)
    );
}

#[test]
fn array_of_tables_appends_a_table_per_header() {
    let document = parse("[[tab]]\nname = \"a\"\n[[tab]]\nname = \"b\"").unwrap();
    let Value::Array(tabs) = value(&document, &["tab"]) else {
        panic!("tab is not an array");
    };
    let names: Vec<_> = tabs
        .iter()
        .map(|tab| match tab {
            Value::Table(table) => table.get("name").cloned(),
            _ => None,
        })
        .collect();
    assert_eq!(names, [Some(string("a")), Some(string("b"))]);
}

#[test]
fn table_under_an_array_of_tables_belongs_to_the_last_element() {
    let document = parse("[[tab]]\n[[tab]]\n[tab.font]\nsize = 9").unwrap();
    let Value::Array(tabs) = value(&document, &["tab"]) else {
        panic!("tab is not an array");
    };
    assert_eq!(tabs[0], Value::Table(Table::default()));
    let Value::Table(last) = &tabs[1] else {
        panic!("the element is not a table");
    };
    assert_eq!(value(last, &["font", "size"]), &Value::Integer(9));
}

#[test]
fn entries_keep_the_order_and_the_line_of_each_key() {
    let document = parse("b = 1\n\na = 2").unwrap();
    let entries: Vec<_> = document
        .entries()
        .iter()
        .map(|entry| (&*entry.key, entry.line))
        .collect();
    assert_eq!(entries, [("b", 1), ("a", 3)]);
}

#[test]
fn basic_string_decodes_escapes() {
    let document = parse("a = \"\\b\\t\\n\\f\\r\\\"\\\\\\u00e9\\U0001F600 end\"").unwrap();
    assert_eq!(
        value(&document, &["a"]),
        &string("\u{8}\t\n\u{c}\r\"\\\u{e9}\u{1f600} end")
    );
}

#[test]
fn empty_strings_are_accepted() {
    let document = parse("a = \"\"\nb = ''").unwrap();
    assert_eq!(value(&document, &["a"]), &string(""));
    assert_eq!(value(&document, &["b"]), &string(""));
}

#[test]
fn literal_string_keeps_backslashes() {
    let document = parse("a = 'C:\\dir\\n'").unwrap();
    assert_eq!(value(&document, &["a"]), &string("C:\\dir\\n"));
}

#[test]
fn multi_line_basic_string_drops_the_first_newline() {
    let document = parse("a = \"\"\"\none\ntwo\\t\"\"\"").unwrap();
    assert_eq!(value(&document, &["a"]), &string("one\ntwo\t"));
}

#[test]
fn multi_line_basic_string_keeps_crlf_line_endings() {
    let document = parse("a = \"\"\"\r\none\r\ntwo\"\"\"").unwrap();
    assert_eq!(value(&document, &["a"]), &string("one\r\ntwo"));
}

#[test]
fn multi_line_basic_string_joins_lines_after_a_trailing_backslash() {
    let document = parse("a = \"\"\"one \\  \n\r\n   two\"\"\"").unwrap();
    assert_eq!(value(&document, &["a"]), &string("one two"));
}

#[test]
fn multi_line_basic_string_may_end_with_quotes() {
    let document = parse("a = \"\"\"say \"hi\"\"\"\"\"").unwrap();
    assert_eq!(value(&document, &["a"]), &string("say \"hi\"\""));
}

#[test]
fn multi_line_literal_string_keeps_its_content() {
    let document = parse("a = '''\none\\\n'two'\r\n'''").unwrap();
    assert_eq!(value(&document, &["a"]), &string("one\\\n'two'\r\n"));
}

#[test]
fn multi_line_literal_string_may_end_with_quotes() {
    let document = parse("a = '''it's''''").unwrap();
    assert_eq!(value(&document, &["a"]), &string("it's'"));
}

#[test]
fn integers_accept_signs_and_underscores() {
    let document = parse("a = +1_000\nb = -17\nc = 0").unwrap();
    assert_eq!(value(&document, &["a"]), &Value::Integer(1000));
    assert_eq!(value(&document, &["b"]), &Value::Integer(-17));
    assert_eq!(value(&document, &["c"]), &Value::Integer(0));
}

#[test]
fn integers_accept_hexadecimal_octal_and_binary() {
    let document = parse("a = 0xDEAD_beef\nb = 0o755\nc = 0b1101").unwrap();
    assert_eq!(value(&document, &["a"]), &Value::Integer(0xDEAD_BEEF));
    assert_eq!(value(&document, &["b"]), &Value::Integer(0o755));
    assert_eq!(value(&document, &["c"]), &Value::Integer(0b1101));
}

#[test]
fn floats_accept_fractions_and_exponents() {
    let document = parse("a = 13.5\nb = -2e3\nc = 6.25E-2\nd = 1_0.2_5e+1_0").unwrap();
    assert_eq!(value(&document, &["a"]), &Value::Float(13.5));
    assert_eq!(value(&document, &["b"]), &Value::Float(-2000.0));
    assert_eq!(value(&document, &["c"]), &Value::Float(0.0625));
    assert_eq!(value(&document, &["d"]), &Value::Float(10.25e10));
}

#[test]
fn floats_accept_infinity() {
    let document = parse("a = inf\nb = +inf\nc = -inf").unwrap();
    assert_eq!(value(&document, &["a"]), &Value::Float(f64::INFINITY));
    assert_eq!(value(&document, &["b"]), &Value::Float(f64::INFINITY));
    assert_eq!(value(&document, &["c"]), &Value::Float(f64::NEG_INFINITY));
}

#[test]
fn floats_accept_not_a_number() {
    let document = parse("a = nan\nb = +nan\nc = -nan").unwrap();
    for key in ["a", "b", "c"] {
        assert!(matches!(value(&document, &[key]), Value::Float(float) if float.is_nan()));
    }
}

#[test]
fn booleans_are_parsed() {
    let document = parse("a = true\nb = false").unwrap();
    assert_eq!(value(&document, &["a"]), &Value::Boolean(true));
    assert_eq!(value(&document, &["b"]), &Value::Boolean(false));
}

#[test]
fn date_times_keep_their_text() {
    let document = parse(
        "a = 1979-05-27T07:32:00Z\nb = 1979-05-27 07:32:00.999-07:00\nc = 1979-05-27\nd = 07:32:00.5\ne = 1979-05-27t07:32:00z\nf = 1979-05-27T07:32:00+09:00\ng = 1979-05-27T07:32:00",
    )
    .unwrap();
    for (key, text) in [
        ("a", "1979-05-27T07:32:00Z"),
        ("b", "1979-05-27 07:32:00.999-07:00"),
        ("c", "1979-05-27"),
        ("d", "07:32:00.5"),
        ("e", "1979-05-27t07:32:00z"),
        ("f", "1979-05-27T07:32:00+09:00"),
        ("g", "1979-05-27T07:32:00"),
    ] {
        assert_eq!(value(&document, &[key]), &Value::Datetime(text));
    }
}

#[test]
fn date_followed_by_a_comment_is_a_date() {
    let document = parse("a = 1979-05-27 # day").unwrap();
    assert_eq!(value(&document, &["a"]), &Value::Datetime("1979-05-27"));
}

#[test]
fn arrays_hold_values_across_lines_with_a_trailing_comma() {
    let document = parse("a = [\n  1, # one\n  2,\n]\nb = []").unwrap();
    assert_eq!(
        value(&document, &["a"]),
        &Value::Array(vec![Value::Integer(1), Value::Integer(2)])
    );
    assert_eq!(value(&document, &["b"]), &Value::Array(Vec::new()));
}

#[test]
fn arrays_nest_and_mix_types() {
    let document = parse("a = [[1], [\"x\", 2.5]]").unwrap();
    assert_eq!(
        value(&document, &["a"]),
        &Value::Array(vec![
            Value::Array(vec![Value::Integer(1)]),
            Value::Array(vec![string("x"), Value::Float(2.5)]),
        ])
    );
}

#[test]
fn inline_tables_hold_plain_and_dotted_keys() {
    let document = parse("a = { b = 1, c.d = \"x\" }\ne = {}").unwrap();
    assert_eq!(value(&document, &["a", "b"]), &Value::Integer(1));
    assert_eq!(value(&document, &["a", "c", "d"]), &string("x"));
    assert_eq!(value(&document, &["e"]), &Value::Table(Table::default()));
}

#[test]
fn nesting_up_to_the_limit_is_accepted() {
    assert!(parse(&format!("a = {}{}", "[".repeat(64), "]".repeat(64))).is_ok());
}

#[test]
fn nested_arrays_beyond_the_limit_are_rejected() {
    assert_eq!(
        error(&format!("a = {}", "[".repeat(65))),
        rejected(1, "values are nested too deeply")
    );
}

#[test]
fn nested_inline_tables_beyond_the_limit_are_rejected() {
    assert_eq!(
        error(&format!("a = {}", "{a = ".repeat(65))),
        rejected(1, "values are nested too deeply")
    );
}

#[test]
fn duplicate_key_is_rejected() {
    assert_eq!(
        error("a = 1\nb = 2\na = 3"),
        rejected(3, "key `a` is defined twice")
    );
}

#[test]
fn key_that_names_a_dotted_table_is_rejected() {
    assert_eq!(
        error("a.b = 1\na = 2"),
        rejected(2, "key `a` is defined twice")
    );
}

#[test]
fn duplicate_key_in_an_inline_table_is_rejected() {
    assert_eq!(
        error("a = { b = 1, b = 2 }"),
        rejected(1, "key `b` is defined twice")
    );
}

#[test]
fn table_defined_twice_is_rejected() {
    assert_eq!(
        error("[a]\n[b]\n[a]"),
        rejected(3, "table `a` is defined twice")
    );
}

#[test]
fn table_made_by_dotted_keys_is_not_reopened() {
    assert_eq!(
        error("[fruit]\napple.color = \"red\"\n[fruit.apple]"),
        rejected(3, "table `apple` is defined twice")
    );
}

#[test]
fn dotted_key_does_not_extend_a_table_header() {
    assert_eq!(
        error("[a.b]\n[a]\nb.c = 1"),
        rejected(3, "table `b` is already defined")
    );
}

#[test]
fn dotted_key_through_a_value_is_rejected() {
    assert_eq!(
        error("a = 1\na.b = 2"),
        rejected(2, "key `a` is not a table")
    );
}

#[test]
fn table_header_through_a_value_is_rejected() {
    assert_eq!(error("a = 1\n[a.b]"), rejected(2, "key `a` is not a table"));
}

#[test]
fn table_header_does_not_extend_an_inline_table() {
    assert_eq!(
        error("a = { b = 1 }\n[a.c]"),
        rejected(2, "key `a` is not a table")
    );
}

#[test]
fn table_header_does_not_extend_a_plain_array() {
    assert_eq!(
        error("a = [1]\n[a.b]"),
        rejected(2, "key `a` is not a table")
    );
}

#[test]
fn table_header_does_not_replace_a_value() {
    assert_eq!(
        error("a = 1\n[a]"),
        rejected(2, "key `a` is already defined as another type")
    );
}

#[test]
fn plain_array_is_not_an_array_of_tables() {
    assert_eq!(
        error("a = []\n[[a]]"),
        rejected(2, "key `a` is already defined as another type")
    );
}

#[test]
fn table_header_does_not_replace_an_array_of_tables() {
    assert_eq!(
        error("[[a]]\n[a]"),
        rejected(2, "key `a` is already defined as another type")
    );
}

#[test]
fn array_of_tables_does_not_replace_a_table() {
    assert_eq!(
        error("[a]\n[[a]]"),
        rejected(2, "key `a` is already defined as another type")
    );
}

#[test]
fn missing_key_is_rejected() {
    assert_eq!(error("= 1"), rejected(1, "expected a key"));
}

#[test]
fn missing_equals_is_rejected() {
    assert_eq!(error("a 1"), rejected(1, "expected `=` after the key"));
}

#[test]
fn missing_value_is_rejected() {
    assert_eq!(error("a ="), rejected(1, "expected a value"));
}

#[test]
fn bare_word_value_is_rejected() {
    assert_eq!(
        error("a = hello"),
        rejected(1, "`hello` is not a valid value")
    );
}

#[test]
fn text_after_a_value_is_rejected() {
    assert_eq!(
        error("a = 1 2"),
        rejected(1, "expected the end of the line")
    );
}

#[test]
fn bare_carriage_return_is_rejected() {
    assert_eq!(
        error("a = 1\rb = 2"),
        rejected(1, "expected the end of the line")
    );
}

#[test]
fn unclosed_basic_string_is_rejected() {
    assert_eq!(
        error("a = \"open\nb = 1"),
        rejected(1, "a string is not closed on its line")
    );
}

#[test]
fn unclosed_literal_string_is_rejected() {
    assert_eq!(
        error("a = 'open"),
        rejected(1, "a string is not closed on its line")
    );
}

#[test]
fn unclosed_multi_line_basic_string_is_rejected() {
    assert_eq!(
        error("a = \"\"\"\nopen\n"),
        rejected(3, "a multi-line string is not closed")
    );
}

#[test]
fn unclosed_multi_line_literal_string_is_rejected() {
    assert_eq!(
        error("a = '''\nopen"),
        rejected(2, "a multi-line string is not closed")
    );
}

#[test]
fn control_character_in_a_multi_line_basic_string_is_rejected() {
    assert_eq!(
        error("a = \"\"\"x\u{1}\"\"\""),
        rejected(1, "a string contains a control character")
    );
}

#[test]
fn control_character_in_a_multi_line_literal_string_is_rejected() {
    assert_eq!(
        error("a = '''x\ry'''"),
        rejected(1, "a string contains a control character")
    );
}

#[test]
fn unknown_escape_is_rejected() {
    assert_eq!(
        error("a = \"\\q\""),
        rejected(1, "an escape sequence is not valid")
    );
}

#[test]
fn escape_at_the_end_of_the_input_is_rejected() {
    assert_eq!(
        error("a = \"\\"),
        rejected(1, "an escape sequence is not valid")
    );
}

#[test]
fn backslash_before_a_space_in_a_multi_line_string_is_rejected() {
    assert_eq!(
        error("a = \"\"\"x\\ y\"\"\""),
        rejected(1, "an escape sequence is not valid")
    );
}

#[test]
fn surrogate_unicode_escape_is_rejected() {
    assert_eq!(
        error("a = \"\\uD800\""),
        rejected(1, "a unicode escape is not valid")
    );
}

#[test]
fn short_unicode_escape_is_rejected() {
    assert_eq!(
        error("a = \"\\u12\""),
        rejected(1, "a unicode escape is not valid")
    );
}

#[test]
fn control_character_in_a_comment_is_rejected() {
    assert_eq!(
        error("# bad\u{7f}"),
        rejected(1, "a comment contains a control character")
    );
}

#[test]
fn leading_zero_is_rejected() {
    assert_eq!(error("a = 012"), rejected(1, "`012` is not a valid value"));
}

#[test]
fn misplaced_underscore_is_rejected() {
    assert_eq!(
        error("a = 1__2"),
        rejected(1, "`1__2` is not a valid value")
    );
}

#[test]
fn integer_overflow_is_rejected() {
    assert_eq!(
        error("a = 9223372036854775808"),
        rejected(1, "`9223372036854775808` is not a valid value")
    );
}

#[test]
fn signed_hexadecimal_is_rejected() {
    assert_eq!(
        error("a = +0x1"),
        rejected(1, "`+0x1` is not a valid value")
    );
}

#[test]
fn hexadecimal_without_digits_is_rejected() {
    assert_eq!(error("a = 0x"), rejected(1, "`0x` is not a valid value"));
}

#[test]
fn float_without_a_fraction_digit_is_rejected() {
    assert_eq!(error("a = 1."), rejected(1, "`1.` is not a valid value"));
}

#[test]
fn float_without_an_exponent_digit_is_rejected() {
    assert_eq!(error("a = 1e"), rejected(1, "`1e` is not a valid value"));
}

#[test]
fn date_time_without_seconds_is_rejected() {
    assert_eq!(
        error("a = 1979-05-27T07:32"),
        rejected(1, "`1979-05-27T07:32` is not a valid value")
    );
}

#[test]
fn date_time_with_an_empty_fraction_is_rejected() {
    assert_eq!(
        error("a = 07:32:00."),
        rejected(1, "`07:32:00.` is not a valid value")
    );
}

#[test]
fn time_with_an_offset_is_rejected() {
    assert_eq!(
        error("a = 07:32:00Z"),
        rejected(1, "`07:32:00Z` is not a valid value")
    );
}

#[test]
fn unclosed_table_header_is_rejected() {
    assert_eq!(
        error("[a"),
        rejected(1, "expected `]` after the table name")
    );
}

#[test]
fn array_of_tables_header_needs_adjacent_brackets() {
    assert_eq!(
        error("[[a] ]"),
        rejected(1, "expected `]` after the table name")
    );
}

#[test]
fn empty_table_header_is_rejected() {
    assert_eq!(error("[]"), rejected(1, "expected a key"));
}

#[test]
fn text_after_a_table_header_is_rejected() {
    assert_eq!(error("[a] b"), rejected(1, "expected the end of the line"));
}

#[test]
fn missing_comma_in_an_array_is_rejected() {
    assert_eq!(
        error("a = [1 2]"),
        rejected(1, "expected `,` or `]` in an array")
    );
}

#[test]
fn trailing_comma_in_an_inline_table_is_rejected() {
    assert_eq!(error("a = { b = 1, }"), rejected(1, "expected a key"));
}

#[test]
fn missing_comma_in_an_inline_table_is_rejected() {
    assert_eq!(
        error("a = { b = 1 c = 2 }"),
        rejected(1, "expected `,` or `}` in an inline table")
    );
}

#[test]
fn newline_in_an_inline_table_is_rejected() {
    assert_eq!(error("a = {\nb = 1 }"), rejected(1, "expected a key"));
}

#[test]
fn error_reports_the_line_after_a_multi_line_string() {
    assert_eq!(
        error("a = \"\"\"\none\ntwo\"\"\"\nb = [\n1,\n2,\n]\nc ="),
        rejected(8, "expected a value")
    );
}

#[test]
fn error_display_includes_the_line() {
    assert_eq!(
        Error {
            line: 4,
            message: "expected a key".to_owned()
        }
        .to_string(),
        "line 4: expected a key"
    );
}
