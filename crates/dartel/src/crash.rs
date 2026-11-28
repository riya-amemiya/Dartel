use std::backtrace::Backtrace;
use std::env::consts;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::panic::{self, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn report(info: &PanicHookInfo<'_>) -> String {
    format!(
        "dartel {} on {} {}, unix time {}\nthread '{}' {info}\nstack backtrace:\n{}\n",
        env!("CARGO_PKG_VERSION"),
        consts::OS,
        consts::ARCH,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        thread::current().name().unwrap_or("<unnamed>"),
        Backtrace::force_capture(),
    )
}

fn append(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(text.as_bytes())
}

pub fn install(path: PathBuf) {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = append(&path, &report(info));
        previous(info);
    }));
}
