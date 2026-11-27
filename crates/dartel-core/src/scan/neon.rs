use std::arch::aarch64::{
    vandq_u8, vcgtq_s8, vcltq_s8, vdupq_n_s8, vget_lane_u64, vld1q_s8, vreinterpret_u64_u8,
    vreinterpretq_u16_u8, vshrn_n_u16,
};

use super::scalar;

const LANES: usize = 16;

#[inline]
pub(super) fn printable_run(bytes: &[u8]) -> usize {
    unsafe { run(bytes) }
}

#[inline]
#[target_feature(enable = "neon")]
fn run(bytes: &[u8]) -> usize {
    let (chunks, tail) = bytes.as_chunks::<LANES>();
    for (index, chunk) in chunks.iter().enumerate() {
        let lanes = unsafe { vld1q_s8(chunk.as_ptr().cast::<i8>()) };
        let rejected = !vget_lane_u64::<0>(vreinterpret_u64_u8(vshrn_n_u16::<4>(
            vreinterpretq_u16_u8(vandq_u8(
                vcgtq_s8(lanes, vdupq_n_s8(0x1F)),
                vcltq_s8(lanes, vdupq_n_s8(0x7F)),
            )),
        )));
        if rejected != 0 {
            return index * LANES + (rejected.trailing_zeros() / 4) as usize;
        }
    }
    chunks.len() * LANES + scalar::printable_run(tail)
}
