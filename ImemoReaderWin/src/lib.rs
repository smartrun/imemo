pub mod crc32;
pub mod scrambler;
pub mod reed_solomon;
pub mod matrix_decoder;
pub mod slide_collector;
pub mod config;
#[cfg(windows)]
pub mod screen_capture;
#[cfg(windows)]
pub mod region_selector;
pub mod app;
