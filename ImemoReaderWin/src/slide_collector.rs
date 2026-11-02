//! Slide & Parity Frame Collector & Decompressor
//! 100% compatible with Swift SlideCollector

use std::collections::HashMap;
use std::io::Read;
use flate2::read::{DeflateDecoder, ZlibDecoder};
use crate::crc32::CRC32;
use crate::matrix_decoder::DecodedFrame;

#[derive(Debug, Clone)]
pub struct RestorationResult {
    pub html: String,
    pub doc_id: u16,
    pub total_frames: usize,
    pub compressed_bytes: usize,
    pub decompressed_bytes: usize,
}

#[derive(Debug, Default)]
pub struct SlideCollector {
    pub current_doc_id: Option<u16>,
    pub total_frames: usize,
    pub data_frame_count: usize,
    pub doc_crc32: u32,
    pub received_frames: HashMap<usize, Vec<u8>>,
}

impl SlideCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.current_doc_id = None;
        self.total_frames = 0;
        self.data_frame_count = 0;
        self.doc_crc32 = 0;
        self.received_frames.clear();
    }

    pub fn progress_text(&self) -> String {
        match self.current_doc_id {
            Some(_) if self.total_frames > 0 => {
                let count = self.received_frames.len();
                format!(
                    "프레임 수집 중: {} / {} (데이터: {}장)",
                    count, self.total_frames, self.data_frame_count
                )
            }
            _ => "대기 중 (코드 미검출)".to_string(),
        }
    }

    pub fn frame_status_list(&self) -> Vec<bool> {
        if self.total_frames == 0 {
            return Vec::new();
        }
        (0..self.total_frames)
            .map(|i| self.received_frames.contains_key(&i))
            .collect()
    }

    pub fn add_frame(&mut self, frame: &DecodedFrame) -> Option<RestorationResult> {
        // If new document arrived, reset
        if self.current_doc_id != Some(frame.doc_id) {
            self.current_doc_id = Some(frame.doc_id);
            self.total_frames = frame.total_frames;
            self.data_frame_count = frame.data_frame_count;
            self.doc_crc32 = frame.doc_crc32;
            self.received_frames.clear();
        }

        // Store this frame payload
        self.received_frames.insert(frame.frame_seq, frame.payload.clone());

        // Check if we can assemble the document
        // Case 1: Collect missing data sequences (0 .. data_frame_count)
        let mut missing_data_seq: Vec<usize> = Vec::new();
        for i in 0..self.data_frame_count {
            if !self.received_frames.contains_key(&i) {
                missing_data_seq.push(i);
            }
        }

        // Case 2: Exactly 1 data frame missing, but parity frame is available!
        // Parity frame is index `data_frame_count`
        if missing_data_seq.len() == 1 {
            let parity_seq = self.data_frame_count;
            if let Some(parity) = self.received_frames.get(&parity_seq).cloned() {
                let missing_idx = missing_data_seq[0];
                let mut recovered = parity;
                for d in 0..self.data_frame_count {
                    if d != missing_idx {
                        if let Some(other_data) = self.received_frames.get(&d) {
                            let common_len = recovered.len().min(other_data.len());
                            for k in 0..common_len {
                                recovered[k] ^= other_data[k];
                            }
                        }
                    }
                }
                self.received_frames.insert(missing_idx, recovered);
                missing_data_seq.clear();
            }
        }

        if missing_data_seq.is_empty() {
            // Concatenate all data chunks in order
            let mut full_data = Vec::new();
            for d in 0..self.data_frame_count {
                if let Some(chunk) = self.received_frames.get(&d) {
                    full_data.extend_from_slice(chunk);
                }
            }

            // Verify CRC32
            let mut matched = CRC32::checksum(&full_data) == self.doc_crc32;
            if !matched {
                // If last frame was recovered from parity, it might have extra padding bytes
                let last_idx = self.data_frame_count - 1;
                if let Some(last_chunk) = self.received_frames.get(&last_idx) {
                    let mut candidate = full_data.clone();
                    for _trim_len in (1..last_chunk.len()).rev() {
                        candidate.pop();
                        if CRC32::checksum(&candidate) == self.doc_crc32 {
                            full_data = candidate;
                            matched = true;
                            break;
                        }
                    }
                }
            }

            if matched {
                // Decompress
                if let Some(html_string) = Self::decompress_to_html(&full_data) {
                    return Some(RestorationResult {
                        decompressed_bytes: html_string.len(),
                        html: html_string,
                        doc_id: frame.doc_id,
                        total_frames: frame.total_frames,
                        compressed_bytes: full_data.len(),
                    });
                }
            }
        }

        None
    }

    pub fn decompress_to_html(data: &[u8]) -> Option<String> {
        // 1. Try raw DEFLATE (used by browser CompressionStream('deflate-raw'))
        let mut def_decoder = DeflateDecoder::new(data);
        let mut decompressed = Vec::new();
        if def_decoder.read_to_end(&mut decompressed).is_ok() {
            if let Ok(s) = String::from_utf8(decompressed) {
                return Some(s);
            }
        }

        // 2. Try Zlib (RFC 1950 header 0x78)
        let mut zlib_decoder = ZlibDecoder::new(data);
        let mut decompressed = Vec::new();
        if zlib_decoder.read_to_end(&mut decompressed).is_ok() {
            if let Ok(s) = String::from_utf8(decompressed) {
                return Some(s);
            }
        }

        // 3. Fallback: If 2-byte header (e.g. 0x78 0x9C) and 4-byte Adler32 trailer exist, strip them
        if data.len() > 6 && data[0] == 0x78 {
            let stripped = &data[2..(data.len() - 4)];
            let mut def_decoder = DeflateDecoder::new(stripped);
            let mut decompressed = Vec::new();
            if def_decoder.read_to_end(&mut decompressed).is_ok() {
                if let Ok(s) = String::from_utf8(decompressed) {
                    return Some(s);
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    #[test]
    fn test_slide_collector_single_frame() {
        let original_html = "<h1>Test HTML Document</h1><p>Content goes here.</p>";
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original_html.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();

        let crc = CRC32::checksum(&compressed);

        let frame = DecodedFrame {
            doc_id: 1234,
            frame_seq: 0,
            total_frames: 1,
            data_frame_count: 1,
            payload_len: compressed.len(),
            doc_crc32: crc,
            payload: compressed,
            color_mode: 0,
            width_cells: 100,
            height_cells: 28,
        };

        let mut collector = SlideCollector::new();
        let res = collector.add_frame(&frame).expect("Should restore single frame");
        assert_eq!(res.html, original_html);
        assert_eq!(res.doc_id, 1234);
    }

    #[test]
    fn test_slide_collector_parity_loss_recovery() {
        // Wrap full with deflate
        let original_html = "<p>Parity Recovery Test With Longer Document Content For Multi-Chunk Splitting</p>";
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original_html.as_bytes()).unwrap();
        let full_compressed = encoder.finish().unwrap();
        let crc = CRC32::checksum(&full_compressed);

        let chunk_size = (full_compressed.len() + 1) / 2;
        let c0 = full_compressed[..chunk_size].to_vec();
        let c1 = full_compressed[chunk_size..].to_vec();

        let max_len = chunk_size;
        let mut p_chunk = vec![0u8; max_len];
        for i in 0..c0.len() { p_chunk[i] ^= c0[i]; }
        for i in 0..c1.len() { p_chunk[i] ^= c1[i]; }

        let frame1 = DecodedFrame {
            doc_id: 999,
            frame_seq: 1, // frame 0 is missing!
            total_frames: 3,
            data_frame_count: 2,
            payload_len: c1.len(),
            doc_crc32: crc,
            payload: c1,
            color_mode: 0,
            width_cells: 100,
            height_cells: 28,
        };

        let parity_frame = DecodedFrame {
            doc_id: 999,
            frame_seq: 2, // parity frame
            total_frames: 3,
            data_frame_count: 2,
            payload_len: p_chunk.len(),
            doc_crc32: crc,
            payload: p_chunk,
            color_mode: 0,
            width_cells: 100,
            height_cells: 28,
        };

        let mut collector = SlideCollector::new();
        assert!(collector.add_frame(&frame1).is_none());
        let res = collector.add_frame(&parity_frame).expect("Should recover frame 0 from parity!");
        assert_eq!(res.html, original_html);
    }
}
