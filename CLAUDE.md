# CLAUDE.md - AI Assistant Guide for Dartel

## Project Overview

Dartel is a terminal emulator written in Rust. Apple Silicon macOS is the
first target and x86_64 Linux is the second. Windows is out of scope.
Throughput is the main design goal, and the parser, the grid, and the renderer
are written for it.

`docs/architecture.md` explains the threads, the data structures, and the
rendering path with diagrams. Read it before changing how data flows, and
update it in the same change.

## Codebase Structure

```tree
Dartel/
├── Cargo.toml                # Workspace manifest, pinned dependencies, lint table
├── rustfmt.toml              # Formatting options that stable rustfmt accepts
├── crates/
│   ├── dartel-core/          # Library crate with no window or GPU dependency
│   │   ├── src/
│   │   │   ├── lib.rs        # Module exports
│   │   │   ├── pty.rs        # PTY allocation, login shell spawn, resize
│   │   │   ├── scan/         # Length of the leading printable ASCII run
│   │   │   │   ├── mod.rs    # Picks the implementation by target feature
│   │   │   │   ├── neon.rs   # aarch64
│   │   │   │   ├── scalar.rs # Fallback and reference behavior
│   │   │   │   └── sse2.rs   # x86_64
│   │   │   ├── search/
│   │   │   │   └── mod.rs    # Literal and regex queries, parallel scan
│   │   │   ├── term/
│   │   │   │   ├── mod.rs     # Term, cursor view, damage, public accessors
│   │   │   │   ├── cell.rs    # 8-byte cell
│   │   │   │   ├── charset.rs # DEC special graphics
│   │   │   │   ├── color.rs   # Rgb, Color, Palette, xterm 256-color table
│   │   │   │   ├── find.rs    # Scrollback search and text extraction
│   │   │   │   ├── grid.rs    # Rows, scrollback, scrolling, resize, reflow
│   │   │   │   ├── modes.rs   # Terminal mode flags
│   │   │   │   ├── osc.rs     # OSC title, palette, and clipboard sequences
│   │   │   │   ├── perform.rs # vt::Perform implementation, CSI and ESC dispatch
│   │   │   │   ├── row.rs     # Row edits, wide characters, combining marks
│   │   │   │   ├── screen.rs  # Writing, cursor movement, erase, alternate screen
│   │   │   │   ├── sgr.rs     # SGR attributes and colors
│   │   │   │   └── style.rs   # Style interning
│   │   │   └── vt/
│   │   │       ├── mod.rs    # Parser state machine and the Perform trait
│   │   │       ├── params.rs # CSI parameters and subparameters
│   │   │       └── utf8.rs   # UTF-8 decoder
│   │   └── tests/            # Integration tests, one file per module
│   │       ├── pty_test.rs
│   │       ├── scan_test.rs
│   │       ├── search_test.rs
│   │       ├── term_test.rs
│   │       └── vt_test.rs
│   └── dartel/               # Binary crate
│       └── src/
│           ├── main.rs       # Event loop start
│           ├── app.rs        # winit handler, selection, search prompt, shortcuts
│           ├── clipboard.rs  # Clipboard through the platform commands
│           ├── font.rs       # Font discovery, fallback, rasterization
│           ├── gpu.rs        # wgpu surface, glyph atlas, draw call
│           ├── input.rs      # Key event to byte sequence encoding
│           ├── scene.rs      # Rows to quads, glyph cache, overlays
│           ├── session.rs    # Reader and writer threads around the PTY
│           └── shader.wgsl   # Vertex and fragment shader
├── docs/
│   └── architecture.md       # Design notes with diagrams
└── .github/
    ├── dependabot.yml        # Weekly cargo and GitHub Actions updates
    ├── ISSUE_TEMPLATE/       # Bug report and feature request forms
    ├── PULL_REQUEST_TEMPLATE.md
    └── workflows/
        ├── codeql.yml        # CodeQL for Rust and workflow files
        └── static-check.yml  # Build, test, format check, clippy
```

## Development Commands

Build and test the workspace:

```sh
cargo build
cargo test
```

Format and lint. Clippy must pass with no warnings:

```sh
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
```

Run the terminal, or build the optimized binary:

```sh
cargo run -p dartel
cargo build --release -p dartel
```

## Toolchain

CI installs the newest stable toolchain on every run. A new stable release can
add clippy lints that fail the lint job, so update the local toolchain before
linting:

```sh
rustup update stable
```

Clippy can check the macOS target from Linux:

```sh
rustup target add aarch64-apple-darwin
cargo clippy --workspace --all-targets --target aarch64-apple-darwin -- -D warnings
```

Build and test for that target run on the macOS CI job.

## Code Conventions

- Source files carry no comments and no doc comments. Add one only when the
  maintainer asks for it.
- Do not suppress lints with `#[allow]` or `#[expect]`. Restructure the code
  until clippy pedantic passes. The workspace allow list lives in `Cargo.toml`.
- Pin every dependency to an exact version in `[workspace.dependencies]` and
  reference it from the crates with `workspace = true`.
- Pin every GitHub Action to a commit hash and write the release tag in a
  trailing comment, for example `actions/checkout@<hash> # v7.0.1`.
- Express platform differences with `cfg`, and keep the macOS and Linux
  variants of a constant or function next to each other.
- `unsafe` appears in five files: `scan/neon.rs`, `scan/sse2.rs`, `pty.rs`,
  `gpu.rs`, and `font.rs`. Keep new code safe.
- Code, tests, commit messages, and documentation are written in English.
- Commit messages follow Conventional Commits and are a single title line.

## Invariants

- `Cell` is 8 bytes. Do not add a field without measuring throughput.
- A wide character is a wide cell followed by a spacer cell. Row edits go
  through `Row::release`, which blanks the other half of a split wide
  character and drops combining marks in the edited range.
- Every change to visible content marks damage through `Term::touch` or
  `damage.full`. `Scene` rebuilds only damaged lines, so a missed mark leaves
  stale pixels.
- A `StyleId` is an index into `StyleTable`, and ids change when the table is
  compacted. Nothing outside `Term` keeps a `StyleId` between frames.
- The reader thread parses into `Term` while it holds the mutex. Keep the work
  under that lock short and never block on I/O there.
- The NEON and SSE2 implementations of `scan::printable_run` return the same
  value as the scalar one for every input.

## Testing Conventions

- Integration tests live in `crates/dartel-core/tests/` as `{module}_test.rs`.
  Unit tests inside a module are reserved for private helpers.
- Each test function checks one behavior and is named as a sentence, for
  example `csi_collects_semicolon_separated_parameters`.
- Non-ASCII test data uses `\u{...}` escapes.
- Terminal tests feed bytes through `Parser::advance` and assert on the grid,
  the cursor, or the reply bytes.
- The `dartel` crate has no automated tests. Check it on a display.

## Checking the Window on Linux Without a Display

These commands exercised the window in a headless Linux container. They
install a software Vulkan driver and X11 tools, start a virtual display, and
drive the window with `xdotool`. `Xvfb` and ImageMagick, which provides
`import`, were already installed there.

```sh
apt-get install -y -qq libegl1 libegl-mesa0 mesa-vulkan-drivers libxkbcommon-x11-0 libgles2 vulkan-tools
apt-get install -y -qq xdotool x11-utils xclip
Xvfb :99 -screen 0 1280x800x24 &
export DISPLAY=:99
cargo build -p dartel
./target/debug/dartel &
xdotool search --name Dartel
xdotool type --delay 20 'echo hello dartel'
xdotool key Return
import -window root shot.png
```

`xdotool key ctrl+shift+f` opens the search prompt, `ctrl+shift+c` copies the
selection, and `ctrl+shift+v` pastes. Running on macOS itself, with Metal, the
system fonts, `pbcopy`, and the IME, cannot be checked this way.

## CI/CD Pipeline

- `static-check.yml` runs on pull requests and pushes to `main`. The `test`
  job builds and tests the workspace. The `lint` job runs
  `cargo fmt --all -- --check` and clippy. Both jobs run for
  `aarch64-apple-darwin` on `macos-latest` and for `x86_64-unknown-linux-gnu`
  on `ubuntu-latest`.
- `codeql.yml` runs CodeQL on the Rust sources and the workflow files.
- `dependabot.yml` opens weekly grouped updates for cargo and GitHub Actions.

## Dependencies

| Crate | Purpose |
| ----- | ------- |
| `fontdb` | System font discovery and fallback lookup |
| `memchr` | Byte and substring search |
| `regex` | Regex search over scrollback |
| `rustix` | PTY, termios, and process calls |
| `swash` | Glyph rasterization |
| `umt_rust` | Hex color parsing and base64 decoding for OSC sequences |
| `unicode-width` | Character cell width |
| `wgpu` | GPU rendering |
| `winit` | Window and input events |

## Notes for AI Assistants

- macOS is the first priority. A change keeps both CI targets passing.
- Measure throughput before and after touching the hot paths, which are
  `Parser::ground`, `Term::write_ascii`, `Row::write_ascii`, and
  `Scene::compose_row`. Timing `seq 1 1000000` inside a release build of the
  window is a quick check.
- The search foundation of happy-cracking is `pub(crate)` and cannot be a
  dependency. Its designs are ported instead, and `docs/architecture.md` lists
  them.
- `umt_rust` already provides hex color parsing and base64 decoding. Prefer it
  over a new dependency when it has the function.
- A new dependency needs the maintainer's approval, an exact version pin, and
  a stated purpose.
- Keep this file and `docs/architecture.md` current when the structure
  changes.
