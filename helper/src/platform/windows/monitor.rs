use crate::platform::{Monitor, Rect};
use anyhow::Result;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW, HDC, HMONITOR,
    MONITORINFOEXW,
};
use windows::Win32::UI::WindowsAndMessaging::EDD_GET_DEVICE_INTERFACE_NAME;

const MONITORINFOF_PRIMARY_VALUE: u32 = 1;

pub fn monitors() -> Result<Vec<Monitor>> {
    let mut monitors = Vec::<Monitor>::new();
    let param = LPARAM((&mut monitors as *mut Vec<Monitor>) as isize);

    unsafe {
        let ok = EnumDisplayMonitors(HDC::default(), None, Some(enum_monitor), param);
        if !ok.as_bool() {
            anyhow::bail!("EnumDisplayMonitors failed");
        }
    }

    Ok(monitors)
}

unsafe extern "system" fn enum_monitor(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let monitors = &mut *(data.0 as *mut Vec<Monitor>);
    let mut info = MONITORINFOEXW {
        monitorInfo: windows::Win32::Graphics::Gdi::MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    if GetMonitorInfoW(monitor, &mut info.monitorInfo).as_bool() {
        monitors.push(Monitor {
            bounds: convert_rect(info.monitorInfo.rcMonitor),
            work_area: convert_rect(info.monitorInfo.rcWork),
            primary: (info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY_VALUE) != 0,
            device_id: monitor_device_id(&info.szDevice),
        });
    }

    true.into()
}

fn monitor_device_id(device_name: &[u16; 32]) -> [u16; 128] {
    let mut display = DISPLAY_DEVICEW {
        cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
        ..Default::default()
    };
    let found = unsafe {
        EnumDisplayDevicesW(
            PCWSTR(device_name.as_ptr()),
            0,
            &mut display,
            EDD_GET_DEVICE_INTERFACE_NAME,
        )
    };
    if found.as_bool() {
        display.DeviceID
    } else {
        [0; 128]
    }
}

fn convert_rect(rect: RECT) -> Rect {
    Rect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}
