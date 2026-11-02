//! Reed-Solomon GF(256) Multi-Block Interleaved Codec
//! 100% identical and compatible with Swift & JS implementations

pub struct ReedSolomon {
    exp_table: [u16; 512],
    log_table: [u16; 256],
}

// Global lazy or const tables
static RS_INSTANCE: std::sync::OnceLock<ReedSolomon> = std::sync::OnceLock::new();

impl ReedSolomon {
    const PRIMITIVE: usize = 0x11D;

    pub fn instance() -> &'static ReedSolomon {
        RS_INSTANCE.get_or_init(|| Self::init_tables())
    }

    fn init_tables() -> Self {
        let mut exp_table = [0u16; 512];
        let mut log_table = [0u16; 256];
        let mut x: usize = 1;

        for i in 0..255 {
            exp_table[i] = x as u16;
            log_table[x] = i as u16;
            x <<= 1;
            if (x & 0x100) != 0 {
                x ^= Self::PRIMITIVE;
            }
        }
        for i in 255..512 {
            exp_table[i] = exp_table[i - 255];
        }

        Self {
            exp_table,
            log_table,
        }
    }

    #[inline(always)]
    pub fn gf_mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            return 0;
        }
        let idx = self.log_table[a as usize] as usize + self.log_table[b as usize] as usize;
        self.exp_table[idx] as u8
    }

    #[inline(always)]
    pub fn gf_inv(&self, a: u8) -> u8 {
        if a == 0 {
            return 0;
        }
        let idx = 255 - self.log_table[a as usize] as usize;
        self.exp_table[idx] as u8
    }

    pub fn rs_generator_poly(&self, n_sym: usize) -> Vec<u8> {
        let mut g = vec![1u8];
        for i in 0..n_sym {
            let p = [1u8, self.exp_table[i] as u8];
            let mut ng = vec![0u8; g.len() + 1];
            for j in 0..g.len() {
                ng[j] ^= g[j];
                ng[j + 1] ^= self.gf_mul(g[j], p[1]);
            }
            g = ng;
        }
        g
    }

    pub fn rs_encode(&self, msg: &[u8], n_sym: usize) -> Vec<u8> {
        let gen = self.rs_generator_poly(n_sym);
        let mut out = Vec::with_capacity(msg.len() + n_sym);
        out.extend_from_slice(msg);
        out.resize(msg.len() + n_sym, 0);

        for i in 0..msg.len() {
            let coef = out[i];
            if coef != 0 {
                for j in 1..gen.len() {
                    out[i + j] ^= self.gf_mul(gen[j], coef);
                }
            }
        }

        let mut res = Vec::with_capacity(msg.len() + n_sym);
        res.extend_from_slice(msg);
        res.extend_from_slice(&out[msg.len()..]);
        res
    }

    pub fn calc_syndromes(&self, msg: &[u8], n_sym: usize) -> Vec<u8> {
        let mut syn = vec![0u8; n_sym];
        for i in 0..n_sym {
            let a = self.exp_table[i] as u8;
            let mut v = 0u8;
            for &b in msg {
                v = self.gf_mul(v, a) ^ b;
            }
            syn[i] = v;
        }
        syn
    }

    pub fn find_error_locator(&self, syn: &[u8], n_sym: usize) -> Vec<u8> {
        let mut c = vec![1u8];
        let mut b_poly = vec![1u8];
        let mut l = 0usize;
        let mut m = 1usize;
        let mut b = 1u8;

        for n in 0..n_sym {
            let mut d = syn[n];
            if l > 0 {
                for i in 1..=l {
                    if i < c.len() {
                        d ^= self.gf_mul(c[i], syn[n - i]);
                    }
                }
            }
            if d == 0 {
                m += 1;
            } else {
                let t = c.clone();
                let scale = self.gf_mul(d, self.gf_inv(b));
                let mut diff = vec![0u8; m];
                for &x in &b_poly {
                    diff.push(self.gf_mul(scale, x));
                }
                while c.len() < diff.len() {
                    c.push(0);
                }
                for i in 0..diff.len() {
                    c[i] ^= diff[i];
                }
                if 2 * l <= n {
                    l = n + 1 - l;
                    b_poly = t;
                    b = d;
                    m = 1;
                } else {
                    m += 1;
                }
            }
        }
        c
    }

    pub fn find_errors(&self, c: &[u8], msg_len: usize) -> Option<Vec<usize>> {
        let mut deg = 0usize;
        for i in (0..c.len()).rev() {
            if c[i] != 0 {
                deg = i;
                break;
            }
        }
        if deg == 0 {
            return Some(vec![]);
        }

        let mut err_pos = Vec::new();
        for i in 0..msg_len {
            let power = (255 - (i % 255)) % 255;
            let ai = self.exp_table[power] as u8;
            let mut val = 0u8;
            let mut term = 1u8;
            for j in 0..c.len() {
                val ^= self.gf_mul(c[j], term);
                term = self.gf_mul(term, ai);
            }
            if val == 0 {
                err_pos.push(msg_len - 1 - i);
            }
        }

        if err_pos.len() != deg {
            return None;
        }
        Some(err_pos)
    }

    pub fn solve_errors(&self, msg: &[u8], syn: &[u8], err_pos: &[usize]) -> Option<Vec<u8>> {
        let m = err_pos.len();
        if m == 0 {
            return Some(msg.to_vec());
        }

        let mut x = Vec::with_capacity(m);
        for &p in err_pos {
            let power = (msg.len() - 1 - p) % 255;
            x.push(self.exp_table[power] as u8);
        }

        let mut a = vec![vec![0u8; m + 1]; m];
        for i in 0..m {
            for k in 0..m {
                let p = (self.log_table[x[k] as usize] as usize * i) % 255;
                a[i][k] = self.exp_table[p] as u8;
            }
            a[i][m] = syn[i];
        }

        for col in 0..m {
            let mut pivot = None;
            for row in col..m {
                if a[row][col] != 0 {
                    pivot = Some(row);
                    break;
                }
            }
            let pivot = pivot?;
            if pivot != col {
                a.swap(col, pivot);
            }
            let inv = self.gf_inv(a[col][col]);
            for c in col..=m {
                a[col][c] = self.gf_mul(a[col][c], inv);
            }
            for row in 0..m {
                if row != col && a[row][col] != 0 {
                    let factor = a[row][col];
                    for c in col..=m {
                        let sub = self.gf_mul(factor, a[col][c]);
                        a[row][c] ^= sub;
                    }
                }
            }
        }

        let mut res = msg.to_vec();
        for (k, &p) in err_pos.iter().enumerate() {
            res[p] ^= a[k][m];
        }
        Some(res)
    }

    pub fn decode(&self, msg: &[u8], n_sym: usize) -> Option<Vec<u8>> {
        let syn = self.calc_syndromes(msg, n_sym);
        let has_error = syn.iter().any(|&x| x != 0);
        if !has_error {
            return Some(msg.to_vec());
        }

        let c = self.find_error_locator(&syn, n_sym);
        let err_pos = self.find_errors(&c, msg.len())?;
        let fixed = self.solve_errors(msg, &syn, &err_pos)?;

        let check_syn = self.calc_syndromes(&fixed, n_sym);
        if check_syn.iter().any(|&x| x != 0) {
            return None;
        }
        Some(fixed)
    }

    // MARK: - Block Interleaved RS (GF(256) Block size <= 180)

    pub fn encode_blocks(&self, data: &[u8], ecc_bytes: usize) -> Vec<u8> {
        let total = data.len() + ecc_bytes;
        let num_blocks = 1.max((total + 179) / 180);
        let ecc_per_block = ecc_bytes / num_blocks;

        let mut blocks_data = vec![Vec::new(); num_blocks];
        for bi in 0..num_blocks {
            let start = bi * data.len() / num_blocks;
            let end = (bi + 1) * data.len() / num_blocks;
            blocks_data[bi] = data[start..end].to_vec();
        }

        let mut encoded_blocks = vec![Vec::new(); num_blocks];
        for bi in 0..num_blocks {
            encoded_blocks[bi] = self.rs_encode(&blocks_data[bi], ecc_per_block);
        }

        let max_len = encoded_blocks.iter().map(|b| b.len()).max().unwrap_or(0);
        let mut interleaved = Vec::with_capacity(total);
        for col in 0..max_len {
            for bi in 0..num_blocks {
                if col < encoded_blocks[bi].len() {
                    interleaved.push(encoded_blocks[bi][col]);
                }
            }
        }
        interleaved
    }

    pub fn decode_blocks(&self, interleaved: &[u8], data_len: usize, ecc_bytes: usize) -> Option<Vec<u8>> {
        let total = data_len + ecc_bytes;
        let num_blocks = 1.max((total + 179) / 180);
        let ecc_per_block = ecc_bytes / num_blocks;

        let mut block_data_lens = vec![0usize; num_blocks];
        for bi in 0..num_blocks {
            let start = bi * data_len / num_blocks;
            let end = (bi + 1) * data_len / num_blocks;
            block_data_lens[bi] = end - start;
        }

        let block_total_lens: Vec<usize> = block_data_lens.iter().map(|&l| l + ecc_per_block).collect();
        let max_len = block_total_lens.iter().copied().max().unwrap_or(0);

        let mut deinterleaved = vec![Vec::new(); num_blocks];
        let mut idx = 0;
        for col in 0..max_len {
            for bi in 0..num_blocks {
                if col < block_total_lens[bi] && idx < interleaved.len() {
                    deinterleaved[bi].push(interleaved[idx]);
                    idx += 1;
                }
            }
        }

        let mut recovered = Vec::with_capacity(data_len);
        for bi in 0..num_blocks {
            let fixed = self.decode(&deinterleaved[bi], ecc_per_block)?;
            recovered.extend_from_slice(&fixed[..block_data_lens[bi]]);
        }
        Some(recovered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rs_single_block_error_correction() {
        let rs = ReedSolomon::instance();
        let msg = b"Hello Reed Solomon Test 1234567890!";
        let ecc = 16;
        let encoded = rs.rs_encode(msg, ecc);

        // Corrupt up to 8 bytes (since ecc=16, it can fix up to 8 byte errors)
        let mut corrupted = encoded.clone();
        corrupted[2] ^= 0xFF;
        corrupted[5] ^= 0x42;
        corrupted[10] ^= 0xAA;
        corrupted[15] ^= 0x55;
        corrupted[20] ^= 0x12;

        let decoded = rs.decode(&corrupted, ecc).expect("Decode should succeed");
        assert_eq!(&decoded[..msg.len()], msg);
    }

    #[test]
    fn test_rs_interleaved_blocks() {
        let rs = ReedSolomon::instance();
        let mut data = vec![0u8; 350];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i * 7 + 13) as u8;
        }
        let ecc = 100;
        let encoded = rs.encode_blocks(&data, ecc);
        // Encoded len is data.len() + num_blocks * (ecc / num_blocks)
        assert!(encoded.len() <= data.len() + ecc);
        assert!(encoded.len() >= data.len() + ecc - 5);

        let mut corrupted = encoded.clone();
        // Burst errors in consecutive positions (interleaving handles this cleanly)
        for i in 10..30 {
            corrupted[i] ^= 0x77;
        }

        let recovered = rs.decode_blocks(&corrupted, data.len(), ecc).expect("Recovery must succeed");
        assert_eq!(recovered, data);
    }
}
