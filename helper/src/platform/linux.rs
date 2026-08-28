use crate::config::OcrConfig;
use crate::platform::{Monitor, Point, Rect, WindowHandle, WindowInfo};
use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as RandrConnectionExt;
use x11rb::protocol::xproto::{
    self, Atom, AtomEnum, ClientMessageData, ClientMessageEvent, ConfigureWindowAux,
    ConnectionExt as XProtoConnectionExt, EventMask, ImageFormat, MapState, PropMode, StackMode,
    Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;

pub fn capabilities() -> super::super::PlatformCapabilities {
    capabilities_for_x11(has_x11_session())
}

fn capabilities_for_x11(x11: bool) -> super::super::PlatformCapabilities {
    super::super::PlatformCapabilities {
        global_input: x11,
        window_control: x11,
        window_topmost: x11,
        screen_capture: x11,
        ocr: false,
        audio: false,
        system_actions: false,
        edge_hide: false,
    }
}

pub fn preflight_input() -> Result<bool> {
    if !has_x11_session() {
        return Ok(false);
    }
    let (connection, screen) = connect()?;
    connection
        .get_input_focus()?
        .reply()
        .context("query X11 input focus")?;
    let root = connection.setup().roots[screen].root;
    let _ = connection.query_pointer(root)?.reply()?;
    Ok(true)
}

fn has_x11_session() -> bool {
    is_x11_session(
        std::env::var_os("DISPLAY").is_some(),
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
    )
}

fn is_x11_session(display: bool, session_type: Option<&str>, wayland_display: bool) -> bool {
    display
        && !session_type.is_some_and(|value| value.eq_ignore_ascii_case("wayland"))
        && !wayland_display
}

fn connect() -> Result<(RustConnection, usize)> {
    x11rb::connect(None).map_err(|error| anyhow::anyhow!("X11 connection failed: {error}"))
}

pub fn cursor_position() -> Result<Point> {
    let (connection, screen) = connect()?;
    let root = connection.setup().roots[screen].root;
    let pointer = connection.query_pointer(root)?.reply()?;
    Ok(Point {
        x: i32::from(pointer.root_x),
        y: i32::from(pointer.root_y),
    })
}

pub fn monitors() -> Result<Vec<Monitor>> {
    let (connection, screen) = connect()?;
    let root = &connection.setup().roots[screen];
    let resources = connection
        .get_screen_resources_current(root.root)?
        .reply()
        .context("query X11 RandR resources")?;
    let mut monitors = Vec::new();
    for (index, crtc) in resources.crtcs.iter().enumerate() {
        let info = connection
            .get_crtc_info(*crtc, resources.config_timestamp)?
            .reply()?;
        if info.mode == 0 || info.width == 0 || info.height == 0 {
            continue;
        }
        let name = info
            .outputs
            .first()
            .and_then(|output| {
                connection
                    .get_output_info(*output, resources.config_timestamp)
                    .ok()?
                    .reply()
                    .ok()
                    .map(|reply| String::from_utf8_lossy(&reply.name).into_owned())
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("X11-CRTC-{crtc}"));
        let bounds = Rect {
            left: i32::from(info.x),
            top: i32::from(info.y),
            right: i32::from(info.x) + i32::from(info.width),
            bottom: i32::from(info.y) + i32::from(info.height),
        };
        monitors.push(Monitor {
            bounds,
            work_area: bounds,
            primary: index == 0,
            device_id: device_id(&name),
        });
    }
    if monitors.is_empty() {
        let bounds = Rect {
            left: 0,
            top: 0,
            right: i32::from(root.width_in_pixels),
            bottom: i32::from(root.height_in_pixels),
        };
        monitors.push(Monitor {
            bounds,
            work_area: bounds,
            primary: true,
            device_id: device_id("X11-ROOT"),
        });
    }
    Ok(monitors)
}

fn device_id(name: &str) -> [u16; 128] {
    let mut result = [0; 128];
    for (target, value) in result.iter_mut().zip(name.encode_utf16()) {
        *target = value;
    }
    result
}

pub fn foreground_window() -> Result<Option<WindowInfo>> {
    let (connection, screen) = connect()?;
    let root = connection.setup().roots[screen].root;
    let active = atom(&connection, "_NET_ACTIVE_WINDOW")?;
    let handle =
        property_u32(&connection, root, active)?.and_then(|values| values.first().copied());
    let window = handle.filter(|window| *window != 0).unwrap_or_else(|| {
        connection
            .get_input_focus()
            .and_then(|cookie| cookie.reply())
            .map(|reply| reply.focus)
            .unwrap_or(0)
    });
    if window == 0 {
        return Ok(None);
    }
    window_info(&connection, root, window)
}

pub fn window_exists(handle: WindowHandle) -> bool {
    let Ok((connection, _)) = connect() else {
        return false;
    };
    connection
        .get_geometry(handle.0 as Window)
        .and_then(|cookie| cookie.reply())
        .is_ok()
}

pub fn window_is_minimized(handle: WindowHandle) -> bool {
    let Ok((connection, _)) = connect() else {
        return false;
    };
    connection
        .get_window_attributes(handle.0 as Window)
        .and_then(|cookie| cookie.reply())
        .map(|reply| reply.map_state != MapState::VIEWABLE)
        .unwrap_or(true)
}

pub fn window_info_for_handle(handle: WindowHandle) -> Result<Option<WindowInfo>> {
    let (connection, screen) = connect()?;
    window_info(
        &connection,
        connection.setup().roots[screen].root,
        handle.0 as Window,
    )
}

fn window_info(
    connection: &RustConnection,
    root: Window,
    window: Window,
) -> Result<Option<WindowInfo>> {
    let attributes = match connection.get_window_attributes(window)?.reply() {
        Ok(attributes) => attributes,
        Err(_) => return Ok(None),
    };
    if attributes.map_state != MapState::VIEWABLE || attributes.override_redirect {
        return Ok(None);
    }
    let geometry = match connection.get_geometry(window)?.reply() {
        Ok(geometry) => geometry,
        Err(_) => return Ok(None),
    };
    if geometry.width == 0 || geometry.height == 0 {
        return Ok(None);
    }
    let translated = connection
        .translate_coordinates(window, root, 0, 0)?
        .reply()?;
    let rect = Rect {
        left: i32::from(translated.dst_x),
        top: i32::from(translated.dst_y),
        right: i32::from(translated.dst_x) + i32::from(geometry.width),
        bottom: i32::from(translated.dst_y) + i32::from(geometry.height),
    };
    let wm_state = atom(connection, "_NET_WM_STATE")?;
    let above = atom(connection, "_NET_WM_STATE_ABOVE")?;
    let states = property_u32(connection, window, wm_state)?.unwrap_or_default();
    let maximized_vert = atom(connection, "_NET_WM_STATE_MAXIMIZED_VERT")?;
    let maximized_horz = atom(connection, "_NET_WM_STATE_MAXIMIZED_HORZ")?;
    let transient = connection
        .get_property(
            false,
            window,
            atom(connection, "WM_TRANSIENT_FOR")?,
            AtomEnum::ANY,
            0,
            1,
        )?
        .reply()
        .map(|reply| !reply.value.is_empty())
        .unwrap_or(false);
    let pid = property_u32(connection, window, atom(connection, "_NET_WM_PID")?)?
        .and_then(|values| values.first().copied())
        .unwrap_or_default();
    Ok(Some(WindowInfo {
        handle: WindowHandle(window as isize),
        rect,
        title: window_title(connection, window),
        class_name: window_class(connection, window),
        process_name: process_name(pid),
        maximized: states.contains(&maximized_vert) && states.contains(&maximized_horz),
        transient,
        arranged: false,
        topmost: states.contains(&above),
    }))
}

pub fn draggable_window_at(point: Point) -> Result<Option<WindowInfo>> {
    let (connection, screen) = connect()?;
    let root = connection.setup().roots[screen].root;
    let root_tree = connection.query_tree(root)?.reply()?;
    let mut selected = None;
    for child in root_tree.children {
        if let Some(candidate) = window_at_point(&connection, root, child, point)? {
            selected = Some(candidate);
        }
    }
    let Some(mut window) = selected else {
        return Ok(None);
    };
    let mut target = None;
    while window != 0 {
        if let Some(info) = window_info(&connection, root, window)? {
            if !info.transient && info.class_name != "Desktop" {
                // Keep walking so a client child resolves to its top-level
                // window, which is the handle that accepts move/resize.
                target = Some(info);
            }
        }
        let tree = match connection.query_tree(window)?.reply() {
            Ok(tree) => tree,
            Err(_) => break,
        };
        if tree.parent == root || tree.parent == 0 {
            break;
        }
        window = tree.parent;
    }
    Ok(target)
}

fn window_at_point(
    connection: &RustConnection,
    root: Window,
    window: Window,
    point: Point,
) -> Result<Option<Window>> {
    let mut selected = window_info(connection, root, window)?
        .filter(|info| info.rect.contains(point))
        .map(|_| window);
    let tree = connection.query_tree(window)?.reply()?;
    // X11 QueryTree returns children from bottom to top; later matches are
    // therefore the visible candidate at the requested screen coordinate.
    for child in tree.children {
        if let Some(candidate) = window_at_point(connection, root, child, point)? {
            selected = Some(candidate);
        }
    }
    Ok(selected)
}

pub fn set_window_rect(handle: WindowHandle, rect: Rect) -> Result<()> {
    let (connection, _) = connect()?;
    connection.configure_window(
        handle.0 as Window,
        &ConfigureWindowAux::new()
            .x(rect.left)
            .y(rect.top)
            .width(rect.width().max(1) as u32)
            .height(rect.height().max(1) as u32),
    )?;
    connection.flush()?;
    Ok(())
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

pub fn set_window_topmost(handle: WindowHandle, topmost: bool) -> Result<()> {
    let (connection, screen) = connect()?;
    let root = connection.setup().roots[screen].root;
    let state = atom(&connection, "_NET_WM_STATE")?;
    let above = atom(&connection, "_NET_WM_STATE_ABOVE")?;
    let event = ClientMessageEvent {
        response_type: xproto::CLIENT_MESSAGE_EVENT,
        format: 32,
        sequence: 0,
        window: handle.0 as Window,
        type_: state,
        data: ClientMessageData::from([if topmost { 1 } else { 0 }, above, 0, 0, 0]),
    };
    connection.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )?;
    connection.configure_window(
        handle.0 as Window,
        &ConfigureWindowAux::new().stack_mode(if topmost {
            StackMode::ABOVE
        } else {
            StackMode::BELOW
        }),
    )?;
    connection.flush()?;
    Ok(())
}

pub fn set_window_rect_topmost(handle: WindowHandle, rect: Rect, topmost: bool) -> Result<()> {
    set_window_rect(handle, rect)?;
    set_window_topmost(handle, topmost)
}

pub fn lock_screen() -> Result<()> {
    bail!(super::unsupported_message("lockScreen"))
}

pub fn show_desktop_with_modifiers(_routing_modifiers: u8) -> Result<()> {
    bail!(super::unsupported_message("showDesktop"))
}

pub fn capture_and_save(rect: Rect, _end_point: Option<Point>, _ocr: &OcrConfig) -> Result<String> {
    let width = rect.width().clamp(1, i32::from(u16::MAX)) as u16;
    let height = rect.height().clamp(1, i32::from(u16::MAX)) as u16;
    let (connection, screen) = connect()?;
    let root = &connection.setup().roots[screen];
    let image = connection
        .get_image(
            ImageFormat::Z_PIXMAP,
            root.root,
            rect.left as i16,
            rect.top as i16,
            width,
            height,
            u32::MAX,
        )?
        .reply()
        .context("capture X11 image")?;
    let rgba = bgra_to_rgba(&image.data, width as usize, height as usize)?;
    let path = capture_path();
    let file = File::create(&path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), u32::from(width), u32::from(height));
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgba)?;
    Ok(path.to_string_lossy().into_owned())
}

fn bgra_to_rgba(data: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let expected = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .context("X11 image dimensions overflow")?;
    if data.len() < expected {
        bail!(
            "X11 image returned {} bytes, expected at least {expected}",
            data.len()
        )
    }
    let mut rgba = Vec::with_capacity(expected);
    for pixel in data[..expected].chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
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

fn atom(connection: &RustConnection, name: &str) -> Result<Atom> {
    Ok(connection
        .intern_atom(false, name.as_bytes())?
        .reply()?
        .atom)
}

fn property_u32(
    connection: &RustConnection,
    window: Window,
    property: Atom,
) -> Result<Option<Vec<u32>>> {
    let reply = connection
        .get_property(false, window, property, AtomEnum::ANY, 0, u32::MAX)?
        .reply()?;
    Ok(reply.value32().map(|values| values.collect()))
}

fn window_title(connection: &RustConnection, window: Window) -> String {
    let utf8 = atom(connection, "UTF8_STRING").ok();
    let net_name = atom(connection, "_NET_WM_NAME").ok();
    for property in [net_name, Some(AtomEnum::WM_NAME.into())]
        .into_iter()
        .flatten()
    {
        let type_ = utf8.unwrap_or(AtomEnum::STRING.into());
        if let Ok(reply) = connection
            .get_property(false, window, property, type_, 0, 4096)
            .and_then(|cookie| cookie.reply())
        {
            let value = String::from_utf8_lossy(&reply.value)
                .trim_matches('\0')
                .to_string();
            if !value.is_empty() {
                return value;
            }
        }
    }
    String::new()
}

fn window_class(connection: &RustConnection, window: Window) -> String {
    connection
        .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
        .and_then(|cookie| cookie.reply())
        .map(|reply| {
            String::from_utf8_lossy(&reply.value)
                .split('\0')
                .nth(1)
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default()
}

fn process_name(pid: u32) -> String {
    if pid == 0 {
        return String::new();
    }
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use x11rb::protocol::xproto::{CreateWindowAux, WindowClass};

    #[test]
    fn bgra_conversion_is_opaque_and_preserves_dimensions() {
        assert_eq!(
            bgra_to_rgba(&[10, 20, 30, 0, 40, 50, 60, 0], 2, 1).unwrap(),
            vec![30, 20, 10, 255, 60, 50, 40, 255]
        );
    }

    #[test]
    fn wayland_session_is_never_reported_as_x11() {
        assert!(!is_x11_session(true, Some("wayland"), true));
        assert!(!is_x11_session(true, Some("wayland"), false));
        assert!(is_x11_session(true, Some("x11"), false));
    }

    #[test]
    fn wayland_capabilities_are_explicitly_degraded() {
        let capabilities = capabilities_for_x11(false);
        assert!(!capabilities.global_input);
        assert!(!capabilities.window_control);
        assert!(!capabilities.window_topmost);
        assert!(!capabilities.screen_capture);
        assert!(!capabilities.edge_hide);
        assert!(!capabilities.system_actions);
    }

    #[test]
    fn x11_capabilities_cover_the_p0_window_surface() {
        let capabilities = capabilities_for_x11(true);
        assert!(capabilities.global_input);
        assert!(capabilities.window_control);
        assert!(capabilities.window_topmost);
        assert!(capabilities.screen_capture);
        assert!(!capabilities.ocr);
        assert!(!capabilities.audio);
        assert!(!capabilities.system_actions);
        assert!(!capabilities.edge_hide);
    }

    #[test]
    fn real_x11_window_move_topmost_and_capture_smoke() {
        assert!(
            has_x11_session(),
            "real X11 smoke requires DISPLAY with XDG_SESSION_TYPE=x11"
        );
        let (connection, screen_index) = connect().unwrap();
        let screen = &connection.setup().roots[screen_index];
        let window = connection.generate_id().unwrap();
        connection
            .create_window(
                screen.root_depth,
                window,
                screen.root,
                12,
                18,
                96,
                72,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new().background_pixel(screen.white_pixel),
            )
            .unwrap();
        connection
            .change_property8(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                b"Convenient Window X11 smoke",
            )
            .unwrap();
        connection.map_window(window).unwrap();
        connection.flush().unwrap();

        let target = Rect {
            left: 40,
            top: 50,
            right: 160,
            bottom: 140,
        };
        let handle = WindowHandle(window as isize);
        set_window_rect(handle, target).unwrap();
        set_window_topmost(handle, true).unwrap();
        connection.flush().unwrap();
        let geometry = connection.get_geometry(window).unwrap().reply().unwrap();
        assert_eq!((geometry.width, geometry.height), (120, 90));
        assert_eq!(
            window_info_for_handle(handle).unwrap().unwrap().rect,
            target
        );
        assert_eq!(
            draggable_window_at(Point { x: 80, y: 90 })
                .unwrap()
                .unwrap()
                .handle,
            handle
        );

        let capture = capture_and_save(target, None, &OcrConfig::default()).unwrap();
        let bytes = std::fs::read(&capture).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        std::fs::remove_file(capture).unwrap();
        connection.destroy_window(window).unwrap();
        connection.flush().unwrap();
    }
}
