use std::env;
use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;

use dartel_term::Term;
use dartel_vt::Parser;

const CHUNK: usize = 64 << 10;
const HISTORY: usize = 10_000;
const WORDS: [&str; 8] = [
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta",
];

struct Setup {
    size: usize,
    lines: usize,
    cols: usize,
    piece: usize,
    runs: usize,
    only: String,
}

struct Load {
    name: &'static str,
    prelude: String,
    chunk: String,
}

fn setup() -> Setup {
    let mut setup = Setup {
        size: 64,
        lines: 24,
        cols: 80,
        piece: CHUNK,
        runs: 7,
        only: String::new(),
    };
    for argument in env::args().skip(1) {
        let Some((key, value)) = argument.split_once('=') else {
            continue;
        };
        match (key, value.parse::<usize>()) {
            ("size", Ok(number)) => setup.size = number.max(1),
            ("lines", Ok(number)) => setup.lines = number.max(3),
            ("cols", Ok(number)) => setup.cols = number.max(2),
            ("piece", Ok(number)) => setup.piece = number.max(1),
            ("runs", Ok(number)) => setup.runs = number.max(1),
            ("only", _) => value.clone_into(&mut setup.only),
            _ => {}
        }
    }
    setup
}

fn fill(line: impl Fn(usize, &mut String)) -> String {
    let mut out = String::new();
    let mut index = 0;
    while out.len() < CHUNK {
        line(index, &mut out);
        index += 1;
    }
    out
}

fn plain(name: &'static str, line: impl Fn(usize, &mut String)) -> Load {
    Load {
        name,
        prelude: String::new(),
        chunk: fill(line),
    }
}

fn loads(lines: usize, cols: usize) -> Vec<Load> {
    vec![
        plain("seq", |index, out| {
            let _ = write!(out, "{}\r\n", 100_000 + index);
        }),
        plain("ascii", |index, out| {
            let _ = write!(
                out,
                "{index:08} the quick brown fox jumps over the lazy dog, again and again\r\n"
            );
        }),
        plain("dense", |index, out| {
            out.extend((0..97).map(|offset| char::from(b' ' + ((index * 7 + offset) % 95) as u8)));
        }),
        plain("sgr16", |index, out| {
            for (slot, word) in WORDS.iter().enumerate() {
                let _ = write!(
                    out,
                    "\x1b[{};{}m{word}\x1b[0m ",
                    slot % 2,
                    31 + (index + slot) % 7
                );
            }
            out.push_str("\r\n");
        }),
        plain("sgr256", |index, out| {
            for (slot, word) in WORDS.iter().enumerate() {
                let _ = write!(out, "\x1b[38;5;{}m{word} ", (index + slot) % 256);
            }
            out.push_str("\x1b[0m\r\n");
        }),
        plain("truecolor", |index, out| {
            for (slot, word) in WORDS.iter().enumerate() {
                let _ = write!(
                    out,
                    "\x1b[38;2;{};{};{}m{word} ",
                    index % 256,
                    slot * 30,
                    (index * 7) % 256
                );
            }
            out.push_str("\x1b[m\r\n");
        }),
        plain("cjk", |_, out| {
            out.push_str(
                "\u{65E5}\u{672C}\u{8A9E}\u{306E}\u{30C6}\u{30AD}\u{30B9}\u{30C8}\u{3092}\u{8868}\u{793A}\u{3059}\u{308B}\u{884C}\u{3067}\u{3059}\r\n",
            );
        }),
        plain("mixed", |index, out| {
            let _ = write!(
                out,
                "{index:06} warning: \u{5909}\u{6570} `{}` \u{304C}\u{4F7F}\u{308F}\u{308C}\u{3066}\u{3044}\u{307E}\u{305B}\u{3093} src/main.rs:{}:{}\r\n",
                WORDS[index % 8],
                index % 900 + 1,
                index % 70 + 1
            );
        }),
        plain("cursor", |index, out| {
            let _ = write!(
                out,
                "\x1b[{};{}Hx\x1b[K",
                index % lines + 1,
                index % cols + 1
            );
        }),
        Load {
            name: "redraw",
            prelude: "\x1b[?1049h".into(),
            chunk: fill(|index, out| {
                let _ = write!(
                    out,
                    "\x1b[{};1H\x1b[4{}m{:>4} \x1b[0m ",
                    index % lines + 1,
                    index / lines % 8,
                    index % 10_000
                );
                out.extend(
                    (0..cols.saturating_sub(10))
                        .map(|offset| char::from(b'a' + ((index + offset) % 26) as u8)),
                );
                out.push_str("\x1b[K");
            }),
        },
        Load {
            name: "region",
            prelude: format!("\x1b[2;{}r\x1b[{};1H", lines - 1, lines - 1),
            chunk: fill(|index, out| {
                let _ = write!(
                    out,
                    "{index:08} the quick brown fox jumps over the lazy dog\r\n"
                );
            }),
        },
    ]
}

fn run(load: &Load, setup: &Setup) -> f64 {
    let mut term = Term::new(setup.lines, setup.cols, HISTORY);
    let mut parser = Parser::new();
    parser.advance(&mut term, load.prelude.as_bytes());
    let rounds = ((setup.size << 20) / load.chunk.len()).max(1);
    let start = Instant::now();
    for _ in 0..rounds {
        for piece in load.chunk.as_bytes().chunks(setup.piece) {
            parser.advance(&mut term, piece);
        }
    }
    let seconds = start.elapsed().as_secs_f64();
    black_box(&term);
    (rounds * load.chunk.len()) as f64 / f64::from(1 << 20) / seconds
}

fn main() {
    let setup = setup();
    println!(
        "{} MiB of each input, {} lines, {} columns, pieces of {} bytes, {} runs, MiB/s",
        setup.size, setup.lines, setup.cols, setup.piece, setup.runs
    );
    for load in loads(setup.lines, setup.cols)
        .iter()
        .filter(|load| load.name.contains(setup.only.as_str()))
    {
        let mut rates: Vec<f64> = (0..setup.runs).map(|_| run(load, &setup)).collect();
        rates.sort_by(f64::total_cmp);
        println!(
            "{:<10} best {:8.1}  median {:8.1}",
            load.name,
            rates[rates.len() - 1],
            rates[rates.len() / 2]
        );
    }
}
