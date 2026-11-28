use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

#[cfg(target_os = "macos")]
const OPENER: &str = "open";
#[cfg(not(target_os = "macos"))]
const OPENER: &str = "xdg-open";

fn launch(target: &OsStr) {
    if let Ok(mut child) = Command::new(OPENER)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        let _ = thread::Builder::new()
            .name("dartel-open".into())
            .spawn(move || child.wait());
    }
}

pub fn url(url: &str) {
    launch(url.as_ref());
}

pub fn path(text: &str, directory: Option<&Path>, home: Option<&Path>) {
    let expanded = dartel_config::expand(text, home);
    let resolved = if expanded.is_absolute() {
        Some(expanded)
    } else {
        directory.map(|directory| directory.join(expanded))
    };
    if let Some(path) = resolved.filter(|path| path.exists()) {
        launch(path.as_os_str());
    }
}
