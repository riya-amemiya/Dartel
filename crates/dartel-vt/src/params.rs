pub const MAX_PARAMS: usize = 32;

#[derive(Clone, Copy, Debug, Default)]
pub struct Params {
    values: [u16; MAX_PARAMS],
    subs: u32,
    len: u8,
}

impl Params {
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        usize::from(self.len)
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    #[must_use]
    pub fn iter(&self) -> ParamsIter<'_> {
        ParamsIter {
            params: self,
            index: 0,
        }
    }

    #[inline]
    pub(super) fn clear(&mut self) {
        self.subs = 0;
        self.len = 0;
    }

    #[inline]
    pub(super) fn is_full(&self) -> bool {
        self.len() == MAX_PARAMS
    }

    #[inline]
    pub(super) fn push(&mut self, value: u16, sub: bool) {
        self.values[self.len()] = value;
        self.subs |= u32::from(sub) << self.len;
        self.len += 1;
    }
}

impl<'a> IntoIterator for &'a Params {
    type Item = &'a [u16];
    type IntoIter = ParamsIter<'a>;

    fn into_iter(self) -> ParamsIter<'a> {
        self.iter()
    }
}

#[derive(Clone, Debug)]
pub struct ParamsIter<'a> {
    params: &'a Params,
    index: usize,
}

impl<'a> Iterator for ParamsIter<'a> {
    type Item = &'a [u16];

    fn next(&mut self) -> Option<&'a [u16]> {
        if self.index >= self.params.len() {
            return None;
        }
        let start = self.index;
        self.index = start
            + 1
            + self
                .params
                .subs
                .checked_shr(start as u32 + 1)
                .unwrap_or(0)
                .trailing_ones() as usize;
        Some(&self.params.values[start..self.index])
    }
}
