#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

use crate::cli::MatrixChoice;

pub struct DeviceInfo {
    pub name: String,
    pub path: String,
    pub formats: Vec<FormatInfo>,
}

pub struct FormatInfo {
    pub pixel_format: String,
    pub resolution: String,
    pub frame_rate: String,
    pub colorspace: ColorspaceInfo,
}

pub struct ColorspaceInfo {
    pub primaries: String,
    pub matrix: String,
    pub transfer: String,
    pub range: String,
}

pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub pixel_format: String,
    pub full_range: bool,
    pub data: Vec<u8>,
}

pub fn enumerate_devices() -> anyhow::Result<Vec<DeviceInfo>> {
    #[cfg(windows)]
    {
        windows::enumerate_devices()
    }
    #[cfg(target_os = "linux")]
    {
        linux::enumerate_devices()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        anyhow::bail!("Unsupported platform")
    }
}

pub fn capture_frame(
    device_index: usize,
    resolution: Option<(u32, u32)>,
) -> anyhow::Result<CapturedFrame> {
    #[cfg(windows)]
    {
        windows::capture_frame(device_index, resolution)
    }
    #[cfg(target_os = "linux")]
    {
        let _ = (device_index, resolution);
        anyhow::bail!("--capture-test is not yet supported on Linux")
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (device_index, resolution);
        anyhow::bail!("Unsupported platform")
    }
}

pub fn force_matrix(device_index: usize, matrix: MatrixChoice) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows::force_matrix(device_index, matrix)
    }
    #[cfg(target_os = "linux")]
    {
        let _ = (device_index, matrix);
        anyhow::bail!("--force-matrix is not yet supported on Linux")
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (device_index, matrix);
        anyhow::bail!("Unsupported platform")
    }
}
