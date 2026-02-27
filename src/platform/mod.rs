#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

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
