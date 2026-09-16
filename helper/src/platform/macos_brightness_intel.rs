use super::native::{verify_display, Library};
use crate::platform::brightness::adjusted_brightness;
use crate::platform::ddc::{brightness_query, brightness_reply, brightness_write};
use anyhow::{ensure, Context, Result};
use core_foundation::base::{CFGetTypeID, CFRelease, CFTypeRef, TCFType};
use core_foundation::number::{CFNumber, CFNumberGetTypeID};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::time::Duration;

// ABI: Apple's IOI2CInterface.h uses four-byte packing, including on 64-bit hosts.
#[repr(C, packed(4))]
#[derive(Default)]
struct Request {
    send_type: u32,
    reply_type: u32,
    send_address: u32,
    reply_address: u32,
    send_subaddress: u8,
    reply_subaddress: u8,
    reserved_a: [u8; 2],
    min_reply_delay: u64,
    result: i32,
    flags: u32,
    pad_a: u32,
    send_bytes: u32,
    reserved_b: [u32; 2],
    pad_b: u32,
    reply_bytes: u32,
    completion: usize,
    send_buffer: usize,
    reply_buffer: usize,
    reserved_c: [u32; 10],
}

const _: () = {
    assert!(std::mem::size_of::<Request>() == 124);
    assert!(std::mem::align_of::<Request>() == 4);
    assert!(std::mem::offset_of!(Request, min_reply_delay) == 20);
    assert!(std::mem::offset_of!(Request, completion) == 60);
    assert!(std::mem::offset_of!(Request, send_buffer) == 68);
    assert!(std::mem::offset_of!(Request, reply_buffer) == 76);
};

struct Bus {
    connection: *mut c_void,
    reply_type: u32,
}

impl Drop for Bus {
    fn drop(&mut self) {
        unsafe {
            IOI2CInterfaceClose(self.connection, 0);
        }
    }
}

impl Bus {
    fn open(framebuffer: u32, index: u32) -> Result<Self> {
        let mut interface = 0;
        ensure!(
            unsafe { IOFBCopyI2CInterfaceForBus(framebuffer, index, &mut interface) } == 0
                && interface != 0,
            "无法打开显示器 I²C 接口"
        );
        let key = CFString::new("IOI2CTransactionTypes");
        let value = unsafe {
            IORegistryEntryCreateCFProperty(
                interface,
                key.as_concrete_TypeRef(),
                std::ptr::null(),
                0,
            )
        };
        let types = if value.is_null() {
            0
        } else if unsafe { CFGetTypeID(value) == CFNumberGetTypeID() } {
            let number = unsafe { CFNumber::wrap_under_create_rule(value.cast()) };
            number.to_i64().unwrap_or(0)
        } else {
            unsafe { CFRelease(value) };
            0
        };
        // Select a transaction type advertised by this exact bus.
        let reply_type = if types & (1 << 2) != 0 {
            2
        } else if types & (1 << 1) != 0 {
            1
        } else {
            0
        };
        let mut connection = std::ptr::null_mut();
        let status = if reply_type != 0 {
            unsafe { IOI2CInterfaceOpen(interface, 0, &mut connection) }
        } else {
            -1
        };
        unsafe { IOObjectRelease(interface) };
        ensure!(
            status == 0 && !connection.is_null(),
            "显示器没有可用的 I²C 读写通道"
        );
        Ok(Self {
            connection,
            reply_type,
        })
    }

    fn exchange(&self, send: &[u8], reply: &mut [u8]) -> Result<()> {
        let mut timebase = Timebase {
            numerator: 0,
            denominator: 0,
        };
        ensure!(
            unsafe { mach_timebase_info(&mut timebase) } == 0
                && timebase.numerator != 0
                && timebase.denominator != 0,
            "无法读取系统时钟比例"
        );
        let mut request = Request {
            send_type: 1,
            send_address: 0x6e,
            send_bytes: send.len() as u32,
            send_buffer: send.as_ptr() as usize,
            reply_type: if reply.is_empty() { 0 } else { self.reply_type },
            reply_address: 0x6f,
            reply_subaddress: 0x51,
            reply_bytes: reply.len() as u32,
            reply_buffer: reply.as_mut_ptr() as usize,
            min_reply_delay: 100_000_000u64 * u64::from(timebase.denominator)
                / u64::from(timebase.numerator),
            ..Default::default()
        };
        let status = unsafe { IOI2CSendRequest(self.connection, 0, &mut request) };
        let result = request.result;
        ensure!(
            status == 0 && result == 0,
            "显示器 I²C 操作失败 ({status}, {result})"
        );
        ensure!(
            reply.is_empty() || request.reply_bytes == reply.len() as u32,
            "显示器返回不完整的 DDC 响应"
        );
        Ok(())
    }

    fn read(&self) -> Result<(u32, u32)> {
        let mut last_error = None;
        for _ in 0..3 {
            let mut reply = [0; 11];
            match self
                .exchange(&brightness_query(), &mut reply)
                .and_then(|_| brightness_reply(&reply))
            {
                Ok(value) => return Ok(value),
                Err(error) => last_error = Some(error),
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(last_error.unwrap())
    }
}

pub(super) fn adjust(id: u32, delta: f32) -> Result<()> {
    let library = Library::open(c"/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")?;
    type Framebuffer = unsafe extern "C" fn(u32) -> u32;
    let framebuffer_for: Framebuffer =
        unsafe { std::mem::transmute(library.symbol(c"CGDisplayIOServicePort")?) };
    let framebuffer = unsafe { framebuffer_for(id) };
    ensure!(framebuffer != 0, "无法定位目标显示器的 framebuffer");
    let mut count = 0;
    ensure!(
        unsafe { IOFBGetI2CInterfaceCount(framebuffer, &mut count) } == 0
            && (1..=16).contains(&count),
        "显示器没有可用的 DDC 通道"
    );
    let mut candidate = None;
    for index in 0..count {
        let Ok(bus) = Bus::open(framebuffer, index) else {
            continue;
        };
        if let Ok((current, max)) = bus.read() {
            ensure!(candidate.is_none(), "多个 DDC 通道响应，无法唯一匹配显示器");
            candidate = Some((bus, current, max));
        }
    }
    let (bus, current, max) = candidate.context("显示器未提供亮度控制，请检查 DDC/CI 设置")?;
    let next = adjusted_brightness(0, current, max, delta)?;
    if next != current {
        verify_display(id)?;
        ensure!(
            unsafe { framebuffer_for(id) } == framebuffer,
            "显示器连接已变更"
        );
        bus.exchange(&brightness_write(next as u16), &mut [])?;
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

#[repr(C)]
struct Timebase {
    numerator: u32,
    denominator: u32,
}

unsafe extern "C" {
    fn mach_timebase_info(info: *mut Timebase) -> i32;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOFBGetI2CInterfaceCount(framebuffer: u32, count: *mut u32) -> i32;
    fn IOFBCopyI2CInterfaceForBus(framebuffer: u32, index: u32, interface: *mut u32) -> i32;
    fn IORegistryEntryCreateCFProperty(
        entry: u32,
        key: CFStringRef,
        allocator: *const c_void,
        options: u32,
    ) -> CFTypeRef;
    fn IOObjectRelease(object: u32) -> i32;
    fn IOI2CInterfaceOpen(interface: u32, options: u32, connection: *mut *mut c_void) -> i32;
    fn IOI2CInterfaceClose(connection: *mut c_void, options: u32) -> i32;
    fn IOI2CSendRequest(connection: *mut c_void, options: u32, request: *mut Request) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iokit_request_matches_the_64_bit_sdk_layout() {
        assert_eq!(std::mem::size_of::<Request>(), 124);
        assert_eq!(std::mem::align_of::<Request>(), 4);
        assert_eq!(std::mem::offset_of!(Request, min_reply_delay), 20);
        assert_eq!(std::mem::offset_of!(Request, completion), 60);
        assert_eq!(std::mem::offset_of!(Request, send_buffer), 68);
        assert_eq!(std::mem::offset_of!(Request, reply_buffer), 76);
    }
}
