#[inline]
pub(super) fn printable_run(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|byte| !(0x20..0x7F).contains(byte))
        .unwrap_or(bytes.len())
}
