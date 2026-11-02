//! Interactive Screen Region Selector Overlay for Windows
//! Matches the macOS RegionSelector UI: semi-transparent full-screen overlay,
//! drag selection with live HUD coordinates and green dashed/solid boundary.

#![cfg(windows)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect,
    SelectObject, SetBkColor, SetBkMode, SetTextColor, TextOutW, HDC, PAINTSTRUCT,
    PS_SOLID, TRANSPARENT,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_ESCAPE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    PostQuitMessage, RegisterClassW, SetLayeredWindowAttributes, ShowWindow, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, GetSystemMetrics, LWA_ALPHA, MSG, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW,
    WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_PAINT, WM_RBUTTONDOWN, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};

use crate::config::CaptureRect;

use std::sync::Mutex;

struct SelectorState {
    start_pt: Option<POINT>,
    curr_pt: Option<POINT>,
    tx: Option<Sender<Option<CaptureRect>>>,
    screen_x: i32,
    screen_y: i32,
}

static GLOBAL_STATE: Mutex<Option<SelectorState>> = Mutex::new(None);
static IS_RUNNING: AtomicBool = AtomicBool::new(false);

#[inline]
fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_LBUTTONDOWN => {
            SetCapture(hwnd);
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            if let Ok(mut lock) = GLOBAL_STATE.lock() {
                if let Some(state) = lock.as_mut() {
                    state.start_pt = Some(POINT { x, y });
                    state.curr_pt = Some(POINT { x, y });
                }
            }
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        WM_MOUSEMOVE => {
            if let Ok(mut lock) = GLOBAL_STATE.lock() {
                if let Some(state) = lock.as_mut() {
                    if state.start_pt.is_some() {
                        let x = (lparam & 0xFFFF) as i16 as i32;
                        let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
                        state.curr_pt = Some(POINT { x, y });
                        InvalidateRect(hwnd, std::ptr::null(), 0);
                    }
                }
            }
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            let mut final_rect = None;
            if let Ok(mut lock) = GLOBAL_STATE.lock() {
                if let Some(state) = lock.as_mut() {
                    if let (Some(start), Some(curr)) = (state.start_pt, state.curr_pt) {
                        let min_x = start.x.min(curr.x);
                        let min_y = start.y.min(curr.y);
                        let w = (start.x - curr.x).abs() as u32;
                        let h = (start.y - curr.y).abs() as u32;

                        if w > 20 && h > 20 {
                            // Offset by virtual screen coordinate
                            final_rect = Some(CaptureRect {
                                x: min_x + state.screen_x,
                                y: min_y + state.screen_y,
                                width: w,
                                height: h,
                            });
                        }
                    }
                    if let Some(tx) = state.tx.take() {
                        let _ = tx.send(final_rect);
                    }
                }
            }
            DestroyWindow(hwnd);
            0
        }
        WM_RBUTTONDOWN => {
            // Right-click to cancel
            if let Ok(mut lock) = GLOBAL_STATE.lock() {
                if let Some(state) = lock.as_mut() {
                    if let Some(tx) = state.tx.take() {
                        let _ = tx.send(None);
                    }
                }
            }
            DestroyWindow(hwnd);
            0
        }
        WM_KEYDOWN => {
            if wparam == VK_ESCAPE as usize {
                if let Ok(mut lock) = GLOBAL_STATE.lock() {
                    if let Some(state) = lock.as_mut() {
                        if let Some(tx) = state.tx.take() {
                            let _ = tx.send(None);
                        }
                    }
                }
                DestroyWindow(hwnd);
            }
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);

            draw_overlay(hwnd, hdc);

            EndPaint(hwnd, &ps);
            0
        }
        WM_DESTROY => {
            if let Ok(mut lock) = GLOBAL_STATE.lock() {
                if let Some(state) = lock.as_mut() {
                    if let Some(tx) = state.tx.take() {
                        let _ = tx.send(None);
                    }
                }
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn draw_overlay(hwnd: HWND, hdc: HDC) {
    let mut client_rect: RECT = std::mem::zeroed();
    windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client_rect);

    // 1. Dark semi-transparent background brush
    let bg_brush = CreateSolidBrush(rgb(15, 23, 42));
    FillRect(hdc, &client_rect, bg_brush);
    DeleteObject(bg_brush as _);

    let lock = match GLOBAL_STATE.lock() {
        Ok(l) => l,
        Err(_) => return,
    };
    let state = match lock.as_ref() {
        Some(s) => s,
        None => return,
    };

    match (state.start_pt, state.curr_pt) {
        (Some(start), Some(curr)) => {
            let sel_left = start.x.min(curr.x);
            let sel_top = start.y.min(curr.y);
            let sel_right = start.x.max(curr.x);
            let sel_bottom = start.y.max(curr.y);
            let sel_w = sel_right - sel_left;
            let sel_h = sel_bottom - sel_top;

            // Highlight selected interior with lighter color
            let sel_rect = RECT {
                left: sel_left,
                top: sel_top,
                right: sel_right,
                bottom: sel_bottom,
            };
            let sel_brush = CreateSolidBrush(rgb(34, 197, 94)); // Green highlight
            FillRect(hdc, &sel_rect, sel_brush);
            DeleteObject(sel_brush as _);

            // Draw bright green solid border
            let pen = CreatePen(PS_SOLID, 3, rgb(34, 197, 94));
            let old_pen = SelectObject(hdc, pen as _);
            let null_brush = windows_sys::Win32::Graphics::Gdi::GetStockObject(windows_sys::Win32::Graphics::Gdi::NULL_BRUSH);
            let old_brush = SelectObject(hdc, null_brush);

            windows_sys::Win32::Graphics::Gdi::Rectangle(hdc, sel_left, sel_top, sel_right, sel_bottom);

            SelectObject(hdc, old_pen);
            SelectObject(hdc, old_brush);
            DeleteObject(pen as _);

            // Live HUD coordinates box
            let actual_x = sel_left + state.screen_x;
            let actual_y = sel_top + state.screen_y;
            let hud_text = format!(
                " 📍 X: {}, Y: {} | W: {} x H: {} px  (ESC: 취소, 마우스 해제: 완료) ",
                actual_x, actual_y, sel_w, sel_h
            );
            let wide_hud: Vec<u16> = hud_text.encode_utf16().collect();

            let hud_top = if sel_top >= 36 { sel_top - 30 } else { sel_bottom + 8 };
            let hud_rect = RECT {
                left: sel_left,
                top: hud_top,
                right: sel_left + 480,
                bottom: hud_top + 24,
            };

            let hud_brush = CreateSolidBrush(rgb(0, 0, 0));
            FillRect(hdc, &hud_rect, hud_brush);
            DeleteObject(hud_brush as _);

            SetBkColor(hdc, rgb(0, 0, 0));
            SetTextColor(hdc, rgb(34, 197, 94));
            TextOutW(hdc, sel_left + 4, hud_top + 4, wide_hud.as_ptr(), wide_hud.len() as _);
        }
        _ => {
            // Draw instruction in center
            let prompt = "원격 데스크톱 또는 브라우저 창의 바코드 영역을 마우스로 드래그하세요 (ESC: 취소)";
            let wide_prompt: Vec<u16> = prompt.encode_utf16().collect();
            SetBkMode(hdc, TRANSPARENT as i32);
            SetTextColor(hdc, rgb(255, 255, 255));
            TextOutW(hdc, client_rect.right / 2 - 240, 80, wide_prompt.as_ptr(), wide_prompt.len() as _);
        }
    }
}

/// Spawns a modal screen overlay and blocks until user finishes dragging or cancels with ESC.
pub fn select_region_modal() -> Option<CaptureRect> {
    if IS_RUNNING.swap(true, Ordering::SeqCst) {
        return None;
    }

    let (tx, rx) = channel();

    unsafe {
        let class_name: Vec<u16> = "iMemoRegionSelector\0".encode_utf16().collect();
        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: std::ptr::null_mut(),
            hIcon: std::ptr::null_mut(),
            hCursor: windows_sys::Win32::UI::WindowsAndMessaging::LoadCursorW(std::ptr::null_mut(), windows_sys::Win32::UI::WindowsAndMessaging::IDC_CROSS),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };

        RegisterClassW(&wnd_class);

        // Encompass entire virtual desktop (multi-monitor support)
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vcx = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vcy = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        *GLOBAL_STATE.lock().unwrap() = Some(SelectorState {
            start_pt: None,
            curr_pt: None,
            tx: Some(tx),
            screen_x: vx,
            screen_y: vy,
        });

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_POPUP,
            vx,
            vy,
            vcx,
            vcy,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );

        if hwnd.is_null() {
            IS_RUNNING.store(false, Ordering::SeqCst);
            return None;
        }

        // Set semi-transparent alpha (140 / 255 ≈ 55%)
        SetLayeredWindowAttributes(hwnd, 0, 140, LWA_ALPHA);
        ShowWindow(hwnd, SW_SHOW);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0 as HWND, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        *GLOBAL_STATE.lock().unwrap() = None;
        IS_RUNNING.store(false, Ordering::SeqCst);
    }

    rx.recv().ok().flatten()
}
