#![cfg(unix)]

use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use dartel_pty::{Pty, Size, login_shell};

const SIZE: Size = Size {
    lines: 24,
    cols: 80,
    width: 800,
    height: 480,
};

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script);
    command
}

fn read_until(reader: &mut impl Read, needle: &str) -> String {
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    while !String::from_utf8_lossy(&output).contains(needle) {
        match reader.read(&mut buffer) {
            Ok(count) if count > 0 => output.extend_from_slice(&buffer[..count]),
            _ => break,
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

#[test]
fn child_output_arrives_on_the_reader() {
    let pty = Pty::spawn(shell("printf hello"), SIZE).unwrap();
    assert!(read_until(&mut pty.reader().unwrap(), "hello").contains("hello"));
}

#[test]
fn writer_input_reaches_the_child() {
    let pty = Pty::spawn(shell("read line; printf 'got %s.' \"$line\""), SIZE).unwrap();
    pty.writer().unwrap().write_all(b"ping\n").unwrap();
    assert!(read_until(&mut pty.reader().unwrap(), "got ping.").contains("got ping."));
}

#[test]
fn child_sees_the_initial_window_size() {
    let pty = Pty::spawn(shell("stty size; printf done"), SIZE).unwrap();
    assert!(read_until(&mut pty.reader().unwrap(), "done").contains("24 80"));
}

#[test]
fn resize_updates_the_window_size_seen_by_the_child() {
    let pty = Pty::spawn(shell("read line; stty size; printf done"), SIZE).unwrap();
    pty.resize(Size {
        lines: 40,
        cols: 132,
        width: 1320,
        height: 800,
    })
    .unwrap();
    pty.writer().unwrap().write_all(b"\n").unwrap();
    assert!(read_until(&mut pty.reader().unwrap(), "done").contains("40 132"));
}

#[test]
fn child_owns_a_controlling_terminal() {
    let pty = Pty::spawn(shell("tty; printf done"), SIZE).unwrap();
    assert!(read_until(&mut pty.reader().unwrap(), "done").contains("/dev/"));
}

#[test]
fn try_wait_reports_the_exit_status() {
    let mut pty = Pty::spawn(shell("exit 3"), SIZE).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = pty.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "child did not exit");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(status.code(), Some(3));
}

#[test]
fn spawn_fails_for_a_missing_program() {
    assert!(Pty::spawn(Command::new("/nonexistent/dartel-shell"), SIZE).is_err());
}

#[test]
fn login_shell_sets_the_terminal_environment() {
    let command = login_shell();
    let variable = |name: &str| {
        command
            .get_envs()
            .find(|(key, _)| *key == OsStr::new(name))
            .and_then(|(_, value)| value)
            .map(OsStr::to_owned)
    };
    assert_eq!(
        variable("TERM").as_deref(),
        Some(OsStr::new("xterm-256color"))
    );
    assert_eq!(
        variable("COLORTERM").as_deref(),
        Some(OsStr::new("truecolor"))
    );
}

#[test]
fn login_shell_starts_with_a_dash_prefixed_name() {
    let mut command = login_shell();
    let name = Path::new(command.get_program())
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    command.arg("-c").arg("printf 'name=%s.' \"$0\"");
    let pty = Pty::spawn(command, SIZE).unwrap();
    assert!(
        read_until(&mut pty.reader().unwrap(), &format!("name=-{name}."))
            .contains(&format!("name=-{name}."))
    );
}
