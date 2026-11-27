# Dartel

A fast terminal emulator written in Rust. Apple Silicon macOS is the first target and x86_64 Linux is the second.

## Features

- GPU rendering with wgpu, one draw call per frame
- VT parser that scans runs of printable ASCII with NEON or SSE2
- Scrollback with literal and regex search
- Lines rewrap when the window width changes
- True color and 256 colors, bold, dim, italic, underline, double underline, strikethrough, and inverse
- Wide characters, combining marks, and emoji through font fallback
- Mouse reporting, bracketed paste, focus events, synchronized updates, and the alternate screen
- IME input
- Selection, copy, and paste through the system clipboard

## Build and run

Dartel builds with stable Rust.

```sh
cargo build --release -p dartel
./target/release/dartel
```

On Linux, Dartel needs a Vulkan or OpenGL ES driver, and copy and paste use `wl-copy` and `wl-paste` or `xclip`.

## Shortcuts

| Action | macOS | Linux |
| --- | --- | --- |
| Paste | Command+V | Ctrl+Shift+V |
| Copy the selection | Command+C | Ctrl+Shift+C |
| Search scrollback | Command+F | Ctrl+Shift+F |
| Scroll one page | Shift+PageUp, Shift+PageDown | Shift+PageUp, Shift+PageDown |

While the search prompt is open, Enter moves to the previous match, Shift+Enter moves to the next match, and Escape closes the prompt. A query is a regex, and it is matched as plain text when it does not compile. Matching ignores case unless the query contains an uppercase letter.

Drag with the left button to select text. Hold Shift while dragging when the application has enabled mouse reporting.

On macOS, the left Option key is Meta.

## Defaults

| Setting | Value |
| --- | --- |
| Window | 80 columns by 24 lines |
| Font | Size 13, the first available of Menlo and Monaco on macOS or DejaVu Sans Mono and Liberation Mono on Linux |
| Scrollback | 10,000 lines |
| Shell | `$SHELL` as a login shell, or `/bin/zsh` on macOS and `/bin/sh` on Linux when it is unset |
| `TERM` | `xterm-256color` |
| `COLORTERM` | `truecolor` |

Dartel has no configuration file. These values are constants in the source.

## Documentation

- [Architecture](docs/architecture.md) explains the threads, the data structures, and the rendering path with diagrams.
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)

## License

[MIT](LICENSE)
