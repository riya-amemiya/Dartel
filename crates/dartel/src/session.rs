use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;

use dartel_core::pty::{Pty, Size, login_shell};
use dartel_core::term::Term;
use dartel_core::vt::Parser;
use winit::event_loop::EventLoopProxy;

const HISTORY: usize = 10_000;
const READ_BUFFER: usize = 1 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Wake,
    Exit,
}

struct Shared {
    term: Mutex<Term>,
    frame_waiting: AtomicBool,
    wake_pending: AtomicBool,
}

pub struct Session {
    shared: Arc<Shared>,
    pty: Pty,
    input: Sender<Vec<u8>>,
}

fn lock(term: &Mutex<Term>) -> MutexGuard<'_, Term> {
    term.lock().unwrap_or_else(PoisonError::into_inner)
}

fn pump(
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
            let _ = proxy.send_event(Signal::Wake);
        }
    }
    let _ = proxy.send_event(Signal::Exit);
}

impl Session {
    pub fn spawn(size: Size, proxy: EventLoopProxy<Signal>) -> io::Result<Session> {
        let pty = Pty::spawn(login_shell(), size)?;
        let shared = Arc::new(Shared {
            term: Mutex::new(Term::new(
                usize::from(size.lines),
                usize::from(size.cols),
                HISTORY,
            )),
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
            move || pump(reader, &shared, &input, &proxy)
        })?;
        Ok(Session { shared, pty, input })
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

    pub fn resize(&self, size: Size) {
        self.term()
            .resize(usize::from(size.lines), usize::from(size.cols));
        let _ = self.pty.resize(size);
    }
}
