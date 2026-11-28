mod app;
mod clipboard;
mod input;
mod opener;
mod session;

use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use dartel_config::Locations;
use winit::event_loop::EventLoop;

use app::App;
use session::Signal;

const USAGE: &str =
    "usage: dartel [config save [FILE] | config load [FILE] | shell zsh | shell bash | shell fish]";

const ZSH: &str = include_str!("../shell/dartel.zsh");
const BASH: &str = include_str!("../shell/dartel.bash");
const FISH: &str = include_str!("../shell/dartel.fish");

fn locations() -> Result<Locations, Box<dyn Error>> {
    Locations::from_env().ok_or_else(|| "the home directory is unknown".into())
}

fn run() -> Result<(), Box<dyn Error>> {
    let library = app::scan()?;
    let locations = Locations::from_env();
    let config = locations
        .as_ref()
        .map_or_default(|locations| dartel_config::active(&locations.state));
    let event_loop = EventLoop::<Signal>::with_user_event().build()?;
    let mut app = App::new(event_loop.create_proxy(), config, locations, library);
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn save(file: Option<&OsString>) -> Result<(), Box<dyn Error>> {
    let locations = locations()?;
    let file = file.map_or(locations.file, PathBuf::from);
    dartel_config::save(&dartel_config::active(&locations.state), &file)?;
    println!("settings saved to {}", file.display());
    Ok(())
}

fn load(file: Option<&OsString>) -> Result<(), Box<dyn Error>> {
    let locations = locations()?;
    let file = file.map_or(locations.file, PathBuf::from);
    dartel_config::load(&file, &locations.state)?;
    println!("settings loaded from {}", file.display());
    Ok(())
}

fn shell(script: &str) -> Result<(), Box<dyn Error>> {
    io::stdout().write_all(script.as_bytes())?;
    Ok(())
}

fn main() -> ExitCode {
    let arguments: Vec<OsString> = env::args_os().skip(1).collect();
    let words: Vec<&str> = arguments
        .iter()
        .map(|argument| argument.to_str().unwrap_or_default())
        .collect();
    let outcome = match words.as_slice() {
        [] => run(),
        ["config", "save"] | ["config", "save", _] => save(arguments.get(2)),
        ["config", "load"] | ["config", "load", _] => load(arguments.get(2)),
        ["shell", "zsh"] => shell(ZSH),
        ["shell", "bash"] => shell(BASH),
        ["shell", "fish"] => shell(FISH),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dartel: {error}");
            ExitCode::FAILURE
        }
    }
}
