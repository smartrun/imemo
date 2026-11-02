//! CRC32 implementation matching IEEE 802.3 standard (identical to Swift & JS)

pub struct CRC32;

impl CRC32 {
    #[inline]
    pub fn checksum(data: &[u8]) -> u32 {
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(data);
        hasher.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_empty() {
        assert_eq!(CRC32::checksum(b""), 0);
    }

    #[test]
    fn test_crc32_standard() {
        assert_eq!(CRC32::checksum(b"123456789"), 0xCBF43926);
    }
}

