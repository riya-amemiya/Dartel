# Dartel

A fast terminal emulator written in Rust. Apple Silicon macOS is the first target and x86_64 Linux is the second.

## Name

Dartel (ダーテル) comes from Dart, to fly like an arrow, and Terminal. With the long vowel, the name sounds like acceleration stretching out.

## Features

- GPU rendering with wgpu, one draw call for a frame of text
- VT parser that scans runs of printable ASCII with NEON or SSE2
- Several windows, each with its own tabs and a tab bar that Dartel draws
- Settings in a TOML file that is loaded and saved on request
- Scrollback with literal and regex search, and a bar on the right edge that shows the position while the view is scrolled back
- Lines rewrap when the window width changes
- True color and 256 colors, bold, dim, italic, straight, double, curly, dotted, and dashed underlines, strikethrough, and inverse
- Wide characters, combining marks, and emoji through font fallback
- Emoji sequences such as families, skin tones, and flags drawn as one emoji when the font joins them, each covering the cells of its characters
- Programming ligatures such as `->` and `!=` when the font has them, each covering the cells of its characters
- Box drawing characters, block elements, and the Powerline arrows drawn by Dartel to fill the cell, so lines join without gaps between cells and rows
- Images through Sixel and the kitty graphics protocol
- The kitty keyboard protocol
- Shell integration through OSC 133 and OSC 7
- Links from OSC 8, and URLs and file paths that open with a click
- Mouse reporting, bracketed paste, focus events, synchronized updates, and the alternate screen
- IME input
- Selection by drag, rectangle, word, or line, and copy and paste through the system clipboard

## Build and run

Dartel builds with stable Rust.

```sh
cargo build --release -p dartel
./target/release/dartel
```

On Linux, Dartel needs a Vulkan or OpenGL ES driver, and copy and paste use `wl-copy` and `wl-paste` or `xclip`.

## Shortcuts

| Action | macOS | Linux | Name in `[keys]` |
| --- | --- | --- | --- |
| Paste | Command+V | Ctrl+Shift+V | `paste` |
| Copy the selection | Command+C | Ctrl+Shift+C | `copy` |
| Copy the output of the last command | Command+O | Ctrl+Shift+O | `copy-output` |
| Search scrollback | Command+F | Ctrl+Shift+F | `search` |
| Scroll to the previous or the next prompt | Command+Up, Command+Down | Ctrl+Shift+Up, Ctrl+Shift+Down | `previous-prompt`, `next-prompt` |
| Open a tab | Command+T | Ctrl+Shift+T | `new-tab` |
| Close the tab | Command+W | Ctrl+Shift+W | `close-tab` |
| Open a window | Command+N | Ctrl+Shift+N | `new-window` |
| Select one of the first nine tabs | Command+1 to Command+9 | Ctrl+Shift+1 to Ctrl+Shift+9 | `tab-1` to `tab-9` |
| Make the font larger or smaller | Command+=, Command+- | Ctrl+Shift+=, Ctrl+Shift+- | `zoom-in`, `zoom-out` |
| Return to the font size of the settings | Command+0 | Ctrl+Shift+0 | `zoom-reset` |
| Save the settings to a file | Command+S | Ctrl+Shift+S | `save-settings` |
| Load the settings from a file | Command+L | Ctrl+Shift+L | `load-settings` |
| Scroll one page | Shift+PageUp, Shift+PageDown | Shift+PageUp, Shift+PageDown | `page-up`, `page-down` |
| Open the link, URL, or path under the pointer | Command+click | Ctrl+Shift+click | |
| Select a rectangle | Option+drag | Alt+drag | |

The `[keys]` table of the [settings](#settings) changes the keyboard shortcuts, one entry per name in the last column. A value is modifiers and one key joined with `+`, such as `"cmd+shift+c"`, and `""` leaves the action without a shortcut.

- The modifiers are `cmd`, `ctrl`, `shift`, and `alt`. `cmd` is Command on macOS and the Super key on Linux, and `alt` is Option on macOS.
- The key is one letter, one digit, one ASCII symbol, or one of `up`, `down`, `left`, `right`, `pageup`, `pagedown`, `home`, `end`, `enter`, `tab`, `escape`, `space`, `backspace`, `delete`, `insert`, and `f1` to `f12`. The `+` key is written `"cmd++"`. Case does not matter.
- A shortcut applies while all of its modifiers are held, and other modifiers may be held too. When two shortcuts apply, the one with more modifiers wins.
- A letter is compared with the character the key types, ignoring case. A digit or a symbol matches the key that carries it on a US layout. A symbol that needs Shift on a US layout, such as `+`, also matches any key that types it. A named key is compared with the key the layout reports.
- Two actions with the same shortcut stop the load with the line number and both names.

The default of `zoom-in` is written `cmd++` on macOS and `ctrl+shift++` on Linux. It matches the key that carries `=` and `+` on a US layout, and any key that types `+`. A key pressed with Command on macOS, or with Super on Linux, never reaches the shell, even when no shortcut uses it.

While the search prompt is open, Enter moves to the previous match, Shift+Enter moves to the next match, and Escape closes the prompt. A query is a regex, and it is matched as plain text when it does not compile. Matching ignores case unless the query contains an uppercase letter.

A font size shortcut changes the size of the window it is pressed in by one point within the range of the settings, 4 to 96. The next start uses the size of the last load again, and a save in the window writes the size in use in that window.

## Tabs

A new tab starts a login shell in the directory of the active tab when its shell reported one with OSC 7, and in the home directory otherwise. The tab bar appears while two or more tabs are open, and a click on a tab selects it. A tab closes when its shell exits or on the close shortcut, and the window closes with its last tab. A tab shows its number and the title that the shell set.

## Windows

A new window starts a login shell in the directory of the active tab of the window the shortcut was pressed in, by the same rule as a new tab. It opens with the `window.columns` and `window.lines` of the settings. Each window has its own tabs, selection, search, prompts, notices, and font size. Closing a window ends the shells of all its tabs as closing each tab would, and Dartel exits when the last window closes.

## Selection

Drag with the left button to select text. A double click selects a word, and a triple click selects a line together with the rows it wraps onto. Hold Shift when the application has enabled mouse reporting.

Drag with Option held on macOS, or with Alt held on Linux, to select a rectangle. The modifier counts when the drag starts. The cells under the start and the end of the drag are opposite corners, and a wide character cut by the left or the right side is selected whole. The copy holds the selected part of each row, top to bottom, with a line break between rows, and leaves out the blank cells after the end of a row. While the application has enabled mouse reporting, hold Shift as well.

## Shell integration

A shell tells Dartel where prompts and output are by printing these sequences.

| Sequence | Meaning | What Dartel does with it |
| --- | --- | --- |
| `OSC 133 ; A` | A prompt starts | The prompt shortcuts scroll to the previous and the next prompt |
| `OSC 133 ; C` | The output of a command starts | The output shortcut copies the output of the last command |
| `OSC 133 ; D ; code` | The command ended with this exit code | A bar in the bright red color marks the prompt of a command whose code is not 0 |
| `OSC 7 ; file://host/path` | The working directory | A new tab or window starts there when the host is this machine |

The OSC 133 marks are ignored on the alternate screen.

Dartel carries scripts that print these sequences for zsh, bash, and fish. A shell that Dartel starts has `DARTEL_BIN` set to the absolute path of the Dartel executable, and `dartel shell NAME` prints the script for the shell `NAME`. Add the line for your shell to its startup file. The line does nothing in a shell that Dartel did not start.

| Shell | File | Line |
| --- | --- | --- |
| zsh | `~/.zshrc` | `[[ -n $DARTEL_BIN ]] && eval "$("$DARTEL_BIN" shell zsh)"` |
| bash | `~/.bash_profile` | `[ -n "$DARTEL_BIN" ] && eval "$("$DARTEL_BIN" shell bash)"` |
| fish | `~/.config/fish/config.fish` | `test -n "$DARTEL_BIN"; and "$DARTEL_BIN" shell fish \| source` |

Dartel starts the shell as a login shell, so bash reads `~/.bash_profile`. The line can go in `~/.bashrc` instead when `~/.bash_profile` sources it.

The scripts print `OSC 133 ; A` before each prompt, `OSC 133 ; C` before a command runs, `OSC 133 ; D` after a command ran, and `OSC 7` when the shell starts and when the working directory changes. An empty command line prints no `OSC 133 ; D`. The scripts add hooks with `add-zsh-hook` in zsh, through `PROMPT_COMMAND` and `PS0` in bash, and through the `fish_prompt` and `fish_preexec` events in fish, so existing hooks and the look of the prompt stay as they were. Loading a script twice adds no second hook.

The bash script needs bash 4.4 or later for `OSC 133 ; C` and `OSC 133 ; D`, because it detects a command through `PS0`. With an older bash, the output shortcut and the bar of a failed command do nothing, and the prompt shortcuts and `OSC 7` still work.

## Links

Text printed inside an OSC 8 link is underlined. A click with Command on macOS, or with Ctrl+Shift on Linux, opens what is under the pointer through `open` or `xdg-open`. Dartel looks for these in order:

1. An OSC 8 link, which opens when its scheme is `http`, `https`, `ftp`, `file`, or `mailto`. A link with another scheme opens nothing
2. A URL in the text that starts with `http://`, `https://`, `ftp://`, or `file://`
3. A path to a file or directory that exists. `~` stands for the home directory, and a trailing `:line` or `:line:column` is dropped. A relative path starts from the directory reported with OSC 7 and is not opened without one

## Keyboard

Dartel implements the kitty keyboard protocol. An application sets the enhancement flags 1, 2, 4, 8, and 16 with `CSI > flags u` and `CSI = flags ; mode u`, restores them with `CSI < number u`, and reads them with `CSI ? u`. The primary screen and the alternate screen each keep a stack of 16 entries. Caps Lock and Num Lock are not reported as modifiers, and media keys are not reported.

On macOS, the left Option key is Meta.

## Images

Dartel draws images sent with Sixel or with the kitty graphics protocol. An image belongs to the rows it covers, so it scrolls into the scrollback with them and moves with them when the window width changes. Clearing the screen with `CSI 2 J`, a full reset, and entering the alternate screen with mode 1049 remove the images on the screen. After a font size change, images are scaled to the new cell size.

An application can read the cell size from the pixel fields of the window size, from `CSI 14 t`, and from `CSI 16 t`. The answer to the primary device attributes request lists Sixel.

An image is at most 4096 pixels on each side. Each tab keeps up to 256 MiB of images sent for later display, animation frames included, and up to 256 MiB of displayed images, and drops the oldest ones beyond that.

### Sixel

- 256 color registers. The first 16 hold the VT340 colors, and the others repeat them until the image defines them.
- Colors are defined in RGB or in HLS.
- Pixels are square, whatever aspect ratio the sequence names.
- Pixels the data leaves unpainted take the background color, unless the second parameter of the sequence is 1.
- After an image, the cursor is on the row below it, in the column where the image started.
- Text written over an image removes the image from those cells.

### kitty graphics protocol

- The actions are transmit (`a=t`), transmit and display (`a=T`), display (`a=p`), delete (`a=d`), query (`a=q`), animation frame (`a=f`), animation control (`a=a`), and frame composition (`a=c`).
- The formats are RGB (`f=24`), RGBA (`f=32`), and PNG (`f=100`), each with optional zlib compression (`o=z`) and split into chunks with `m`.
- Image ids (`i`), image numbers (`I`), placement ids (`p`), and replies with the quiet levels of `q`.
- A placement takes a source rectangle (`x`, `y`, `w`, `h`), an offset inside the first cell (`X`, `Y`), a size in cells (`c`, `r`), and the cursor policy (`C`).
- Delete takes the targets `a`, `c`, `f`, `i`, `n`, `p`, `q`, `r`, `x`, `y`, and `z`, and the uppercase forms also free the image data. `f` and `F` remove every frame of the image named by `i` or `I` except the root frame.
- Data is read from the escape code only (`t=d`). Transmission through a file, a temporary file, or shared memory is not supported, and neither are relative placements (`P`), the indexed formats (`f=8`, `f=16`), and feature queries (`e`). Such a command gets an error reply when it names an image.
- Every image is drawn above the text. `z` does not order images, and only the delete targets read it.
- Text written over an image leaves it in place, and so do the erase commands other than `CSI 2 J`.

### kitty Unicode placeholders

- `U=1` with `a=p` or `a=T` creates a virtual placement of `c` columns and `r` rows. It needs an image id, puts nothing on the screen, and leaves the cursor in place. The image is scaled into that rectangle and keeps its aspect ratio.
- The character U+10EEEE shows one cell of a virtual placement. Its foreground color is the image id, a 256-color index or a 24-bit color, and its underline color is the placement id. Without a placement id, the newest virtual placement of the image is shown.
- The first, second, and third combining marks from the kitty list of row and column diacritics give the row, the column, and the most significant byte of the image id. A cell whose marks are missing takes them from a placeholder cell directly to its left with the same foreground and underline colors: no marks continue its row in the next column with its high byte, a row alone continues the column on the same row, and a row and a column take its high byte when they follow its column.
- Placeholder cells are ordinary text, so they scroll, wrap, and are erased or overwritten like other characters. Their combining marks are not drawn and are not part of copied text. A cell outside the rectangle of its placement shows nothing.
- Delete removes virtual placements only for the targets `i`, `n`, and `r` and their uppercase forms. Sending an image again, or a placement with the same image id and placement id, removes its virtual placements, and so does dropping the image or its picture at the 256 MiB limits.
- An animation shown through placeholders plays while one of its placeholder cells is on the screen.

### kitty animation

- `a=f` adds a frame to the image named by `i` or `I`. The data is drawn at `x`, `y` onto a canvas that is the frame `c`, or the color `Y` in 32-bit RGBA, transparent black by default. `r` edits that frame instead of adding one. `X=1` replaces the canvas pixels, and the default is alpha blending. `z` is the gap in milliseconds, 40 for a new frame by default, and a negative gap makes a frame that is skipped.
- `a=a` stops the animation with `s=1`, runs it with `s=3`, and runs it until the last frame and waits there for more frames with `s=2`. `v=1` loops forever, which is the default, and a larger `v` plays the frames `v - 1` times and stops on the last frame. Stopping resets the loop count. `c` makes a frame the current one, and `r` with `z` sets the gap of a frame.
- `a=c` copies the rectangle of size `w` by `h` at `X`, `Y` in frame `r` onto `x`, `y` in frame `c`, blending by default and replacing with `C=1`. A missing image or frame gets `ENOENT`, and a rectangle outside the image or two overlapping rectangles in one frame get `EINVAL`.
- A new animation is stopped. The root frame has no gap until `a=a` gives it one, so a running animation skips it.
- A frame number past the last frame gets `ENOENT`, and frame data outside the image gets `EINVAL`. A frame that would make the frames of one image larger than 256 MiB gets `ENOSPC`.
- An animation plays only while one of its placements is on the screen of the active tab, and it waits where it is while it is out of view.

## Settings

Dartel reads a settings file only when asked. `load` parses a TOML file and keeps the result, and every later start uses the kept result without reading the TOML file again. `save` writes the settings in use to a TOML file, with every key.

```sh
dartel config save [FILE]
dartel config load [FILE]
```

`FILE` defaults to `$XDG_CONFIG_HOME/dartel/dartel.toml`, or `~/.config/dartel/dartel.toml` when the variable is unset. The kept result lives in `$XDG_STATE_HOME/dartel/settings.bin`, or `~/.local/state/dartel/settings.bin`. Both platforms use these paths.

In the window, Command+S and Command+L on macOS, or Ctrl+Shift+S and Ctrl+Shift+L on Linux, open a prompt with the default path. Enter confirms and Escape cancels. A load in a window applies the font, the fallback fonts, the padding, the colors, and the shortcuts to every open window at once. The scrollback limit applies to tabs opened after the load, and the window size applies to windows opened after the load. Windows opened after the load use the loaded settings.

A file may leave out any key, and a missing key keeps its default. An unknown key, a wrong type, or a value outside its range stops the load with the line number, and the kept result stays as it was.

```toml
[font]
family = "Menlo"
fallback = []
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

[keys]
paste = "cmd+v"
copy = "cmd+c"
copy-output = "cmd+o"
search = "cmd+f"
previous-prompt = "cmd+up"
next-prompt = "cmd+down"
new-tab = "cmd+t"
close-tab = "cmd+w"
new-window = "cmd+n"
tab-1 = "cmd+1"
tab-2 = "cmd+2"
tab-3 = "cmd+3"
tab-4 = "cmd+4"
tab-5 = "cmd+5"
tab-6 = "cmd+6"
tab-7 = "cmd+7"
tab-8 = "cmd+8"
tab-9 = "cmd+9"
zoom-in = "cmd++"
zoom-out = "cmd+-"
zoom-reset = "cmd+0"
save-settings = "cmd+s"
load-settings = "cmd+l"
page-up = "shift+pageup"
page-down = "shift+pagedown"
```

| Key | Range | Default |
| --- | --- | --- |
| `font.family` | A font family name | `Menlo` on macOS, `DejaVu Sans Mono` on Linux |
| `font.fallback` | An array of at most 16 font family names, each 1 to 255 bytes of UTF-8 | `[]` |
| `font.size` | 4 to 96 | 13 |
| `window.columns` | 2 to 1000 | 80 |
| `window.lines` | 1 to 1000 | 24 |
| `window.padding` | 0 to 64 | 4 |
| `scrollback.lines` | 0 to 1000000 | 10000 |
| `colors.*` | `#RGB`, `#RRGGBB`, or `#RRGGBBAA` with the alpha ignored | The values above |
| `keys.*` | A shortcut as described in [Shortcuts](#shortcuts), or `""` for none | The values above on macOS, with `ctrl+shift` in place of `cmd` on Linux |

When the font family is not installed, Dartel uses the first available of Menlo and Monaco on macOS or DejaVu Sans Mono and Liberation Mono on Linux.

When the regular face of the font family has the `calt`, `liga`, or `clig` feature for the default or Latin script, Dartel shapes text with it and draws the ligatures that the font produces. A ligature takes the cells of the characters it replaces, so the columns of the text around it stay where they are. Only characters of one style in one row join, and a wide character, a character with a combining mark, a box drawing character, a block element, or a Powerline arrow ends the sequence. Selection, search, copy, and the cursor keep working on the cells. There is no setting for this.

Dartel draws an emoji sequence as one emoji when the font that draws its first character shapes the whole sequence into one glyph. A sequence is characters joined by the zero width joiner U+200D, an emoji followed by a skin tone modifier from U+1F3FB to U+1F3FF, or two regional indicators that form a flag, paired from the left. The emoji is drawn from the first cell of the sequence, and the other cells of the sequence stay empty. The cells do not change, so the cursor, selection, search, copy, and the columns of the text after the sequence stay where they are. A sequence that the font does not join, and a joined glyph wider than the cells of the sequence, are drawn one character at a time. A character with only U+FE0F after it is not joined. There is no setting for this.

A character that the font family does not have is drawn from the first font in `font.fallback` that has it, in the order of the array, such as `fallback = ["IPAGothic", "Noto Sans CJK JP"]`. A name that is not installed is skipped. When none of them has the character, Dartel looks in Hiragino Sans, Apple Color Emoji, and Apple Symbols on macOS, or in Noto Sans Mono CJK JP, Noto Sans CJK JP, IPAGothic, Noto Color Emoji, and DejaVu Sans Mono on Linux, then in the installed monospaced fonts, and then in every installed font.

## Defaults outside the settings file

| Setting | Value |
| --- | --- |
| Shell | `$SHELL` as a login shell, or `/bin/zsh` on macOS and `/bin/sh` on Linux when it is unset |
| Start directory | `$HOME`, as `cd` without an argument |
| `TERM` | `xterm-256color` |
| `COLORTERM` | `truecolor` |

## Crash log

When a thread of the window panics, Dartel appends the panic message, its location, the thread name, and a backtrace to `$XDG_STATE_HOME/dartel/crash.log`, or `~/.local/state/dartel/crash.log` when the variable is unset. Each report starts with the version, the platform, and the Unix time. Attach the reports to a bug report.

## Documentation

- [Architecture](docs/architecture.md) explains the threads, the data structures, and the rendering path with diagrams.
- [Contributing](CONTRIBUTING.md)
- [Security policy](SECURITY.md)

## License

[MIT](LICENSE)
