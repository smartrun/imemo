//! End-to-End IMC Barcode Generation, Image Rendering, and MatrixDecoder Roundtrip Test

use flate2::write::DeflateEncoder;
use flate2::Compression;
use imemo_reader_win::crc32::CRC32;
use imemo_reader_win::matrix_decoder::MatrixDecoder;
use imemo_reader_win::reed_solomon::ReedSolomon;
use imemo_reader_win::scrambler::Scrambler;
use imemo_reader_win::slide_collector::SlideCollector;
use std::io::Write;

struct TestGrid {
    w: usize,
    h: usize,
    mode: usize,
    bits_per_cell: usize,
    cells: Vec<i16>,
    reserved: Vec<bool>,
}

impl TestGrid {
    fn new(w: usize, h: usize, mode: usize) -> Self {
        let bpc = [1, 2, 3, 4][mode];
        let mut grid = Self {
            w,
            h,
            mode,
            bits_per_cell: bpc,
            cells: vec![-1; w * h],
            reserved: vec![false; w * h],
        };
        grid.init_structure();
        grid
    }

    fn set_reserved(&mut self, r: usize, c: usize, val: i16) {
        if r < self.h && c < self.w {
            self.cells[r * self.w + c] = val;
            self.reserved[r * self.w + c] = true;
        }
    }

    fn draw_finder(&mut self, start_r: usize, start_c: usize, finder_type: &str) {
        for r in 0..7 {
            for c in 0..7 {
                let is_outer = r == 0 || r == 6 || c == 0 || c == 6;
                let is_inner = r >= 2 && r <= 4 && c >= 2 && c <= 4;
                let mut val = 0;
                if !is_outer && !is_inner {
                    val = 1;
                } else if is_inner {
                    match finder_type {
                        "TL" => val = 0,
                        "TR" => val = if r == 3 && c == 3 { 1 } else { 0 },
                        "BL" => val = if r >= 3 && c >= 3 { 1 } else { 0 },
                        "BR" => val = 1,
                        _ => {}
                    }
                }
                self.set_reserved(start_r + r, start_c + c, val);
            }
        }
    }

    fn init_structure(&mut self) {
        let w = self.w;
        let h = self.h;

        // 1. Four 7x7 Finders
        self.draw_finder(0, 0, "TL");
        self.draw_finder(0, w - 7, "TR");
        self.draw_finder(h - 7, 0, "BL");
        self.draw_finder(h - 7, w - 7, "BR");

        // 2. Borders (Black = 0)
        for c in 0..w {
            self.set_reserved(0, c, 0);
            self.set_reserved(h - 1, c, 0);
        }
        for r in 0..h {
            self.set_reserved(r, 0, 0);
            self.set_reserved(r, w - 1, 0);
        }

        // 3. Timing Tracks
        for c in 7..(w - 7) {
            let bit = if c % 2 == 0 { 0 } else { 1 };
            self.set_reserved(1, c, bit);
            self.set_reserved(h - 2, c, bit);
        }
        for r in 7..(h - 7) {
            let bit = if r % 2 == 0 { 0 } else { 1 };
            self.set_reserved(r, 1, bit);
            self.set_reserved(r, w - 2, bit);
        }

        // 4. Calibration Palette Bars
        let num_colors = [2, 4, 8, 16][self.mode];
        for i in 0..num_colors {
            self.set_reserved(3, 8 + i, i as i16);
            self.set_reserved(h - 4, 8 + i, i as i16);
        }

        // 5. Format Metadata
        for i in 0..8 {
            self.set_reserved(4, 8 + i, 0);
            self.set_reserved(h - 5, 8 + i, 0);
        }
    }

    fn get_data_cells(&self) -> Vec<(usize, usize)> {
        let mut list = Vec::new();
        for r in 2..(self.h - 2) {
            let is_even = (r % 2) == 0;
            for ci in 2..(self.w - 2) {
                let c = if is_even { ci } else { self.w - 1 - ci };
                if !self.reserved[r * self.w + c] {
                    list.push((r, c));
                }
            }
        }
        list
    }
}

fn encode_html_to_image(html: &str, color_mode: usize, width_px: usize, cell_size: usize) -> (Vec<u8>, usize, usize) {
    let width_cells = width_px / cell_size;
    let height_cells = 28;

    let mut grid = TestGrid::new(width_cells, height_cells, color_mode);
    let data_cells = grid.get_data_cells();

    let total_cell_bits = data_cells.len() * grid.bits_per_cell;
    let total_frame_bytes = total_cell_bits / 8;

    // Compress
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(html.as_bytes()).unwrap();
    let compressed_bytes = encoder.finish().unwrap();

    let doc_crc = CRC32::checksum(&compressed_bytes);
    let doc_id: u16 = 0xABCD;

    let header_size = 16;
    let ecc_size = 10.max((total_frame_bytes as f64 * 0.30) as usize);
    let unencoded_len = total_frame_bytes - ecc_size;

    let mut unencoded_buf = vec![0u8; unencoded_len];
    unencoded_buf[0] = 0x49; // 'I'
    unencoded_buf[1] = 0x4D; // 'M'
    unencoded_buf[2] = 1;    // 1 data frame
    unencoded_buf[3] = color_mode as u8;
    unencoded_buf[4] = (width_cells & 0xFF) as u8;
    unencoded_buf[5] = (height_cells & 0xFF) as u8;
    unencoded_buf[6] = (doc_id >> 8) as u8;
    unencoded_buf[7] = (doc_id & 0xFF) as u8;
    unencoded_buf[8] = 0;    // frameSeq = 0
    unencoded_buf[9] = 1;    // totalFrames = 1
    unencoded_buf[10] = ((compressed_bytes.len() >> 8) & 0xFF) as u8;
    unencoded_buf[11] = (compressed_bytes.len() & 0xFF) as u8;
    unencoded_buf[12] = ((doc_crc >> 24) & 0xFF) as u8;
    unencoded_buf[13] = ((doc_crc >> 16) & 0xFF) as u8;
    unencoded_buf[14] = ((doc_crc >> 8) & 0xFF) as u8;
    unencoded_buf[15] = (doc_crc & 0xFF) as u8;

    unencoded_buf[header_size..(header_size + compressed_bytes.len())].copy_from_slice(&compressed_bytes);

    // RS Encode
    let rs = ReedSolomon::instance();
    let encoded = rs.encode_blocks(&unencoded_buf, ecc_size);

    let mut frame_buf = vec![0u8; total_frame_bytes];
    let copy_len = encoded.len().min(total_frame_bytes);
    frame_buf[..copy_len].copy_from_slice(&encoded[..copy_len]);

    // Scramble
    let mut scrambler = Scrambler::new(0x494D434D);
    for b in frame_buf.iter_mut() {
        *b ^= scrambler.next_byte();
    }

    // Populate data cells
    let bpc = grid.bits_per_cell;
    let mut byte_idx = 0;
    let mut bit_offset: i8 = 7;

    for &(r, c) in &data_cells {
        let mut sym_val = 0i16;
        if bpc == 1 {
            if byte_idx < total_frame_bytes {
                sym_val = ((frame_buf[byte_idx] >> bit_offset) & 1) as i16;
                bit_offset -= 1;
                if bit_offset < 0 { bit_offset = 7; byte_idx += 1; }
            }
        } else if bpc == 2 {
            if byte_idx < total_frame_bytes {
                sym_val = ((frame_buf[byte_idx] >> (bit_offset - 1)) & 0x03) as i16;
                bit_offset -= 2;
                if bit_offset < 0 { bit_offset = 7; byte_idx += 1; }
            }
        } else if bpc == 3 {
            for _ in 0..3 {
                if byte_idx < total_frame_bytes {
                    let bit = ((frame_buf[byte_idx] >> bit_offset) & 1) as i16;
                    sym_val = (sym_val << 1) | bit;
                    bit_offset -= 1;
                    if bit_offset < 0 { bit_offset = 7; byte_idx += 1; }
                }
            }
        } else if bpc == 4 {
            if byte_idx < total_frame_bytes {
                sym_val = ((frame_buf[byte_idx] >> (bit_offset - 3)) & 0x0F) as i16;
                bit_offset -= 4;
                if bit_offset < 0 { bit_offset = 7; byte_idx += 1; }
            }
        }
        grid.cells[r * width_cells + c] = sym_val;
    }

    // Render RGBA image with quiet zone (margin = 24px)
    let margin = 24;
    let img_w = width_cells * cell_size + margin * 2;
    let img_h = height_cells * cell_size + margin * 2;
    let mut rgba = vec![255u8; img_w * img_h * 4]; // white quiet zone

    let palette = MatrixDecoder::STANDARD_PALETTES[color_mode];

    for r in 0..height_cells {
        for c in 0..width_cells {
            let val = grid.cells[r * width_cells + c];
            let color = if val >= 0 && (val as usize) < palette.len() {
                palette[val as usize]
            } else {
                [255.0, 255.0, 255.0]
            };

            for py in 0..cell_size {
                for px in 0..cell_size {
                    let ix = margin + c * cell_size + px;
                    let iy = margin + r * cell_size + py;
                    let offset = (iy * img_w + ix) * 4;
                    rgba[offset] = color[0] as u8;
                    rgba[offset + 1] = color[1] as u8;
                    rgba[offset + 2] = color[2] as u8;
                    rgba[offset + 3] = 255;
                }
            }
        }
    }

    (rgba, img_w, img_h)
}

#[test]
fn test_roundtrip_2color_mode() {
    let html = "<h1>보고서</h1><p>이것은 Windows iMemo 리더기 2컬러 복원 테스트 문서입니다.</p>";
    let (rgba, w, h) = encode_html_to_image(html, 0, 1000, 8);

    let decoded = MatrixDecoder::decode_rgba_image(&rgba, w, h).expect("MatrixDecoder must decode 2-color IMC image");
    assert_eq!(decoded.color_mode, 0);

    let mut collector = SlideCollector::new();
    let res = collector.add_frame(&decoded).expect("SlideCollector must restore document");
    assert_eq!(res.html, html);
}

#[test]
fn test_roundtrip_4color_mode() {
    let html = "<p>4-Color Palette Test <b>굵은 텍스트</b> 및 테이블 데이터 포함</p>";
    let (rgba, w, h) = encode_html_to_image(html, 1, 1000, 8);

    let decoded = MatrixDecoder::decode_rgba_image(&rgba, w, h).expect("MatrixDecoder must decode 4-color IMC image");
    assert_eq!(decoded.color_mode, 1);

    let mut collector = SlideCollector::new();
    let res = collector.add_frame(&decoded).expect("SlideCollector must restore document");
    assert_eq!(res.html, html);
}

#[test]
fn test_roundtrip_16color_mode() {
    let html = "<h2>16-Color Ultra Density Test</h2><p>대용량 첨부 이미지가 포함된 시나리오 시뮬레이션 문서</p>";
    let (rgba, w, h) = encode_html_to_image(html, 3, 1000, 8);

    let decoded = MatrixDecoder::decode_rgba_image(&rgba, w, h).expect("MatrixDecoder must decode 16-color IMC image");
    assert_eq!(decoded.color_mode, 3);

    let mut collector = SlideCollector::new();
    let res = collector.add_frame(&decoded).expect("SlideCollector must restore document");
    assert_eq!(res.html, html);
}

