//! Ogg's CRC-32, below the app (moved from `opus_copy`, which keeps its name): the served files' ETags are made with it
//! too (`serve::files`).

/// Ogg's CRC-32: polynomial 0x04C11DB7, not reflected, starting at 0.
pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, e) in t.iter_mut().enumerate() {
            let mut r = (i as u32) << 24;
            for _ in 0..8 {
                r = if r & 0x8000_0000 != 0 { (r << 1) ^ 0x04C1_1DB7 } else { r << 1 };
            }
            *e = r;
        }
        t
    });
    data.iter().fold(0u32, |crc, &b| (crc << 8) ^ table[((crc >> 24) as u8 ^ b) as usize])
}
