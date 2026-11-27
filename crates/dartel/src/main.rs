mod app;
mod clipboard;
mod font;
mod gpu;
mod input;
mod scene;
mod session;

use std::error::Error;

use winit::event_loop::EventLoop;

use app::App;
use session::Signal;

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::<Signal>::with_user_event().build()?;
    let mut app = App::new(event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    Ok(())
}
