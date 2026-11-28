use std::borrow::Cow;
use std::fmt;

const MAX_DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq)]
pub enum Value<'a> {
    String(Cow<'a, str>),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Datetime(&'a str),
    Array(Vec<Value<'a>>),
    Table(Table<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Value,
    Implicit,
    Header,
    Dotted,
    Tables,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry<'a> {
    pub key: Cow<'a, str>,
    pub line: usize,
    pub value: Value<'a>,
    kind: Kind,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table<'a> {
    entries: Vec<Entry<'a>>,
}

impl<'a> Table<'a> {
    #[must_use]
    pub fn entries(&self) -> &[Entry<'a>] {
        &self.entries
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value<'a>> {
        self.find(key).map(|index| &self.entries[index].value)
    }

    fn find(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|entry| entry.key == key)
    }

    fn insert(&mut self, key: Cow<'a, str>, line: usize, value: Value<'a>, kind: Kind) -> usize {
        self.entries.push(Entry {
            key,
            line,
            value,
            kind,
        });
        self.entries.len() - 1
    }
}

struct Parser<'a> {
    input: &'a str,
    at: usize,
    line: usize,
}

fn failure<T>(line: usize, message: impl Into<String>) -> Result<T, Error> {
    Err(Error {
        line,
        message: message.into(),
    })
}

fn innermost<'t, 'a>(value: &'t mut Value<'a>) -> Option<&'t mut Table<'a>> {
    match value {
        Value::Table(table) => Some(table),
        Value::Array(items) => match items.last_mut() {
            Some(Value::Table(table)) => Some(table),
            _ => None,
        },
        _ => None,
    }
}

fn dotted<'t, 'a>(
    table: &'t mut Table<'a>,
    key: Cow<'a, str>,
    line: usize,
) -> Result<&'t mut Table<'a>, Error> {
    let index = table
        .find(&key)
        .unwrap_or_else(|| table.insert(key, line, Value::Table(Table::default()), Kind::Dotted));
    match &mut table.entries[index] {
        Entry {
            value: Value::Table(inner),
            kind: Kind::Dotted | Kind::Implicit,
            ..
        } => Ok(inner),
        Entry {
            key,
            kind: Kind::Header,
            ..
        } => failure(line, format!("table `{key}` is already defined")),
        Entry { key, .. } => failure(line, format!("key `{key}` is not a table")),
    }
}

fn pass<'t, 'a>(
    table: &'t mut Table<'a>,
    key: Cow<'a, str>,
    line: usize,
) -> Result<&'t mut Table<'a>, Error> {
    let index = table
        .find(&key)
        .unwrap_or_else(|| table.insert(key, line, Value::Table(Table::default()), Kind::Implicit));
    let entry = &mut table.entries[index];
    match (entry.kind, innermost(&mut entry.value)) {
        (Kind::Implicit | Kind::Header | Kind::Dotted | Kind::Tables, Some(inner)) => Ok(inner),
        _ => failure(line, format!("key `{}` is not a table", entry.key)),
    }
}

fn open<'t, 'a>(
    table: &'t mut Table<'a>,
    key: Cow<'a, str>,
    line: usize,
    array: bool,
) -> Result<&'t mut Table<'a>, Error> {
    let index = table.find(&key).unwrap_or_else(|| {
        if array {
            table.insert(key, line, Value::Array(Vec::new()), Kind::Tables)
        } else {
            table.insert(key, line, Value::Table(Table::default()), Kind::Implicit)
        }
    });
    let entry = &mut table.entries[index];
    match (&mut entry.value, entry.kind, array) {
        (Value::Table(inner), Kind::Implicit, false) => {
            entry.kind = Kind::Header;
            Ok(inner)
        }
        (Value::Array(items), Kind::Tables, true) => {
            items.push(Value::Table(Table::default()));
            match items.last_mut() {
                Some(Value::Table(inner)) => Ok(inner),
                _ => failure(line, format!("key `{}` is not a table", entry.key)),
            }
        }
        (Value::Table(_), Kind::Header | Kind::Dotted, false) => {
            failure(line, format!("table `{}` is defined twice", entry.key))
        }
        _ => failure(
            line,
            format!("key `{}` is already defined as another type", entry.key),
        ),
    }
}

fn shaped(text: &[u8], pattern: &[u8]) -> bool {
    text.len() == pattern.len()
        && text
            .iter()
            .zip(pattern)
            .all(|(byte, expected)| match expected {
                b'd' => byte.is_ascii_digit(),
                _ => byte == expected,
            })
}

fn is_date(text: &[u8]) -> bool {
    shaped(text, b"dddd-dd-dd")
}

fn is_datetime(token: &str) -> bool {
    let bytes = token.as_bytes();
    if is_date(bytes) {
        return true;
    }
    let (dated, clock) = match bytes.split_at_checked(11) {
        Some((head, clock)) if is_date(&head[..10]) && matches!(head[10], b'T' | b't' | b' ') => {
            (true, clock)
        }
        _ => (false, bytes),
    };
    let Some((time, tail)) = clock.split_at_checked(8) else {
        return false;
    };
    let zone = match tail.split_first() {
        Some((b'.', fraction)) => {
            let digits = fraction
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if digits == 0 {
                return false;
            }
            &fraction[digits..]
        }
        _ => tail,
    };
    shaped(time, b"dd:dd:dd")
        && (zone.is_empty()
            || (dated
                && (matches!(zone, b"Z" | b"z")
                    || shaped(zone, b"+dd:dd")
                    || shaped(zone, b"-dd:dd"))))
}

fn grouped(digits: &str, digit: impl Fn(u8) -> bool) -> bool {
    !digits.is_empty()
        && digits
            .split('_')
            .all(|group| !group.is_empty() && group.bytes().all(&digit))
}

fn plain(token: &str) -> Cow<'_, str> {
    if token.contains('_') {
        Cow::Owned(token.replace('_', ""))
    } else {
        Cow::Borrowed(token)
    }
}

fn radix(digits: &str, base: u32) -> Option<Value<'static>> {
    grouped(digits, |byte| char::from(byte).is_digit(base))
        .then(|| plain(digits))
        .and_then(|digits| i64::from_str_radix(&digits, base).ok())
        .map(Value::Integer)
}

fn number(token: &str) -> Option<Value<'static>> {
    for (prefix, base) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(digits) = token.strip_prefix(prefix) {
            return radix(digits, base);
        }
    }
    let unsigned = token.strip_prefix(['+', '-']).unwrap_or(token);
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (unsigned, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    let decimal = |digits: &str| grouped(digits, |byte| byte.is_ascii_digit());
    if !decimal(whole)
        || (whole.len() > 1 && whole.starts_with('0'))
        || fraction.is_some_and(|fraction| !decimal(fraction))
        || exponent
            .is_some_and(|exponent| !decimal(exponent.strip_prefix(['+', '-']).unwrap_or(exponent)))
    {
        return None;
    }
    if fraction.is_none() && exponent.is_none() {
        plain(token).parse().ok().map(Value::Integer)
    } else {
        plain(token).parse().ok().map(Value::Float)
    }
}

impl<'a> Parser<'a> {
    fn bytes(&self) -> &'a [u8] {
        self.input.as_bytes()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes().get(self.at).copied()
    }

    fn follows(&self, text: &str) -> bool {
        self.bytes()[self.at..].starts_with(text.as_bytes())
    }

    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        self.at += usize::from(found);
        found
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T, Error> {
        failure(self.line, message)
    }

    fn skip_blank(&mut self) {
        while let Some(b' ' | b'\t') = self.peek() {
            self.at += 1;
        }
    }

    fn newline(&mut self) -> bool {
        let width = match self.peek() {
            Some(b'\n') => 1,
            Some(b'\r') if self.follows("\r\n") => 2,
            _ => return false,
        };
        self.at += width;
        self.line += 1;
        true
    }

    fn comment(&mut self) -> Result<(), Error> {
        loop {
            match self.peek() {
                None | Some(b'\n') => return Ok(()),
                Some(b'\r') if self.follows("\r\n") => return Ok(()),
                Some(0..=8 | 10..=31 | 127) => {
                    return self.error("a comment contains a control character");
                }
                Some(_) => self.at += 1,
            }
        }
    }

    fn skip_filler(&mut self) -> Result<(), Error> {
        loop {
            self.skip_blank();
            if self.peek() == Some(b'#') {
                self.comment()?;
            } else if !self.newline() {
                return Ok(());
            }
        }
    }

    fn finish_line(&mut self) -> Result<(), Error> {
        self.skip_blank();
        if self.peek() == Some(b'#') {
            self.comment()?;
        }
        if self.newline() || self.peek().is_none() {
            Ok(())
        } else {
            self.error("expected the end of the line")
        }
    }

    fn key(&mut self) -> Result<Cow<'a, str>, Error> {
        match self.peek() {
            Some(b'"') => {
                self.at += 1;
                self.basic_line()
            }
            Some(b'\'') => {
                self.at += 1;
                self.literal_line()
            }
            _ => {
                let start = self.at;
                while let Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-') = self.peek()
                {
                    self.at += 1;
                }
                if self.at == start {
                    return self.error("expected a key");
                }
                Ok(Cow::Borrowed(&self.input[start..self.at]))
            }
        }
    }

    fn header<'t>(&mut self, root: &'t mut Table<'a>) -> Result<&'t mut Table<'a>, Error> {
        let line = self.line;
        self.at += 1;
        let array = self.eat(b'[');
        let mut table = root;
        loop {
            self.skip_blank();
            let key = self.key()?;
            self.skip_blank();
            if self.eat(b'.') {
                table = pass(table, key, line)?;
            } else if self.eat(b']') && (!array || self.eat(b']')) {
                return open(table, key, line, array);
            } else {
                return self.error("expected `]` after the table name");
            }
        }
    }

    fn pair(&mut self, table: &mut Table<'a>, depth: usize) -> Result<(), Error> {
        let line = self.line;
        let mut table = table;
        let mut key = self.key()?;
        loop {
            self.skip_blank();
            if !self.eat(b'.') {
                break;
            }
            self.skip_blank();
            table = dotted(table, key, line)?;
            key = self.key()?;
        }
        if !self.eat(b'=') {
            return self.error("expected `=` after the key");
        }
        if table.find(&key).is_some() {
            return failure(line, format!("key `{key}` is defined twice"));
        }
        self.skip_blank();
        let value = self.value(depth)?;
        table.insert(key, line, value, Kind::Value);
        Ok(())
    }

    fn value(&mut self, depth: usize) -> Result<Value<'a>, Error> {
        match self.peek() {
            Some(b'"') => self.basic().map(Value::String),
            Some(b'\'') => self.literal().map(Value::String),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.inline(depth),
            _ => self.scalar(),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value<'a>, Error> {
        if depth == MAX_DEPTH {
            return self.error("values are nested too deeply");
        }
        self.at += 1;
        let mut items = Vec::new();
        loop {
            self.skip_filler()?;
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            items.push(self.value(depth + 1)?);
            self.skip_filler()?;
            if !self.eat(b',') && self.peek() != Some(b']') {
                return self.error("expected `,` or `]` in an array");
            }
        }
    }

    fn inline(&mut self, depth: usize) -> Result<Value<'a>, Error> {
        if depth == MAX_DEPTH {
            return self.error("values are nested too deeply");
        }
        self.at += 1;
        let mut table = Table::default();
        self.skip_blank();
        if self.eat(b'}') {
            return Ok(Value::Table(table));
        }
        loop {
            self.skip_blank();
            self.pair(&mut table, depth + 1)?;
            self.skip_blank();
            if self.eat(b'}') {
                return Ok(Value::Table(table));
            }
            if !self.eat(b',') {
                return self.error("expected `,` or `}` in an inline table");
            }
        }
    }

    fn word(&mut self) {
        while let Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'-' | b'_' | b'.' | b':') =
            self.peek()
        {
            self.at += 1;
        }
    }

    fn scalar(&mut self) -> Result<Value<'a>, Error> {
        let start = self.at;
        self.word();
        if is_date(&self.bytes()[start..self.at])
            && self.peek() == Some(b' ')
            && self
                .bytes()
                .get(self.at + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            self.at += 1;
            self.word();
        }
        let token = &self.input[start..self.at];
        match token {
            "" => self.error("expected a value"),
            "true" => Ok(Value::Boolean(true)),
            "false" => Ok(Value::Boolean(false)),
            "inf" | "+inf" => Ok(Value::Float(f64::INFINITY)),
            "-inf" => Ok(Value::Float(f64::NEG_INFINITY)),
            "nan" | "+nan" | "-nan" => Ok(Value::Float(f64::NAN)),
            _ => number(token)
                .or_else(|| is_datetime(token).then_some(Value::Datetime(token)))
                .map_or_else(|| self.error(format!("`{token}` is not a valid value")), Ok),
        }
    }

    fn escape(&mut self) -> Result<char, Error> {
        let escaped = self.peek();
        self.at += 1;
        match escaped {
            Some(b'b') => Ok('\u{8}'),
            Some(b't') => Ok('\t'),
            Some(b'n') => Ok('\n'),
            Some(b'f') => Ok('\u{c}'),
            Some(b'r') => Ok('\r'),
            Some(b'"') => Ok('"'),
            Some(b'\\') => Ok('\\'),
            Some(b'u') => self.unicode(4),
            Some(b'U') => self.unicode(8),
            _ => self.error("an escape sequence is not valid"),
        }
    }

    fn unicode(&mut self, digits: usize) -> Result<char, Error> {
        let scalar = self
            .input
            .get(self.at..self.at + digits)
            .filter(|hex| hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .and_then(char::from_u32);
        self.at += digits;
        scalar.map_or_else(|| self.error("a unicode escape is not valid"), Ok)
    }

    fn basic(&mut self) -> Result<Cow<'a, str>, Error> {
        if self.follows("\"\"\"") {
            self.at += 3;
            self.multiline_basic()
        } else {
            self.at += 1;
            self.basic_line()
        }
    }

    fn literal(&mut self) -> Result<Cow<'a, str>, Error> {
        if self.follows("'''") {
            self.at += 3;
            self.multiline_literal()
        } else {
            self.at += 1;
            self.literal_line()
        }
    }

    fn basic_line(&mut self) -> Result<Cow<'a, str>, Error> {
        let mut owned: Option<String> = None;
        let mut from = self.at;
        loop {
            match self.peek() {
                Some(b'"') => {
                    let chunk = &self.input[from..self.at];
                    self.at += 1;
                    return Ok(owned.map_or(Cow::Borrowed(chunk), |text| Cow::Owned(text + chunk)));
                }
                Some(b'\\') => {
                    let chunk = &self.input[from..self.at];
                    self.at += 1;
                    let escaped = self.escape()?;
                    let text = owned.get_or_insert_default();
                    text.push_str(chunk);
                    text.push(escaped);
                    from = self.at;
                }
                None | Some(0..=8 | 10..=31 | 127) => {
                    return self.error("a string is not closed on its line");
                }
                Some(_) => self.at += 1,
            }
        }
    }

    fn literal_line(&mut self) -> Result<Cow<'a, str>, Error> {
        let start = self.at;
        loop {
            match self.peek() {
                Some(b'\'') => {
                    self.at += 1;
                    return Ok(Cow::Borrowed(&self.input[start..self.at - 1]));
                }
                None | Some(0..=8 | 10..=31 | 127) => {
                    return self.error("a string is not closed on its line");
                }
                Some(_) => self.at += 1,
            }
        }
    }

    fn closing(&mut self, quote: u8) -> usize {
        let extra = self.bytes()[self.at + 3..]
            .iter()
            .take(2)
            .take_while(|&&byte| byte == quote)
            .count();
        self.at += 3 + extra;
        self.at - 3
    }

    fn continuation(&mut self) -> bool {
        let rest = &self.bytes()[self.at..];
        let blank = rest
            .iter()
            .take_while(|&&byte| byte == b' ' || byte == b'\t')
            .count();
        if !rest[blank..].starts_with(b"\n") && !rest[blank..].starts_with(b"\r\n") {
            return false;
        }
        loop {
            self.skip_blank();
            if !self.newline() {
                return true;
            }
        }
    }

    fn multiline_basic(&mut self) -> Result<Cow<'a, str>, Error> {
        self.newline();
        let mut owned: Option<String> = None;
        let mut from = self.at;
        loop {
            match self.peek() {
                Some(b'"') if self.follows("\"\"\"") => {
                    let end = self.closing(b'"');
                    let chunk = &self.input[from..end];
                    return Ok(owned.map_or(Cow::Borrowed(chunk), |text| Cow::Owned(text + chunk)));
                }
                Some(b'\\') => {
                    let chunk = &self.input[from..self.at];
                    self.at += 1;
                    let escaped = if self.continuation() {
                        None
                    } else {
                        Some(self.escape()?)
                    };
                    let text = owned.get_or_insert_default();
                    text.push_str(chunk);
                    text.extend(escaped);
                    from = self.at;
                }
                Some(b'\n' | b'\r') if self.newline() => {}
                None => return self.error("a multi-line string is not closed"),
                Some(0..=8 | 10..=31 | 127) => {
                    return self.error("a string contains a control character");
                }
                Some(_) => self.at += 1,
            }
        }
    }

    fn multiline_literal(&mut self) -> Result<Cow<'a, str>, Error> {
        self.newline();
        let start = self.at;
        loop {
            match self.peek() {
                Some(b'\'') if self.follows("'''") => {
                    let end = self.closing(b'\'');
                    return Ok(Cow::Borrowed(&self.input[start..end]));
                }
                Some(b'\n' | b'\r') if self.newline() => {}
                None => return self.error("a multi-line string is not closed"),
                Some(0..=8 | 10..=31 | 127) => {
                    return self.error("a string contains a control character");
                }
                Some(_) => self.at += 1,
            }
        }
    }
}

pub fn parse(input: &str) -> Result<Table<'_>, Error> {
    let mut parser = Parser {
        input,
        at: 0,
        line: 1,
    };
    let mut root = Table::default();
    let mut current = &mut root;
    loop {
        parser.skip_blank();
        match parser.peek() {
            None => return Ok(root),
            Some(b'[') => current = parser.header(&mut root)?,
            Some(b'\n' | b'\r' | b'#') => {}
            Some(_) => parser.pair(current, 0)?,
        }
        parser.finish_line()?;
    }
}
