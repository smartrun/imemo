//! XorShift32 PRNG keystream scrambler (identical to Swift & JS)

#[derive(Debug, Clone)]
pub struct Scrambler {
    state: u32,
}

impl Scrambler {
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed != 0 { seed } else { 0x12345678 },
        }
    }

    #[inline]
    pub fn next_byte(&mut self) -> u8 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        (self.state & 0xFF) as u8
    }

    pub fn scramble_slice(&mut self, data: &mut [u8]) {
        for b in data.iter_mut() {
            *b ^= self.next_byte();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrambler_roundtrip() {
        let original = b"Hello, iMemo Matrix Code!";
        let mut encrypted = original.to_vec();

        let mut enc_scrambler = Scrambler::new(0x494D434D);
        enc_scrambler.scramble_slice(&mut encrypted);

        assert_ne!(&encrypted[..], &original[..]);

        let mut dec_scrambler = Scrambler::new(0x494D434D);
        dec_scrambler.scramble_slice(&mut encrypted);

        assert_eq!(&encrypted[..], &original[..]);
    }
}

