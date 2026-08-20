use crate::config::{OcrConfig, OcrLanguage, ScreenshotResultMode};
use crate::platform::{Point, Rect};
use anyhow::{bail, Result};
use std::ffi::c_void;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    EndPaint, GetDC, GetDIBits, ReleaseDC, ScreenToClient, SelectObject, SetStretchBltMode,
    StretchBlt, UpdateWindow, BITMAPINFO, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HALFTONE, HBITMAP,
    PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Controls::Dialogs::{
    GetSaveFileNameW, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW, GetSystemMetrics,
    GetWindowLongPtrW, GetWindowRect, LoadCursorW, MessageBoxW, PostQuitMessage, RegisterClassW,
    SendMessageW, SetCursor, SetForegroundWindow, SetLayeredWindowAttributes, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, TrackPopupMenu, TranslateMessage, CREATESTRUCTW, CS_HREDRAW,
    CS_VREDRAW, GWLP_USERDATA, HTCAPTION, IDC_ARROW, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE,
    IDC_SIZEWE, LWA_ALPHA, MB_ICONERROR, MB_OK, MF_SEPARATOR, MF_STRING, MSG, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SWP_NOACTIVATE, SWP_NOZORDER,
    SW_SHOW, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_STYLE, WM_CAPTURECHANGED, WM_CONTEXTMENU,
    WM_DESTROY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE,
    WM_NCLBUTTONDOWN, WM_PAINT, WM_SETCURSOR, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

const CLASS_NAME: windows::core::PCWSTR = w!("ConvenientWindowPinnedImage");
const COPY_MENU_ID: usize = 1;
const SAVE_MENU_ID: usize = 2;
const OCR_MENU_ID: usize = 3;
const CLOSE_MENU_ID: usize = 4;
const CF_DIB_FORMAT: u32 = 8;

struct PinState {
    bitmap: HBITMAP,
    width: i32,
    height: i32,
    pixels: Arc<Vec<u8>>,
    ocr_language: OcrLanguage,
    opacity: u8,
    resize: Option<ResizeSession>,
}

fn pin_window_style() -> WINDOW_STYLE {
    WS_VISIBLE | WS_POPUP
}

fn initial_pin_size(width: i32, height: i32) -> (i32, i32) {
    const MIN_WIDTH: f64 = 180.0;
    const MIN_HEIGHT: f64 = 120.0;
    const MAX_WIDTH: f64 = 1200.0;
    const MAX_HEIGHT: f64 = 900.0;

    let width = width.max(1) as f64;
    let height = height.max(1) as f64;
    let grow_scale = 1.0_f64.max(MIN_WIDTH / width).max(MIN_HEIGHT / height);
    let shrink_scale = 1.0_f64.min(MAX_WIDTH / width).min(MAX_HEIGHT / height);
    let scale = if grow_scale > 1.0 {
        grow_scale.min((MAX_WIDTH / width).min(MAX_HEIGHT / height))
    } else {
        shrink_scale
    };

    (
        (width * scale).round().max(1.0) as i32,
        (height * scale).round().max(1.0) as i32,
    )
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ResizeEdges(u8);

impl ResizeEdges {
    const LEFT: u8 = 1;
    const RIGHT: u8 = 2;
    const TOP: u8 = 4;
    const BOTTOM: u8 = 8;

    fn contains(self, edge: u8) -> bool {
        self.0 & edge != 0
    }

    fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResizeCursor {
    WestEast,
    NorthSouth,
    NorthWestSouthEast,
    NorthEastSouthWest,
}

fn resize_cursor_kind(edges: ResizeEdges) -> Option<ResizeCursor> {
    let horizontal = edges.contains(ResizeEdges::LEFT) || edges.contains(ResizeEdges::RIGHT);
    let vertical = edges.contains(ResizeEdges::TOP) || edges.contains(ResizeEdges::BOTTOM);
    match (horizontal, vertical) {
        (true, false) => Some(ResizeCursor::WestEast),
        (false, true) => Some(ResizeCursor::NorthSouth),
        (true, true) if edges.contains(ResizeEdges::LEFT) == edges.contains(ResizeEdges::TOP) => {
            Some(ResizeCursor::NorthWestSouthEast)
        }
        (true, true) => Some(ResizeCursor::NorthEastSouthWest),
        (false, false) => None,
    }
}

#[derive(Clone, Copy)]
struct ResizeSession {
    edges: ResizeEdges,
    cursor: POINT,
    window: RECT,
}

fn resize_edges_at(x: i32, y: i32, width: i32, height: i32, grab_margin: i32) -> ResizeEdges {
    let mut edges = 0;
    if x < grab_margin {
        edges |= ResizeEdges::LEFT;
    } else if x >= width - grab_margin {
        edges |= ResizeEdges::RIGHT;
    }
    if y < grab_margin {
        edges |= ResizeEdges::TOP;
    } else if y >= height - grab_margin {
        edges |= ResizeEdges::BOTTOM;
    }
    ResizeEdges(edges)
}

fn resize_grab_margin(hwnd: HWND) -> i32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    ((10 * dpi + 95) / 96).clamp(10, 30) as i32
}

const PIN_OVERLAP_OFFSET: i32 = 16;

fn rects_overlap(left: i32, top: i32, width: i32, height: i32, capture: Rect) -> bool {
    let right = left.saturating_add(width);
    let bottom = top.saturating_add(height);
    left < capture.right && right > capture.left && top < capture.bottom && bottom > capture.top
}

fn pin_position(
    capture: Rect,
    end_point: Option<Point>,
    width: i32,
    height: i32,
    virtual_desktop: Rect,
) -> (i32, i32) {
    let point = end_point.unwrap_or(Point {
        x: capture.left,
        y: capture.top,
    });
    let mut left = point.x;
    let mut top = point.y;
    if rects_overlap(left, top, width, height, capture) {
        left = left.saturating_add(PIN_OVERLAP_OFFSET);
        top = top.saturating_add(PIN_OVERLAP_OFFSET);
    }
    let max_left = virtual_desktop.right.saturating_sub(width);
    let max_top = virtual_desktop.bottom.saturating_sub(height);
    (
        left.clamp(virtual_desktop.left, max_left.max(virtual_desktop.left)),
        top.clamp(virtual_desktop.top, max_top.max(virtual_desktop.top)),
    )
}

fn virtual_desktop_rect() -> Rect {
    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        Rect {
            left,
            top,
            right: left.saturating_add(width),
            bottom: top.saturating_add(height),
        }
    }
}

unsafe fn set_resize_cursor(edges: ResizeEdges) -> bool {
    let Some(kind) = resize_cursor_kind(edges) else {
        return false;
    };
    let resource = match kind {
        ResizeCursor::WestEast => IDC_SIZEWE,
        ResizeCursor::NorthSouth => IDC_SIZENS,
        ResizeCursor::NorthWestSouthEast => IDC_SIZENWSE,
        ResizeCursor::NorthEastSouthWest => IDC_SIZENESW,
    };
    let cursor = LoadCursorW(None, resource).unwrap_or_default();
    let _ = SetCursor(cursor);
    true
}

fn resized_window_rect(session: ResizeSession, cursor: POINT) -> RECT {
    const MIN_WIDTH: i32 = 80;
    const MIN_HEIGHT: i32 = 60;
    let dx = cursor.x - session.cursor.x;
    let dy = cursor.y - session.cursor.y;
    let mut rect = session.window;
    if session.edges.contains(ResizeEdges::LEFT) {
        rect.left = (rect.left + dx).min(rect.right - MIN_WIDTH);
    }
    if session.edges.contains(ResizeEdges::RIGHT) {
        rect.right = (rect.right + dx).max(rect.left + MIN_WIDTH);
    }
    if session.edges.contains(ResizeEdges::TOP) {
        rect.top = (rect.top + dy).min(rect.bottom - MIN_HEIGHT);
    }
    if session.edges.contains(ResizeEdges::BOTTOM) {
        rect.bottom = (rect.bottom + dy).max(rect.top + MIN_HEIGHT);
    }
    rect
}

pub fn capture_and_pin(rect: Rect, end_point: Option<Point>, ocr: &OcrConfig) -> Result<()> {
    let width = rect.width();
    let height = rect.height();
    if width < 2 || height < 2 || width > 16_384 || height > 16_384 {
        bail!("screenshot region is outside the supported size");
    }
    let (bitmap, pixels) = unsafe {
        let screen = GetDC(HWND::default());
        if screen.is_invalid() {
            bail!("GetDC failed for screenshot");
        }
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        if memory.is_invalid() || bitmap.is_invalid() {
            if !memory.is_invalid() {
                let _ = DeleteDC(memory);
            }
            ReleaseDC(HWND::default(), screen);
            bail!("failed to allocate screenshot bitmap");
        }
        let old = SelectObject(memory, bitmap);
        let copied = BitBlt(
            memory,
            0,
            0,
            width,
            height,
            screen,
            rect.left,
            rect.top,
            SRCCOPY | CAPTUREBLT,
        )
        .is_ok();
        SelectObject(memory, old);
        if !copied {
            let _ = DeleteDC(memory);
            ReleaseDC(HWND::default(), screen);
            let _ = DeleteObject(bitmap);
            bail!("BitBlt failed while capturing screen");
        }
        let pixels = read_bitmap_pixels(memory, bitmap, width, height);
        let _ = DeleteDC(memory);
        ReleaseDC(HWND::default(), screen);
        let pixels = match pixels {
            Ok(pixels) => pixels,
            Err(error) => {
                let _ = DeleteObject(bitmap);
                return Err(error);
            }
        };
        (bitmap, pixels)
    };

    let bitmap_handle = bitmap.0 as isize;
    let pixels = Arc::new(pixels);
    let should_pin = matches!(
        ocr.screenshot_result,
        ScreenshotResultMode::Pin | ScreenshotResultMode::PinAndCopy
    );
    let should_ocr = matches!(
        ocr.screenshot_result,
        ScreenshotResultMode::CopyText | ScreenshotResultMode::PinAndCopy
    );
    if should_ocr {
        let ocr_pixels = Arc::clone(&pixels);
        let language = ocr.language;
        super::ocr::recognize_and_copy_async(ocr_pixels, width, height, language, |result| {
            super::ocr::record_ocr_completion(&result);
            show_ocr_result(HWND::default(), result)
        });
    }
    if should_pin {
        let language = ocr.language;
        std::thread::Builder::new()
            .name("pinned-screenshot".into())
            .spawn(move || {
                run_pin_window(
                    HBITMAP(bitmap_handle as *mut c_void),
                    width,
                    height,
                    pixels,
                    language,
                    rect,
                    end_point,
                )
            })?;
    } else {
        unsafe {
            let _ = DeleteObject(bitmap);
        }
    }
    Ok(())
}

unsafe fn read_bitmap_pixels(
    dc: windows::Win32::Graphics::Gdi::HDC,
    bitmap: HBITMAP,
    width: i32,
    height: i32,
) -> Result<Vec<u8>> {
    let byte_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| anyhow::anyhow!("screenshot bitmap is too large"))?;
    let mut pixels = vec![0_u8; byte_len];
    let mut info = BITMAPINFO::default();
    info.bmiHeader.biSize = size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB.0;
    let lines = GetDIBits(
        dc,
        bitmap,
        0,
        height as u32,
        Some(pixels.as_mut_ptr().cast()),
        &mut info,
        DIB_RGB_COLORS,
    );
    if lines != height {
        bail!("GetDIBits failed while reading screenshot pixels");
    }
    Ok(pixels)
}

fn run_pin_window(
    bitmap: HBITMAP,
    width: i32,
    height: i32,
    pixels: Arc<Vec<u8>>,
    ocr_language: OcrLanguage,
    capture: Rect,
    end_point: Option<Point>,
) {
    unsafe {
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(pin_window_proc),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        let _ = RegisterClassW(&class);
        let state = Box::new(PinState {
            bitmap,
            width,
            height,
            pixels,
            ocr_language,
            opacity: 255,
            resize: None,
        });
        let state_ptr = Box::into_raw(state);
        let (window_width, window_height) = initial_pin_size(width, height);
        let (window_left, window_top) = pin_position(
            capture,
            end_point,
            window_width,
            window_height,
            virtual_desktop_rect(),
        );
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            CLASS_NAME,
            w!("便捷窗口 · 悬浮贴图（右键复制、保存或关闭）"),
            pin_window_style(),
            window_left,
            window_top,
            window_width,
            window_height,
            HWND::default(),
            None,
            None,
            Some(state_ptr.cast::<c_void>()),
        );
        let Ok(hwnd) = hwnd else {
            drop(Box::from_raw(state_ptr));
            let _ = DeleteObject(bitmap);
            return;
        };
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);
        let mut message = MSG::default();
        while GetMessageW(&mut message, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn pin_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        return LRESULT(1);
    }
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut PinState;
    match message {
        WM_PAINT if !state_ptr.is_null() => {
            let state = &*state_ptr;
            let mut paint = PAINTSTRUCT::default();
            let target = BeginPaint(hwnd, &mut paint);
            let source = CreateCompatibleDC(target);
            let old = SelectObject(source, state.bitmap);
            let mut client = Default::default();
            let _ = GetClientRect(hwnd, &mut client);
            let _ = SetStretchBltMode(target, HALFTONE);
            let _ = StretchBlt(
                target,
                0,
                0,
                client.right,
                client.bottom,
                source,
                0,
                0,
                state.width,
                state.height,
                SRCCOPY,
            );
            SelectObject(source, old);
            let _ = DeleteDC(source);
            let _ = EndPaint(hwnd, &paint);
            LRESULT(0)
        }
        WM_MOUSEWHEEL if !state_ptr.is_null() => {
            let state = &mut *state_ptr;
            let delta = ((wparam.0 >> 16) as u16) as i16;
            state.opacity = if delta > 0 {
                state.opacity.saturating_add(16)
            } else {
                state.opacity.saturating_sub(16).max(48)
            };
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), state.opacity, LWA_ALPHA);
            LRESULT(0)
        }
        WM_LBUTTONDOWN if !state_ptr.is_null() => {
            let state = &mut *state_ptr;
            let mut client = RECT::default();
            let mut window = RECT::default();
            let mut cursor = POINT::default();
            let x = (lparam.0 as u32 & 0xffff) as u16 as i16 as i32;
            let y = ((lparam.0 as u32 >> 16) & 0xffff) as u16 as i16 as i32;
            let _ = GetClientRect(hwnd, &mut client);
            let edges =
                resize_edges_at(x, y, client.right, client.bottom, resize_grab_margin(hwnd));
            if !edges.is_empty()
                && GetWindowRect(hwnd, &mut window).is_ok()
                && GetCursorPos(&mut cursor).is_ok()
            {
                state.resize = Some(ResizeSession {
                    edges,
                    cursor,
                    window,
                });
                let _ = SetCapture(hwnd);
                return LRESULT(0);
            }
            let _ = ReleaseCapture();
            let _ = SendMessageW(
                hwnd,
                WM_NCLBUTTONDOWN,
                WPARAM(HTCAPTION as usize),
                LPARAM(0),
            );
            LRESULT(0)
        }
        WM_MOUSEMOVE if !state_ptr.is_null() => {
            if let Some(resize) = (*state_ptr).resize {
                let mut cursor = POINT::default();
                if GetCursorPos(&mut cursor).is_ok() {
                    let rect = resized_window_rect(resize, cursor);
                    let _ = SetWindowPos(
                        hwnd,
                        HWND::default(),
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP if !state_ptr.is_null() => {
            (*state_ptr).resize = None;
            let _ = ReleaseCapture();
            LRESULT(0)
        }
        WM_CAPTURECHANGED if !state_ptr.is_null() => {
            (*state_ptr).resize = None;
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let mut cursor = POINT::default();
            let mut client = RECT::default();
            if GetCursorPos(&mut cursor).is_ok()
                && ScreenToClient(hwnd, &mut cursor).as_bool()
                && GetClientRect(hwnd, &mut client).is_ok()
            {
                let edges = resize_edges_at(
                    cursor.x,
                    cursor.y,
                    client.right,
                    client.bottom,
                    resize_grab_margin(hwnd),
                );
                if set_resize_cursor(edges) {
                    return LRESULT(1);
                }
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_CONTEXTMENU if !state_ptr.is_null() => {
            show_pin_context_menu(hwnd, lparam, &*state_ptr);
            LRESULT(0)
        }
        WM_DESTROY => {
            if !state_ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let state = Box::from_raw(state_ptr);
                let _ = DeleteObject(state.bitmap);
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn show_pin_context_menu(hwnd: HWND, lparam: LPARAM, state: &PinState) {
    let Ok(menu) = CreatePopupMenu() else {
        return;
    };
    if AppendMenuW(menu, MF_STRING, COPY_MENU_ID, w!("复制图片")).is_err()
        || AppendMenuW(menu, MF_STRING, SAVE_MENU_ID, w!("另存为 PNG…")).is_err()
        || AppendMenuW(menu, MF_STRING, OCR_MENU_ID, w!("识别文字并复制")).is_err()
        || AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()).is_err()
        || AppendMenuW(menu, MF_STRING, CLOSE_MENU_ID, w!("关闭")).is_err()
    {
        let _ = DestroyMenu(menu);
        return;
    }
    let mut point = if lparam.0 == -1 {
        POINT::default()
    } else {
        POINT {
            x: (lparam.0 as u32 & 0xffff) as u16 as i16 as i32,
            y: ((lparam.0 as u32 >> 16) & 0xffff) as u16 as i16 as i32,
        }
    };
    if lparam.0 == -1 && GetCursorPos(&mut point).is_err() {
        let _ = DestroyMenu(menu);
        return;
    }
    let _ = SetForegroundWindow(hwnd);
    let selected = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        hwnd,
        None,
    );
    let _ = DestroyMenu(menu);
    match selected.0 as usize {
        COPY_MENU_ID => {
            if let Err(error) =
                copy_pixels_to_clipboard(hwnd, &state.pixels, state.width, state.height)
            {
                show_pin_error(hwnd, &format!("复制图片失败：{error}"));
            }
        }
        SAVE_MENU_ID => {
            if let Some(path) = choose_png_path(hwnd) {
                if let Err(error) =
                    save_pixels_as_png(&path, &state.pixels, state.width, state.height)
                {
                    show_pin_error(hwnd, &format!("保存图片失败：{error}"));
                }
            }
        }
        OCR_MENU_ID => {
            let pixels = Arc::clone(&state.pixels);
            let width = state.width;
            let height = state.height;
            let language = state.ocr_language;
            let owner = hwnd.0 as isize;
            super::ocr::recognize_and_copy_async(pixels, width, height, language, move |result| {
                super::ocr::record_ocr_completion(&result);
                show_ocr_result(HWND(owner as *mut c_void), result)
            });
        }
        CLOSE_MENU_ID => {
            let _ = DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe fn copy_pixels_to_clipboard(
    hwnd: HWND,
    pixels: &[u8],
    width: i32,
    height: i32,
) -> Result<()> {
    let dib = clipboard_dib(pixels, width, height)?;
    let memory = GlobalAlloc(GMEM_MOVEABLE, dib.len())?;
    let target = GlobalLock(memory);
    if target.is_null() {
        let _ = windows::Win32::Foundation::GlobalFree(memory);
        bail!("GlobalLock failed for clipboard image");
    }

    std::ptr::copy_nonoverlapping(dib.as_ptr(), target.cast::<u8>(), dib.len());
    let _ = GlobalUnlock(memory);

    if let Err(error) = OpenClipboard(hwnd) {
        let _ = windows::Win32::Foundation::GlobalFree(memory);
        return Err(error.into());
    }
    let result = (|| -> Result<()> {
        EmptyClipboard()?;
        SetClipboardData(CF_DIB_FORMAT, HANDLE(memory.0))?;
        Ok(())
    })();
    let _ = CloseClipboard();
    if result.is_err() {
        let _ = windows::Win32::Foundation::GlobalFree(memory);
    }
    result
}

fn clipboard_dib(pixels: &[u8], width: i32, height: i32) -> Result<Vec<u8>> {
    let row_bytes = width as usize * 4;
    if width <= 0
        || height <= 0
        || row_bytes
            .checked_mul(height as usize)
            .is_none_or(|expected| expected != pixels.len())
    {
        bail!("invalid clipboard image dimensions");
    }
    let header_size = size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>();
    let mut dib = Vec::with_capacity(header_size + pixels.len());
    let mut header = windows::Win32::Graphics::Gdi::BITMAPINFOHEADER::default();
    header.biSize = header_size as u32;
    header.biWidth = width;
    header.biHeight = height;
    header.biPlanes = 1;
    header.biBitCount = 32;
    header.biCompression = BI_RGB.0;
    header.biSizeImage = pixels.len() as u32;
    unsafe {
        dib.extend_from_slice(std::slice::from_raw_parts(
            (&header as *const windows::Win32::Graphics::Gdi::BITMAPINFOHEADER).cast::<u8>(),
            header_size,
        ));
    }
    for row in (0..height as usize).rev() {
        let offset = row * row_bytes;
        dib.extend_from_slice(&pixels[offset..offset + row_bytes]);
    }
    Ok(dib)
}

unsafe fn choose_png_path(hwnd: HWND) -> Option<PathBuf> {
    let filename = format!(
        "便捷窗口截图-{}.png",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let mut file_buffer = [0_u16; 32_768];
    for (target, value) in file_buffer.iter_mut().zip(filename.encode_utf16()) {
        *target = value;
    }
    let filter: Vec<u16> = "PNG 图片 (*.png)\0*.png\0所有文件 (*.*)\0*.*\0\0"
        .encode_utf16()
        .collect();
    let title: Vec<u16> = "保存悬浮截图\0".encode_utf16().collect();
    let default_extension: Vec<u16> = "png\0".encode_utf16().collect();
    let mut dialog = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        nFilterIndex: 1,
        lpstrFile: PWSTR(file_buffer.as_mut_ptr()),
        nMaxFile: file_buffer.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        lpstrDefExt: PCWSTR(default_extension.as_ptr()),
        ..Default::default()
    };
    if !GetSaveFileNameW(&mut dialog).as_bool() {
        return None;
    }
    let length = file_buffer.iter().position(|value| *value == 0)?;
    Some(PathBuf::from(String::from_utf16_lossy(
        &file_buffer[..length],
    )))
}

fn save_pixels_as_png(path: &Path, pixels: &[u8], width: i32, height: i32) -> Result<()> {
    let file = File::create(path)?;
    encode_pixels_as_png(BufWriter::new(file), pixels, width, height)
}

fn encode_pixels_as_png<W: Write>(writer: W, pixels: &[u8], width: i32, height: i32) -> Result<()> {
    if width <= 0 || height <= 0 || pixels.len() != width as usize * height as usize * 4 {
        bail!("invalid PNG image dimensions");
    }
    let mut rgba = Vec::with_capacity(pixels.len());
    for pixel in pixels.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    let mut encoder = png::Encoder::new(writer, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgba)?;
    Ok(())
}

fn show_ocr_result(hwnd: HWND, result: Result<usize>) {
    let (text, success) = match result {
        Ok(characters) => (format!("已识别并复制 {characters} 个字符"), true),
        Err(error) => (format!("文字识别失败：{error:#}"), false),
    };
    super::hints::show_ocr_toast(hwnd.0 as isize, text, success);
}

unsafe fn show_pin_error(hwnd: HWND, message: &str) {
    let mut wide: Vec<u16> = message.encode_utf16().collect();
    wide.push(0);
    let _ = MessageBoxW(
        hwnd,
        PCWSTR(wide.as_ptr()),
        w!("便捷窗口"),
        MB_OK | MB_ICONERROR,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{WS_CAPTION, WS_SYSMENU, WS_THICKFRAME};

    #[test]
    fn pinned_image_uses_only_a_borderless_popup_surface() {
        let style = pin_window_style();

        assert_eq!(style & WS_CAPTION, WINDOW_STYLE(0));
        assert_eq!(style & WS_SYSMENU, WINDOW_STYLE(0));
        assert_ne!(style & WS_POPUP, WINDOW_STYLE(0));
        assert_eq!(style & WS_THICKFRAME, WINDOW_STYLE(0));
    }

    #[test]
    fn small_pinned_image_keeps_its_capture_aspect_ratio() {
        assert_eq!(initial_pin_size(80, 40), (240, 120));
        assert_eq!(initial_pin_size(40, 80), (180, 360));
    }

    #[test]
    fn large_pinned_image_is_scaled_down_without_distortion() {
        assert_eq!(initial_pin_size(2000, 1000), (1200, 600));
    }

    #[test]
    fn pin_starts_at_the_gesture_endpoint_when_not_overlapping() {
        let capture = Rect {
            left: 100,
            top: 100,
            right: 140,
            bottom: 140,
        };
        let desktop = Rect {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 800,
        };
        assert_eq!(
            pin_position(capture, Some(Point { x: 500, y: 300 }), 180, 120, desktop),
            (500, 300)
        );
    }

    #[test]
    fn pin_moves_down_and_right_only_when_it_overlaps_capture() {
        let capture = Rect {
            left: 100,
            top: 100,
            right: 300,
            bottom: 220,
        };
        let desktop = Rect {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 800,
        };
        assert_eq!(
            pin_position(capture, Some(Point { x: 120, y: 120 }), 180, 120, desktop),
            (136, 136)
        );
    }

    #[test]
    fn pin_position_supports_negative_virtual_desktop_coordinates() {
        let capture = Rect {
            left: -700,
            top: 100,
            right: -500,
            bottom: 220,
        };
        let desktop = Rect {
            left: -1920,
            top: -200,
            right: 1920,
            bottom: 1080,
        };
        assert_eq!(
            pin_position(capture, Some(Point { x: -900, y: 300 }), 180, 120, desktop),
            (-900, 300)
        );
    }

    #[test]
    fn pin_position_clamps_to_virtual_desktop_after_overlap_offset() {
        let capture = Rect {
            left: 900,
            top: 700,
            right: 1000,
            bottom: 800,
        };
        let desktop = Rect {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 800,
        };
        assert_eq!(
            pin_position(capture, Some(Point { x: 900, y: 700 }), 180, 120, desktop),
            (820, 680)
        );
    }

    #[test]
    fn client_edge_resize_preserves_a_minimum_image_surface() {
        let edges = resize_edges_at(2, 118, 160, 120, 10);
        assert!(edges.contains(ResizeEdges::LEFT));
        assert!(edges.contains(ResizeEdges::BOTTOM));

        let rect = resized_window_rect(
            ResizeSession {
                edges,
                cursor: POINT { x: 100, y: 100 },
                window: RECT {
                    left: 20,
                    top: 30,
                    right: 180,
                    bottom: 150,
                },
            },
            POINT { x: 240, y: 160 },
        );

        assert_eq!(rect.left, 100);
        assert_eq!(rect.bottom, 210);
        assert_eq!(rect.right - rect.left, 80);
    }

    #[test]
    fn resize_edges_map_to_native_cursor_directions() {
        assert_eq!(
            resize_cursor_kind(ResizeEdges(ResizeEdges::LEFT)),
            Some(ResizeCursor::WestEast)
        );
        assert_eq!(
            resize_cursor_kind(ResizeEdges(ResizeEdges::TOP | ResizeEdges::RIGHT)),
            Some(ResizeCursor::NorthEastSouthWest)
        );
        assert_eq!(resize_cursor_kind(ResizeEdges::default()), None);
    }

    #[test]
    fn clipboard_dib_uses_bottom_up_rows_without_resizing_pixels() {
        let top = [1_u8, 2, 3, 0];
        let bottom = [4_u8, 5, 6, 0];
        let dib = clipboard_dib(&[top, bottom].concat(), 1, 2).unwrap();
        let header_size = size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>();

        assert_eq!(&dib[header_size..header_size + 4], &bottom);
        assert_eq!(&dib[header_size + 4..], &top);
    }

    #[test]
    fn png_export_keeps_original_dimensions_and_converts_bgra_to_opaque_rgba() {
        let mut encoded = Vec::new();
        encode_pixels_as_png(&mut encoded, &[10, 20, 30, 0, 40, 50, 60, 0], 2, 1).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(encoded));
        let mut reader = decoder.read_info().unwrap();
        let mut output = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut output).unwrap();

        assert_eq!((info.width, info.height), (2, 1));
        assert_eq!(
            &output[..info.buffer_size()],
            &[30, 20, 10, 255, 60, 50, 40, 255]
        );
    }
}
