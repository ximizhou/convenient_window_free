use crate::platform::adjustment::Level;
use anyhow::Result;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use windows::core::{implement, PCWSTR, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, MMDeviceEnumerator, DEVICE_STATE,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct Output {
    endpoint: IAudioEndpointVolume,
    name: String,
}

#[implement(IMMNotificationClient)]
struct DeviceChanges(Arc<AtomicBool>);

#[allow(non_snake_case)]
impl IMMNotificationClient_Impl for DeviceChanges_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
        self.0.store(true, Ordering::Release);
        Ok(())
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.0.store(true, Ordering::Release);
        Ok(())
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
        self.0.store(true, Ordering::Release);
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _: &PCWSTR,
    ) -> windows::core::Result<()> {
        if flow == eRender && role == eMultimedia {
            self.0.store(true, Ordering::Release);
        }
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, key: &PROPERTYKEY) -> windows::core::Result<()> {
        if *key == PKEY_Device_FriendlyName {
            self.0.store(true, Ordering::Release);
        }
        Ok(())
    }
}

struct AudioSession {
    output: Option<Output>,
    enumerator: IMMDeviceEnumerator,
    notifications: IMMNotificationClient,
    changed: Arc<AtomicBool>,
    // COM interfaces must be released before the apartment is uninitialized.
    _apartment: Apartment,
}

impl AudioSession {
    fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let apartment = Apartment;
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let changed = Arc::new(AtomicBool::new(true));
            let notifications: IMMNotificationClient = DeviceChanges(changed.clone()).into();
            enumerator.RegisterEndpointNotificationCallback(&notifications)?;
            Ok(Self {
                output: None,
                enumerator,
                notifications,
                changed,
                _apartment: apartment,
            })
        }
    }

    fn adjust(&mut self, delta: f32) -> Result<Level> {
        unsafe {
            // Consume invalidation before querying so concurrent notifications survive.
            if self.changed.swap(false, Ordering::AcqRel) || self.output.is_none() {
                let device = self
                    .enumerator
                    .GetDefaultAudioEndpoint(eRender, eMultimedia)?;
                let endpoint = device.Activate(CLSCTX_ALL, None)?;
                let properties = device.OpenPropertyStore(STGM_READ)?;
                let name = take_com_string(PropVariantToStringAlloc(
                    &properties.GetValue(&PKEY_Device_FriendlyName)?,
                )?)?;
                self.output = Some(Output { endpoint, name });
            }
            let output = self.output.as_ref().unwrap();
            let current = output.endpoint.GetMasterVolumeLevelScalar()?;
            let target = (current + delta).clamp(0.0, 1.0);
            if target != current {
                output
                    .endpoint
                    .SetMasterVolumeLevelScalar(target, std::ptr::null())?;
            }
            if delta > 0.0 && output.endpoint.GetMute()?.as_bool() {
                output.endpoint.SetMute(false, std::ptr::null())?;
            }
            Ok(Level {
                value: output.endpoint.GetMasterVolumeLevelScalar()?,
                muted: output.endpoint.GetMute()?.as_bool(),
                device_name: output.name.clone(),
            })
        }
    }
}

impl Drop for AudioSession {
    fn drop(&mut self) {
        unsafe {
            let _ = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.notifications);
        }
    }
}

unsafe fn take_com_string(value: PWSTR) -> Result<String> {
    let text = value.to_string();
    CoTaskMemFree(Some(value.0.cast()));
    Ok(text?)
}

thread_local! {
    static AUDIO: RefCell<Option<AudioSession>> = const { RefCell::new(None) };
}

pub(crate) fn adjust_system_volume(delta: f32) -> Result<Level> {
    AUDIO.with(|slot| {
        let mut session = slot.borrow_mut();
        if session.is_none() {
            *session = Some(AudioSession::new()?);
        }
        let result = session.as_mut().unwrap().adjust(delta);
        if result.is_err() {
            // Reconnect on the next input; retrying a write could apply the delta twice.
            *session = None;
        }
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_notifications_invalidate_output_without_reacting_to_other_roles() {
        use windows::Win32::Media::Audio::{eCapture, eCommunications, DEVICE_STATE_DISABLED};
        let changed = Arc::new(AtomicBool::new(false));
        let callback: IMMNotificationClient = DeviceChanges(changed.clone()).into();
        unsafe {
            callback
                .OnDefaultDeviceChanged(eCapture, eMultimedia, PCWSTR::null())
                .unwrap();
            callback
                .OnDefaultDeviceChanged(eRender, eCommunications, PCWSTR::null())
                .unwrap();
            callback
                .OnPropertyValueChanged(PCWSTR::null(), PROPERTYKEY::default())
                .unwrap();
            assert!(!changed.load(Ordering::Acquire));
            callback
                .OnDefaultDeviceChanged(eRender, eMultimedia, PCWSTR::null())
                .unwrap();
            assert!(changed.swap(false, Ordering::AcqRel));
            callback.OnDeviceRemoved(PCWSTR::null()).unwrap();
            assert!(changed.swap(false, Ordering::AcqRel));
            callback.OnDeviceAdded(PCWSTR::null()).unwrap();
            assert!(changed.swap(false, Ordering::AcqRel));
            callback
                .OnDeviceStateChanged(PCWSTR::null(), DEVICE_STATE_DISABLED)
                .unwrap();
            assert!(changed.swap(false, Ordering::AcqRel));
            callback
                .OnPropertyValueChanged(PCWSTR::null(), PKEY_Device_FriendlyName)
                .unwrap();
            assert!(changed.load(Ordering::Acquire));
        }
    }

    #[test]
    #[ignore = "briefly adjusts the default output and restores its volume and mute state"]
    fn real_endpoint_roundtrip_restores_the_original_state() {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok().unwrap();
            let _apartment = Apartment;
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .unwrap();
            let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).unwrap();
            let original = endpoint.GetMasterVolumeLevelScalar().unwrap();
            let muted = endpoint.GetMute().unwrap().as_bool();
            struct Restore(IAudioEndpointVolume, f32, bool);
            impl Drop for Restore {
                fn drop(&mut self) {
                    unsafe {
                        self.0
                            .SetMasterVolumeLevelScalar(self.1, std::ptr::null())
                            .unwrap();
                        self.0.SetMute(self.2, std::ptr::null()).unwrap();
                    }
                }
            }
            let _restore = Restore(endpoint.clone(), original, muted);
            let down = adjust_system_volume(-0.02).unwrap();
            assert!((down.value - (original - 0.02).max(0.0)).abs() < 0.005);
            assert_eq!(down.muted, muted);
            assert!(!down.device_name.is_empty());
            let up = adjust_system_volume(0.02).unwrap();
            assert!((up.value - (down.value + 0.02).min(1.0)).abs() < 0.005);
            assert!(!up.muted);
            let started = std::time::Instant::now();
            for _ in 0..100 {
                let down = adjust_system_volume(-0.001).unwrap();
                let up = adjust_system_volume(0.001).unwrap();
                assert!((up.value - (down.value + 0.001).min(1.0)).abs() < 0.005);
            }
            eprintln!("200 audio adjustments: {:?}", started.elapsed());
            AUDIO.with(|slot| {
                let mut slot = slot.borrow_mut();
                let session = slot.as_mut().unwrap();
                session.output.as_mut().unwrap().name = "stale device".into();
                session
                    .notifications
                    .OnDefaultDeviceChanged(eRender, eMultimedia, PCWSTR::null())
                    .unwrap();
            });
            assert_eq!(
                adjust_system_volume(0.0).unwrap().device_name,
                up.device_name
            );
            endpoint
                .SetMasterVolumeLevelScalar(0.31, std::ptr::null())
                .unwrap();
            endpoint.SetMute(true, std::ptr::null()).unwrap();
            let down = adjust_system_volume(-0.01).unwrap();
            assert!((down.value - 0.30).abs() < 0.005);
            assert!(down.muted);
            let up = adjust_system_volume(0.01).unwrap();
            assert!((up.value - 0.31).abs() < 0.005);
            assert!(!up.muted);
            assert!(adjust_system_volume(2.0).unwrap().value > 0.995);
            assert!(adjust_system_volume(2.0).unwrap().value > 0.995);
            assert!(adjust_system_volume(-2.0).unwrap().value < 0.005);
            assert!(adjust_system_volume(-2.0).unwrap().value < 0.005);
            assert!((adjust_system_volume(0.02).unwrap().value - 0.02).abs() < 0.005);
        }
    }
}
