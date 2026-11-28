use std::num::NonZero;
use std::ops::Range;
use std::sync::LazyLock;

pub use hc_internal::grep::matcher::Match;
use hc_internal::grep::matcher::Matcher;
use hc_internal::grep::regex::{Error, RegexMatcher, RegexMatcherBuilder};
use hc_internal::grep::searcher::split_chunks;
use hc_internal::walker::tree_threads;
use memchr::{memchr, memchr_iter};
use regex::bytes::Regex;

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Case {
    Sensitive,
    Insensitive,
    #[default]
    Smart,
}

#[derive(Clone, Debug)]
pub struct Query(RegexMatcher);

impl Query {
    pub fn literal(text: &str, case: Case) -> Result<Query, Error> {
        Query::build(text, case, true)
    }

    pub fn regex(pattern: &str, case: Case) -> Result<Query, Error> {
        Query::build(pattern, case, false)
    }

    fn build(pattern: &str, case: Case, fixed: bool) -> Result<Query, Error> {
        RegexMatcherBuilder::new()
            .fixed_strings(fixed)
            .case_insensitive(case == Case::Insensitive)
            .case_smart(case == Case::Smart)
            .multi_line(true)
            .line_terminator(Some(b'\n'))
            .build_many(&[pattern])
            .map(Query)
    }

    #[inline]
    #[must_use]
    pub fn find_at(&self, haystack: &[u8], at: usize) -> Option<Match> {
        (at <= haystack.len())
            .then(|| self.0.find_at(haystack, at).ok().flatten())
            .flatten()
    }
}

fn worker_count() -> usize {
    tree_threads(std::thread::available_parallelism().map_or(1, NonZero::get))
}

fn find_in(query: &Query, haystack: &[u8], range: Range<usize>, limit: usize) -> Vec<Match> {
    let mut found = Vec::new();
    let mut at = range.start;
    while found.len() < limit
        && let Some(hit) = query.find_at(&haystack[..range.end], at)
    {
        at = hit.end().max(hit.start() + 1);
        if !hit.is_empty() && memchr(b'\n', &haystack[hit.range()]).is_none() {
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
        split_chunks(haystack, b'\n', haystack.len().div_ceil(worker_count()))
            .into_iter()
            .map(|(start, end)| scope.spawn(move || find_in(query, haystack, start..end, limit)))
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|worker| worker.join().unwrap_or_default())
            .take(limit)
            .collect()
    })
}
