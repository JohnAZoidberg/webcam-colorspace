use anyhow::{Context, Result};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::*;

use super::{CapturedFrame, ColorspaceInfo, DeviceInfo, FormatInfo};
use crate::cli::MatrixChoice;

const FIRST_VIDEO_STREAM: u32 = 0xFFFFFFFC; // MF_SOURCE_READER_FIRST_VIDEO_STREAM

/// Run a closure with COM + Media Foundation initialized, cleaning up afterward.
fn with_mf<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .context("Failed to initialize COM")?;
        MFStartup(MF_API_VERSION, MFSTARTUP_FULL)
            .context("Failed to initialize Media Foundation")?;

        let result = f();

        MFShutdown().ok();
        CoUninitialize();

        result
    }
}

pub fn enumerate_devices() -> Result<Vec<DeviceInfo>> {
    with_mf(|| unsafe { enumerate_devices_inner() })
}

pub fn capture_frame(device_index: usize, resolution: Option<(u32, u32)>) -> Result<CapturedFrame> {
    with_mf(|| unsafe { capture_frame_inner(device_index, resolution) })
}

pub fn force_matrix(device_index: usize, matrix: MatrixChoice) -> Result<()> {
    with_mf(|| unsafe { force_matrix_inner(device_index, matrix) })
}

// ---------------------------------------------------------------------------
// Device activation helpers
// ---------------------------------------------------------------------------

/// Enumerate all video capture activate objects.
unsafe fn enum_activates() -> Result<Vec<IMFActivate>> {
    let mut attributes: Option<IMFAttributes> = None;
    MFCreateAttributes(&mut attributes, 1).context("Failed to create MF attributes")?;
    let attributes = attributes.unwrap();

    attributes
        .SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        )
        .context("Failed to set vidcap attribute")?;

    let mut sources_ptr: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count: u32 = 0;

    MFEnumDeviceSources(&attributes, &mut sources_ptr, &mut count)
        .context("Failed to enumerate device sources")?;

    if count == 0 || sources_ptr.is_null() {
        return Ok(Vec::new());
    }

    let raw = std::slice::from_raw_parts(sources_ptr, count as usize);
    let activates: Vec<IMFActivate> = raw.iter().filter_map(|o| o.clone()).collect();

    CoTaskMemFree(Some(sources_ptr as *const _));

    Ok(activates)
}

/// Activate a specific device by 0-based index. Returns (source, friendly_name).
unsafe fn activate_device_by_index(index: usize) -> Result<(IMFMediaSource, String)> {
    let activates = enum_activates()?;

    if activates.is_empty() {
        anyhow::bail!("No camera devices found.");
    }
    if index >= activates.len() {
        anyhow::bail!(
            "Device {} does not exist. Found {} device(s).",
            index + 1,
            activates.len()
        );
    }

    let activate = &activates[index];

    let name = get_string_attribute(activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
        .unwrap_or_else(|_| "Unknown".to_string());

    let source: IMFMediaSource = activate
        .ActivateObject()
        .context("Failed to activate media source")?;

    Ok((source, name))
}

// ---------------------------------------------------------------------------
// Enumerate
// ---------------------------------------------------------------------------

unsafe fn enumerate_devices_inner() -> Result<Vec<DeviceInfo>> {
    let activates = enum_activates()?;
    let mut devices = Vec::new();

    for activate in &activates {
        match read_device(activate) {
            Ok(device) => devices.push(device),
            Err(e) => eprintln!("Warning: failed to read device: {e:#}"),
        }
    }

    Ok(devices)
}

unsafe fn read_device(activate: &IMFActivate) -> Result<DeviceInfo> {
    let name = get_string_attribute(activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
        .unwrap_or_else(|_| "Unknown".to_string());

    let path = get_string_attribute(
        activate,
        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
    )
    .unwrap_or_default();

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

    let _ = source.Shutdown();

    Ok(DeviceInfo {
        name,
        path,
        formats,
    })
}

// ---------------------------------------------------------------------------
// Capture
// ---------------------------------------------------------------------------

/// Find an NV12 media type matching the requested resolution, or the highest-res if none specified.
unsafe fn find_nv12_type(
    source: &IMFMediaSource,
    requested: Option<(u32, u32)>,
) -> Result<(IMFMediaType, u32, u32)> {
    let pd = source
        .CreatePresentationDescriptor()
        .context("Failed to create presentation descriptor")?;

    let stream_count = pd.GetStreamDescriptorCount()?;

    let mut best: Option<(IMFMediaType, u32, u32)> = None;
    let mut best_pixels: u64 = 0;
    let mut available: Vec<(u32, u32)> = Vec::new();

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

            let Ok(subtype) = media_type.GetGUID(&MF_MT_SUBTYPE) else {
                continue;
            };
            if subtype != MFVideoFormat_NV12 {
                continue;
            }

            let Ok(packed_size) = media_type.GetUINT64(&MF_MT_FRAME_SIZE) else {
                continue;
            };
            let w = (packed_size >> 32) as u32;
            let h = packed_size as u32;

            if !available.contains(&(w, h)) {
                available.push((w, h));
            }

            if let Some((rw, rh)) = requested {
                // Exact match requested
                if w == rw && h == rh {
                    return Ok((media_type, w, h));
                }
            } else {
                // Pick highest resolution
                let pixels = w as u64 * h as u64;
                if pixels > best_pixels {
                    best_pixels = pixels;
                    best = Some((media_type, w, h));
                }
            }
        }
    }

    if let Some((rw, rh)) = requested {
        let avail_str: Vec<String> = available.iter().map(|(w, h)| format!("{w}x{h}")).collect();
        anyhow::bail!(
            "No NV12 format at {rw}x{rh}. Available NV12 resolutions: {}",
            avail_str.join(", ")
        );
    }

    best.context("No NV12 media type found on this device")
}

unsafe fn capture_frame_inner(
    device_index: usize,
    resolution: Option<(u32, u32)>,
) -> Result<CapturedFrame> {
    let (source, name) = activate_device_by_index(device_index)?;
    println!("Capturing from device {}: {}", device_index + 1, name);

    let (nv12_type, width, height) = find_nv12_type(&source, resolution)?;
    println!("Selected NV12 {}x{}", width, height);

    // Read nominal range from the media type
    let full_range = matches!(
        nv12_type.GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE),
        Ok(v) if v == MFNominalRange_0_255.0 as u32
    );
    let range_label = if full_range {
        "Full (0-255)"
    } else {
        "Limited (16-235)"
    };
    println!("Nominal range: {}", range_label);

    let reader = MFCreateSourceReaderFromMediaSource(&source, None)
        .context("Failed to create source reader")?;

    reader
        .SetCurrentMediaType(FIRST_VIDEO_STREAM, None, &nv12_type)
        .context("Failed to set media type on reader")?;

    // Read several frames to let auto-exposure settle, keep the last one
    let mut last_sample: Option<IMFSample> = None;
    let frames_to_skip = 5;

    for i in 0..frames_to_skip {
        let mut flags: u32 = 0;
        let mut sample: Option<IMFSample> = None;

        reader
            .ReadSample(
                FIRST_VIDEO_STREAM,
                0,
                None,
                Some(&mut flags),
                None,
                Some(&mut sample),
            )
            .with_context(|| format!("ReadSample failed on frame {}", i + 1))?;

        if let Some(s) = sample {
            last_sample = Some(s);
        }
    }

    let sample = last_sample.context("No sample received from camera")?;

    let buffer = sample
        .ConvertToContiguousBuffer()
        .context("Failed to convert sample to contiguous buffer")?;

    let mut buf_ptr: *mut u8 = std::ptr::null_mut();
    let mut cur_len: u32 = 0;

    buffer
        .Lock(&mut buf_ptr, None, Some(&mut cur_len))
        .context("Failed to lock buffer")?;

    let data = std::slice::from_raw_parts(buf_ptr, cur_len as usize).to_vec();

    buffer.Unlock().context("Failed to unlock buffer")?;

    let _ = source.Shutdown();

    Ok(CapturedFrame {
        width,
        height,
        pixel_format: "NV12".to_string(),
        full_range,
        data,
    })
}

// ---------------------------------------------------------------------------
// Force matrix
// ---------------------------------------------------------------------------

unsafe fn force_matrix_inner(device_index: usize, matrix: MatrixChoice) -> Result<()> {
    let (source, name) = activate_device_by_index(device_index)?;
    println!("Device {}: {}", device_index + 1, name);

    let reader = MFCreateSourceReaderFromMediaSource(&source, None)
        .context("Failed to create source reader")?;

    let current_type = reader
        .GetCurrentMediaType(FIRST_VIDEO_STREAM)
        .context("Failed to get current media type")?;

    // Report current value
    match current_type.GetUINT32(&MF_MT_YUV_MATRIX) {
        Ok(v) => println!("Current MF_MT_YUV_MATRIX: {} ({})", matrix_name(v), v),
        Err(_) => println!("Current MF_MT_YUV_MATRIX: Not specified"),
    }

    let target_value = match matrix {
        MatrixChoice::Bt601 => MFVideoTransferMatrix_BT601.0 as u32,
        MatrixChoice::Bt709 => MFVideoTransferMatrix_BT709.0 as u32,
    };
    let target_name = match matrix {
        MatrixChoice::Bt601 => "BT.601",
        MatrixChoice::Bt709 => "BT.709",
    };

    // Create new type with overridden matrix
    let new_type = MFCreateMediaType().context("Failed to create media type")?;
    current_type
        .CopyAllItems(&new_type)
        .context("Failed to copy media type attributes")?;
    new_type
        .SetUINT32(&MF_MT_YUV_MATRIX, target_value)
        .context("Failed to set MF_MT_YUV_MATRIX")?;

    match reader.SetCurrentMediaType(FIRST_VIDEO_STREAM, None, &new_type) {
        Ok(()) => {
            println!("Successfully set MF_MT_YUV_MATRIX to {target_name}.");
            println!();
            println!("Note: This override only affects this source reader session.");
            println!("It does not persist after the program exits.");
        }
        Err(e) => {
            println!("Failed to set MF_MT_YUV_MATRIX to {target_name}: {e}");
            println!();
            println!("The driver may not support overriding the YUV matrix attribute.");
        }
    }

    let _ = source.Shutdown();
    Ok(())
}

// ---------------------------------------------------------------------------
// Format reading helpers
// ---------------------------------------------------------------------------

unsafe fn read_format(media_type: &IMFMediaType) -> Result<FormatInfo> {
    let pixel_format = match media_type.GetGUID(&MF_MT_SUBTYPE) {
        Ok(guid) => subtype_name(&guid),
        Err(_) => "Unknown".to_string(),
    };

    let resolution = match media_type.GetUINT64(&MF_MT_FRAME_SIZE) {
        Ok(packed) => {
            let width = (packed >> 32) as u32;
            let height = packed as u32;
            format!("{width}x{height}")
        }
        Err(_) => "Unknown".to_string(),
    };

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
