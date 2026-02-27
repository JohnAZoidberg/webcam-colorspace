use anyhow::{Context, Result};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::*;

use super::{ColorspaceInfo, DeviceInfo, FormatInfo};

pub fn enumerate_devices() -> Result<Vec<DeviceInfo>> {
    unsafe {
        // Initialize COM
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .context("Failed to initialize COM")?;

        // Initialize Media Foundation
        MFStartup(MF_API_VERSION, MFSTARTUP_FULL)
            .context("Failed to initialize Media Foundation")?;

        let result = enumerate_devices_inner();

        MFShutdown().ok();
        CoUninitialize();

        result
    }
}

unsafe fn enumerate_devices_inner() -> Result<Vec<DeviceInfo>> {
    // Create attribute store requesting video capture devices
    let mut attributes: Option<IMFAttributes> = None;
    MFCreateAttributes(&mut attributes, 1).context("Failed to create MF attributes")?;
    let attributes = attributes.unwrap();

    attributes
        .SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        )
        .context("Failed to set vidcap attribute")?;

    // Enumerate devices
    let mut sources_ptr: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count: u32 = 0;

    MFEnumDeviceSources(&attributes, &mut sources_ptr, &mut count)
        .context("Failed to enumerate device sources")?;

    if count == 0 || sources_ptr.is_null() {
        return Ok(Vec::new());
    }

    let activates = std::slice::from_raw_parts(sources_ptr, count as usize);
    let mut devices = Vec::new();

    for activate_opt in activates {
        let Some(activate) = activate_opt else {
            continue;
        };

        match read_device(activate) {
            Ok(device) => devices.push(device),
            Err(e) => eprintln!("Warning: failed to read device: {e:#}"),
        }
    }

    // Free the array allocated by MFEnumDeviceSources
    CoTaskMemFree(Some(sources_ptr as *const _));

    Ok(devices)
}

unsafe fn read_device(activate: &IMFActivate) -> Result<DeviceInfo> {
    // Get friendly name
    let name = get_string_attribute(activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
        .unwrap_or_else(|_| "Unknown".to_string());

    // Get symbolic link (device path)
    let path = get_string_attribute(
        activate,
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
    )
    .unwrap_or_default();

    // Activate the media source
    let source: IMFMediaSource = activate
        .ActivateObject()
        .context("Failed to activate media source")?;

    let pd = source
        .CreatePresentationDescriptor()
        .context("Failed to create presentation descriptor")?;

    let stream_count = pd
        .GetStreamDescriptorCount()
        .context("Failed to get stream descriptor count")?;

    let mut formats = Vec::new();

    for i in 0..stream_count {
        let mut selected = windows::core::BOOL::default();
        let mut sd: Option<IMFStreamDescriptor> = None;

        if pd
            .GetStreamDescriptorByIndex(i, &mut selected, &mut sd)
            .is_err()
        {
            continue;
        }

        let Some(sd) = sd else { continue };

        let Ok(handler) = sd.GetMediaTypeHandler() else {
            continue;
        };

        let Ok(type_count) = handler.GetMediaTypeCount() else {
            continue;
        };

        for j in 0..type_count {
            let Ok(media_type) = handler.GetMediaTypeByIndex(j) else {
                continue;
            };

            match read_format(&media_type) {
                Ok(format) => formats.push(format),
                Err(_) => continue,
            }
        }
    }

    // Shut down the source to release resources
    let _ = source.Shutdown();

    Ok(DeviceInfo {
        name,
        path,
        formats,
    })
}

unsafe fn read_format(media_type: &IMFMediaType) -> Result<FormatInfo> {
    // Pixel format (subtype GUID)
    let pixel_format = match media_type.GetGUID(&MF_MT_SUBTYPE) {
        Ok(guid) => subtype_name(&guid),
        Err(_) => "Unknown".to_string(),
    };

    // Resolution (packed as width << 32 | height)
    let resolution = match media_type.GetUINT64(&MF_MT_FRAME_SIZE) {
        Ok(packed) => {
            let width = (packed >> 32) as u32;
            let height = packed as u32;
            format!("{width}x{height}")
        }
        Err(_) => "Unknown".to_string(),
    };

    // Frame rate (packed as numerator << 32 | denominator)
    let frame_rate = match media_type.GetUINT64(&MF_MT_FRAME_RATE) {
        Ok(packed) => {
            let num = (packed >> 32) as u32;
            let den = packed as u32;
            if den > 0 {
                let fps = num as f64 / den as f64;
                format!("{fps:.2} fps")
            } else {
                "Unknown".to_string()
            }
        }
        Err(_) => "Unknown".to_string(),
    };

    // Colorspace attributes
    let primaries = match media_type.GetUINT32(&MF_MT_VIDEO_PRIMARIES) {
        Ok(v) => primaries_name(v),
        Err(_) => "Not specified".to_string(),
    };

    let matrix = match media_type.GetUINT32(&MF_MT_YUV_MATRIX) {
        Ok(v) => matrix_name(v),
        Err(_) => "Not specified".to_string(),
    };

    let transfer = match media_type.GetUINT32(&MF_MT_TRANSFER_FUNCTION) {
        Ok(v) => transfer_name(v),
        Err(_) => "Not specified".to_string(),
    };

    let range = match media_type.GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE) {
        Ok(v) => range_name(v),
        Err(_) => "Not specified".to_string(),
    };

    Ok(FormatInfo {
        pixel_format,
        resolution,
        frame_rate,
        colorspace: ColorspaceInfo {
            primaries,
            matrix,
            transfer,
            range,
        },
    })
}

unsafe fn get_string_attribute(attrs: &IMFActivate, key: &windows::core::GUID) -> Result<String> {
    let mut pwstr = windows::core::PWSTR::null();
    let mut len: u32 = 0;

    attrs
        .GetAllocatedString(key, &mut pwstr, &mut len)
        .context("GetAllocatedString failed")?;

    let s = pwstr.to_string().unwrap_or_default();
    CoTaskMemFree(Some(pwstr.as_ptr() as *const _));
    Ok(s)
}

fn subtype_name(guid: &windows::core::GUID) -> String {
    // Common video subtypes — the first 4 bytes of the GUID encode the FourCC
    let known: &[(windows::core::GUID, &str)] = &[
        (MFVideoFormat_NV12, "NV12"),
        (MFVideoFormat_YUY2, "YUY2"),
        (MFVideoFormat_MJPG, "MJPG"),
        (MFVideoFormat_RGB24, "RGB24"),
        (MFVideoFormat_RGB32, "RGB32"),
        (MFVideoFormat_ARGB32, "ARGB32"),
        (MFVideoFormat_UYVY, "UYVY"),
        (MFVideoFormat_I420, "I420"),
        (MFVideoFormat_IYUV, "IYUV"),
        (MFVideoFormat_YV12, "YV12"),
        (MFVideoFormat_H264, "H264"),
        (MFVideoFormat_HEVC, "HEVC"),
    ];

    for (known_guid, name) in known {
        if guid == known_guid {
            return name.to_string();
        }
    }

    // Fall back to FourCC from the first 4 bytes
    let bytes = guid.data1.to_le_bytes();
    if bytes.iter().all(|b| b.is_ascii_graphic()) {
        String::from_utf8_lossy(&bytes).to_string()
    } else {
        format!("{guid:?}")
    }
}

fn primaries_name(v: u32) -> String {
    match v as i32 {
        v if v == MFVideoPrimaries_BT709.0 => "BT.709",
        v if v == MFVideoPrimaries_BT470_2_SysM.0 => "BT.470-2 System M",
        v if v == MFVideoPrimaries_BT470_2_SysBG.0 => "BT.470-2 System B/G",
        v if v == MFVideoPrimaries_SMPTE170M.0 => "SMPTE 170M",
        v if v == MFVideoPrimaries_SMPTE240M.0 => "SMPTE 240M",
        v if v == MFVideoPrimaries_EBU3213.0 => "EBU 3213",
        v if v == MFVideoPrimaries_SMPTE_C.0 => "SMPTE C",
        v if v == MFVideoPrimaries_BT2020.0 => "BT.2020",
        v if v == MFVideoPrimaries_XYZ.0 => "XYZ",
        v if v == MFVideoPrimaries_DCI_P3.0 => "DCI-P3",
        v if v == MFVideoPrimaries_ACES.0 => "ACES",
        0 | 1 => "Unknown",
        _ => return format!("Unknown ({v})"),
    }
    .to_string()
}

fn matrix_name(v: u32) -> String {
    match v as i32 {
        v if v == MFVideoTransferMatrix_BT709.0 => "BT.709",
        v if v == MFVideoTransferMatrix_BT601.0 => "BT.601",
        v if v == MFVideoTransferMatrix_SMPTE240M.0 => "SMPTE 240M",
        v if v == MFVideoTransferMatrix_BT2020_10.0 => "BT.2020 (10-bit)",
        v if v == MFVideoTransferMatrix_BT2020_12.0 => "BT.2020 (12-bit)",
        0 => "Unknown",
        _ => return format!("Unknown ({v})"),
    }
    .to_string()
}

fn transfer_name(v: u32) -> String {
    match v as i32 {
        v if v == MFVideoTransFunc_709.0 => "BT.709",
        v if v == MFVideoTransFunc_sRGB.0 => "sRGB",
        v if v == MFVideoTransFunc_10.0 => "Linear (gamma 1.0)",
        v if v == MFVideoTransFunc_18.0 => "Gamma 1.8",
        v if v == MFVideoTransFunc_20.0 => "Gamma 2.0",
        v if v == MFVideoTransFunc_22.0 => "Gamma 2.2",
        v if v == MFVideoTransFunc_240M.0 => "SMPTE 240M",
        v if v == MFVideoTransFunc_28.0 => "Gamma 2.8",
        v if v == MFVideoTransFunc_Log_100.0 => "Log 100",
        v if v == MFVideoTransFunc_Log_316.0 => "Log 316",
        v if v == MFVideoTransFunc_2020_const.0 => "BT.2020 (constant)",
        v if v == MFVideoTransFunc_2020.0 => "BT.2020",
        v if v == MFVideoTransFunc_26.0 => "Gamma 2.6",
        v if v == MFVideoTransFunc_2084.0 => "SMPTE 2084 (PQ)",
        v if v == MFVideoTransFunc_HLG.0 => "HLG",
        0 => "Unknown",
        _ => return format!("Unknown ({v})"),
    }
    .to_string()
}

fn range_name(v: u32) -> String {
    match v as i32 {
        v if v == MFNominalRange_0_255.0 => "Full (0-255)",
        v if v == MFNominalRange_16_235.0 => "Limited (16-235)",
        v if v == MFNominalRange_48_208.0 => "48-208",
        v if v == MFNominalRange_64_127.0 => "64-127",
        0 => "Unknown",
        _ => return format!("Unknown ({v})"),
    }
    .to_string()
}
