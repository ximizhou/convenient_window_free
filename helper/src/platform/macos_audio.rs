use super::adjustment::Level;
use anyhow::{ensure, Context, Result};
use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;

#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}

const GLOBAL: u32 = u32::from_be_bytes(*b"glob");
const OUTPUT: u32 = u32::from_be_bytes(*b"outp");
const VOLUME: u32 = u32::from_be_bytes(*b"volm");
const MUTE: u32 = u32::from_be_bytes(*b"mute");

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectGetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
    fn AudioObjectHasProperty(object: u32, address: *const Address) -> u8;
    fn AudioObjectIsPropertySettable(
        object: u32,
        address: *const Address,
        settable: *mut u8,
    ) -> i32;
    fn AudioObjectGetPropertyDataSize(
        object: u32,
        address: *const Address,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
    ) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct AudioBuffer {
    channels: u32,
    byte_size: u32,
    data: *mut c_void,
}

#[repr(C)]
struct AudioBufferList {
    count: u32,
    buffers: [AudioBuffer; 1],
}

fn output_channels(device: u32) -> Result<u32> {
    let address = Address {
        selector: u32::from_be_bytes(*b"slay"),
        scope: OUTPUT,
        element: 0,
    };
    let mut size = 0;
    let status =
        unsafe { AudioObjectGetPropertyDataSize(device, &address, 0, std::ptr::null(), &mut size) };
    ensure!(
        status == 0 && (4..=65536).contains(&size),
        "无法读取输出声道布局 ({status})"
    );
    // Align the storage for AudioBufferList, including its pointer fields.
    let mut storage = vec![0usize; (size as usize).div_ceil(std::mem::size_of::<usize>())];
    let capacity = size;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            storage.as_mut_ptr().cast(),
        )
    };
    ensure!(
        status == 0 && size <= capacity && size >= 4,
        "输出声道布局已变更 ({status})"
    );
    let bytes = storage.as_ptr().cast::<u8>();
    let count = unsafe { bytes.cast::<u32>().read() } as usize;
    let offset = std::mem::offset_of!(AudioBufferList, buffers);
    ensure!(
        count <= (size as usize).saturating_sub(offset) / std::mem::size_of::<AudioBuffer>(),
        "无效的输出声道布局"
    );
    let mut channels = 0u32;
    for index in 0..count {
        let buffer = unsafe {
            bytes
                .add(offset + index * std::mem::size_of::<AudioBuffer>())
                .cast::<AudioBuffer>()
                .read()
        };
        channels = channels
            .checked_add(buffer.channels)
            .context("输出声道数量溢出")?;
    }
    ensure!(channels <= 1024, "输出声道数量超出范围");
    Ok(channels)
}

fn get<T: Copy + Default>(object: u32, address: &Address) -> Result<T> {
    let mut value = T::default();
    let mut size = std::mem::size_of::<T>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            &mut size,
            (&mut value as *mut T).cast(),
        )
    };
    ensure!(
        status == 0 && size as usize == std::mem::size_of::<T>(),
        "音频属性读取失败 ({status})"
    );
    Ok(value)
}

fn set<T>(object: u32, address: &Address, value: T) -> Result<()> {
    let status = unsafe {
        AudioObjectSetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            std::mem::size_of::<T>() as u32,
            (&value as *const T).cast(),
        )
    };
    ensure!(status == 0, "音频属性写入失败 ({status})");
    Ok(())
}

fn writable(object: u32, address: &Address) -> bool {
    let mut writable = 0;
    unsafe { AudioObjectIsPropertySettable(object, address, &mut writable) == 0 && writable != 0 }
}

pub(crate) fn adjust_system_volume(delta: f32) -> Result<Level> {
    let device: u32 = get(
        1,
        &Address {
            selector: u32::from_be_bytes(*b"dOut"),
            scope: GLOBAL,
            element: 0,
        },
    )?;
    ensure!(device != 0, "未找到默认音频输出设备");
    let name: CFStringRef = get(
        device,
        &Address {
            selector: u32::from_be_bytes(*b"lnam"),
            scope: GLOBAL,
            element: 0,
        },
    )?;
    ensure!(!name.is_null(), "音频设备名称为空");
    let device_name = unsafe { CFString::wrap_under_create_rule(name) }.to_string();
    let master = Address {
        selector: VOLUME,
        scope: OUTPUT,
        element: 0,
    };
    let addresses = if writable(device, &master) {
        vec![master]
    } else {
        // HAL devices may expose only per-channel controls.
        (1..=output_channels(device)?)
            .map(|element| Address {
                selector: VOLUME,
                scope: OUTPUT,
                element,
            })
            .filter(|a| writable(device, a))
            .collect::<Vec<_>>()
    };
    ensure!(!addresses.is_empty(), "此音频输出设备不支持系统音量调节");
    let levels = addresses
        .iter()
        .map(|a| get::<f32>(device, a))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        levels
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
        "无效的音量读数"
    );
    let current = levels.iter().copied().fold(0.0, f32::max);
    let next = (current + delta).clamp(0.0, 1.0);
    for (address, value) in addresses.iter().zip(levels) {
        set(
            device,
            address,
            if current > 0.0 {
                value / current * next
            } else {
                next
            },
        )?;
    }
    let mute = Address {
        selector: MUTE,
        scope: OUTPUT,
        element: 0,
    };
    let has_mute = unsafe { AudioObjectHasProperty(device, &mute) } != 0;
    let muted = has_mute && get::<u32>(device, &mute)? != 0;
    if delta > 0.0 && muted {
        ensure!(writable(device, &mute), "此音频设备无法解除静音");
        set(device, &mute, 0u32)?;
    }
    let value = addresses
        .iter()
        .map(|a| get::<f32>(device, a))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .reduce(f32::max)
        .context("音频设备没有音量控制")?;
    ensure!(
        value.is_finite() && (0.0..=1.0).contains(&value),
        "无效的音量读回值"
    );
    Ok(Level {
        value,
        muted: has_mute && get::<u32>(device, &mute)? != 0,
        device_name,
    })
}
