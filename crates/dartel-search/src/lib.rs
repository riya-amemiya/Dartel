use std::num::NonZero;
use std::ops::Range;
use std::sync::LazyLock;

use memchr::memmem::Finder;
use memchr::{memchr, memchr_iter};
use regex::bytes::{Regex, RegexBuilder};

const PARALLEL_THRESHOLD: usize = 1 << 20;

static URL: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r#"(?:https?|ftp|file)://[^\s<>"'`\\^{}|]+"#).ok());

fn trimmed(url: &[u8]) -> usize {
    let mut end = url.len();
    loop {
        let count = |wanted: u8| memchr_iter(wanted, &url[..end]).count();
        match url[..end].last() {
            Some(b'.' | b',' | b';' | b':' | b'!' | b'?') => end -= 1,
            Some(b')') if count(b')') > count(b'(') => end -= 1,
            Some(b']') if count(b']') > count(b'[') => end -= 1,
            _ => return end,
        }
    }
}

#[must_use]
pub fn url_at(text: &[u8], offset: usize) -> Option<Range<usize>> {
    URL.as_ref()?
        .find_iter(text)
        .map(|found| found.start()..found.start() + trimmed(found.as_bytes()))
        .find(|range| range.contains(&offset))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    start: usize,
    end: usize,
}

impl Match {
    #[inline]
    #[must_use]
    pub const fn start(&self) -> usize {
        self.start
    }

    #[inline]
    #[must_use]
    pub const fn end(&self) -> usize {
        self.end
    }

    #[inline]
    #[must_use]
    pub const fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Case {
    Sensitive,
    Insensitive,
    #[default]
    Smart,
}

#[derive(Clone, Debug)]
enum Engine {
    Literal(Box<Finder<'static>>),
    Pattern(Regex),
}

#[derive(Clone, Debug)]
pub struct Query(Engine);

fn folds(mut chars: impl Iterator<Item = char>, case: Case) -> bool {
    match case {
        Case::Sensitive => false,
        Case::Insensitive => true,
        Case::Smart => !chars.any(char::is_uppercase),
    }
}

fn unescaped(pattern: &str) -> impl Iterator<Item = char> + '_ {
    let mut escaped = false;
    pattern.chars().filter(move |&c| {
        let keep = !escaped && c != '\\';
        escaped = !escaped && c == '\\';
        keep
    })
}

fn is_meta(c: char) -> bool {
    matches!(
        c,
        '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
    )
}

impl Query {
    pub fn literal(text: &str, case: Case) -> Result<Query, regex::Error> {
        if folds(text.chars(), case) {
            Query::pattern(&regex::escape(text), true)
        } else {
            Ok(Query(Engine::Literal(Box::new(
                Finder::new(text).into_owned(),
            ))))
        }
    }

    pub fn regex(pattern: &str, case: Case) -> Result<Query, regex::Error> {
        if folds(unescaped(pattern), case) || pattern.chars().any(is_meta) {
            Query::pattern(pattern, folds(unescaped(pattern), case))
        } else {
            Query::literal(pattern, Case::Sensitive)
        }
    }

    fn pattern(pattern: &str, fold: bool) -> Result<Query, regex::Error> {
        RegexBuilder::new(pattern)
            .case_insensitive(fold)
            .multi_line(true)
            .build()
            .map(|regex| Query(Engine::Pattern(regex)))
    }

    #[inline]
    #[must_use]
    pub fn find_at(&self, haystack: &[u8], at: usize) -> Option<Match> {
        match &self.0 {
            Engine::Literal(finder) => finder.find(haystack.get(at..)?).map(|offset| Match {
                start: at + offset,
                end: at + offset + finder.needle().len(),
            }),
            Engine::Pattern(regex) => (at <= haystack.len())
                .then(|| regex.find_at(haystack, at))
                .flatten()
                .map(|found| Match {
                    start: found.start(),
                    end: found.end(),
                }),
        }
    }
}

#[cfg(target_os = "macos")]
fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map_or(1, NonZero::get)
        .min(6)
}

#[cfg(not(target_os = "macos"))]
fn worker_count() -> usize {
    std::thread::available_parallelism().map_or(1, NonZero::get)
}

fn split_chunks(haystack: &[u8], chunk_size: usize) -> Vec<Range<usize>> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < haystack.len() {
        let end = haystack
            .get(start + chunk_size..)
            .and_then(|rest| memchr(b'\n', rest))
            .map_or(haystack.len(), |offset| start + chunk_size + offset + 1);
        chunks.push(start..end);
        start = end;
    }
    chunks
}

fn find_in(query: &Query, haystack: &[u8], range: Range<usize>, limit: usize) -> Vec<Match> {
    let mut found = Vec::new();
    let mut at = range.start;
    while found.len() < limit
        && let Some(hit) = query.find_at(&haystack[..range.end], at)
    {
        at = hit.end.max(hit.start + 1);
        if hit.start < hit.end && memchr(b'\n', &haystack[hit.range()]).is_none() {
            found.push(hit);
        }
    }
    found
}

#[must_use]
pub fn find_all(query: &Query, haystack: &[u8], limit: usize) -> Vec<Match> {
    if worker_count() < 2 || haystack.len() < PARALLEL_THRESHOLD {
        return find_in(query, haystack, 0..haystack.len(), limit);
    }
    std::thread::scope(|scope| {
        split_chunks(haystack, haystack.len().div_ceil(worker_count()))
            .into_iter()
            .map(|range| scope.spawn(move || find_in(query, haystack, range, limit)))
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .take(limit)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::split_chunks;

    #[test]
    fn chunks_end_on_line_terminators_and_cover_the_haystack() {
        let haystack = b"alpha\nbeta\ngamma\ndelta";
        assert_eq!(split_chunks(haystack, 7), vec![0..11, 11..22]);
        assert_eq!(split_chunks(haystack, 1), vec![0..6, 6..11, 11..17, 17..22]);
        assert_eq!(split_chunks(haystack, 100), vec![0..22]);
        assert_eq!(split_chunks(b"", 4), Vec::<std::ops::Range<usize>>::new());
    }
}
