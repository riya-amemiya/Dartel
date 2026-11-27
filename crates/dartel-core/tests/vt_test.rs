use dartel_core::vt::{MAX_PARAMS, Params, Parser, Perform};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    Print(String),
    Execute(u8),
    Csi {
        params: Vec<Vec<u16>>,
        intermediates: Vec<u8>,
        ignore: bool,
        action: u8,
    },
    Esc {
        intermediates: Vec<u8>,
        ignore: bool,
        byte: u8,
    },
    Osc {
        params: Vec<Vec<u8>>,
        bell: bool,
    },
    Hook {
        params: Vec<Vec<u16>>,
        intermediates: Vec<u8>,
        ignore: bool,
        action: u8,
    },
    Put(u8),
    Unhook,
}

#[derive(Default)]
struct Recorder {
    actions: Vec<Action>,
}

impl Recorder {
    fn text(&mut self, text: &str) {
        if let Some(Action::Print(existing)) = self.actions.last_mut() {
            existing.push_str(text);
        } else {
            self.actions.push(Action::Print(text.to_owned()));
        }
    }
}

fn groups(params: &Params) -> Vec<Vec<u16>> {
    params.iter().map(<[u16]>::to_vec).collect()
}

impl Perform for Recorder {
    fn print(&mut self, c: char) {
        self.text(c.encode_utf8(&mut [0; 4]));
    }

    fn print_ascii(&mut self, bytes: &[u8]) {
        self.text(std::str::from_utf8(bytes).unwrap());
    }

    fn execute(&mut self, byte: u8) {
        self.actions.push(Action::Execute(byte));
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8) {
        self.actions.push(Action::Csi {
            params: groups(params),
            intermediates: intermediates.to_vec(),
            ignore,
            action,
        });
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        self.actions.push(Action::Esc {
            intermediates: intermediates.to_vec(),
            ignore,
            byte,
        });
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        self.actions.push(Action::Osc {
            params: params.iter().map(|param| param.to_vec()).collect(),
            bell: bell_terminated,
        });
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8) {
        self.actions.push(Action::Hook {
            params: groups(params),
            intermediates: intermediates.to_vec(),
            ignore,
            action,
        });
    }

    fn put(&mut self, byte: u8) {
        self.actions.push(Action::Put(byte));
    }

    fn unhook(&mut self) {
        self.actions.push(Action::Unhook);
    }
}

#[derive(Default)]
struct CharCounter {
    printed: Vec<char>,
}

impl Perform for CharCounter {
    fn print(&mut self, c: char) {
        self.printed.push(c);
    }

    fn execute(&mut self, _byte: u8) {}

    fn csi_dispatch(&mut self, _: &Params, _: &[u8], _: bool, _: u8) {}

    fn esc_dispatch(&mut self, _: &[u8], _: bool, _: u8) {}

    fn osc_dispatch(&mut self, _: &[&[u8]], _: bool) {}
}

fn parse(chunks: &[&[u8]]) -> Vec<Action> {
    let mut parser = Parser::new();
    let mut recorder = Recorder::default();
    for chunk in chunks {
        parser.advance(&mut recorder, chunk);
    }
    recorder.actions
}

fn csi(params: &[&[u16]], intermediates: &[u8], action: u8) -> Action {
    Action::Csi {
        params: params.iter().map(|group| group.to_vec()).collect(),
        intermediates: intermediates.to_vec(),
        ignore: false,
        action,
    }
}

fn print(text: &str) -> Action {
    Action::Print(text.to_owned())
}

#[test]
fn printable_ascii_is_delivered_as_text() {
    assert_eq!(parse(&[b"hello, world"]), vec![print("hello, world")]);
}

#[test]
fn default_print_ascii_forwards_each_byte_to_print() {
    let mut counter = CharCounter::default();
    Parser::new().advance(&mut counter, b"abc\x1b[0m\x1b7\x1b]0;t\x07\rd");
    assert_eq!(counter.printed, vec!['a', 'b', 'c', 'd']);
}

#[test]
fn c0_controls_are_executed_between_text() {
    assert_eq!(
        parse(&[b"a\r\nb\x07"]),
        vec![
            print("a"),
            Action::Execute(b'\r'),
            Action::Execute(b'\n'),
            print("b"),
            Action::Execute(0x07)
        ]
    );
}

#[test]
fn delete_is_ignored_in_ground_state() {
    assert_eq!(parse(&[b"a\x7fb"]), vec![print("ab")]);
}

#[test]
fn csi_collects_semicolon_separated_parameters() {
    assert_eq!(
        parse(&[b"\x1b[1;31m"]),
        vec![csi(&[&[1], &[31]], b"", b'm')]
    );
}

#[test]
fn csi_without_parameters_dispatches_an_empty_list() {
    assert_eq!(parse(&[b"\x1b[m"]), vec![csi(&[], b"", b'm')]);
}

#[test]
fn csi_empty_parameters_default_to_zero() {
    assert_eq!(
        parse(&[b"\x1b[;5;H"]),
        vec![csi(&[&[0], &[5], &[0]], b"", b'H')]
    );
}

#[test]
fn csi_groups_colon_separated_subparameters() {
    assert_eq!(
        parse(&[b"\x1b[4:3;38:2::10:20:30m"]),
        vec![csi(&[&[4, 3], &[38, 2, 0, 10, 20, 30]], b"", b'm')]
    );
}

#[test]
fn csi_keeps_private_markers_and_intermediates() {
    assert_eq!(
        parse(&[b"\x1b[?25h\x1b[ q\x1b[?2026$p\x1b[>c"]),
        vec![
            csi(&[&[25]], b"?", b'h'),
            csi(&[], b" ", b'q'),
            csi(&[&[2026]], b"?$", b'p'),
            csi(&[], b">", b'c')
        ]
    );
}

#[test]
fn csi_parameter_values_saturate() {
    assert_eq!(
        parse(&[b"\x1b[99999999A"]),
        vec![csi(&[&[u16::MAX]], b"", b'A')]
    );
}

#[test]
fn csi_with_too_many_parameters_is_flagged_as_ignored() {
    let sequence = format!("\x1b[{}m", "1;".repeat(MAX_PARAMS));
    assert_eq!(
        parse(&[sequence.as_bytes()]),
        vec![Action::Csi {
            params: vec![vec![1]; MAX_PARAMS],
            intermediates: Vec::new(),
            ignore: true,
            action: b'm'
        }]
    );
}

#[test]
fn csi_with_too_many_intermediates_is_flagged_as_ignored() {
    assert_eq!(
        parse(&[b"\x1b[ !\"p"]),
        vec![Action::Csi {
            params: Vec::new(),
            intermediates: b" !".to_vec(),
            ignore: true,
            action: b'p'
        }]
    );
}

#[test]
fn csi_with_a_misplaced_private_marker_is_dropped() {
    assert_eq!(parse(&[b"\x1b[1?hx"]), vec![print("x")]);
}

#[test]
fn csi_with_parameters_after_intermediates_is_dropped() {
    assert_eq!(parse(&[b"\x1b[ 1qx"]), vec![print("x")]);
}

#[test]
fn c0_controls_inside_csi_are_executed_without_ending_it() {
    assert_eq!(
        parse(&[b"\x1b[1\n;2H"]),
        vec![Action::Execute(b'\n'), csi(&[&[1], &[2]], b"", b'H')]
    );
}

#[test]
fn cancel_aborts_a_sequence() {
    assert_eq!(
        parse(&[b"\x1b[1;2\x18m"]),
        vec![Action::Execute(0x18), print("m")]
    );
}

#[test]
fn escape_restarts_a_sequence() {
    assert_eq!(parse(&[b"\x1b[1;2\x1b[3m"]), vec![csi(&[&[3]], b"", b'm')]);
}

#[test]
fn esc_dispatches_final_bytes_with_intermediates() {
    assert_eq!(
        parse(&[b"\x1b7\x1b(0\x1b#8"]),
        vec![
            Action::Esc {
                intermediates: Vec::new(),
                ignore: false,
                byte: b'7'
            },
            Action::Esc {
                intermediates: b"(".to_vec(),
                ignore: false,
                byte: b'0'
            },
            Action::Esc {
                intermediates: b"#".to_vec(),
                ignore: false,
                byte: b'8'
            }
        ]
    );
}

#[test]
fn osc_terminated_by_bell_splits_parameters() {
    assert_eq!(
        parse(&[b"\x1b]0;title\x07"]),
        vec![Action::Osc {
            params: vec![b"0".to_vec(), b"title".to_vec()],
            bell: true
        }]
    );
}

#[test]
fn osc_terminated_by_st_dispatches_before_the_terminator() {
    assert_eq!(
        parse(&[b"\x1b]11;?\x1b\\"]),
        vec![
            Action::Osc {
                params: vec![b"11".to_vec(), b"?".to_vec()],
                bell: false
            },
            Action::Esc {
                intermediates: Vec::new(),
                ignore: false,
                byte: b'\\'
            }
        ]
    );
}

#[test]
fn osc_keeps_utf8_payload_bytes() {
    assert_eq!(
        parse(&["\x1b]2;\u{65E5}\u{672C}\x07".as_bytes()]),
        vec![Action::Osc {
            params: vec![b"2".to_vec(), "\u{65E5}\u{672C}".as_bytes().to_vec()],
            bell: true
        }]
    );
}

#[test]
fn osc_is_discarded_by_cancel() {
    assert_eq!(
        parse(&[b"\x1b]0;title\x18x"]),
        vec![Action::Execute(0x18), print("x")]
    );
}

#[test]
fn osc_ignores_other_c0_controls_in_the_payload() {
    assert_eq!(
        parse(&[b"\x1b]0;a\nb\x07"]),
        vec![Action::Osc {
            params: vec![b"0".to_vec(), b"ab".to_vec()],
            bell: true
        }]
    );
}

#[test]
fn osc_keeps_extra_separators_in_the_last_parameter() {
    let sequence = format!("\x1b]{}\x07", vec!["p"; 20].join(";"));
    let Some(Action::Osc { params, .. }) = parse(&[sequence.as_bytes()]).pop() else {
        panic!("expected an OSC dispatch");
    };
    assert_eq!(params.len(), 16);
    assert_eq!(params[15], b"p;p;p;p;p".to_vec());
}

#[test]
fn dcs_hooks_streams_and_unhooks() {
    assert_eq!(
        parse(&[b"\x1bP1$qm\x1b\\"]),
        vec![
            Action::Hook {
                params: vec![vec![1]],
                intermediates: b"$".to_vec(),
                ignore: false,
                action: b'q'
            },
            Action::Put(b'm'),
            Action::Unhook,
            Action::Esc {
                intermediates: Vec::new(),
                ignore: false,
                byte: b'\\'
            }
        ]
    );
}

#[test]
fn dcs_with_a_colon_parameter_is_ignored_until_st() {
    assert_eq!(
        parse(&[b"\x1bP1:2qdata\x1b\\x"]),
        vec![
            Action::Esc {
                intermediates: Vec::new(),
                ignore: false,
                byte: b'\\'
            },
            print("x")
        ]
    );
}

#[test]
fn sos_pm_and_apc_strings_are_skipped() {
    assert_eq!(
        parse(&[b"\x1b_payload\x1b\\\x1b^pm\x1b\\\x1bXsos\x1b\\x"]),
        vec![
            Action::Esc {
                intermediates: Vec::new(),
                ignore: false,
                byte: b'\\'
            },
            Action::Esc {
                intermediates: Vec::new(),
                ignore: false,
                byte: b'\\'
            },
            Action::Esc {
                intermediates: Vec::new(),
                ignore: false,
                byte: b'\\'
            },
            print("x")
        ]
    );
}

#[test]
fn multibyte_text_is_decoded() {
    assert_eq!(
        parse(&["a\u{3042}\u{1F600}b".as_bytes()]),
        vec![print("a\u{3042}\u{1F600}b")]
    );
}

#[test]
fn multibyte_text_split_across_chunks_is_decoded() {
    let bytes = "\u{3042}\u{1F600}".as_bytes();
    for split in 0..=bytes.len() {
        assert_eq!(
            parse(&[&bytes[..split], &bytes[split..]]),
            vec![print("\u{3042}\u{1F600}")],
            "split {split}"
        );
    }
}

#[test]
fn invalid_utf8_becomes_replacement_characters() {
    assert_eq!(
        parse(&[b"a\xFFb\xE3\x81c"]),
        vec![print("a\u{FFFD}b\u{FFFD}c")]
    );
}

#[test]
fn truncated_utf8_before_escape_is_replaced_and_the_escape_survives() {
    assert_eq!(
        parse(&[b"\xE3", b"\x1b[m"]),
        vec![print("\u{FFFD}"), csi(&[], b"", b'm')]
    );
}

#[test]
fn every_split_position_yields_the_same_actions() {
    let stream = "ls\r\n\x1b[1;38:2::1:2:3mfile\x1b[0m \u{65E5}\u{672C}\u{8A9E}\x1b]0;t;i\x07\x1b(0qq\x1b(B\x1bP+q\x1b\\\x1b[?1049h\tend"
        .as_bytes();
    let whole = parse(&[stream]);
    for split in 0..=stream.len() {
        assert_eq!(
            parse(&[&stream[..split], &stream[split..]]),
            whole,
            "split {split}"
        );
    }
}

#[test]
fn byte_at_a_time_input_yields_the_same_actions() {
    let stream = "\x1b[31m\u{1F600}\x1b]2;x\x1b\\ok\r\n".as_bytes();
    let chunks: Vec<&[u8]> = stream.chunks(1).collect();
    assert_eq!(parse(&chunks), parse(&[stream]));
}
