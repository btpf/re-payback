//! `Sounds/*` — headerless signed 8-bit mono PCM (Paula format).
//!
//! The playback rate is not stored in the file; it is chosen by the engine per
//! sound (see `docs/formats.md`). Until recovered, callers supply a rate.

/// Convert signed 8-bit samples to f32 in [-1, 1).
pub fn pcm8_to_f32(data: &[u8]) -> Vec<f32> {
    data.iter().map(|&b| (b as i8) as f32 / 128.0).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn converts() {
        assert_eq!(
            super::pcm8_to_f32(&[0x00, 0x7f, 0x80]),
            vec![0.0, 127.0 / 128.0, -1.0]
        );
    }
}
