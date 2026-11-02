//! Windows Native High-Speed Screen Capture using Win32 GDI & BitBlt
//! Provides sub-millisecond screen rect grabbing with Per-Monitor DPI awareness.

#![cfg(windows)]

use std::ptr;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext;

pub struct ScreenCapturer;

impl ScreenCapturer {
    pub fn enable_dpi_awareness() {
        unsafe {
            // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4
            SetProcessDpiAwarenessContext((-4isize) as *mut _);
        }
    }

    /// Captures a rectangle from the desktop screen and returns RGBA pixel bytes (width * height * 4).
    pub fn capture_rect(x: i32, y: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        if width == 0 || height == 0 {
            return Err("Width or height is zero".to_string());
        }

        unsafe {
            let hdc_screen = GetDC(ptr::null_mut() as HWND);
            if hdc_screen.is_null() {
                return Err("Failed to get screen DC".to_string());
            }

            let hdc_mem = CreateCompatibleDC(hdc_screen);
            if hdc_mem.is_null() {
                ReleaseDC(ptr::null_mut() as HWND, hdc_screen);
                return Err("Failed to create compatible memory DC".to_string());
            }

            let hbitmap = CreateCompatibleBitmap(hdc_screen, width as i32, height as i32);
            if hbitmap.is_null() {
                DeleteDC(hdc_mem);
                ReleaseDC(ptr::null_mut() as HWND, hdc_screen);
                return Err("Failed to create compatible bitmap".to_string());
            }

            let old_obj = SelectObject(hdc_mem, hbitmap);

            // Copy from screen DC to memory DC
            let blt_res = BitBlt(
                hdc_mem,
                0,
                0,
                width as i32,
                height as i32,
                hdc_screen,
                x,
                y,
                SRCCOPY,
            );

            if blt_res == 0 {
                SelectObject(hdc_mem, old_obj);
                DeleteObject(hbitmap);
                DeleteDC(hdc_mem);
                ReleaseDC(ptr::null_mut() as HWND, hdc_screen);
                return Err("BitBlt failed".to_string());
            }

            // Prepare BITMAPINFO for top-down 32-bit RGBA
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width as i32;
            bmi.bmiHeader.biHeight = -(height as i32); // Top-down DIB
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let total_bytes = (width as usize) * (height as usize) * 4;
            let mut bgra_buf = vec![0u8; total_bytes];

            let lines = GetDIBits(
                hdc_mem,
                hbitmap,
                0,
                height,
                bgra_buf.as_mut_ptr() as *mut _,
                &mut bmi,
                DIB_RGB_COLORS,
            );

            // Clean up GDI objects
            SelectObject(hdc_mem, old_obj);
            DeleteObject(hbitmap);
            DeleteDC(hdc_mem);
            ReleaseDC(ptr::null_mut() as HWND, hdc_screen);

            if lines == 0 {
                return Err("GetDIBits failed to read pixels".to_string());
            }

            // Convert BGRA (Windows default) to RGBA
            for chunk in bgra_buf.chunks_exact_mut(4) {
                let b = chunk[0];
                let r = chunk[2];
                chunk[0] = r;
                chunk[2] = b;
                chunk[3] = 255; // Ensure alpha is fully opaque
            }

            Ok(bgra_buf)
        }
    }
}
