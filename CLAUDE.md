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
├── .mergify.yml              # Merge protection and merge queue rules
├── crates/
│   ├── dartel-vt/            # VT parser, depends on memchr
│   │   ├── src/
│   │   │   ├── lib.rs        # Parser state machine and the Perform trait
│   │   │   ├── params.rs     # CSI parameters and subparameters
│   │   │   ├── utf8.rs       # UTF-8 decoder
│   │   │   └── scan/         # Length of the leading printable ASCII run
│   │   │       ├── mod.rs    # Picks the implementation by target feature
│   │   │       ├── neon.rs   # aarch64
│   │   │       ├── scalar.rs # Fallback and reference behavior
│   │   │       └── sse2.rs   # x86_64
│   │   └── tests/
│   │       ├── scan_test.rs
│   │       └── vt_test.rs
│   ├── dartel-search/        # Search over a byte buffer
│   │   ├── src/
│   │   │   └── lib.rs        # Literal and regex queries, parallel scan, URL detection
│   │   └── tests/
│   │       └── search_test.rs
│   ├── dartel-image/         # Image decoders, no dependency
│   │   ├── src/
│   │   │   ├── lib.rs        # Image and its constructors
│   │   │   ├── base64.rs     # Base64 decoder
│   │   │   ├── inflate.rs    # Deflate and zlib decoder
│   │   │   ├── png.rs        # PNG decoder
│   │   │   ├── scale.rs      # Crop, scale, and premultiply
│   │   │   └── sixel.rs      # Sixel decoder
│   │   └── tests/
│   │       ├── base64_test.rs
│   │       ├── image_test.rs
│   │       ├── inflate_test.rs
│   │       ├── png_test.rs
│   │       ├── scale_test.rs
│   │       └── sixel_test.rs
│   ├── dartel-term/          # Terminal state
│   │   ├── benches/
│   │   │   └── throughput.rs # Parser and grid throughput over generated input
│   │   ├── src/
│   │   │   ├── lib.rs        # Term, cursor view, damage, public accessors
│   │   │   ├── cell.rs       # 8-byte cell
│   │   │   ├── charset.rs    # DEC special graphics
│   │   │   ├── color.rs      # Rgb, Color, Palette, xterm 256-color table
│   │   │   ├── find.rs       # Scrollback search, text extraction, words, link targets, prompt rows
│   │   │   ├── graphics.rs   # Kitty graphics commands, Sixel placement, pictures and tiles
│   │   │   ├── grid.rs       # Rows, scrollback, scrolling, resize, reflow
│   │   │   ├── keys.rs       # Kitty keyboard protocol encoder
│   │   │   ├── modes.rs      # Terminal mode flags
│   │   │   ├── osc.rs        # OSC title, palette, clipboard, directory, link, and shell mark sequences
│   │   │   ├── perform.rs    # Perform implementation, CSI, ESC, DCS, and APC dispatch
│   │   │   ├── row.rs        # Row edits, wide characters, combining marks, shell marks, image tiles
│   │   │   ├── screen.rs     # Writing, cursor movement, erase, alternate screen
│   │   │   ├── sgr.rs        # SGR attributes and colors
│   │   │   └── style.rs      # Style interning
│   │   └── tests/
│   │       ├── find_test.rs  # Term::search, words, link targets, prompt rows
│   │       ├── graphics_test.rs
│   │       ├── keys_test.rs
│   │       └── term_test.rs
│   ├── dartel-pty/           # Unix only
│   │   ├── src/
│   │   │   └── lib.rs        # PTY allocation, login shell spawn with DARTEL_BIN, resize, hangup, host name
│   │   └── tests/
│   │       └── pty_test.rs
│   ├── dartel-config/        # Settings
│   │   ├── src/
│   │   │   ├── lib.rs        # Config, TOML and state codecs, locations, save, load
│   │   │   └── toml.rs       # TOML parser
│   │   └── tests/
│   │       ├── config_test.rs
│   │       └── toml_test.rs
│   ├── dartel-render/        # GPU renderer
│   │   └── src/
│   │       ├── lib.rs        # Exports
│   │       ├── font.rs       # Font discovery, fallback, rasterization
│   │       ├── gpu.rs        # wgpu surface, glyph atlas, picture textures, draw calls
│   │       ├── scene.rs      # Rows to quads, glyph cache, pictures, overlays, tab bar
│   │       └── shader.wgsl   # Vertex and fragment shader
│   └── dartel/               # Binary crate
│       ├── macos/
│       │   ├── Info.plist    # Bundle metadata for Dartel.app
│       │   ├── icon.png      # 1024 px rendering of icon.svg, source of Dartel.icns
│       │   └── icon.svg      # Application icon drawing
│       ├── shell/            # Shell integration scripts embedded in the binary
│       │   ├── dartel.bash   # PROMPT_COMMAND and PS0 hooks
│       │   ├── dartel.fish   # fish_prompt and fish_preexec handlers
│       │   └── dartel.zsh    # precmd and preexec hooks
│       └── src/
│           ├── main.rs       # config and shell subcommands, event loop start
│           ├── app.rs        # winit handler, tabs, selection, prompts, shortcuts
│           ├── clipboard.rs  # Clipboard through the platform commands
│           ├── input.rs      # Key event to byte sequence encoding
│           ├── opener.rs     # Links and paths through the platform opener
│           └── session.rs    # Reader and writer threads around the PTY
├── docs/
│   └── architecture.md       # Design notes with diagrams
└── .github/
    ├── dependabot.yml        # Weekly cargo and GitHub Actions updates
    ├── ISSUE_TEMPLATE/       # Bug report and feature request forms
    ├── PULL_REQUEST_TEMPLATE.md
    └── workflows/
        ├── codeql.yml        # CodeQL for Rust and workflow files
        ├── release.yml       # Release notes and packaged builds on a version tag
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

Measure the throughput of the parser and the grid. The benchmark prints the
best and the median rate of each input in MiB/s. Arguments are optional and
take the form `key=value`: `size` is the MiB of each input, `lines` and `cols`
are the grid, `piece` is the bytes of one `Parser::advance` call, `runs` is the
number of repetitions, and `only` keeps the inputs whose name contains it:

```sh
cargo bench -p dartel-term --bench throughput
cargo bench -p dartel-term --bench throughput -- size=16 runs=3 only=sgr
```

Write the settings in use to a TOML file, or load a TOML file and keep it for
later starts:

```sh
cargo run -p dartel -- config save [FILE]
cargo run -p dartel -- config load [FILE]
```

Print the shell integration script for zsh, bash, or fish. A shell that
Dartel starts has `DARTEL_BIN` set to the Dartel executable, and the line in
its startup file runs this subcommand through that variable:

```sh
cargo run -p dartel -- shell zsh
cargo run -p dartel -- shell bash
cargo run -p dartel -- shell fish
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
- `unsafe` appears in five files: `scan/neon.rs` and `scan/sse2.rs` in
  `dartel-vt`, `lib.rs` in `dartel-pty`, and `gpu.rs` and `font.rs` in
  `dartel-render`. Keep new code safe.
- A library crate has no dependency on the window. `dartel-render` is the only
  library crate that depends on `wgpu` and `winit`.
- Code, tests, commit messages, and documentation are written in English.
- Commit messages follow Conventional Commits and are a single title line.

## Invariants

- `Cell` is 8 bytes, one `u64` whose zero value is the default cell. Do not
  add a field without measuring throughput.
- `Row::used` is an upper bound on the leading cells that differ from the
  default cell, and `Row::wide` is set once a wide character was written.
  Every method of `Row` that writes cells keeps both, because clearing a row
  and the check for a split wide character rely on them.
- `Row` is 40 bytes. Combining marks and image tiles live in `Extras` behind
  `Row::extras`, which is `None` unless the row holds one of them. A method
  that can empty both calls `Row::prune`, because `Row::release` takes the
  slow path for every row whose `extras` is set. Do not add a field without
  measuring throughput.
- `Grid` is a ring. `Grid::slot` maps a row index to its place in the `Vec`,
  and a method that changes the number of rows calls `Grid::settle` first.
- A change to `pen.style` is followed by `Term::restyle`, which makes the next
  character intern the style again.
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
- Every tab is resized with the window, and `Scene::invalidate` runs when the
  active tab changes, because `Scene` caches the rows of one `Term`.
- An image is one `Picture` cut into `Tile`s that live in the rows it covers.
  `Row::reset` drops every tile, `Row::clear` keeps the tiles of the kitty
  protocol, and `Row::release` cuts the written cells out of Sixel tiles. A new
  way to blank a row picks one of the three.
- A `Picture` holds premultiplied RGBA. Its serial number orders the pictures
  of one `Term` and repeats across tabs, so `Gpu` keys textures by the address
  of the `Arc<Picture>`.
- The counts in `Scene::layers` add up to the length of `Scene::quads`. The
  texture rectangle of a `Quad` has a size, and a size of zero means a solid
  quad.
- The TOML file is parsed only by `dartel_config::load`. Start-up reads the
  state file through `Config::decode`. A change to the fields of `Config`
  changes the layout of the state file, so change the last byte of `MAGIC`
  with it.

## Testing Conventions

- Integration tests live in the `tests/` directory of a library crate as
  `{module}_test.rs`. Unit tests inside a module are reserved for private
  helpers.
- Each test function checks one behavior and is named as a sentence, for
  example `csi_collects_semicolon_separated_parameters`.
- Non-ASCII test data uses `\u{...}` escapes.
- Terminal tests feed bytes through `Parser::advance` and assert on the grid,
  the cursor, or the reply bytes.
- Decoder tests in `dartel-image` read bytes that ImageMagick and gzip wrote,
  and build the error cases from parts in the test file.
- A test that depends on an environment variable calls a function that takes
  the value as an argument, such as `start_directory` and
  `Locations::resolve`, because `std::env::set_var` is unsafe.
- The `dartel-render` and `dartel` crates have no integration tests. Check
  them on a display. `scene.rs` unit-tests the private helpers that build the
  underline masks.

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

Without a window manager the window has no focus, so run
`xdotool windowfocus` with the id that `xdotool search` printed before typing.
`xdotool key ctrl+shift+f` opens the search prompt, `ctrl+shift+c` copies the
selection, `ctrl+shift+v` pastes, `ctrl+shift+t` opens a tab, and
`ctrl+shift+s` and `ctrl+shift+l` open the settings prompts.
`ctrl+shift+w` closes the tab, `ctrl+shift+1` selects the first tab,
`ctrl+shift+equal`, `ctrl+shift+minus`, and `ctrl+shift+0` change the font
size, and `ctrl+shift+o` copies the last output. Set `HOME` to a
scratch directory and unset `XDG_CONFIG_HOME` and `XDG_STATE_HOME` for the
run, so the settings files stay out of the real home directory. Running on
macOS itself, with Metal, the system fonts, `pbcopy`, and the IME, cannot be
checked this way.

Images were checked in that window with these programs:

```sh
apt-get install -y -qq chafa libsixel-bin
img2sixel image.png
chafa --format sixels image.png
chafa --format kitty image.png
```

## Application Icon

`crates/dartel/macos/icon.svg` is the drawing. `icon.png` is its 1024 px
rendering, and the release workflow builds `Dartel.icns` from it. After
editing the drawing, render it again with headless Chromium so the corners
stay transparent:

```sh
chromium --headless --no-sandbox --hide-scrollbars --default-background-color=00000000 --window-size=1024,1024 --screenshot=crates/dartel/macos/icon.png "file://$PWD/crates/dartel/macos/icon.svg"
```

## CI/CD Pipeline

- `static-check.yml` runs on pull requests and pushes to `main`. The `test`
  job builds and tests the workspace. The `lint` job runs
  `cargo fmt --all -- --check` and clippy. Both jobs run for
  `aarch64-apple-darwin` on `macos-latest` and for `x86_64-unknown-linux-gnu`
  on `ubuntu-latest`.
- `codeql.yml` runs CodeQL on the Rust sources and the workflow files.
- `release.yml` runs when a tag shaped like `1.2.3` is pushed. It packages
  `Dartel.app` for `aarch64-apple-darwin` and the `dartel` binary for
  `x86_64-unknown-linux-gnu`, then creates a GitHub Release with generated
  notes and both packages attached. A pull request that touches the workflow
  or `crates/dartel/macos/` runs the packaging jobs without creating a
  release.
- `dependabot.yml` opens weekly grouped updates for cargo and GitHub Actions.
- `.mergify.yml` protects `main` and queues squash merges. Both wait for the
  four `static-check.yml` jobs and the two CodeQL `Analyze` jobs. A CI job
  that should gate merging needs its check name added there. A pull request
  that someone other than `riya-amemiya` opened also waits for an approving
  review from `riya-amemiya`.

## Dependencies

| Crate | Purpose |
| ----- | ------- |
| `fontdb` | System font discovery and fallback lookup |
| `memchr` | Byte and substring search |
| `regex` | Regex search over scrollback |
| `rustix` | PTY, termios, and process calls |
| `swash` | Glyph rasterization |
| `umt_rust` | Hex color parsing for OSC sequences and settings, base64 decoding for OSC 52 |
| `unicode-width` | Character cell width |
| `wgpu` | GPU rendering |
| `winit` | Window and input events |

## Notes for AI Assistants

- macOS is the first priority. A change keeps both CI targets passing.
- Measure throughput before and after touching the hot paths, which are
  `Parser::ground`, `Parser::csi`, `Term::write_ascii`, `Term::write_chars`,
  `Row::write_ascii`, `Grid::scroll_up`, and `Scene::compose_row`. Timing
  `seq 1 1000000` inside a release build of the window is a quick check. The
  PTY of the kernel limits that number, so compare the parser and the grid
  with `cargo bench -p dartel-term --bench throughput`, which feeds generated
  input to `Parser::advance` outside the window. Run it on the base commit and
  on the change, and compare the best rates.
- The search foundation of happy-cracking is `pub(crate)` and cannot be a
  dependency. Its designs are ported instead, and `docs/architecture.md` lists
  them.
- `umt_rust` already provides hex color parsing and base64 decoding. Prefer it
  over a new dependency when it has the function. `umt_from_base64` returns a
  UTF-8 string, so binary payloads go through `dartel_image::base64`.
- The settings file has no parser dependency. `dartel-config` parses TOML in
  `toml.rs`, and a new setting needs a field in `Config`, its key in
  `from_toml` and `to_toml`, its bytes in `encode` and `decode`, and a row in
  the README table.
- A new dependency needs the maintainer's approval, an exact version pin, and
  a stated purpose.
- Keep this file and `docs/architecture.md` current when the structure
  changes.
