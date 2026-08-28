pub mod audio;
pub mod hints;
pub mod input;
pub mod keyboard;
pub mod monitor;
pub mod mouse;
pub mod ocr;
pub mod screenshot;
pub mod topmost_pin;
pub mod window;

pub use audio::*;
pub use hints::*;
pub use input::*;
pub use keyboard::*;
pub use monitor::*;
pub use mouse::*;
pub use ocr::*;
pub use screenshot::*;
pub use topmost_pin::*;
pub use window::*;

/// Windows renders the captured bitmap in a native pin window. Unix backends
/// save the bitmap to a file and expose that path through the same engine hook.
pub fn take_capture_result() -> Option<String> {
    None
}

#[cfg(test)]
pub(crate) fn assert_capture_exclusion_affinity(affinity: u32) {
    use std::mem::size_of;
    use windows::Win32::System::SystemInformation::{
        VerSetConditionMask, VerifyVersionInfoW, OSVERSIONINFOEXW, VER_PRODUCT_TYPE,
    };
    use windows::Win32::UI::WindowsAndMessaging::WDA_EXCLUDEFROMCAPTURE;

    const VER_EQUAL: u8 = 1;
    const VER_NT_WORKSTATION: u8 = 1;

    let mut version = OSVERSIONINFOEXW {
        dwOSVersionInfoSize: size_of::<OSVERSIONINFOEXW>() as u32,
        wProductType: VER_NT_WORKSTATION,
        ..Default::default()
    };
    let condition_mask = unsafe { VerSetConditionMask(0, VER_PRODUCT_TYPE, VER_EQUAL) };
    let is_workstation =
        unsafe { VerifyVersionInfoW(&mut version, VER_PRODUCT_TYPE, condition_mask).is_ok() };

    if is_workstation {
        assert_eq!(affinity, WDA_EXCLUDEFROMCAPTURE.0);
    } else {
        assert!(
            affinity == 0 || affinity == WDA_EXCLUDEFROMCAPTURE.0,
            "Windows Server returned unexpected display affinity {affinity}"
        );
    }
}
