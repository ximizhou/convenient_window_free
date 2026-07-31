use anyhow::{bail, Result};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

pub fn adjust_volume(delta: f32) -> Result<()> {
    if !delta.is_finite() || delta == 0.0 {
        bail!("volume delta must be a finite non-zero value");
    }

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let result = (|| -> windows::core::Result<()> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
            let current = endpoint.GetMasterVolumeLevelScalar()?;
            endpoint.SetMasterVolumeLevelScalar((current + delta).clamp(0.0, 1.0), std::ptr::null())
        })();
        CoUninitialize();
        result.map_err(Into::into)
    }
}
