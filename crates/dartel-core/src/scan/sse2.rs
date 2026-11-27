use std::arch::x86_64::{
    __m128i, _mm_and_si128, _mm_cmpgt_epi8, _mm_cmplt_epi8, _mm_movemask_epi8, _mm_set1_epi8,
};

use super::scalar;

const LANES: usize = 16;

#[inline]
pub(super) fn printable_run(bytes: &[u8]) -> usize {
    unsafe { run(bytes) }
}

#[inline]
#[target_feature(enable = "sse2")]
fn run(bytes: &[u8]) -> usize {
    let (chunks, tail) = bytes.as_chunks::<LANES>();
    for (index, chunk) in chunks.iter().enumerate() {
        let lanes = unsafe { std::mem::transmute::<[u8; LANES], __m128i>(*chunk) };
        let rejected = !_mm_movemask_epi8(_mm_and_si128(
            _mm_cmpgt_epi8(lanes, _mm_set1_epi8(0x1F)),
            _mm_cmplt_epi8(lanes, _mm_set1_epi8(0x7F)),
        )) & 0xFFFF;
        if rejected != 0 {
            return index * LANES + rejected.trailing_zeros() as usize;
        }
    }
    chunks.len() * LANES + scalar::printable_run(tail)
}
