//! Non-standard Matrix Code (IMC) Decoder
//! 100% compatible with Swift MatrixDecoder and web/index.html

use crate::reed_solomon::ReedSolomon;
use crate::scrambler::Scrambler;

#[derive(Debug, Clone)]
pub struct DecodedFrame {
    pub doc_id: u16,
    pub frame_seq: usize,
    pub total_frames: usize,
    pub data_frame_count: usize,
    pub payload_len: usize,
    pub doc_crc32: u32,
    pub payload: Vec<u8>,
    pub color_mode: usize,
    pub width_cells: usize,
    pub height_cells: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub min_x: usize,
    pub min_y: usize,
    pub max_x: usize,
    pub max_y: usize,
}

impl BoundingBox {
    pub fn width(&self) -> usize {
        self.max_x - self.min_x + 1
    }
    pub fn height(&self) -> usize {
        self.max_y - self.min_y + 1
    }
}

pub struct MatrixDecoder;

impl MatrixDecoder {
    pub const STANDARD_PALETTES: [&'static [[f64; 3]]; 4] = [
        // Mode 0: 2 colors
        &[[0.0, 0.0, 0.0], [255.0, 255.0, 255.0]],
        // Mode 1: 4 colors
        &[
            [0.0, 0.0, 0.0],
            [0.0, 85.0, 255.0],
            [255.0, 221.0, 0.0],
            [255.0, 255.0, 255.0],
        ],
        // Mode 2: 8 colors
        &[
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 255.0],
            [0.0, 255.0, 0.0],
            [0.0, 255.0, 255.0],
            [255.0, 0.0, 0.0],
            [255.0, 0.0, 255.0],
            [255.0, 255.0, 0.0],
            [255.0, 255.0, 255.0],
        ],
        // Mode 3: 16 colors
        &[
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 128.0],
            [0.0, 128.0, 0.0],
            [0.0, 128.0, 128.0],
            [128.0, 0.0, 0.0],
            [128.0, 0.0, 128.0],
            [128.0, 128.0, 0.0],
            [192.0, 192.0, 192.0],
            [128.0, 128.0, 128.0],
            [0.0, 0.0, 255.0],
            [0.0, 255.0, 0.0],
            [0.0, 255.0, 255.0],
            [255.0, 0.0, 0.0],
            [255.0, 0.0, 255.0],
            [255.0, 255.0, 0.0],
            [255.0, 255.0, 255.0],
        ],
    ];

    pub fn decode_rgba_image(data: &[u8], width: usize, height: usize) -> Option<DecodedFrame> {
        if width < 50 || height < 20 || data.len() < width * height * 4 {
            return None;
        }

        // 1. Find outer dark perimeter inside quiet zone
        let bounds = Self::find_barcode_bounds(data, width, height)?;

        // 2. Try decoding with estimated dimensions and each color mode
        for mode in 0..4 {
            if let Some(frame) = Self::try_decode_with_bounds(data, width, height, bounds, mode) {
                return Some(frame);
            }
        }
        None
    }

    pub fn find_barcode_bounds(data: &[u8], width: usize, height: usize) -> Option<BoundingBox> {
        let mut min_x = width;
        let mut max_x = 0;
        let mut min_y = height;
        let mut max_y = 0;

        let get_luminance = |x: usize, y: usize| -> f64 {
            let offset = (y * width + x) * 4;
            let r = data[offset] as f64;
            let g = data[offset + 1] as f64;
            let b = data[offset + 2] as f64;
            0.299 * r + 0.587 * g + 0.114 * b
        };

        let mut found_black = false;
        // Step 2 for fast sampling
        for y in (2..(height.saturating_sub(2))).step_by(2) {
            for x in (2..(width.saturating_sub(2))).step_by(2) {
                if get_luminance(x, y) < 80.0 {
                    found_black = true;
                    if x < min_x { min_x = x; }
                    if x > max_x { max_x = x; }
                    if y < min_y { min_y = y; }
                    if y > max_y { max_y = y; }
                }
            }
        }

        if !found_black || (max_x <= min_x) || (max_y <= min_y) {
            return None;
        }
        if (max_x - min_x < 80) || (max_y - min_y < 20) {
            return None;
        }

        Some(BoundingBox {
            min_x,
            min_y,
            max_x: max_x.min(width - 1),
            max_y: max_y.min(height - 1),
        })
    }

    fn try_decode_with_bounds(
        data: &[u8],
        width: usize,
        height: usize,
        bounds: BoundingBox,
        forced_mode: usize,
    ) -> Option<DecodedFrame> {
        let h_cells = 28usize;
        let cell_h = bounds.height() as f64 / h_cells as f64;
        if cell_h < 2.0 {
            return None;
        }

        let estimated_w = (bounds.width() as f64 / cell_h).round() as isize;
        if estimated_w < 30 || estimated_w > 400 {
            return None;
        }

        for delta in [0isize, -1, 1, -2, 2] {
            let w_cells = estimated_w + delta;
            if w_cells < 30 {
                continue;
            }
            if let Some(frame) = Self::sample_and_decode(
                data,
                width,
                height,
                bounds,
                w_cells as usize,
                h_cells,
                forced_mode,
            ) {
                return Some(frame);
            }
        }
        None
    }

    fn sample_and_decode(
        data: &[u8],
        width: usize,
        height: usize,
        bounds: BoundingBox,
        w_cells: usize,
        h_cells: usize,
        color_mode: usize,
    ) -> Option<DecodedFrame> {
        let cell_w = bounds.width() as f64 / w_cells as f64;
        let cell_h = bounds.height() as f64 / h_cells as f64;

        let sample_rgb = |col: usize, row: usize| -> [f64; 3] {
            let cx = bounds.min_x as f64 + (col as f64 + 0.5) * cell_w;
            let cy = bounds.min_y as f64 + (row as f64 + 0.5) * cell_h;
            let ix = (cx as usize).min(width - 1);
            let iy = (cy as usize).min(height - 1);
            let offset = (iy * width + ix) * 4;
            [
                data[offset] as f64,
                data[offset + 1] as f64,
                data[offset + 2] as f64,
            ]
        };

        // Read Calibration Palette from Row 3 (cols 8 .. 8 + K - 1)
        let num_colors = [2, 4, 8, 16][color_mode];
        let mut palette = vec![[0.0, 0.0, 0.0]; num_colors];
        let std_pal = Self::STANDARD_PALETTES[color_mode];

        for i in 0..num_colors {
            let col = 8 + i;
            if col < w_cells.saturating_sub(7) {
                let [r1, g1, b1] = sample_rgb(col, 3);
                let [r2, g2, b2] = sample_rgb(col, h_cells - 4);
                palette[i] = [(r1 + r2) / 2.0, (g1 + g2) / 2.0, (b1 + b2) / 2.0];
            } else {
                palette[i] = std_pal[i];
            }
        }

        // Determine reserved cells
        let mut is_reserved = vec![false; w_cells * h_cells];
        let mut mark_reserved = |r: usize, c: usize| {
            if r < h_cells && c < w_cells {
                is_reserved[r * w_cells + c] = true;
            }
        };

        // 1. Four 7x7 finders
        for r in 0..7 {
            for c in 0..7 {
                mark_reserved(r, c);
                mark_reserved(r, w_cells - 7 + c);
                mark_reserved(h_cells - 7 + r, c);
                mark_reserved(h_cells - 7 + r, w_cells - 7 + c);
            }
        }
        // 2. Borders
        for c in 0..w_cells {
            mark_reserved(0, c);
            mark_reserved(h_cells - 1, c);
        }
        for r in 0..h_cells {
            mark_reserved(r, 0);
            mark_reserved(r, w_cells - 1);
        }
        // 3. Timing tracks
        for c in 7..(w_cells.saturating_sub(7)) {
            mark_reserved(1, c);
            mark_reserved(h_cells - 2, c);
        }
        for r in 7..(h_cells.saturating_sub(7)) {
            mark_reserved(r, 1);
            mark_reserved(r, w_cells - 2);
        }
        // 4. Calibration Bars
        for i in 0..num_colors {
            mark_reserved(3, 8 + i);
            mark_reserved(h_cells - 4, 8 + i);
        }
        // 5. Format Metadata
        for i in 0..8 {
            mark_reserved(4, 8 + i);
            mark_reserved(h_cells - 5, 8 + i);
        }

        // Collect data cell coordinates in serpentine order
        let mut data_cells = Vec::new();
        for r in 2..(h_cells.saturating_sub(2)) {
            let is_even = (r % 2) == 0;
            for ci in 2..(w_cells.saturating_sub(2)) {
                let c = if is_even { ci } else { w_cells - 1 - ci };
                if !is_reserved[r * w_cells + c] {
                    data_cells.push((r, c));
                }
            }
        }

        let bits_per_cell = [1, 2, 3, 4][color_mode];
        let total_bits = data_cells.len() * bits_per_cell;
        let total_bytes = total_bits / 8;
        if total_bytes < 24 {
            return None;
        }

        // Classify each data cell into nearest palette index
        let mut symbols = Vec::with_capacity(data_cells.len());
        for &(r, c) in &data_cells {
            let [sr, sg, sb] = sample_rgb(c, r);
            let mut best_idx = 0;
            let mut best_dist = f64::INFINITY;
            for (i, p) in palette.iter().enumerate() {
                let dr = sr - p[0];
                let dg = sg - p[1];
                let db = sb - p[2];
                let dist = dr * dr + dg * dg + db * db;
                if dist < best_dist {
                    best_dist = dist;
                    best_idx = i;
                }
            }
            symbols.push(best_idx);
        }

        // Unpack symbols to bytes
        let mut raw_bytes = vec![0u8; total_bytes];
        let mut byte_idx = 0;
        let mut bit_offset: i8 = 7;

        match bits_per_cell {
            1 => {
                for sym in symbols {
                    if byte_idx >= total_bytes { break; }
                    if sym == 1 {
                        raw_bytes[byte_idx] |= 1 << bit_offset;
                    }
                    bit_offset -= 1;
                    if bit_offset < 0 {
                        bit_offset = 7;
                        byte_idx += 1;
                    }
                }
            }
            2 => {
                for sym in symbols {
                    if byte_idx >= total_bytes { break; }
                    raw_bytes[byte_idx] |= ((sym as u8) & 0x03) << (bit_offset - 1);
                    bit_offset -= 2;
                    if bit_offset < 0 {
                        bit_offset = 7;
                        byte_idx += 1;
                    }
                }
            }
            3 => {
                for sym in symbols {
                    for bit in 0..3 {
                        if byte_idx >= total_bytes { break; }
                        let b = ((sym >> (2 - bit)) & 1) as u8;
                        if b == 1 {
                            raw_bytes[byte_idx] |= 1 << bit_offset;
                        }
                        bit_offset -= 1;
                        if bit_offset < 0 {
                            bit_offset = 7;
                            byte_idx += 1;
                        }
                    }
                }
            }
            4 => {
                for sym in symbols {
                    if byte_idx >= total_bytes { break; }
                    raw_bytes[byte_idx] |= ((sym as u8) & 0x0F) << (bit_offset - 3);
                    bit_offset -= 4;
                    if bit_offset < 0 {
                        bit_offset = 7;
                        byte_idx += 1;
                    }
                }
            }
            _ => return None,
        }

        // Unscramble all bytes with fixed non-standard keystream (0x494D434D)
        let mut scrambler = Scrambler::new(0x494D434D);
        let mut unscrambled = vec![0u8; total_bytes];
        for i in 0..total_bytes {
            unscrambled[i] = raw_bytes[i] ^ scrambler.next_byte();
        }

        // Multi-block Interleaved Reed-Solomon Decode
        let header_size = 16usize;
        let ecc_size = 10.max((total_bytes as f64 * 0.30) as usize);
        if total_bytes <= ecc_size + header_size {
            return None;
        }
        let unencoded_len = total_bytes - ecc_size;

        let rs = ReedSolomon::instance();
        let fixed = rs.decode_blocks(&unscrambled, unencoded_len, ecc_size)?;

        // Validate Header (16 bytes)
        // Byte 0..1: 'IM'
        if fixed.len() < header_size || fixed[0] != 0x49 || fixed[1] != 0x4D {
            return None;
        }

        let data_frame_count = fixed[2] as usize;
        let doc_id = ((fixed[6] as u16) << 8) | (fixed[7] as u16);
        let frame_seq = fixed[8] as usize;
        let total_frames = fixed[9] as usize;
        let payload_len = ((fixed[10] as usize) << 8) | (fixed[11] as usize);
        let doc_crc32 = ((fixed[12] as u32) << 24)
            | ((fixed[13] as u32) << 16)
            | ((fixed[14] as u32) << 8)
            | (fixed[15] as u32);

        if payload_len == 0 || (header_size + payload_len) > unencoded_len {
            return None;
        }

        let payload = fixed[header_size..(header_size + payload_len)].to_vec();

        Some(DecodedFrame {
            doc_id,
            frame_seq,
            total_frames,
            data_frame_count: if data_frame_count > 0 { data_frame_count } else { total_frames },
            payload_len,
            doc_crc32,
            payload,
            color_mode,
            width_cells: w_cells,
            height_cells: h_cells,
        })
    }
}

