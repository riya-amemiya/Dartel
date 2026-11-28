mod params;
pub mod scan;

pub use params::{MAX_PARAMS, Params, ParamsIter};

use hc_internal::grep::bytes::{Decoded, decode};

pub const MAX_INTERMEDIATES: usize = 2;
pub const MAX_OSC_PARAMS: usize = 16;
pub const MAX_OSC_BYTES: usize = 16 * 1024 * 1024;

const CHAR_BATCH: usize = 32;

pub trait Perform {
    fn print(&mut self, c: char);

    fn print_ascii(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.print(char::from(byte));
        }
    }

    fn print_chars(&mut self, chars: &[char]) {
        for &c in chars {
            self.print(c);
        }
    }

    fn execute(&mut self, byte: u8);

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: u8);

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8);

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool);

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: u8) {}

    fn put(&mut self, _bytes: &[u8]) {}

    fn unhook(&mut self) {}

    fn apc_dispatch(&mut self, _payload: &[u8]) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    EscapeIntermediate,
    CsiEntry,
    CsiParam,
    CsiIntermediate,
    CsiIgnore,
    DcsEntry,
    DcsParam,
    DcsIntermediate,
    DcsPassthrough,
    DcsIgnore,
    OscString,
    ApcString,
    SosPmString,
}

#[derive(Clone, Debug)]
pub struct Parser {
    state: State,
    params: Params,
    current: u16,
    param_seen: bool,
    sub_pending: bool,
    intermediates: [u8; MAX_INTERMEDIATES],
    intermediate_len: u8,
    ignoring: bool,
    osc: Vec<u8>,
    pending: [u8; 4],
    pending_len: u8,
}

impl Default for Parser {
    fn default() -> Parser {
        Parser::new()
    }
}

impl Parser {
    #[must_use]
    pub fn new() -> Parser {
        Parser {
            state: State::Ground,
            params: Params::default(),
            current: 0,
            param_seen: false,
            sub_pending: false,
            intermediates: [0; MAX_INTERMEDIATES],
            intermediate_len: 0,
            ignoring: false,
            osc: Vec::new(),
            pending: [0; 4],
            pending_len: 0,
        }
    }

    pub fn advance<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) {
        let mut index = self.resume_multibyte(performer, bytes);
        while index < bytes.len() {
            index += match self.state {
                State::Ground => self.ground(performer, &bytes[index..]),
                State::CsiEntry | State::CsiParam => self.csi(performer, &bytes[index..]),
                State::OscString => self.osc_string(performer, &bytes[index..]),
                State::DcsPassthrough => self.dcs_passthrough(performer, &bytes[index..]),
                State::ApcString => self.apc_string(performer, &bytes[index..]),
                _ => {
                    self.step(performer, bytes[index]);
                    1
                }
            };
        }
    }

    fn resume_multibyte<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) -> usize {
        let mut index = 0;
        while self.pending_len > 0 && index < bytes.len() {
            self.pending[usize::from(self.pending_len)] = bytes[index];
            self.pending_len += 1;
            index += 1;
            match decode(&self.pending[..usize::from(self.pending_len)]) {
                Decoded::Char(c, _) => {
                    performer.print(c);
                    self.pending_len = 0;
                }
                Decoded::Invalid(_) => {
                    performer.print(char::REPLACEMENT_CHARACTER);
                    self.pending_len = 0;
                    index -= 1;
                }
                Decoded::Incomplete => {}
            }
        }
        index
    }

    #[inline]
    fn ground<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) -> usize {
        let mut index = 0;
        while let Some(&byte) = bytes.get(index) {
            match byte {
                0x20..=0x7E => {
                    let run = scan::printable_run(&bytes[index..]);
                    performer.print_ascii(&bytes[index..index + run]);
                    index += run;
                }
                0x1B => {
                    self.enter_escape();
                    if bytes.get(index + 1) != Some(&b'[') {
                        return index + 1;
                    }
                    self.state = State::CsiEntry;
                    index += 2 + self.csi(performer, &bytes[index + 2..]);
                    if self.state != State::Ground {
                        return index;
                    }
                }
                0x7F => index += 1,
                0x00..=0x1F => {
                    performer.execute(byte);
                    index += 1;
                }
                _ => index += self.multibyte(performer, &bytes[index..]),
            }
        }
        index
    }

    #[inline]
    fn csi<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) -> usize {
        let mut current = self.current;
        let mut entered = false;
        for (index, &byte) in bytes.iter().enumerate() {
            match byte {
                b'0'..=b'9' => {
                    current = current
                        .saturating_mul(10)
                        .saturating_add(u16::from(byte - b'0'));
                    entered = true;
                }
                b';' | b':' => {
                    if self.params.is_full() {
                        self.ignoring = true;
                    } else {
                        self.params.push(current, self.sub_pending);
                    }
                    current = 0;
                    self.sub_pending = byte == b':';
                    entered = true;
                }
                _ => {
                    self.enter_params(current, entered);
                    if matches!(byte, 0x40..=0x7E) {
                        self.dispatch_csi(performer, byte);
                    } else {
                        self.step(performer, byte);
                    }
                    return index + 1;
                }
            }
        }
        self.enter_params(current, entered);
        bytes.len()
    }

    #[inline]
    fn enter_params(&mut self, current: u16, entered: bool) {
        self.current = current;
        if entered {
            self.param_seen = true;
            self.state = State::CsiParam;
        }
    }

    fn multibyte<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) -> usize {
        let mut chars = ['\0'; CHAR_BATCH];
        let mut count = 0;
        let mut index = 0;
        while index < bytes.len() && bytes[index] >= 0x80 {
            let (c, len) = match decode(&bytes[index..]) {
                Decoded::Char(c, len) => (c, len),
                Decoded::Invalid(len) => (char::REPLACEMENT_CHARACTER, len),
                Decoded::Incomplete => {
                    performer.print_chars(&chars[..count]);
                    self.pending_len = (bytes.len() - index) as u8;
                    self.pending[..bytes.len() - index].copy_from_slice(&bytes[index..]);
                    return bytes.len();
                }
            };
            chars[count] = c;
            count += 1;
            index += len;
            if count == CHAR_BATCH {
                performer.print_chars(&chars);
                count = 0;
            }
        }
        performer.print_chars(&chars[..count]);
        index
    }

    fn gather(&mut self, bytes: &[u8]) -> usize {
        let run = bytes
            .iter()
            .position(|&byte| byte < 0x20)
            .unwrap_or(bytes.len());
        self.osc
            .extend_from_slice(&bytes[..run.min(MAX_OSC_BYTES.saturating_sub(self.osc.len()))]);
        run
    }

    fn apc_string<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) -> usize {
        let run = self.gather(bytes);
        let Some(&byte) = bytes.get(run) else {
            return run;
        };
        match byte {
            0x1B => {
                performer.apc_dispatch(&self.osc);
                self.enter_escape();
            }
            0x18 | 0x1A => {
                self.state = State::Ground;
                performer.execute(byte);
            }
            _ => {}
        }
        run + 1
    }

    fn dcs_passthrough<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) -> usize {
        let run = bytes
            .iter()
            .position(|&byte| matches!(byte, 0x18 | 0x1A | 0x1B | 0x7F))
            .unwrap_or(bytes.len());
        if run > 0 {
            performer.put(&bytes[..run]);
        }
        let Some(&byte) = bytes.get(run) else {
            return run;
        };
        self.step(performer, byte);
        run + 1
    }

    fn osc_string<P: Perform>(&mut self, performer: &mut P, bytes: &[u8]) -> usize {
        let run = self.gather(bytes);
        let Some(&byte) = bytes.get(run) else {
            return run;
        };
        match byte {
            0x07 => {
                self.state = State::Ground;
                self.dispatch_osc(performer, true);
            }
            0x1B => {
                self.dispatch_osc(performer, false);
                self.enter_escape();
            }
            0x18 | 0x1A => {
                self.state = State::Ground;
                performer.execute(byte);
            }
            _ => {}
        }
        run + 1
    }

    fn dispatch_osc<P: Perform>(&self, performer: &mut P, bell_terminated: bool) {
        let mut params: [&[u8]; MAX_OSC_PARAMS] = [&[]; MAX_OSC_PARAMS];
        let mut count = 0;
        let mut rest = self.osc.as_slice();
        while count < MAX_OSC_PARAMS - 1
            && let Some(split) = memchr::memchr(b';', rest)
        {
            params[count] = &rest[..split];
            rest = &rest[split + 1..];
            count += 1;
        }
        params[count] = rest;
        performer.osc_dispatch(&params[..=count], bell_terminated);
    }

    fn step<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x18 | 0x1A => {
                self.leave(performer);
                self.state = State::Ground;
                performer.execute(byte);
            }
            0x1B => {
                self.leave(performer);
                self.enter_escape();
            }
            _ => match self.state {
                State::Escape => self.escape(performer, byte),
                State::EscapeIntermediate => self.escape_intermediate(performer, byte),
                State::CsiEntry => self.csi_entry(performer, byte),
                State::CsiParam => self.csi_param(performer, byte),
                State::CsiIntermediate => self.csi_intermediate(performer, byte),
                State::CsiIgnore => self.csi_ignore(performer, byte),
                State::DcsEntry => self.dcs_entry(performer, byte),
                State::DcsParam => self.dcs_param(performer, byte),
                State::DcsIntermediate => self.dcs_intermediate(performer, byte),
                State::Ground
                | State::OscString
                | State::ApcString
                | State::DcsPassthrough
                | State::DcsIgnore
                | State::SosPmString => {}
            },
        }
    }

    fn leave<P: Perform>(&self, performer: &mut P) {
        if self.state == State::DcsPassthrough {
            performer.unhook();
        }
    }

    fn enter_escape(&mut self) {
        self.state = State::Escape;
        self.params.clear();
        self.current = 0;
        self.param_seen = false;
        self.sub_pending = false;
        self.intermediate_len = 0;
        self.ignoring = false;
    }

    fn collected(&self) -> &[u8] {
        &self.intermediates[..usize::from(self.intermediate_len)]
    }

    fn collect(&mut self, byte: u8) {
        if usize::from(self.intermediate_len) == MAX_INTERMEDIATES {
            self.ignoring = true;
        } else {
            self.intermediates[usize::from(self.intermediate_len)] = byte;
            self.intermediate_len += 1;
        }
    }

    fn param(&mut self, byte: u8) {
        self.param_seen = true;
        match byte {
            b';' => {
                self.push_param();
                self.sub_pending = false;
            }
            b':' => {
                self.push_param();
                self.sub_pending = true;
            }
            _ => {
                self.current = self
                    .current
                    .saturating_mul(10)
                    .saturating_add(u16::from(byte - b'0'));
            }
        }
    }

    fn push_param(&mut self) {
        if self.params.is_full() {
            self.ignoring = true;
        } else {
            self.params.push(self.current, self.sub_pending);
        }
        self.current = 0;
    }

    fn finish_params(&mut self) {
        if self.param_seen {
            self.push_param();
        }
    }

    fn dispatch_esc<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        self.state = State::Ground;
        performer.esc_dispatch(self.collected(), self.ignoring, byte);
    }

    fn dispatch_csi<P: Perform>(&mut self, performer: &mut P, action: u8) {
        self.finish_params();
        self.state = State::Ground;
        performer.csi_dispatch(&self.params, self.collected(), self.ignoring, action);
    }

    fn hook<P: Perform>(&mut self, performer: &mut P, action: u8) {
        self.finish_params();
        self.state = State::DcsPassthrough;
        performer.hook(&self.params, self.collected(), self.ignoring, action);
    }

    fn escape<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x1F => performer.execute(byte),
            0x20..=0x2F => {
                self.collect(byte);
                self.state = State::EscapeIntermediate;
            }
            b'P' => self.state = State::DcsEntry,
            b'X' | b'^' => self.state = State::SosPmString,
            b'_' => {
                self.osc.clear();
                self.state = State::ApcString;
            }
            b'[' => self.state = State::CsiEntry,
            b']' => {
                self.osc.clear();
                self.state = State::OscString;
            }
            0x30..=0x7E => self.dispatch_esc(performer, byte),
            _ => {}
        }
    }

    fn escape_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x1F => performer.execute(byte),
            0x20..=0x2F => self.collect(byte),
            0x30..=0x7E => self.dispatch_esc(performer, byte),
            _ => {}
        }
    }

    fn csi_entry<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x1F => performer.execute(byte),
            0x20..=0x2F => {
                self.collect(byte);
                self.state = State::CsiIntermediate;
            }
            0x30..=0x3B => {
                self.param(byte);
                self.state = State::CsiParam;
            }
            0x3C..=0x3F => {
                self.collect(byte);
                self.state = State::CsiParam;
            }
            0x40..=0x7E => self.dispatch_csi(performer, byte),
            _ => {}
        }
    }

    fn csi_param<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x1F => performer.execute(byte),
            0x20..=0x2F => {
                self.collect(byte);
                self.state = State::CsiIntermediate;
            }
            0x30..=0x3B => self.param(byte),
            0x3C..=0x3F => self.state = State::CsiIgnore,
            0x40..=0x7E => self.dispatch_csi(performer, byte),
            _ => {}
        }
    }

    fn csi_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x1F => performer.execute(byte),
            0x20..=0x2F => self.collect(byte),
            0x30..=0x3F => self.state = State::CsiIgnore,
            0x40..=0x7E => self.dispatch_csi(performer, byte),
            _ => {}
        }
    }

    fn csi_ignore<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x00..=0x1F => performer.execute(byte),
            0x40..=0x7E => self.state = State::Ground,
            _ => {}
        }
    }

    fn dcs_entry<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x20..=0x2F => {
                self.collect(byte);
                self.state = State::DcsIntermediate;
            }
            0x30..=0x39 | 0x3B => {
                self.param(byte);
                self.state = State::DcsParam;
            }
            0x3A => self.state = State::DcsIgnore,
            0x3C..=0x3F => {
                self.collect(byte);
                self.state = State::DcsParam;
            }
            0x40..=0x7E => self.hook(performer, byte),
            _ => {}
        }
    }

    fn dcs_param<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x20..=0x2F => {
                self.collect(byte);
                self.state = State::DcsIntermediate;
            }
            0x30..=0x39 | 0x3B => self.param(byte),
            0x3A | 0x3C..=0x3F => self.state = State::DcsIgnore,
            0x40..=0x7E => self.hook(performer, byte),
            _ => {}
        }
    }

    fn dcs_intermediate<P: Perform>(&mut self, performer: &mut P, byte: u8) {
        match byte {
            0x20..=0x2F => self.collect(byte),
            0x30..=0x3F => self.state = State::DcsIgnore,
            0x40..=0x7E => self.hook(performer, byte),
            _ => {}
        }
    }
}
