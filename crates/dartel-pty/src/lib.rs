#![cfg(unix)]

use std::env;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus};

use rustix::io::{FdFlags, fcntl_setfd};
use rustix::process::{ioctl_tiocsctty, setsid};
use rustix::pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt};
use rustix::stdio::stdin;
use rustix::termios::{InputModes, OptionalActions, Winsize, tcgetattr, tcsetattr, tcsetwinsize};

#[cfg(target_os = "macos")]
const DEFAULT_SHELL: &str = "/bin/zsh";
#[cfg(not(target_os = "macos"))]
const DEFAULT_SHELL: &str = "/bin/sh";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub lines: u16,
    pub cols: u16,
    pub width: u16,
    pub height: u16,
}

impl From<Size> for Winsize {
    fn from(size: Size) -> Winsize {
        Winsize {
            ws_row: size.lines,
            ws_col: size.cols,
            ws_xpixel: size.width,
            ws_ypixel: size.height,
        }
    }
}

#[derive(Debug)]
pub struct Pty {
    master: File,
    child: Child,
}

#[must_use]
pub fn login_shell() -> Command {
    let shell = env::var_os("SHELL")
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| DEFAULT_SHELL.into());
    let mut command = Command::new(&shell);
    command
        .arg0(format!(
            "-{}",
            Path::new(&shell)
                .file_name()
                .unwrap_or(shell.as_os_str())
                .to_string_lossy()
        ))
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor");
    command
}

impl Pty {
    pub fn spawn(mut command: Command, size: Size) -> io::Result<Pty> {
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY)?;
        fcntl_setfd(&master, FdFlags::CLOEXEC)?;
        grantpt(&master)?;
        unlockpt(&master)?;
        let slave = OpenOptions::new()
            .read(true)
            .write(true)
            .open(OsStr::from_bytes(ptsname(&master, Vec::new())?.as_bytes()))?;
        tcsetwinsize(&slave, size.into())?;
        let mut attributes = tcgetattr(&slave)?;
        attributes.input_modes |= InputModes::IUTF8;
        tcsetattr(&slave, OptionalActions::Now, &attributes)?;
        command
            .stdin(slave.try_clone()?)
            .stdout(slave.try_clone()?)
            .stderr(slave);
        unsafe {
            command.pre_exec(|| {
                setsid()?;
                ioctl_tiocsctty(stdin())?;
                Ok(())
            });
        }
        Ok(Pty {
            child: command.spawn()?,
            master: File::from(master),
        })
    }

    pub fn resize(&self, size: Size) -> io::Result<()> {
        Ok(tcsetwinsize(&self.master, size.into())?)
    }

    pub fn reader(&self) -> io::Result<File> {
        self.master.try_clone()
    }

    pub fn writer(&self) -> io::Result<File> {
        self.master.try_clone()
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }
}
