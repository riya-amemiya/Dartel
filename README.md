# Dartel

A fast terminal emulator written in Rust. Apple Silicon macOS is the first target and x86_64 Linux is the second.

## Name

Dartel (ダーテル) comes from Dart, to fly like an arrow, and Terminal. With the long vowel, the name sounds like acceleration stretching out.

## Features

- GPU rendering with wgpu, one draw call per frame
- VT parser that scans runs of printable ASCII with NEON or SSE2
- Tabs in one window, with a tab bar that Dartel draws
- Settings in a TOML file that is loaded and saved on request
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
| Open a tab | Command+T | Ctrl+Shift+T |
| Save the settings to a file | Command+S | Ctrl+Shift+S |
| Load the settings from a file | Command+L | Ctrl+Shift+L |
| Scroll one page | Shift+PageUp, Shift+PageDown | Shift+PageUp, Shift+PageDown |

While the search prompt is open, Enter moves to the previous match, Shift+Enter moves to the next match, and Escape closes the prompt. A query is a regex, and it is matched as plain text when it does not compile. Matching ignores case unless the query contains an uppercase letter.

## Tabs

A new tab starts a login shell in the home directory. The tab bar appears while two or more tabs are open, and a click on a tab selects it. A tab closes when its shell exits, and the window closes with the last tab. A tab shows its number and the title that the shell set.

Drag with the left button to select text. Hold Shift while dragging when the application has enabled mouse reporting.

On macOS, the left Option key is Meta.

## Settings

Dartel reads a settings file only when asked. `load` parses a TOML file and keeps the result, and every later start uses the kept result without reading the TOML file again. `save` writes the settings in use to a TOML file, with every key.

```sh
dartel config save [FILE]
dartel config load [FILE]
```

`FILE` defaults to `$XDG_CONFIG_HOME/dartel/dartel.toml`, or `~/.config/dartel/dartel.toml` when the variable is unset. The kept result lives in `$XDG_STATE_HOME/dartel/settings.bin`, or `~/.local/state/dartel/settings.bin`. Both platforms use these paths.

In the window, Command+S and Command+L on macOS, or Ctrl+Shift+S and Ctrl+Shift+L on Linux, open a prompt with the default path. Enter confirms and Escape cancels. A load in the window applies the font, the padding, and the colors at once. The scrollback limit applies to tabs opened after the load, and the window size applies at the next start.

A file may leave out any key, and a missing key keeps its default. An unknown key, a wrong type, or a value outside its range stops the load with the line number, and the kept result stays as it was.

```toml
[font]
family = "Menlo"
size = 13.0

[window]
columns = 80
lines = 24
padding = 4.0

[scrollback]
lines = 10000

[colors]
foreground = "#C5C8C6"
background = "#212733"
cursor = "#C5C8C6"

[colors.normal]
black = "#1D1F21"
red = "#CC6666"
green = "#B5BD68"
yellow = "#F0C674"
blue = "#81A2BE"
magenta = "#BD7BCD"
cyan = "#8ABEB7"
white = "#C5C8C6"

[colors.bright]
black = "#666666"
red = "#DF6C5A"
green = "#B9CA4A"
yellow = "#E7C547"
blue = "#7AA6DA"
magenta = "#C397D8"
cyan = "#70C0B1"
white = "#EAEAEA"
```

| Key | Range | Default |
| --- | --- | --- |
| `font.family` | A font family name | `Menlo` on macOS, `DejaVu Sans Mono` on Linux |
| `font.size` | 4 to 96 | 13 |
| `window.columns` | 2 to 1000 | 80 |
| `window.lines` | 1 to 1000 | 24 |
| `window.padding` | 0 to 64 | 4 |
| `scrollback.lines` | 0 to 1000000 | 10000 |
| `colors.*` | `#RGB`, `#RRGGBB`, or `#RRGGBBAA` with the alpha ignored | The values above |

When the font family is not installed, Dartel uses the first available of Menlo and Monaco on macOS or DejaVu Sans Mono and Liberation Mono on Linux.

## Defaults outside the settings file

| Setting | Value |
| --- | --- |
| Shell | `$SHELL` as a login shell, or `/bin/zsh` on macOS and `/bin/sh` on Linux when it is unset |
| Start directory | `$HOME`, as `cd` without an argument |
| `TERM` | `xterm-256color` |
| `COLORTERM` | `truecolor` |

## Documentation

- [Architecture](docs/architecture.md) explains the threads, the data structures, and the rendering path with diagrams.
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)

## License

[MIT](LICENSE)
