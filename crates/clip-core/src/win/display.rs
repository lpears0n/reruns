//! Display queries: refresh rates through the GDI display-settings API,
//! HDR state through DXGI and the display-config API.

use anyhow::Result;
use windows::{
    Win32::Graphics::Gdi::{
        DEVMODEW, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS, EnumDisplayDevicesW, EnumDisplaySettingsW,
    },
    core::PCWSTR,
};

/// Highest current refresh rate across connected displays, in Hz.
pub fn max_refresh_hz() -> Result<Option<f64>> {
    let mut best: Option<u32> = None;
    let mut i = 0u32;
    loop {
        let mut dev = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: `dev` is a correctly sized, writable DISPLAY_DEVICEW.
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), i, &mut dev, 0) }.as_bool() {
            break;
        }
        i += 1;
        let mut mode = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        // SAFETY: `dev.DeviceName` is NUL-terminated; `mode` is sized.
        let ok = unsafe {
            EnumDisplaySettingsW(
                PCWSTR(dev.DeviceName.as_ptr()),
                ENUM_CURRENT_SETTINGS,
                &mut mode,
            )
        }
        .as_bool();
        if ok && mode.dmDisplayFrequency > 1 {
            best = Some(best.map_or(mode.dmDisplayFrequency, |b| b.max(mode.dmDisplayFrequency)));
        }
    }
    Ok(best.map(f64::from))
}

/// The monitor's HDR state, if it is running in HDR ("Use HDR" on in
/// Windows' display settings). `monitor` is an `HMONITOR`.
pub fn hdr_display(monitor: isize) -> Option<crate::HdrDisplay> {
    use windows::{
        Win32::Graphics::Dxgi::{
            Common::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, CreateDXGIFactory1, IDXGIFactory1,
            IDXGIOutput6,
        },
        core::Interface,
    };

    // SAFETY: COM calls on interfaces DXGI hands out; each is released when
    // dropped.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        for a in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(a) else {
                break;
            };
            for o in 0.. {
                let Ok(output) = adapter.EnumOutputs(o) else {
                    break;
                };
                let Ok(output) = output.cast::<IDXGIOutput6>() else {
                    continue;
                };
                let Ok(desc) = output.GetDesc1() else {
                    continue;
                };
                if desc.Monitor.0 as isize != monitor {
                    continue;
                }
                if desc.ColorSpace != DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 {
                    return None;
                }
                // Unread is 0, not a guess: while the display is asleep
                // Windows reports neither level, and the recorder asks
                // again until it does (`tonemap` assumes 80 meanwhile).
                return Some(crate::HdrDisplay {
                    sdr_white_nits: sdr_white_nits(&desc.DeviceName).unwrap_or(0.0),
                    peak_nits: desc.MaxLuminance.max(0.0),
                });
            }
        }
    }
    None
}

/// The LUID of the GPU driving `monitor` (an `HMONITOR`), as GStreamer's
/// D3D11 elements take it.
pub fn adapter_luid(monitor: isize) -> Option<i64> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    // SAFETY: COM calls on interfaces DXGI hands out.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        for a in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(a) else {
                break;
            };
            for o in 0.. {
                let Ok(output) = adapter.EnumOutputs(o) else {
                    break;
                };
                if output
                    .GetDesc()
                    .is_ok_and(|d| d.Monitor.0 as isize == monitor)
                {
                    let luid = adapter.GetDesc1().ok()?.AdapterLuid;
                    return Some(((luid.HighPart as i64) << 32) | luid.LowPart as i64);
                }
            }
        }
    }
    None
}

/// The DXGI index of the adapter with this LUID (as GStreamer's D3D11
/// elements report it in `adapter-luid`).
pub fn adapter_index(luid: i64) -> Option<u32> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    // SAFETY: COM calls on interfaces DXGI hands out.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        for i in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(i) else {
                break;
            };
            let l = adapter.GetDesc1().ok()?.AdapterLuid;
            if ((l.HighPart as i64) << 32) | l.LowPart as i64 == luid {
                return Some(i);
            }
        }
    }
    None
}

/// Windows' "SDR content brightness" for the display whose GDI name is
/// `device` (`\\.\DISPLAY1`), in nits.
fn sdr_white_nits(device: &[u16; 32]) -> Option<f32> {
    use windows::Win32::{
        Devices::Display::{
            DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
            DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_MODE_INFO,
            DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SDR_WHITE_LEVEL,
            DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo,
            GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
        },
        Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
    };

    let name_len = device.iter().position(|&c| c == 0).unwrap_or(device.len());
    let device = &device[..name_len];

    // The configuration can change between sizing and querying; retry a
    // few times, as GStreamer's DXGI capture does.
    let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = Vec::new();
    for _ in 0..10 {
        let (mut n_path, mut n_mode) = (0u32, 0u32);
        // SAFETY: plain out-parameters.
        if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut n_path, &mut n_mode) }
            != ERROR_SUCCESS
        {
            return None;
        }
        paths = vec![DISPLAYCONFIG_PATH_INFO::default(); n_path as usize];
        let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); n_mode as usize];
        // SAFETY: the arrays are sized from the counts passed alongside.
        let ret = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut n_path,
                paths.as_mut_ptr(),
                &mut n_mode,
                modes.as_mut_ptr(),
                None,
            )
        };
        if ret == ERROR_INSUFFICIENT_BUFFER {
            continue;
        }
        if ret != ERROR_SUCCESS {
            return None;
        }
        paths.truncate(n_path as usize);
        break;
    }

    for path in &paths {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME::default();
        source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
        source.header.size = std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
        source.header.adapterId = path.sourceInfo.adapterId;
        source.header.id = path.sourceInfo.id;
        // SAFETY: the header describes the struct it heads.
        if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0 {
            continue;
        }
        let len = source
            .viewGdiDeviceName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(32);
        if &source.viewGdiDeviceName[..len] != device {
            continue;
        }
        let mut level = DISPLAYCONFIG_SDR_WHITE_LEVEL::default();
        level.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL;
        level.header.size = std::mem::size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32;
        level.header.adapterId = path.targetInfo.adapterId;
        level.header.id = path.targetInfo.id;
        // SAFETY: as above.
        if unsafe { DisplayConfigGetDeviceInfo(&mut level.header) } != 0 {
            return None;
        }
        // In thousandths of 80 nits.
        return Some(level.SDRWhiteLevel as f32 * 80.0 / 1000.0);
    }
    None
}
