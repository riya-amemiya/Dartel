use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use dartel_config::{
    Action, Config, Error, Key, Keys, Locations, Modifiers, Named, Shortcut, active, expand, load,
    save, toml,
};
use dartel_term::{Palette, Rgb};

#[cfg(target_os = "macos")]
const DEFAULT_SHORTCUTS: [&str; 24] = [
    "cmd+v",
    "cmd+c",
    "cmd+o",
    "cmd+f",
    "cmd+up",
    "cmd+down",
    "cmd+t",
    "cmd+w",
    "cmd+1",
    "cmd+2",
    "cmd+3",
    "cmd+4",
    "cmd+5",
    "cmd+6",
    "cmd+7",
    "cmd+8",
    "cmd+9",
    "cmd++",
    "cmd+-",
    "cmd+0",
    "cmd+s",
    "cmd+l",
    "shift+pageup",
    "shift+pagedown",
];
#[cfg(not(target_os = "macos"))]
const DEFAULT_SHORTCUTS: [&str; 24] = [
    "ctrl+shift+v",
    "ctrl+shift+c",
    "ctrl+shift+o",
    "ctrl+shift+f",
    "ctrl+shift+up",
    "ctrl+shift+down",
    "ctrl+shift+t",
    "ctrl+shift+w",
    "ctrl+shift+1",
    "ctrl+shift+2",
    "ctrl+shift+3",
    "ctrl+shift+4",
    "ctrl+shift+5",
    "ctrl+shift+6",
    "ctrl+shift+7",
    "ctrl+shift+8",
    "ctrl+shift+9",
    "ctrl+shift++",
    "ctrl+shift+-",
    "ctrl+shift+0",
    "ctrl+shift+s",
    "ctrl+shift+l",
    "shift+pageup",
    "shift+pagedown",
];

const DOCUMENT: &str = "[font]
family = \"Test Mono\"
size = 13.0

[window]
columns = 80
lines = 24
padding = 4.0

[scrollback]
lines = 10000

[colors]
foreground = \"#C5C8C6\"
background = \"#212733\"
cursor = \"#C5C8C6\"

[colors.normal]
black = \"#1D1F21\"
red = \"#CC6666\"
green = \"#B5BD68\"
yellow = \"#F0C674\"
blue = \"#81A2BE\"
magenta = \"#BD7BCD\"
cyan = \"#8ABEB7\"
white = \"#C5C8C6\"

[colors.bright]
black = \"#666666\"
red = \"#DF6C5A\"
green = \"#B9CA4A\"
yellow = \"#E7C547\"
blue = \"#7AA6DA\"
magenta = \"#C397D8\"
cyan = \"#70C0B1\"
white = \"#EAEAEA\"

[keys]
paste = \"ctrl+shift+v\"
copy = \"ctrl+shift+c\"
copy-output = \"ctrl+shift+o\"
search = \"ctrl+shift+f\"
previous-prompt = \"ctrl+shift+up\"
next-prompt = \"ctrl+shift+down\"
new-tab = \"ctrl+shift+t\"
close-tab = \"ctrl+shift+w\"
tab-1 = \"ctrl+shift+1\"
tab-2 = \"ctrl+shift+2\"
tab-3 = \"ctrl+shift+3\"
tab-4 = \"ctrl+shift+4\"
tab-5 = \"ctrl+shift+5\"
tab-6 = \"ctrl+shift+6\"
tab-7 = \"ctrl+shift+7\"
tab-8 = \"ctrl+shift+8\"
tab-9 = \"ctrl+shift+9\"
zoom-in = \"ctrl+shift++\"
zoom-out = \"ctrl+shift+-\"
zoom-reset = \"ctrl+shift+0\"
save-settings = \"ctrl+shift+s\"
load-settings = \"ctrl+shift+l\"
page-up = \"shift+pageup\"
page-down = \"shift+pagedown\"
";

fn portable() -> Keys {
    let mut keys = Keys::default();
    for action in Action::ALL {
        let shortcut = keys.get(action).map(|shortcut| Shortcut {
            modifiers: if shortcut.modifiers == Modifiers::SHIFT {
                Modifiers::SHIFT
            } else {
                Modifiers::CTRL | Modifiers::SHIFT
            },
            ..shortcut
        });
        keys.set(action, shortcut);
    }
    keys
}

fn sample() -> Config {
    Config {
        font_family: "Test Mono".to_owned(),
        keys: portable(),
        ..Config::default()
    }
}

fn chord(modifiers: Modifiers, key: Key) -> Shortcut {
    Shortcut { modifiers, key }
}

fn bound(text: &str, action: Action) -> Option<Shortcut> {
    Config::from_toml(text).unwrap().keys.get(action)
}

fn read(text: &str, action: Action) -> Shortcut {
    bound(text, action).unwrap()
}

fn rejected(text: &str) -> (usize, String) {
    let error = Config::from_toml(text).unwrap_err();
    (error.line, error.message)
}

fn refusal(line: usize, message: &str) -> (usize, String) {
    (line, message.to_owned())
}

fn scratch(name: &str) -> PathBuf {
    let path = env::temp_dir().join(format!("dartel-config-{}-{name}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn patched(offset: usize, bytes: &[u8]) -> Vec<u8> {
    let mut encoded = sample().encode();
    encoded[offset..offset + bytes.len()].copy_from_slice(bytes);
    encoded
}

#[test]
fn default_config_carries_the_default_palette() {
    assert_eq!(Config::default().palette(), Palette::default());
}

#[test]
fn default_config_uses_the_documented_sizes() {
    let config = Config::default();
    assert_eq!(
        (
            config.font_size,
            config.columns,
            config.lines,
            config.padding,
            config.scrollback
        ),
        (13.0, 80, 24, 4.0, 10_000)
    );
}

#[test]
fn to_toml_writes_every_setting() {
    assert_eq!(sample().to_toml(), DOCUMENT);
}

#[test]
fn written_toml_reads_back_as_the_same_config() {
    assert_eq!(Config::from_toml(DOCUMENT), Ok(sample()));
}

#[test]
fn font_family_with_special_characters_survives_a_round_trip() {
    let config = Config {
        font_family: "A \"B\" \\ C\u{7}\u{e9}".to_owned(),
        ..Config::default()
    };
    assert!(
        config
            .to_toml()
            .contains("family = \"A \\\"B\\\" \\\\ C\\u0007\u{e9}\"")
    );
    assert_eq!(Config::from_toml(&config.to_toml()), Ok(config));
}

#[test]
fn empty_document_yields_the_defaults() {
    assert_eq!(Config::from_toml(""), Ok(Config::default()));
}

#[test]
fn font_settings_are_read() {
    let config = Config::from_toml("[font]\nfamily = \"Monaco\"\nsize = 15.5").unwrap();
    assert_eq!(config.font_family, "Monaco");
    assert_eq!(config.font_size, 15.5);
}

#[test]
fn font_size_accepts_an_integer() {
    assert_eq!(Config::from_toml("font.size = 16").unwrap().font_size, 16.0);
}

#[test]
fn window_settings_are_read() {
    let config = Config::from_toml("[window]\ncolumns = 120\nlines = 40\npadding = 0").unwrap();
    assert_eq!(
        (config.columns, config.lines, config.padding),
        (120, 40, 0.0)
    );
}

#[test]
fn scrollback_lines_are_read() {
    assert_eq!(
        Config::from_toml("scrollback.lines = 0")
            .unwrap()
            .scrollback,
        0
    );
}

#[test]
fn base_colors_are_read() {
    let config = Config::from_toml(
        "[colors]\nforeground = \"#010203\"\nbackground = \"#abc\"\ncursor = \"#0A0B0C\"",
    )
    .unwrap();
    assert_eq!(config.foreground, Rgb::new(1, 2, 3));
    assert_eq!(config.background, Rgb::new(0xAA, 0xBB, 0xCC));
    assert_eq!(config.cursor, Rgb::new(10, 11, 12));
}

#[test]
fn color_with_an_alpha_channel_drops_the_alpha() {
    assert_eq!(
        Config::from_toml("colors.cursor = \"#01020380\"")
            .unwrap()
            .cursor,
        Rgb::new(1, 2, 3)
    );
}

#[test]
fn normal_colors_fill_the_first_eight_entries() {
    let config = Config::from_toml(
        "[colors.normal]\nblack = \"#000001\"\nred = \"#000002\"\ngreen = \"#000003\"\nyellow = \"#000004\"\nblue = \"#000005\"\nmagenta = \"#000006\"\ncyan = \"#000007\"\nwhite = \"#000008\"",
    )
    .unwrap();
    let expected: Vec<_> = (1..=8).map(|blue| Rgb::new(0, 0, blue)).collect();
    assert_eq!(config.ansi[..8], expected);
    assert_eq!(config.ansi[8..], Config::default().ansi[8..]);
}

#[test]
fn bright_colors_fill_the_last_eight_entries() {
    let config = Config::from_toml(
        "[colors.bright]\nblack = \"#000001\"\nred = \"#000002\"\ngreen = \"#000003\"\nyellow = \"#000004\"\nblue = \"#000005\"\nmagenta = \"#000006\"\ncyan = \"#000007\"\nwhite = \"#000008\"",
    )
    .unwrap();
    let expected: Vec<_> = (1..=8).map(|blue| Rgb::new(0, 0, blue)).collect();
    assert_eq!(config.ansi[8..], expected);
    assert_eq!(config.ansi[..8], Config::default().ansi[..8]);
}

#[test]
fn syntax_error_keeps_its_line() {
    assert_eq!(rejected("[font]\nsize ="), refusal(2, "expected a value"));
}

#[test]
fn unknown_top_level_key_is_rejected() {
    assert_eq!(rejected("theme = 1"), refusal(1, "unknown key `theme`"));
}

#[test]
fn unknown_font_key_is_rejected() {
    assert_eq!(
        rejected("[font]\nweight = 1"),
        refusal(2, "unknown key `font.weight`")
    );
}

#[test]
fn unknown_window_key_is_rejected() {
    assert_eq!(
        rejected("[window]\ntitle = 1"),
        refusal(2, "unknown key `window.title`")
    );
}

#[test]
fn unknown_scrollback_key_is_rejected() {
    assert_eq!(
        rejected("[scrollback]\nlimit = 1"),
        refusal(2, "unknown key `scrollback.limit`")
    );
}

#[test]
fn unknown_colors_key_is_rejected() {
    assert_eq!(
        rejected("[colors]\nselection = \"#000000\""),
        refusal(2, "unknown key `colors.selection`")
    );
}

#[test]
fn unknown_color_name_is_rejected() {
    assert_eq!(
        rejected("[colors.normal]\norange = \"#000000\""),
        refusal(2, "unknown key `colors.normal.orange`")
    );
}

#[test]
fn section_that_is_not_a_table_is_rejected() {
    for section in ["font", "window", "scrollback", "colors", "keys"] {
        assert_eq!(
            rejected(&format!("{section} = 1")),
            refusal(1, &format!("`{section}` must be a table"))
        );
    }
}

#[test]
fn color_group_that_is_not_a_table_is_rejected() {
    for group in ["normal", "bright"] {
        assert_eq!(
            rejected(&format!("[colors]\n{group} = 1")),
            refusal(2, &format!("`colors.{group}` must be a table"))
        );
    }
}

#[test]
fn font_family_that_is_not_a_string_is_rejected() {
    assert_eq!(
        rejected("font.family = 1"),
        refusal(1, "`font.family` must be a string")
    );
}

#[test]
fn font_size_outside_the_range_is_rejected() {
    for size in ["3.9", "97", "nan", "\"13\""] {
        assert_eq!(
            rejected(&format!("font.size = {size}")),
            refusal(1, "`font.size` must be a number from 4 to 96")
        );
    }
}

#[test]
fn window_padding_outside_the_range_is_rejected() {
    for padding in ["-1", "64.5", "true"] {
        assert_eq!(
            rejected(&format!("window.padding = {padding}")),
            refusal(1, "`window.padding` must be a number from 0 to 64")
        );
    }
}

#[test]
fn window_columns_outside_the_range_are_rejected() {
    for columns in ["1", "1001", "80.0"] {
        assert_eq!(
            rejected(&format!("window.columns = {columns}")),
            refusal(1, "`window.columns` must be an integer from 2 to 1000")
        );
    }
}

#[test]
fn window_lines_outside_the_range_are_rejected() {
    for lines in ["0", "1001", "\"24\""] {
        assert_eq!(
            rejected(&format!("window.lines = {lines}")),
            refusal(1, "`window.lines` must be an integer from 1 to 1000")
        );
    }
}

#[test]
fn scrollback_lines_outside_the_range_are_rejected() {
    for lines in ["-1", "1000001", "1.5"] {
        assert_eq!(
            rejected(&format!("scrollback.lines = {lines}")),
            refusal(1, "`scrollback.lines` must be an integer from 0 to 1000000")
        );
    }
}

#[test]
fn malformed_base_color_is_rejected() {
    for color in [
        "\"212733\"",
        "\"#21273\"",
        "\"#GG2733\"",
        "1",
        "\"rgb:21/27/33\"",
    ] {
        assert_eq!(
            rejected(&format!("colors.cursor = {color}")),
            refusal(1, "`colors.cursor` must be a color such as \"#212733\"")
        );
    }
}

#[test]
fn malformed_named_color_is_rejected() {
    assert_eq!(
        rejected("[colors.bright]\nred = \"red\""),
        refusal(2, "`colors.bright.red` must be a color such as \"#212733\"")
    );
}

#[test]
fn default_keys_are_the_platform_shortcuts() {
    assert_eq!(
        Action::ALL.map(|action| Keys::default()
            .get(action)
            .map_or_default(|shortcut| shortcut.to_string())),
        DEFAULT_SHORTCUTS
    );
}

#[test]
fn default_config_carries_the_default_keys() {
    assert_eq!(Config::default().keys, Keys::default());
}

#[test]
fn action_names_are_the_keys_of_the_keys_table() {
    assert_eq!(
        Action::ALL.map(Action::name),
        [
            "paste",
            "copy",
            "copy-output",
            "search",
            "previous-prompt",
            "next-prompt",
            "new-tab",
            "close-tab",
            "tab-1",
            "tab-2",
            "tab-3",
            "tab-4",
            "tab-5",
            "tab-6",
            "tab-7",
            "tab-8",
            "tab-9",
            "zoom-in",
            "zoom-out",
            "zoom-reset",
            "save-settings",
            "load-settings",
            "page-up",
            "page-down",
        ]
    );
}

#[test]
fn modifiers_contain_their_parts_and_nothing_else() {
    let held = Modifiers::CTRL | Modifiers::ALT;
    assert!(held.contains(Modifiers::CTRL));
    assert!(held.contains(Modifiers::ALT));
    assert!(held.contains(Modifiers::NONE));
    assert!(!held.contains(Modifiers::CTRL | Modifiers::SHIFT));
    assert!(!held.contains(Modifiers::CMD));
}

#[test]
fn modifiers_count_their_parts() {
    assert_eq!(Modifiers::NONE.count(), 0);
    assert_eq!((Modifiers::CMD | Modifiers::SHIFT).count(), 2);
    assert_eq!((Modifiers::CMD | Modifiers::CMD).count(), 1);
}

#[test]
fn keys_set_replaces_one_shortcut() {
    let mut keys = Keys::default();
    keys.set(Action::Copy, None);
    assert_eq!(keys.get(Action::Copy), None);
    assert_eq!(keys.get(Action::Paste), Keys::default().get(Action::Paste));
}

#[test]
fn shortcut_display_writes_the_modifiers_in_a_fixed_order() {
    assert_eq!(
        chord(
            Modifiers::ALT | Modifiers::SHIFT | Modifiers::CTRL | Modifiers::CMD,
            Key::Character('k')
        )
        .to_string(),
        "cmd+ctrl+shift+alt+k"
    );
}

#[test]
fn shortcut_display_writes_the_name_of_a_named_key() {
    assert_eq!(
        chord(Modifiers::NONE, Key::Named(Named::PageDown)).to_string(),
        "pagedown"
    );
}

#[test]
fn keys_are_read() {
    assert_eq!(
        read("[keys]\ncopy = \"alt+c\"", Action::Copy),
        chord(Modifiers::ALT, Key::Character('c'))
    );
}

#[test]
fn keys_left_out_keep_their_defaults() {
    assert_eq!(
        bound("[keys]\ncopy = \"alt+c\"", Action::Paste),
        Keys::default().get(Action::Paste)
    );
}

#[test]
fn key_without_modifiers_is_read() {
    assert_eq!(
        read("keys.search = \"f5\"", Action::Search),
        chord(Modifiers::NONE, Key::Named(Named::F5))
    );
}

#[test]
fn shortcut_names_ignore_case() {
    assert_eq!(
        read("keys.page-up = \"CTRL+Shift+PageUp\"", Action::PageUp),
        chord(
            Modifiers::CTRL | Modifiers::SHIFT,
            Key::Named(Named::PageUp)
        )
    );
}

#[test]
fn letter_key_is_kept_in_lowercase() {
    assert_eq!(
        read("keys.copy = \"ctrl+C\"", Action::Copy),
        chord(Modifiers::CTRL, Key::Character('c'))
    );
}

#[test]
fn plus_key_follows_the_last_plus() {
    assert_eq!(
        read("keys.zoom-in = \"cmd++\"", Action::ZoomIn),
        chord(Modifiers::CMD, Key::Character('+'))
    );
}

#[test]
fn lone_plus_is_the_plus_key() {
    assert_eq!(
        read("keys.zoom-in = \"+\"", Action::ZoomIn),
        chord(Modifiers::NONE, Key::Character('+'))
    );
}

#[test]
fn every_named_key_is_read_by_its_name() {
    for (name, named) in [
        ("up", Named::Up),
        ("down", Named::Down),
        ("left", Named::Left),
        ("right", Named::Right),
        ("pageup", Named::PageUp),
        ("pagedown", Named::PageDown),
        ("home", Named::Home),
        ("end", Named::End),
        ("enter", Named::Enter),
        ("tab", Named::Tab),
        ("escape", Named::Escape),
        ("space", Named::Space),
        ("backspace", Named::Backspace),
        ("delete", Named::Delete),
        ("insert", Named::Insert),
        ("f1", Named::F1),
        ("f2", Named::F2),
        ("f3", Named::F3),
        ("f4", Named::F4),
        ("f5", Named::F5),
        ("f6", Named::F6),
        ("f7", Named::F7),
        ("f8", Named::F8),
        ("f9", Named::F9),
        ("f10", Named::F10),
        ("f11", Named::F11),
        ("f12", Named::F12),
    ] {
        assert_eq!(
            read(&format!("keys.copy = \"alt+{name}\""), Action::Copy),
            chord(Modifiers::ALT, Key::Named(named))
        );
        assert_eq!(
            chord(Modifiers::ALT, Key::Named(named)).to_string(),
            format!("alt+{name}")
        );
    }
}

#[test]
fn empty_string_leaves_the_action_without_a_shortcut() {
    assert_eq!(bound("keys.copy = \"\"", Action::Copy), None);
}

#[test]
fn swapped_shortcuts_are_accepted() {
    let config =
        Config::from_toml("[keys]\npaste = \"ctrl+shift+c\"\ncopy = \"ctrl+shift+v\"").unwrap();
    assert_eq!(
        (
            config.keys.get(Action::Paste),
            config.keys.get(Action::Copy)
        ),
        (
            Some(chord(
                Modifiers::CTRL | Modifiers::SHIFT,
                Key::Character('c')
            )),
            Some(chord(
                Modifiers::CTRL | Modifiers::SHIFT,
                Key::Character('v')
            ))
        )
    );
}

#[test]
fn to_toml_writes_an_empty_string_for_an_action_without_a_shortcut() {
    let mut config = sample();
    config.keys.set(Action::CloseTab, None);
    assert!(config.to_toml().contains("\nclose-tab = \"\"\n"));
}

#[test]
fn keys_with_quote_characters_survive_a_round_trip() {
    let mut config = sample();
    config.keys.set(
        Action::Copy,
        Some(chord(Modifiers::ALT, Key::Character('"'))),
    );
    config.keys.set(
        Action::Paste,
        Some(chord(Modifiers::ALT, Key::Character('\\'))),
    );
    assert!(config.to_toml().contains("\ncopy = \"alt+\\\"\"\n"));
    assert!(config.to_toml().contains("\npaste = \"alt+\\\\\"\n"));
    assert_eq!(Config::from_toml(&config.to_toml()), Ok(config));
}

#[test]
fn unknown_action_is_rejected() {
    assert_eq!(
        rejected("[keys]\nquit = \"ctrl+q\""),
        refusal(2, "unknown key `keys.quit`")
    );
}

#[test]
fn shortcut_that_is_not_a_string_is_rejected() {
    assert_eq!(
        rejected("keys.copy = 1"),
        refusal(1, "`keys.copy` must be a string")
    );
}

#[test]
fn shortcut_ending_in_a_modifier_is_rejected() {
    assert_eq!(
        rejected("keys.copy = \"ctrl+\""),
        refusal(1, "`keys.copy` must be a shortcut such as \"ctrl+shift+c\"")
    );
}

#[test]
fn unknown_modifier_is_rejected() {
    assert_eq!(
        rejected("keys.copy = \"meta+c\""),
        refusal(
            1,
            "`keys.copy` must be a shortcut whose modifiers are cmd, ctrl, shift, or alt, not `meta`"
        )
    );
}

#[test]
fn unknown_key_name_is_rejected() {
    assert_eq!(
        rejected("keys.copy = \"ctrl+f13\""),
        refusal(
            1,
            "`keys.copy` must be a shortcut that ends in a known key, not `f13`"
        )
    );
}

#[test]
fn key_outside_ascii_is_rejected() {
    assert_eq!(
        rejected("keys.copy = \"ctrl+\u{e9}\""),
        refusal(
            1,
            "`keys.copy` must be a shortcut that ends in a known key, not `\u{e9}`"
        )
    );
}

#[test]
fn shortcut_used_twice_in_the_file_is_rejected_at_the_second_use() {
    assert_eq!(
        rejected("[keys]\npaste = \"alt+p\"\n\ncopy = \"ALT+P\""),
        refusal(
            4,
            "`keys.copy` must be a shortcut that `keys.paste` does not use"
        )
    );
}

#[test]
fn shortcut_of_a_default_action_is_rejected() {
    assert_eq!(
        rejected(&format!("keys.copy = \"{}\"", DEFAULT_SHORTCUTS[0])),
        refusal(
            1,
            "`keys.copy` must be a shortcut that `keys.paste` does not use"
        )
    );
}

#[test]
fn load_keeps_the_keys_for_the_next_start() {
    let directory = scratch("load-keys");
    let (file, state) = (
        directory.join("dartel.toml"),
        directory.join("settings.bin"),
    );
    fs::write(&file, "[keys]\ncopy = \"alt+c\"\npaste = \"\"").unwrap();
    let keys = load(&file, &state).unwrap().keys;
    assert_eq!(active(&state).keys, keys);
    assert_eq!(keys.get(Action::Paste), None);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn encoded_config_decodes_to_the_same_config() {
    let config = Config {
        font_family: "\u{30e1}\u{30a4}\u{30ea}\u{30aa}".to_owned(),
        font_size: 15.5,
        columns: 132,
        lines: 43,
        padding: 2.5,
        scrollback: 250_000,
        foreground: Rgb::new(1, 2, 3),
        background: Rgb::new(4, 5, 6),
        cursor: Rgb::new(7, 8, 9),
        ansi: std::array::from_fn(|index| Rgb::new(index as u8, 100, 200)),
        keys: sample().keys,
    };
    assert_eq!(Config::decode(&config.encode()), Some(config));
}

#[test]
fn encoded_keys_decode_to_the_same_keys() {
    let mut keys = Keys::default();
    keys.set(Action::Paste, None);
    keys.set(
        Action::Copy,
        Some(chord(
            Modifiers::CMD | Modifiers::CTRL | Modifiers::SHIFT | Modifiers::ALT,
            Key::Character('~'),
        )),
    );
    keys.set(
        Action::Search,
        Some(chord(Modifiers::NONE, Key::Named(Named::F12))),
    );
    let config = Config {
        keys,
        ..Config::default()
    };
    assert_eq!(Config::decode(&config.encode()), Some(config));
}

#[test]
fn encode_writes_two_bytes_for_each_action_before_the_font_family() {
    let encoded = sample().encode();
    assert_eq!(encoded.len(), 137 + "Test Mono".len());
    assert_eq!(&encoded[89..91], &[6, b'v']);
    assert_eq!(&encoded[133..137], &[4, 0x84, 4, 0x85]);
}

#[test]
fn decode_rejects_a_truncated_state() {
    let encoded = sample().encode();
    for length in [0, 7, 15, 23, 25, 27, 31, 88, 136] {
        assert_eq!(Config::decode(&encoded[..length]), None);
    }
}

#[test]
fn decode_accepts_an_empty_font_family() {
    assert_eq!(
        Config::decode(&Config::default().encode()[..137]),
        Some(Config {
            font_family: String::new(),
            ..Config::default()
        })
    );
}

#[test]
fn decode_rejects_another_magic() {
    assert_eq!(Config::decode(&patched(7, &[1])), None);
}

#[test]
fn decode_reads_two_zero_bytes_as_no_shortcut() {
    assert_eq!(
        Config::decode(&patched(89, &[0, 0]))
            .unwrap()
            .keys
            .get(Action::Paste),
        None
    );
}

#[test]
fn decode_rejects_modifiers_outside_the_four() {
    assert_eq!(Config::decode(&patched(89, &[16, b'v'])), None);
}

#[test]
fn decode_rejects_an_uppercase_letter() {
    assert_eq!(Config::decode(&patched(89, &[6, b'V'])), None);
}

#[test]
fn decode_rejects_a_byte_that_is_no_key() {
    for key in [0, 0x20, 0x7F] {
        assert_eq!(Config::decode(&patched(89, &[6, key])), None);
    }
}

#[test]
fn decode_rejects_a_named_key_past_the_list() {
    assert_eq!(Config::decode(&patched(89, &[6, 0x80 + 27])), None);
}

#[test]
fn decode_rejects_two_actions_with_one_shortcut() {
    assert_eq!(Config::decode(&patched(89, &[6, b'c'])), None);
}

#[test]
fn decode_rejects_a_font_size_outside_the_range() {
    assert_eq!(Config::decode(&patched(8, &200.0_f64.to_le_bytes())), None);
}

#[test]
fn decode_rejects_a_padding_outside_the_range() {
    assert_eq!(Config::decode(&patched(16, &f64::NAN.to_le_bytes())), None);
}

#[test]
fn decode_rejects_columns_outside_the_range() {
    assert_eq!(Config::decode(&patched(24, &1_u16.to_le_bytes())), None);
}

#[test]
fn decode_rejects_lines_outside_the_range() {
    assert_eq!(Config::decode(&patched(26, &0_u16.to_le_bytes())), None);
}

#[test]
fn decode_rejects_a_scrollback_outside_the_range() {
    assert_eq!(
        Config::decode(&patched(28, &2_000_000_u32.to_le_bytes())),
        None
    );
}

#[test]
fn decode_rejects_a_font_family_that_is_not_utf8() {
    assert_eq!(Config::decode(&patched(137, &[0xFF])), None);
}

#[test]
fn palette_takes_the_configured_colors() {
    let config = Config {
        foreground: Rgb::new(1, 1, 1),
        background: Rgb::new(2, 2, 2),
        cursor: Rgb::new(3, 3, 3),
        ansi: std::array::from_fn(|index| Rgb::new(index as u8, 0, 0)),
        ..Config::default()
    };
    let palette = config.palette();
    assert_eq!(
        (palette.foreground, palette.background, palette.cursor),
        (Rgb::new(1, 1, 1), Rgb::new(2, 2, 2), Rgb::new(3, 3, 3))
    );
    for index in 0..16 {
        assert_eq!(palette.indexed(index), Rgb::new(index, 0, 0));
    }
    assert_eq!(palette.indexed(16), Palette::default().indexed(16));
}

#[test]
fn locations_fall_back_to_the_home_directory() {
    assert_eq!(
        Locations::resolve(Some("/home/u".into()), None, None),
        Some(Locations {
            file: "/home/u/.config/dartel/dartel.toml".into(),
            state: "/home/u/.local/state/dartel/settings.bin".into(),
        })
    );
}

#[test]
fn locations_follow_absolute_xdg_directories() {
    assert_eq!(
        Locations::resolve(
            Some("/home/u".into()),
            Some("/etc/xdg".into()),
            Some("/var/state".into())
        ),
        Some(Locations {
            file: "/etc/xdg/dartel/dartel.toml".into(),
            state: "/var/state/dartel/settings.bin".into(),
        })
    );
}

#[test]
fn locations_ignore_relative_xdg_directories() {
    assert_eq!(
        Locations::resolve(
            Some("/home/u".into()),
            Some("config".into()),
            Some(String::new().into())
        ),
        Locations::resolve(Some("/home/u".into()), None, None)
    );
}

#[test]
fn locations_resolve_without_a_home_when_both_xdg_directories_are_set() {
    assert_eq!(
        Locations::resolve(None, Some("/c".into()), Some("/s".into())),
        Some(Locations {
            file: "/c/dartel/dartel.toml".into(),
            state: "/s/dartel/settings.bin".into(),
        })
    );
}

#[test]
fn locations_are_unknown_without_a_home() {
    assert_eq!(Locations::resolve(None, None, None), None);
    assert_eq!(
        Locations::resolve(Some(String::new().into()), None, None),
        None
    );
    assert_eq!(Locations::resolve(None, Some("/c".into()), None), None);
    assert_eq!(Locations::resolve(None, None, Some("/s".into())), None);
}

#[test]
fn locations_from_the_environment_match_the_resolved_ones() {
    assert_eq!(
        Locations::from_env(),
        Locations::resolve(
            env::var_os("HOME"),
            env::var_os("XDG_CONFIG_HOME"),
            env::var_os("XDG_STATE_HOME")
        )
    );
}

#[test]
fn expand_replaces_a_lone_tilde_with_the_home_directory() {
    assert_eq!(
        expand("~", Some(Path::new("/home/u"))),
        PathBuf::from("/home/u")
    );
}

#[test]
fn expand_replaces_a_leading_tilde_directory() {
    assert_eq!(
        expand("~/x/dartel.toml", Some(Path::new("/home/u"))),
        PathBuf::from("/home/u/x/dartel.toml")
    );
}

#[test]
fn expand_keeps_other_paths() {
    for path in ["~user/x", "/etc/dartel.toml", "relative/~/x", ""] {
        assert_eq!(
            expand(path, Some(Path::new("/home/u"))),
            PathBuf::from(path)
        );
    }
}

#[test]
fn expand_keeps_a_tilde_without_a_home_directory() {
    assert_eq!(expand("~/x", None), PathBuf::from("~/x"));
}

#[test]
fn active_config_is_the_default_without_a_state() {
    let directory = scratch("active-missing");
    assert_eq!(active(&directory.join("settings.bin")), Config::default());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn active_config_is_the_default_for_an_unreadable_state() {
    let directory = scratch("active-corrupt");
    fs::write(directory.join("settings.bin"), b"not a state").unwrap();
    assert_eq!(active(&directory.join("settings.bin")), Config::default());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn save_writes_the_toml_and_creates_the_directories() {
    let directory = scratch("save");
    let file = directory.join("nested/dartel.toml");
    save(&sample(), &file).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), DOCUMENT);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn save_fails_when_the_parent_is_a_file() {
    let directory = scratch("save-blocked");
    fs::write(directory.join("blocker"), b"").unwrap();
    assert!(save(&sample(), &directory.join("blocker/dartel.toml")).is_err());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn load_returns_the_config_and_keeps_it_as_the_active_one() {
    let directory = scratch("load");
    let (file, state) = (
        directory.join("dartel.toml"),
        directory.join("state/settings.bin"),
    );
    fs::write(&file, "[font]\nfamily = \"Test Mono\"\nsize = 20").unwrap();
    let expected = Config {
        font_family: "Test Mono".to_owned(),
        font_size: 20.0,
        ..Config::default()
    };
    assert_eq!(load(&file, &state).unwrap(), expected);
    assert_eq!(active(&state), expected);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn load_fails_for_a_missing_file() {
    let directory = scratch("load-missing");
    let outcome = load(
        &directory.join("dartel.toml"),
        &directory.join("settings.bin"),
    );
    assert!(matches!(outcome, Err(Error::Io(error)) if error.kind() == io::ErrorKind::NotFound));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn load_fails_for_an_invalid_file_and_keeps_the_state() {
    let directory = scratch("load-invalid");
    let (file, state) = (
        directory.join("dartel.toml"),
        directory.join("settings.bin"),
    );
    fs::write(&file, "font.size = 1").unwrap();
    assert!(matches!(
        load(&file, &state),
        Err(Error::Toml(toml::Error { line: 1, .. }))
    ));
    assert!(!state.exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn load_fails_when_the_state_cannot_be_written() {
    let directory = scratch("load-blocked");
    let file = directory.join("dartel.toml");
    fs::write(&file, "").unwrap();
    assert!(matches!(
        load(&file, &file.join("settings.bin")),
        Err(Error::Io(_))
    ));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn error_display_shows_the_cause() {
    assert_eq!(
        Error::from(toml::Error {
            line: 2,
            message: "expected a value".to_owned()
        })
        .to_string(),
        "line 2: expected a value"
    );
    assert_eq!(
        Error::from(io::Error::other("disk is full")).to_string(),
        "disk is full"
    );
}
