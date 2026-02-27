use anyhow::{Context, Result};
use v4l::capability::Flags;
use v4l::context;
use v4l::format::colorspace::Colorspace;
use v4l::format::quantization::Quantization;
use v4l::format::transfer::TransferFunction;
use v4l::prelude::*;
use v4l::video::Capture;
use v4l::FourCC;

use super::{ColorspaceInfo, DeviceInfo, FormatInfo};

pub fn enumerate_devices() -> Result<Vec<DeviceInfo>> {
    let nodes = context::enum_devices();
    let mut devices = Vec::new();

    for node in nodes {
        match read_device(&node) {
            Ok(Some(device)) => devices.push(device),
            Ok(None) => {} // not a capture device
            Err(e) => eprintln!("Warning: failed to read {}: {e:#}", node.path().display()),
        }
    }

    Ok(devices)
}

fn read_device(node: &context::Node) -> Result<Option<DeviceInfo>> {
    let path = node.path().to_string_lossy().to_string();

    let dev = Device::with_path(&path).with_context(|| format!("Failed to open {path}"))?;

    let caps = dev
        .query_caps()
        .with_context(|| format!("Failed to query caps for {path}"))?;

    // Only interested in video capture devices
    if !caps.capabilities.contains(Flags::VIDEO_CAPTURE) {
        return Ok(None);
    }

    let name = caps.card.clone();

    // Get current format for colorspace info
    let current_fmt = dev.format().ok();

    let mut formats = Vec::new();

    for desc in dev.enum_formats().unwrap_or_default() {
        let framesizes = dev.enum_framesizes(desc.fourcc).unwrap_or_default();

        for framesize in &framesizes {
            for discrete in framesize.size.to_discrete() {
                let intervals = dev
                    .enum_frameintervals(desc.fourcc, discrete.width, discrete.height)
                    .unwrap_or_default();

                let frame_rates: Vec<String> = intervals
                    .iter()
                    .filter_map(|fi| match &fi.interval {
                        v4l::frameinterval::FrameIntervalEnum::Discrete(frac) => {
                            if frac.numerator > 0 {
                                Some(format!(
                                    "{:.2} fps",
                                    frac.denominator as f64 / frac.numerator as f64
                                ))
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();

                let frame_rate = if frame_rates.is_empty() {
                    "Unknown".to_string()
                } else {
                    frame_rates.join(", ")
                };

                let colorspace = current_fmt
                    .as_ref()
                    .map(|f| colorspace_info(f.colorspace, f.transfer, f.quantization))
                    .unwrap_or_else(|| ColorspaceInfo {
                        primaries: "Not available".to_string(),
                        matrix: "Not available".to_string(),
                        transfer: "Not available".to_string(),
                        range: "Not available".to_string(),
                    });

                formats.push(FormatInfo {
                    pixel_format: fourcc_name(desc.fourcc),
                    resolution: format!("{}x{}", discrete.width, discrete.height),
                    frame_rate,
                    colorspace,
                });
            }
        }
    }

    Ok(Some(DeviceInfo {
        name,
        path,
        formats,
    }))
}

fn fourcc_name(fourcc: FourCC) -> String {
    fourcc
        .str()
        .map(|s| s.to_string())
        .unwrap_or_else(|_| format!("{fourcc:?}"))
}

fn colorspace_info(cs: Colorspace, tf: TransferFunction, quant: Quantization) -> ColorspaceInfo {
    let (primaries, matrix) = match cs {
        Colorspace::Rec709 => ("BT.709".to_string(), "BT.709".to_string()),
        Colorspace::SMPTE170M => ("SMPTE 170M".to_string(), "BT.601".to_string()),
        Colorspace::SMPTE240M => ("SMPTE 240M".to_string(), "SMPTE 240M".to_string()),
        Colorspace::Rec2020 => ("BT.2020".to_string(), "BT.2020".to_string()),
        Colorspace::SRGB => ("sRGB".to_string(), "sRGB".to_string()),
        Colorspace::OPRGB => ("opRGB".to_string(), "opRGB".to_string()),
        Colorspace::JPEG => ("BT.601".to_string(), "BT.601 (JPEG)".to_string()),
        Colorspace::NTSC => ("NTSC".to_string(), "BT.601".to_string()),
        Colorspace::EBUTech3212 => ("EBU Tech 3213".to_string(), "BT.601".to_string()),
        Colorspace::RAW => ("Raw".to_string(), "None".to_string()),
        Colorspace::DCIP3 => ("DCI-P3".to_string(), "DCI-P3".to_string()),
        Colorspace::Default => ("Default".to_string(), "Default".to_string()),
    };

    let transfer = match tf {
        TransferFunction::Rec709 => "BT.709",
        TransferFunction::SRGB => "sRGB",
        TransferFunction::OPRGB => "opRGB",
        TransferFunction::SMPTE240M => "SMPTE 240M",
        TransferFunction::None => "None (linear)",
        TransferFunction::DCIP3 => "DCI-P3",
        TransferFunction::SMPTE2084 => "SMPTE 2084 (PQ)",
        TransferFunction::Default => "Default",
    }
    .to_string();

    let range = match quant {
        Quantization::FullRange => "Full (0-255)",
        Quantization::LimitedRange => "Limited (16-235)",
        Quantization::Default => "Default",
    }
    .to_string();

    ColorspaceInfo {
        primaries,
        matrix,
        transfer,
        range,
    }
}
