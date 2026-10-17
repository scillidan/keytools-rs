//! Win32 overlay region selector (the "builtin" screenshot tool).
//! A layered fullscreen window: drag a rubber band, Esc cancels.

use std::sync::Once;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, FillRect, GetStockObject, InvalidateRect, BLACK_BRUSH, HBRUSH,
    PAINTSTRUCT, WHITE_BRUSH,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Default, Clone, Copy)]
struct Sel {
    start: POINT,
    cur: POINT,
    dragging: bool,
    done: bool,
    cancel: bool,
}

static REGISTER: Once = Once::new();

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let state = unsafe { &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Sel) };
    match msg {
        WM_LBUTTONDOWN => {
            state.start = point(l);
            state.cur = state.start;
            state.dragging = true;
            unsafe { SetCapture(hwnd) };
        }
        WM_MOUSEMOVE => {
            if state.dragging {
                state.cur = point(l);
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
        }
        WM_LBUTTONUP => {
            state.cur = point(l);
            state.dragging = false;
            state.done = true;
            unsafe {
                let _ = ReleaseCapture();
                let _ = DestroyWindow(hwnd);
            }
        }
        WM_KEYDOWN if w.0 as u32 == VK_ESCAPE.0 as u32 => {
            state.cancel = true;
            state.done = true;
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
        WM_PAINT => unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let _ = FillRect(hdc, &ps.rcPaint, HBRUSH(GetStockObject(BLACK_BRUSH).0));
            let band = band_rect(state);
            let _ = FillRect(hdc, &band, HBRUSH(GetStockObject(WHITE_BRUSH).0));
            let _ = EndPaint(hwnd, &ps);
        },
        _ => return unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
    LRESULT(0)
}

fn point(l: LPARAM) -> POINT {
    POINT {
        x: (l.0 & 0xffff) as i16 as i32,
        y: ((l.0 >> 16) & 0xffff) as i16 as i32,
    }
}

fn band_rect(s: &Sel) -> RECT {
    RECT {
        left: s.start.x.min(s.cur.x),
        right: s.start.x.max(s.cur.x),
        top: s.start.y.min(s.cur.y),
        bottom: s.start.y.max(s.cur.y),
    }
}

/// Show the overlay over the virtual screen; return the selected rect in
/// *screen* coordinates, or None on cancel / too-small drag.
pub fn select_region() -> Option<RECT> {
    unsafe {
        REGISTER.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                lpszClassName: windows::core::w!("screenocr_sel"),
                hCursor: HCURSOR(LoadCursorW(None, IDC_CROSS).unwrap().0),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let mut state = Sel::default();
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            windows::core::w!("screenocr_sel"),
            None,
            WS_POPUP,
            vx,
            vy,
            vw,
            vh,
            None,
            None,
            None,
            None,
        )
        .ok()?;
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 120, LWA_ALPHA);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut state as *mut Sel as isize);
        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while !state.done {
            if GetMessageW(&mut msg, None, 0, 0).0 <= 0 {
                state.cancel = true;
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if state.cancel {
            return None;
        }
        // Client coords -> screen coords (window origin is vx,vy).
        let r = band_rect(&state);
        let rect = RECT {
            left: r.left + vx,
            top: r.top + vy,
            right: r.right + vx,
            bottom: r.bottom + vy,
        };
        if rect.right - rect.left < 3 || rect.bottom - rect.top < 3 {
            return None;
        }
        Some(rect)
    }
}

/// Capture a screen rect into a BMP file (BitBlt into a 32-bpp DIB section).
pub fn capture_rect(rect: RECT, path: &std::path::Path) -> bool {
    use windows::Win32::Graphics::Gdi::*;
    let w = (rect.right - rect.left) as u32;
    let h = (rect.bottom - rect.top) as u32;
    unsafe {
        let screen = GetDC(None);
        if screen.0.is_null() {
            return false;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: h as i32, // positive = bottom-up, matching memory order
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(Some(screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
        if bmp.is_err() {
            let _ = ReleaseDC(None, screen);
            return false;
        }
        let bmp = bmp.unwrap();
        let old = SelectObject(mem, bmp.into());
        let ok = BitBlt(
            mem,
            0,
            0,
            w as i32,
            h as i32,
            Some(screen),
            rect.left,
            rect.top,
            SRCCOPY,
        )
        .is_ok();
        let _ = ReleaseDC(None, screen);

        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        if ok && !bits.is_null() {
            let bytes = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
            // memory is bottom-up BGRA -> top-down RGBA
            let row = (w * 4) as usize;
            for y in (0..h as usize).rev() {
                let r = &bytes[y * row..(y + 1) * row];
                for px in r.as_chunks::<4>().0 {
                    rgba.push(px[2]);
                    rgba.push(px[1]);
                    rgba.push(px[0]);
                    rgba.push(px[3]);
                }
            }
        }
        let _ = SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
        ok && crate::capture::write_bmp(path, w, h, &rgba)
    }
}
