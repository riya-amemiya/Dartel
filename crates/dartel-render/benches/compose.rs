use std::env;
use std::fmt::Write as _;
use std::hint::black_box;
use std::time::{Duration, Instant};

use dartel_render::{Overlay, Scene};
use dartel_term::Term;
use dartel_vt::Parser;

const HISTORY: usize = 10_000;
const FONT_SIZE: f32 = 13.0;
const PADDING: i32 = 4;
const CASES: [&str; 3] = ["full", "scroll", "idle"];
const WORDS: [&str; 8] = [
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta",
];

struct Setup {
    lines: usize,
    cols: usize,
    frames: usize,
    runs: usize,
    only: String,
}

struct Load {
    name: &'static str,
    line: fn(usize, usize, &mut String),
}

fn setup() -> Setup {
    let mut setup = Setup {
        lines: 24,
        cols: 80,
        frames: 1000,
        runs: 7,
        only: String::new(),
    };
    for argument in env::args().skip(1) {
        let Some((key, value)) = argument.split_once('=') else {
            continue;
        };
        match (key, value.parse::<usize>()) {
            ("lines", Ok(number)) => setup.lines = number.max(3),
            ("cols", Ok(number)) => setup.cols = number.max(2),
            ("frames", Ok(number)) => setup.frames = number.max(1),
            ("runs", Ok(number)) => setup.runs = number.max(1),
            ("only", _) => value.clone_into(&mut setup.only),
            _ => {}
        }
    }
    setup
}

fn loads() -> [Load; 6] {
    [
        Load {
            name: "seq",
            line: |index, _, out| {
                let _ = write!(out, "{}", 100_000 + index);
            },
        },
        Load {
            name: "ascii",
            line: |index, _, out| {
                let _ = write!(
                    out,
                    "{index:08} the quick brown fox jumps over the lazy dog, again and again"
                );
            },
        },
        Load {
            name: "dense",
            line: |index, cols, out| {
                out.extend(
                    (0..cols).map(|offset| char::from(b'!' + ((index * 7 + offset) % 94) as u8)),
                );
            },
        },
        Load {
            name: "sgr",
            line: |index, _, out| {
                for (slot, word) in WORDS.iter().enumerate() {
                    let _ = write!(
                        out,
                        "\x1b[{};{}m{word}\x1b[0m ",
                        slot % 2,
                        31 + (index + slot) % 7
                    );
                }
            },
        },
        Load {
            name: "paint",
            line: |index, _, out| {
                let _ = write!(out, "\x1b[4{}m{index:>6} \x1b[K\x1b[0m", index % 8);
            },
        },
        Load {
            name: "cjk",
            line: |_, _, out| {
                out.push_str(
                    "\u{65E5}\u{672C}\u{8A9E}\u{306E}\u{30C6}\u{30AD}\u{30B9}\u{30C8}\u{3092}\u{8868}\u{793A}\u{3059}\u{308B}\u{884C}\u{3067}\u{3059}",
                );
            },
        },
    ]
}

fn feed(parser: &mut Parser, term: &mut Term, load: &Load, index: usize, text: &mut String) {
    text.clear();
    text.push_str("\r\n");
    (load.line)(index, term.cols(), text);
    parser.advance(term, text.as_bytes());
}

fn frame(scene: &mut Scene, term: &mut Term, width: u32) -> Duration {
    let start = Instant::now();
    scene.compose(
        term,
        &Overlay {
            focused: true,
            caret: true,
            preedit: "",
            selection: None,
            matches: &[],
            current: None,
            banner: None,
            tabs: &[],
            active: 0,
            width,
        },
    );
    let elapsed = start.elapsed();
    black_box(scene.quads());
    term.reset_damage();
    elapsed
}

fn measure(scene: &mut Scene, load: &Load, setup: &Setup, width: u32) -> [f64; 3] {
    let mut term = Term::new(setup.lines, setup.cols, HISTORY);
    let mut parser = Parser::new();
    let mut text = String::new();
    for index in 0..setup.lines {
        feed(&mut parser, &mut term, load, index, &mut text);
    }
    scene.invalidate();
    frame(scene, &mut term, width);
    let full: Duration = (0..setup.frames)
        .map(|_| {
            scene.invalidate();
            frame(scene, &mut term, width)
        })
        .sum();
    let scroll: Duration = (0..setup.frames)
        .map(|index| {
            feed(&mut parser, &mut term, load, setup.lines + index, &mut text);
            frame(scene, &mut term, width)
        })
        .sum();
    let idle: Duration = (0..setup.frames)
        .map(|_| frame(scene, &mut term, width))
        .sum();
    [full, scroll, idle].map(|total| total.as_secs_f64() * 1e6 / setup.frames as f64)
}

fn main() {
    let setup = setup();
    let Some(mut scene) = Scene::new("", &[], FONT_SIZE, PADDING) else {
        eprintln!("no monospace font is installed");
        return;
    };
    let width = setup.cols as u32 * scene.metrics().width + 2 * PADDING as u32;
    println!(
        "{} lines, {} columns, {} frames, {} runs, microseconds for one frame",
        setup.lines, setup.cols, setup.frames, setup.runs
    );
    for load in loads()
        .iter()
        .filter(|load| load.name.contains(setup.only.as_str()))
    {
        let mut times: Vec<[f64; 3]> = (0..setup.runs)
            .map(|_| measure(&mut scene, load, &setup, width))
            .collect();
        for (slot, case) in CASES.into_iter().enumerate() {
            times.sort_by(|a, b| a[slot].total_cmp(&b[slot]));
            println!(
                "{:<8} {case:<6} best {:9.2}  median {:9.2}",
                load.name,
                times[0][slot],
                times[times.len() / 2][slot]
            );
        }
    }
}
