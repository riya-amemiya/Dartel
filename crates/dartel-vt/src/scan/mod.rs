#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
mod neon;
mod scalar;
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
mod sse2;

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
use neon as imp;
#[cfg(not(any(
    all(target_arch = "aarch64", target_feature = "neon"),
    all(target_arch = "x86_64", target_feature = "sse2")
)))]
use scalar as imp;
#[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
use sse2 as imp;

#[inline]
#[must_use]
pub fn printable_run(bytes: &[u8]) -> usize {
    imp::printable_run(bytes)
}
