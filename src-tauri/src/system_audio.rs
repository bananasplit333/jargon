#[cfg(windows)]
use std::sync::{Mutex, OnceLock};

#[cfg(windows)]
use windows::core::Error;
#[cfg(windows)]
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
#[cfg(windows)]
use windows::Win32::Media::Audio::{
    eConsole, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator,
};
#[cfg(windows)]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

const FADE_DURATION_MS: u64 = 150;
const FADE_STEPS: u32 = 15;
const DUCKED_VOLUME_RATIO: f32 = 0.5; // Duck to 50% of og vol

#[cfg(windows)]
struct AudioState {
    original_volume: Option<f32>,
    was_muted: Option<bool>,
}

#[cfg(windows)]
fn audio_state_storage() -> &'static Mutex<AudioState> {
    static STATE: OnceLock<Mutex<AudioState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(AudioState {
            original_volume: None,
            was_muted: None,
        })
    })
}

#[cfg(windows)]
fn with_endpoint_volume<F, T>(callback: F) -> Result<T, String>
where
    F: FnOnce(&IAudioEndpointVolume) -> Result<T, Error>,
{
    unsafe {
        let init_result = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut needs_uninit = false;
        if init_result.is_ok() {
            needs_uninit = true;
        } else if init_result != RPC_E_CHANGED_MODE {
            return Err(format!("CoInitializeEx failed: {:?}", init_result));
        }

        let result = (|| {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
            callback(&endpoint)
        })();

        if needs_uninit {
            CoUninitialize();
        }

        result.map_err(|err| format!("{err:?}"))
    }
}

#[cfg(windows)]
fn get_volume() -> Result<f32, String> {
    with_endpoint_volume(|endpoint: &IAudioEndpointVolume| unsafe {
        endpoint.GetMasterVolumeLevelScalar()
    })
}

#[cfg(windows)]
fn set_volume(level: f32) -> Result<(), String> {
    with_endpoint_volume(|endpoint: &IAudioEndpointVolume| unsafe {
        endpoint.SetMasterVolumeLevelScalar(level, std::ptr::null())?;
        Ok(())
    })
}

#[cfg(windows)]
fn get_mute() -> Result<bool, String> {
    with_endpoint_volume(|endpoint: &IAudioEndpointVolume| unsafe {
        endpoint.GetMute().map(|m| m.as_bool())
    })
}

#[cfg(windows)]
#[allow(dead_code)]
fn set_mute(muted: bool) -> Result<(), String> {
    with_endpoint_volume(|endpoint: &IAudioEndpointVolume| unsafe {
        endpoint.SetMute(muted, std::ptr::null())?;
        Ok(())
    })
}

/// Fade volume from current level to target over FADE_DURATION_MS
#[cfg(windows)]
fn fade_volume(from: f32, to: f32) {
    let step_duration = std::time::Duration::from_millis(FADE_DURATION_MS / FADE_STEPS as u64);
    let step_size = (to - from) / FADE_STEPS as f32;

    for i in 1..=FADE_STEPS {
        let level = from + step_size * i as f32;
        let _ = set_volume(level.clamp(0.0, 1.0));
        if i < FADE_STEPS {
            std::thread::sleep(step_duration);
        }
    }
}

/// Duck or restore audio when dictation starts/stops
/// When `duck` is true: fade volume down and store original
/// When `duck` is false: fade volume back to original
#[cfg(windows)]
pub fn set_music_muted(duck: bool) -> Result<(), String> {
    let mut guard = audio_state_storage()
        .lock()
        .map_err(|_| "Audio state lock poisoned".to_string())?;

    if duck {
        // Already ducked
        if guard.original_volume.is_some() {
            return Ok(());
        }

        // Check if muted - if so, nothing to duck
        let is_muted = get_mute().unwrap_or(false);
        if is_muted {
            guard.was_muted = Some(true);
            guard.original_volume = Some(0.0);
            return Ok(());
        }

        // Get current volume and fade down
        let current_volume = get_volume()?;
        guard.original_volume = Some(current_volume);
        guard.was_muted = Some(false);

        // Only fade if there's meaningful volume
        if current_volume > 0.01 {
            let target = current_volume * DUCKED_VOLUME_RATIO;
            fade_volume(current_volume, target);
        }

        return Ok(());
    }

    // Restore: fade back to original volume
    if let Some(original) = guard.original_volume.take() {
        let was_muted = guard.was_muted.take().unwrap_or(false);

        // If it was muted before, don't restore
        if was_muted {
            return Ok(());
        }

        // Get current (ducked) volume and fade back up
        let current = get_volume().unwrap_or(original * DUCKED_VOLUME_RATIO);
        if original > 0.01 {
            fade_volume(current, original);
        }
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn set_music_muted(_duck: bool) -> Result<(), String> {
    Ok(())
}
