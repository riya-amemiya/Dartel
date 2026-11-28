use std::env;
use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use dartel_pty::{Pty, Size, login_shell};
use dartel_term::{Palette, Term};
use dartel_vt::Parser;
use winit::event_loop::EventLoopProxy;

const READ_BUFFER: usize = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Wake(u64),
    Exit(u64),
}

struct Shared {
    term: Mutex<Term>,
    frame_waiting: AtomicBool,
    wake_pending: AtomicBool,
}

pub struct Session {
    id: u64,
    shared: Arc<Shared>,
    pty: Pty,
    input: Sender<Vec<u8>>,
}

fn lock(term: &Mutex<Term>) -> MutexGuard<'_, Term> {
    term.lock().unwrap_or_else(PoisonError::into_inner)
}

fn pump(
    id: u64,
    mut reader: File,
    shared: &Shared,
    input: &Sender<Vec<u8>>,
    proxy: &EventLoopProxy<Signal>,
) {
    let mut parser = Parser::new();
    let mut buffer = vec![0; READ_BUFFER];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        while shared.frame_waiting.load(Ordering::Acquire) {
            thread::yield_now();
        }
        let reply = {
            let mut term = lock(&shared.term);
            parser.advance(&mut *term, &buffer[..count]);
            term.take_reply()
        };
        if !reply.is_empty() {
            let _ = input.send(reply);
        }
        if !shared.wake_pending.swap(true, Ordering::AcqRel) {
            let _ = proxy.send_event(Signal::Wake(id));
        }
    }
    let _ = proxy.send_event(Signal::Exit(id));
}

impl Session {
    pub fn spawn(
        id: u64,
        size: Size,
        cell: [u16; 2],
        directory: Option<PathBuf>,
        palette: Palette,
        history: usize,
        proxy: EventLoopProxy<Signal>,
    ) -> io::Result<Session> {
        let pty = Pty::spawn(login_shell(directory, env::current_exe().ok()), size)?;
        let mut term = Term::with_palette(
            usize::from(size.lines),
            usize::from(size.cols),
            history,
            palette,
        );
        term.set_cell_size(cell[0], cell[1]);
        let shared = Arc::new(Shared {
            term: Mutex::new(term),
            frame_waiting: AtomicBool::new(false),
            wake_pending: AtomicBool::new(false),
        });
        let (input, pending) = mpsc::channel::<Vec<u8>>();
        let mut writer = pty.writer()?;
        thread::Builder::new()
            .name("dartel-write".into())
            .spawn(move || {
                for bytes in pending {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                }
            })?;
        let reader = pty.reader()?;
        thread::Builder::new().name("dartel-read".into()).spawn({
            let shared = Arc::clone(&shared);
            let input = input.clone();
            move || pump(id, reader, &shared, &input, &proxy)
        })?;
        Ok(Session {
            id,
            shared,
            pty,
            input,
        })
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn send(&self, bytes: Vec<u8>) {
        let _ = self.input.send(bytes);
    }

    pub fn term(&self) -> MutexGuard<'_, Term> {
        self.shared.frame_waiting.store(true, Ordering::Release);
        let term = lock(&self.shared.term);
        self.shared.frame_waiting.store(false, Ordering::Release);
        term
    }

    pub fn frame(&self) -> MutexGuard<'_, Term> {
        self.shared.wake_pending.store(false, Ordering::Release);
        self.term()
    }

    pub fn resize(&self, size: Size, cell: [u16; 2]) {
        {
            let mut term = self.term();
            term.set_cell_size(cell[0], cell[1]);
            term.resize(usize::from(size.lines), usize::from(size.cols));
        }
        let _ = self.pty.resize(size);
    }

    pub fn close(mut self) {
        if !matches!(self.pty.try_wait(), Ok(Some(_))) {
            let _ = self.pty.hangup();
            let _ = thread::Builder::new()
                .name("dartel-reap".into())
                .spawn(move || self.pty.wait());
        }
    }
}
