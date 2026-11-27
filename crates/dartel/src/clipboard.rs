use std::io::Write;
use std::process::{Command, Stdio};

#[cfg(target_os = "macos")]
const READERS: &[&[&str]] = &[&["pbpaste"]];
#[cfg(not(target_os = "macos"))]
const READERS: &[&[&str]] = &[
    &["wl-paste", "--no-newline"],
    &["xclip", "-selection", "clipboard", "-out"],
];

#[cfg(target_os = "macos")]
const WRITERS: &[&[&str]] = &[&["pbcopy"]];
#[cfg(not(target_os = "macos"))]
const WRITERS: &[&[&str]] = &[&["wl-copy"], &["xclip", "-selection", "clipboard", "-in"]];

pub fn read() -> Option<String> {
    READERS.iter().find_map(|command| {
        Command::new(command[0])
            .args(&command[1..])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
    })
}

pub fn write(text: &str) -> bool {
    WRITERS.iter().any(|command| {
        Command::new(command[0])
            .args(&command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok_and(|mut child| {
                child
                    .stdin
                    .take()
                    .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok())
                    && child.wait().is_ok_and(|status| status.success())
            })
    })
}
