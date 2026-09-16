use crate::platform::Monitor;
use anyhow::{ensure, Context, Result};

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
#[path = "macos_brightness_intel.rs"]
mod intel;

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::ffi::{c_char, c_void, CStr};

    pub(super) struct Library(*mut c_void);

    impl Library {
        pub(super) fn open(path: &CStr) -> Result<Self> {
            let handle = unsafe { dlopen(path.as_ptr(), 1) };
            ensure!(!handle.is_null(), "macOS 亮度接口不可用");
            Ok(Self(handle))
        }

        pub(super) fn symbol(&self, name: &CStr) -> Result<*mut c_void> {
            let symbol = unsafe { dlsym(self.0, name.as_ptr()) };
            ensure!(
                !symbol.is_null(),
                "macOS 缺少接口 {}",
                name.to_string_lossy()
            );
            Ok(symbol)
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                dlclose(self.0);
            }
        }
    }

    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlclose(handle: *mut c_void) -> i32;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGDisplayIsActive(display: u32) -> u32;
        fn CGDisplayIsBuiltin(display: u32) -> u32;
        fn CGDisplayIsInMirrorSet(display: u32) -> u32;
    }

    pub(super) fn verify_display(id: u32) -> Result<()> {
        ensure!(unsafe { CGDisplayIsActive(id) } != 0, "目标显示器已断开");
        ensure!(
            unsafe { CGDisplayIsInMirrorSet(id) } == 0,
            "镜像显示无法唯一确定亮度设备"
        );
        Ok(())
    }

    fn apple_brightness(id: u32, delta: f32) -> Result<()> {
        let library = Library::open(
            c"/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices",
        )?;
        type Get = unsafe extern "C" fn(u32, *mut f32) -> i32;
        type Set = unsafe extern "C" fn(u32, f32) -> i32;
        // Signatures are those exported by DisplayServices; keep its handle alive for both calls.
        let get: Get =
            unsafe { std::mem::transmute(library.symbol(c"DisplayServicesGetBrightness")?) };
        let set: Set =
            unsafe { std::mem::transmute(library.symbol(c"DisplayServicesSetBrightness")?) };
        let mut current = 0.0;
        let status = unsafe { get(id, &mut current) };
        ensure!(status == 0, "系统亮度读取失败 ({status})");
        let next = adjusted_native(current, delta)?;
        if next != current {
            verify_display(id)?;
            let status = unsafe { set(id, next) };
            ensure!(status == 0, "系统亮度写入失败 ({status})");
        }
        Ok(())
    }

    pub(crate) fn adjust(monitor: &Monitor, delta: f32) -> Result<()> {
        let id = display_id(monitor)?;
        verify_display(id)?;
        match apple_brightness(id, delta) {
            Ok(()) => Ok(()),
            Err(error) if unsafe { CGDisplayIsBuiltin(id) } != 0 => Err(error),
            Err(error) => external(id, delta).with_context(|| format!("系统亮度接口：{error:#}")),
        }
    }

    #[cfg(target_arch = "aarch64")]
    fn external(id: u32, delta: f32) -> Result<()> {
        use crate::platform::brightness::adjusted_brightness;
        use crate::platform::brightness_command::run;
        use std::path::Path;
        use std::process::Command;
        use std::time::Duration;

        // GUI apps do not inherit the user's Homebrew PATH.
        let program = ["/opt/homebrew/bin/m1ddc", "/usr/local/bin/m1ddc"]
            .into_iter()
            .find(|path| Path::new(path).is_file())
            .unwrap_or("m1ddc");
        let target = format!("id={id}");
        let timeout = Duration::from_secs(5);
        let current = parse_m1ddc_value(
            &run(
                Command::new(program).args(["display", &target, "get", "luminance"]),
                timeout,
            )
            .context("Apple Silicon 外屏需要支持 display id= 的 m1ddc 及 DDC/CI")?,
        )?;
        let max = parse_m1ddc_value(&run(
            Command::new(program).args(["display", &target, "max", "luminance"]),
            timeout,
        )?)?;
        let next = adjusted_brightness(0, current, max, delta)?;
        if next != current {
            verify_display(id)?;
            run(
                Command::new(program).args([
                    "display",
                    &target,
                    "set",
                    "luminance",
                    &next.to_string(),
                ]),
                timeout,
            )?;
        }
        Ok(())
    }

    #[cfg(target_arch = "x86_64")]
    fn external(id: u32, delta: f32) -> Result<()> {
        super::intel::adjust(id, delta)
    }
}

#[cfg(target_os = "macos")]
pub(crate) use native::adjust;

fn display_id(monitor: &Monitor) -> Result<u32> {
    let length = monitor
        .device_id
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(128);
    let name = String::from_utf16_lossy(&monitor.device_id[..length]);
    let id: u32 = name
        .strip_prefix("CGDisplay-")
        .context("无效的 macOS 显示器标识")?
        .parse()?;
    ensure!(id != 0, "无效的 macOS 显示器标识");
    Ok(id)
}

fn adjusted_native(current: f32, delta: f32) -> Result<f32> {
    ensure!(
        current.is_finite() && (0.0..=1.0).contains(&current) && delta.is_finite() && delta != 0.0,
        "无效的系统亮度数值"
    );
    Ok((current + delta).clamp(0.0, 1.0))
}

#[cfg(any(target_arch = "aarch64", test))]
fn parse_m1ddc_value(output: &str) -> Result<u32> {
    let value = output.trim();
    ensure!(
        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()),
        "m1ddc 未返回有效亮度读数"
    );
    let value = value.parse()?;
    ensure!(value <= u16::MAX as u32, "m1ddc 返回的亮度超出 DDC 范围");
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Rect;

    #[test]
    fn native_brightness_clamps_and_rejects_invalid_values() {
        assert_eq!(adjusted_native(0.5, 0.05).unwrap(), 0.55);
        assert_eq!(adjusted_native(0.98, 0.05).unwrap(), 1.0);
        assert_eq!(adjusted_native(0.02, -0.05).unwrap(), 0.0);
        for (current, delta) in [
            (f32::NAN, 0.05),
            (-1.0, 0.05),
            (0.5, f32::INFINITY),
            (0.5, 0.0),
        ] {
            assert!(adjusted_native(current, delta).is_err());
        }
    }

    #[test]
    fn display_identifiers_never_fall_back_to_the_first_display() {
        let rect = Rect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let mut monitor = Monitor {
            bounds: rect,
            work_area: rect,
            primary: true,
            device_id: [0; 128],
        };
        assert!(display_id(&monitor).is_err());
        for (dst, src) in monitor
            .device_id
            .iter_mut()
            .zip("CGDisplay-12345".encode_utf16())
        {
            *dst = src;
        }
        assert_eq!(display_id(&monitor).unwrap(), 12345);
    }

    #[test]
    fn m1ddc_errors_and_truncated_replies_are_not_brightness_values() {
        assert_eq!(parse_m1ddc_value("50\n").unwrap(), 50);
        for value in ["", "-1", "DDC communication failure\n0", "50\n100", "65536"] {
            assert!(parse_m1ddc_value(value).is_err());
        }
    }
}
