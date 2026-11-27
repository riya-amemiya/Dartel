# Contributing to Dartel

Please read [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) before opening an issue or
pull request.

## Setup

This repository is a Rust 2024 workspace. `dartel-core` holds the VT parser,
terminal state, scrollback search, and PTY. `dartel` holds the window and the
GPU renderer. Stable Rust builds, tests, and formats the project.

```sh
cargo build
cargo test
```

`cargo run -p dartel` opens the terminal window.

## Checks

Pull requests to `main` run these commands for `aarch64-apple-darwin` on macOS
and for `x86_64-unknown-linux-gnu` on Linux:

```sh
cargo build --workspace --target <target> --verbose
cargo test --workspace --target <target> --verbose
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --target <target> -- -D warnings
```

`cargo fmt` rewrites the workspace. Clippy must pass with no warnings.

## Tests

Integration tests live in `crates/dartel-core/tests/` and are named
`{module}_test.rs`. When parser or terminal behavior changes, add a test that
feeds the bytes to the parser and asserts the resulting screen.

## Pull requests

Target `main` and fill in the pull request template.

## Security

Report vulnerabilities as described in [SECURITY.md](SECURITY.md). Do not open
a public issue for a security problem.
