use crate::config::OcrConfig;
use crate::platform::{Monitor, Point, Rect, WindowHandle, WindowInfo};
use anyhow::{bail, Context, Result};
use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFRelease, CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::access::ScreenCaptureAccess;
use core_graphics::display::{CGDisplay, CGPoint, CGRect};
use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::window::{
    kCGWindowBounds, kCGWindowIsOnscreen, kCGWindowLayer, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly, kCGWindowName, kCGWindowNumber, kCGWindowOwnerName,
    kCGWindowOwnerPID, CGWindowListOption,
};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

type WindowDictionary = CFDictionary<CFString, CFType>;

pub fn capabilities() -> super::super::PlatformCapabilities {
    let accessibility = accessibility_trusted();
    super::super::PlatformCapabilities {
        global_input: accessibility,
        window_control: accessibility,
        window_topmost: false,
        screen_capture: ScreenCaptureAccess::default().preflight(),
        ocr: false,
        audio: false,
        system_actions: false,
        edge_hide: false,
    }
}

pub fn preflight_input() -> Result<bool> {
    if !accessibility_trusted() {
        // Ask macOS to show the standard Accessibility permission entry once.
        let _ = request_accessibility_prompt();
    }
    // Re-read after prompting so a newly granted permission takes effect
    // without requiring a helper restart.
    Ok(accessibility_trusted())
}

fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

fn request_accessibility_prompt() -> bool {
    let key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
    let value = CFBoolean::true_value();
    let options = CFDictionary::from_CFType_pairs(&[(key, value)]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

pub fn cursor_position() -> Result<Point> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow::anyhow!("无法创建 macOS 输入事件源"))?;
    let event = CGEvent::new(source).map_err(|_| anyhow::anyhow!("无法读取 macOS 光标位置"))?;
    let point = event.location();
    Ok(Point {
        x: point.x.round() as i32,
        y: point.y.round() as i32,
    })
}

pub fn monitors() -> Result<Vec<Monitor>> {
    let ids = CGDisplay::active_displays()
        .map_err(|error| anyhow::anyhow!("查询 macOS 显示器失败: {error:?}"))?;
    let mut result = Vec::new();
    for (index, id) in ids.into_iter().enumerate() {
        let display = CGDisplay::new(id);
        let bounds = display.bounds();
        let rect = Rect {
            left: bounds.origin.x.round() as i32,
            top: bounds.origin.y.round() as i32,
            right: (bounds.origin.x + bounds.size.width).round() as i32,
            bottom: (bounds.origin.y + bounds.size.height).round() as i32,
        };
        result.push(Monitor {
            bounds: rect,
            work_area: rect,
            primary: index == 0,
            device_id: device_id(id),
        });
    }
    if result.is_empty() {
        bail!("macOS 没有可用显示器")
    }
    Ok(result)
}

fn device_id(id: u32) -> [u16; 128] {
    let mut result = [0; 128];
    for (target, value) in result
        .iter_mut()
        .zip(format!("CGDisplay-{id}").encode_utf16())
    {
        *target = value;
    }
    result
}

pub fn foreground_window() -> Result<Option<WindowInfo>> {
    let windows = window_list()?;
    Ok(windows.into_iter().next())
}

pub fn window_exists(handle: WindowHandle) -> bool {
    window_list()
        .map(|windows| windows.iter().any(|window| window.handle.0 == handle.0))
        .unwrap_or(false)
}

pub fn window_is_minimized(handle: WindowHandle) -> bool {
    !window_exists(handle)
}

pub fn window_info_for_handle(handle: WindowHandle) -> Result<Option<WindowInfo>> {
    Ok(window_list()?
        .into_iter()
        .find(|window| window.handle == handle))
}

pub fn draggable_window_at(point: Point) -> Result<Option<WindowInfo>> {
    Ok(window_list()?
        .into_iter()
        .find(|window| window.rect.contains(point) && !window.transient && window.rect.width() > 0))
}

pub fn set_window_rect(handle: WindowHandle, rect: Rect) -> Result<()> {
    ax_set_window_rect(handle, rect)
}

pub fn toggle_window_topmost_at(point: Option<Point>) -> Result<(String, bool)> {
    let target = match point {
        Some(point) => draggable_window_at(point)?,
        None => foreground_window()?,
    };
    let Some(window) = target else {
        bail!("未找到可置顶的窗口")
    };
    let topmost = !window.topmost;
    set_window_topmost(window.handle, topmost)?;
    Ok((window.title, topmost))
}

pub fn set_window_topmost(_handle: WindowHandle, _topmost: bool) -> Result<()> {
    bail!(super::unsupported_message(
        "windowTopmost on macOS Accessibility API"
    ))
}

pub fn set_window_rect_topmost(handle: WindowHandle, rect: Rect, _topmost: bool) -> Result<()> {
    set_window_rect(handle, rect)
}

pub fn lock_screen() -> Result<()> {
    bail!(super::unsupported_message("lockScreen"))
}

pub fn show_desktop_with_modifiers(_routing_modifiers: u8) -> Result<()> {
    bail!(super::unsupported_message("showDesktop"))
}

pub fn capture_and_save(rect: Rect, _end_point: Option<Point>, _ocr: &OcrConfig) -> Result<String> {
    let capture_access = ScreenCaptureAccess::default();
    if !capture_access.preflight() {
        let _ = capture_access.request();
    }
    if !capture_access.preflight() {
        bail!("macOS 屏幕录制权限未授予，请在系统设置的隐私与安全性中允许本应用")
    }
    let bounds = CGRect::new(
        &CGPoint::new(rect.left as f64, rect.top as f64),
        &core_graphics::geometry::CGSize::new(
            rect.width().max(1) as f64,
            rect.height().max(1) as f64,
        ),
    );
    let image = CGDisplay::screenshot(
        bounds,
        kCGWindowListOptionOnScreenOnly,
        0,
        core_graphics::window::kCGWindowImageBestResolution,
    )
    .context("macOS 无法捕获指定区域")?;
    let width = image.width();
    let height = image.height();
    let data = image.data();
    let rgba = bgra_to_rgba(data.bytes(), width, height, image.bytes_per_row())?;
    let path = capture_path();
    let file = File::create(&path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgba)?;
    Ok(path.to_string_lossy().into_owned())
}

fn window_list() -> Result<Vec<WindowInfo>> {
    let options: CGWindowListOption =
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let Some(array) = CGDisplay::window_list_info(options, None) else {
        bail!("无法读取 macOS 窗口列表")
    };
    let mut result = Vec::new();
    for raw in array.get_all_values() {
        let dictionary: WindowDictionary =
            unsafe { TCFType::wrap_under_get_rule(raw as CFDictionaryRef) };
        let (layer, onscreen, number, pid, bounds, owner, title) = unsafe {
            (
                cf_number(&dictionary, kCGWindowLayer).unwrap_or(1),
                cf_bool(&dictionary, kCGWindowIsOnscreen).unwrap_or(true),
                cf_number(&dictionary, kCGWindowNumber).unwrap_or_default(),
                cf_number(&dictionary, kCGWindowOwnerPID).unwrap_or_default(),
                cf_rect(&dictionary, kCGWindowBounds),
                cf_string(&dictionary, kCGWindowOwnerName).unwrap_or_default(),
                cf_string(&dictionary, kCGWindowName).unwrap_or_default(),
            )
        };
        if layer != 0 || !onscreen {
            continue;
        }
        let Some(rect) = bounds else {
            continue;
        };
        if rect.width() <= 0 || rect.height() <= 0 {
            continue;
        }
        result.push(WindowInfo {
            handle: WindowHandle(encode_window_handle(pid, number)),
            rect,
            title,
            class_name: owner.clone(),
            process_name: owner,
            maximized: false,
            transient: false,
            arranged: false,
            topmost: false,
        });
    }
    Ok(result)
}

fn encode_window_handle(pid: i32, number: i32) -> isize {
    ((u64::from(pid.max(0) as u32) << 32) | u64::from(number.max(0) as u32)) as isize
}

fn decode_window_handle(handle: WindowHandle) -> (i32, i32) {
    let value = handle.0 as u64;
    ((value >> 32) as i32, value as u32 as i32)
}

fn cf_type(dictionary: &WindowDictionary, key: CFStringRef) -> Option<CFType> {
    let key = unsafe { CFString::wrap_under_get_rule(key) };
    dictionary.find(&key).map(|value| value.clone())
}

fn cf_number(dictionary: &WindowDictionary, key: CFStringRef) -> Option<i32> {
    cf_type(dictionary, key)?.downcast::<CFNumber>()?.to_i32()
}

fn cf_bool(dictionary: &WindowDictionary, key: CFStringRef) -> Option<bool> {
    let value = cf_type(dictionary, key)?;
    Some(bool::from(value.downcast::<CFBoolean>()?))
}

fn cf_string(dictionary: &WindowDictionary, key: CFStringRef) -> Option<String> {
    cf_type(dictionary, key)
        .and_then(|value| value.downcast::<CFString>())
        .map(|value| value.to_string())
}

fn cf_rect(dictionary: &WindowDictionary, key: CFStringRef) -> Option<Rect> {
    let value = cf_type(dictionary, key)?;
    let raw = value.as_CFTypeRef() as CFDictionaryRef;
    let bounds: WindowDictionary = unsafe { TCFType::wrap_under_get_rule(raw) };
    let x = cf_number_name(&bounds, "X")?;
    let y = cf_number_name(&bounds, "Y")?;
    let width = cf_number_name(&bounds, "Width")?;
    let height = cf_number_name(&bounds, "Height")?;
    Some(Rect {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    })
}

fn cf_number_name(dictionary: &WindowDictionary, name: &str) -> Option<i32> {
    let key = CFString::new(name);
    dictionary
        .find(&key)
        .and_then(|value| value.downcast::<CFNumber>())
        .and_then(|value| value.to_i32())
}

fn bgra_to_rgba(data: &[u8], width: usize, height: usize, stride: usize) -> Result<Vec<u8>> {
    let row_bytes = width.checked_mul(4).context("macOS 图像宽度溢出")?;
    if stride < row_bytes || data.len() < stride.saturating_mul(height) {
        bail!("macOS 图像数据长度不足")
    }
    let mut rgba = Vec::with_capacity(row_bytes * height);
    for row in 0..height {
        for pixel in data[row * stride..row * stride + row_bytes].chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    Ok(rgba)
}

fn capture_path() -> PathBuf {
    crate::paths::data_file(&format!(
        "screenshot-{}.png",
        chrono::Local::now().format("%Y%m%d-%H%M%S-%3f")
    ))
    .unwrap_or_else(|| std::env::temp_dir().join("convenient-window-screenshot.png"))
}

fn ax_set_window_rect(handle: WindowHandle, rect: Rect) -> Result<()> {
    if !accessibility_trusted() {
        bail!("macOS 辅助功能权限未授予，请在系统设置的隐私与安全性中允许本应用控制电脑")
    }
    let Some(target) = window_info_for_handle(handle)? else {
        bail!("目标窗口已关闭或无法访问")
    };
    let (pid, _) = decode_window_handle(handle);
    if pid <= 0 {
        bail!("目标窗口进程无效")
    }
    let application = unsafe { AXUIElementCreateApplication(pid) };
    if application.is_null() {
        bail!("无法创建 macOS 辅助功能应用对象")
    }
    let result = ax_find_window_and_set_rect(application, &target, rect);
    unsafe { CFRelease(application as CFTypeRef) };
    result
}

fn ax_find_window_and_set_rect(
    application: AXUIElementRef,
    target: &WindowInfo,
    rect: Rect,
) -> Result<()> {
    let windows = ax_copy_attribute(application, unsafe { kAXWindowsAttribute })?
        .ok_or_else(|| anyhow::anyhow!("macOS 应用未返回可访问窗口列表"))?;
    let windows = unsafe { CFArray::<CFType>::wrap_under_create_rule(windows as CFArrayRef) };
    for raw in windows.get_all_values() {
        let window = raw as AXUIElementRef;
        if !ax_window_matches(window, target) {
            continue;
        }
        let position = CGPoint::new(rect.left as f64, rect.top as f64);
        let size = core_graphics::geometry::CGSize::new(
            rect.width().max(1) as f64,
            rect.height().max(1) as f64,
        );
        let position_value =
            unsafe { AXValueCreate(AXVALUE_TYPE_CGPOINT, &position as *const _ as *const _) };
        let size_value =
            unsafe { AXValueCreate(AXVALUE_TYPE_CGSIZE, &size as *const _ as *const _) };
        if position_value.is_null() || size_value.is_null() {
            if !position_value.is_null() {
                unsafe { CFRelease(position_value as CFTypeRef) };
            }
            if !size_value.is_null() {
                unsafe { CFRelease(size_value as CFTypeRef) };
            }
            bail!("无法创建 macOS 窗口位置参数")
        }
        let position_result = unsafe {
            AXUIElementSetAttributeValue(window, kAXPositionAttribute, position_value as CFTypeRef)
        };
        let size_result = unsafe {
            AXUIElementSetAttributeValue(window, kAXSizeAttribute, size_value as CFTypeRef)
        };
        unsafe {
            CFRelease(position_value as CFTypeRef);
            CFRelease(size_value as CFTypeRef);
        }
        if position_result != AXERROR_SUCCESS || size_result != AXERROR_SUCCESS {
            bail!("macOS 辅助功能拒绝修改窗口位置或大小")
        }
        return Ok(());
    }
    bail!("未找到目标窗口的 macOS 辅助功能对象")
}

fn ax_window_matches(window: AXUIElementRef, target: &WindowInfo) -> bool {
    let title = ax_copy_attribute(window, unsafe { kAXTitleAttribute })
        .ok()
        .flatten()
        .and_then(|value| {
            let value = unsafe { CFString::wrap_under_create_rule(value as CFStringRef) };
            Some(value.to_string())
        })
        .unwrap_or_default();
    if !target.title.is_empty() && !title.is_empty() && title != target.title {
        return false;
    }
    let Some(position) = ax_copy_value::<CGPoint>(
        window,
        unsafe { kAXPositionAttribute },
        AXVALUE_TYPE_CGPOINT,
    ) else {
        return false;
    };
    let Some(size) = ax_copy_value::<core_graphics::geometry::CGSize>(
        window,
        unsafe { kAXSizeAttribute },
        AXVALUE_TYPE_CGSIZE,
    ) else {
        return false;
    };
    (position.x.round() as i32 - target.rect.left).abs() <= 2
        && (position.y.round() as i32 - target.rect.top).abs() <= 2
        && (size.width.round() as i32 - target.rect.width()).abs() <= 2
        && (size.height.round() as i32 - target.rect.height()).abs() <= 2
}

fn ax_copy_attribute(element: AXUIElementRef, attribute: CFStringRef) -> Result<Option<CFTypeRef>> {
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe { AXUIElementCopyAttributeValue(element, attribute, &mut value) };
    if error == AXERROR_NO_VALUE {
        return Ok(None);
    }
    if error != AXERROR_SUCCESS || value.is_null() {
        bail!("macOS 辅助功能属性读取失败（错误码 {error}）")
    }
    Ok(Some(value))
}

fn ax_copy_value<T: Copy>(
    element: AXUIElementRef,
    attribute: CFStringRef,
    value_type: u32,
) -> Option<T> {
    let value = ax_copy_attribute(element, attribute).ok().flatten()?;
    let mut output = std::mem::MaybeUninit::<T>::uninit();
    let ok = unsafe {
        AXValueGetValue(
            value as AXValueRef,
            value_type,
            output.as_mut_ptr() as *mut _,
        )
    };
    unsafe { CFRelease(value) };
    if ok {
        Some(unsafe { output.assume_init() })
    } else {
        None
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> i32;
    fn AXValueCreate(value_type: u32, value_ptr: *const std::ffi::c_void) -> AXValueRef;
    fn AXValueGetValue(
        value: AXValueRef,
        value_type: u32,
        value_ptr: *mut std::ffi::c_void,
    ) -> bool;
}

// The macOS 15 SDK text-based stubs stop re-exporting HIServices data
// symbols through the ApplicationServices umbrella in release links, so the
// AX attribute constants must link against the subframework directly.
#[link(name = "HIServices", kind = "framework")]
unsafe extern "C" {
    static kAXWindowsAttribute: CFStringRef;
    static kAXTitleAttribute: CFStringRef;
    static kAXPositionAttribute: CFStringRef;
    static kAXSizeAttribute: CFStringRef;
}

type AXUIElementRef = *const std::ffi::c_void;
type AXValueRef = *const std::ffi::c_void;
const AXERROR_SUCCESS: i32 = 0;
const AXERROR_NO_VALUE: i32 = -25212;
const AXVALUE_TYPE_CGPOINT: u32 = 1;
const AXVALUE_TYPE_CGSIZE: u32 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_conversion_honors_stride() {
        let data = [10, 20, 30, 0, 0, 0, 0, 0, 40, 50, 60, 0, 0, 0, 0, 0];
        assert_eq!(
            bgra_to_rgba(&data, 1, 2, 8).unwrap(),
            vec![30, 20, 10, 255, 60, 50, 40, 255]
        );
    }
}
