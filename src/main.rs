mod bmp;
mod cli;
mod platform;
mod yuv;

use cli::Command;
use platform::FormatInfo;

fn main() -> anyhow::Result<()> {
    let command = cli::parse_args()?;

    match command {
        Command::Enumerate => run_enumerate(),
        Command::CaptureTest {
            device_index,
            resolution,
            mirror,
        } => run_capture_test(device_index, resolution, mirror),
        Command::ForceMatrix {
            matrix,
            device_index,
        } => run_force_matrix(device_index, matrix),
    }
}

fn run_enumerate() -> anyhow::Result<()> {
    print_header();
    print_os_info();
    println!();

    let devices = platform::enumerate_devices()?;

    if devices.is_empty() {
        println!("No camera devices found.");
        return Ok(());
    }

    println!("Found {} camera device(s):\n", devices.len());

    for (i, device) in devices.iter().enumerate() {
        println!("━━━ Device {}: {} ━━━", i + 1, device.name);
        if !device.path.is_empty() {
            println!("    Path: {}", device.path);
        }

        if device.formats.is_empty() {
            println!("    No formats reported.");
            continue;
        }

        // Deduplicate: group by pixel_format + resolution, show colorspace once
        let mut seen = std::collections::HashSet::new();
        let mut unique_formats: Vec<&FormatInfo> = Vec::new();

        for fmt in &device.formats {
            let key = format!("{}|{}|{}", fmt.pixel_format, fmt.resolution, fmt.frame_rate);
            if seen.insert(key) {
                unique_formats.push(fmt);
            }
        }

        println!("    Formats ({} unique):", unique_formats.len());

        for fmt in &unique_formats {
            println!(
                "      {} {} @ {}",
                fmt.pixel_format, fmt.resolution, fmt.frame_rate
            );

            let cs = &fmt.colorspace;
            let matrix_display = format_matrix_highlight(&cs.matrix);
            println!("        Primaries: {}", cs.primaries);
            println!("        YUV Matrix: {}", matrix_display);
            println!("        Transfer: {}", cs.transfer);
            println!("        Range: {}", cs.range);
        }
        println!();
    }

    print_legend();

    Ok(())
}

fn mirror_rgb(data: &mut [u8], width: u32, height: u32) {
    let w = width as usize;
    let row_bytes = w * 3;
    for row in 0..height as usize {
        let start = row * row_bytes;
        let row_slice = &mut data[start..start + row_bytes];
        // Swap pixel [col] with pixel [w-1-col]
        for col in 0..w / 2 {
            let l = col * 3;
            let r = (w - 1 - col) * 3;
            for c in 0..3 {
                row_slice.swap(l + c, r + c);
            }
        }
    }
}

fn run_capture_test(
    device_index: usize,
    resolution: Option<(u32, u32)>,
    mirror: bool,
) -> anyhow::Result<()> {
    print_header();
    println!();

    let frame = platform::capture_frame(device_index, resolution)?;

    println!(
        "Captured {} frame: {}x{}",
        frame.pixel_format, frame.width, frame.height
    );

    if frame.pixel_format != "NV12" {
        anyhow::bail!(
            "Expected NV12 pixel format, got {}. Cannot decode.",
            frame.pixel_format
        );
    }

    let expected_size = (frame.width * frame.height * 3 / 2) as usize;
    if frame.data.len() < expected_size {
        anyhow::bail!(
            "Buffer too small: got {} bytes, expected at least {} for NV12 {}x{}",
            frame.data.len(),
            expected_size,
            frame.width,
            frame.height
        );
    }

    // Decode with both matrices
    let matrices = [&yuv::BT601, &yuv::BT709];
    println!("Decoding with {}...", matrices[0].name);
    let mut rgb_601 = yuv::nv12_to_rgb24(
        &frame.data,
        frame.width,
        frame.height,
        matrices[0],
        frame.full_range,
    );

    println!("Decoding with {}...", matrices[1].name);
    let mut rgb_709 = yuv::nv12_to_rgb24(
        &frame.data,
        frame.width,
        frame.height,
        matrices[1],
        frame.full_range,
    );

    if mirror {
        mirror_rgb(&mut rgb_601, frame.width, frame.height);
        mirror_rgb(&mut rgb_709, frame.width, frame.height);
    }

    // Write BMP files
    let path_601 = std::path::PathBuf::from("capture_bt601.bmp");
    let path_709 = std::path::PathBuf::from("capture_bt709.bmp");

    bmp::write_bmp(&path_601, frame.width, frame.height, &rgb_601)?;
    println!("Saved: {}", path_601.display());

    bmp::write_bmp(&path_709, frame.width, frame.height, &rgb_709)?;
    println!("Saved: {}", path_709.display());

    println!();
    println!("Compare the two images side by side:");
    println!("  - The image with correct colors reveals which matrix the firmware uses.");
    println!("  - If capture_bt601.bmp looks correct, firmware encodes BT.601.");
    println!("  - If capture_bt709.bmp looks correct, firmware encodes BT.709.");

    Ok(())
}

fn run_force_matrix(device_index: usize, matrix: cli::MatrixChoice) -> anyhow::Result<()> {
    print_header();
    println!();

    platform::force_matrix(device_index, matrix)?;

    Ok(())
}

fn print_header() {
    println!("webcam-colorspace — Camera Colorspace Diagnostic Tool");
    println!("======================================================");
}

fn print_os_info() {
    println!();
    print!("OS: ");

    #[cfg(windows)]
    {
        if let Ok(output) = std::process::Command::new("cmd")
            .args(["/C", "ver"])
            .output()
        {
            let ver = String::from_utf8_lossy(&output.stdout);
            let ver = ver.trim();
            if !ver.is_empty() {
                println!("{ver}");
            } else {
                println!("Windows (version unknown)");
            }
        } else {
            println!("Windows (version unknown)");
        }
    }

    #[cfg(target_os = "linux")]
    {
        let distro = std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|l| l.starts_with("PRETTY_NAME="))
                    .map(|l| {
                        l.trim_start_matches("PRETTY_NAME=")
                            .trim_matches('"')
                            .to_string()
                    })
            })
            .unwrap_or_else(|| "Linux".to_string());

        let kernel = std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        if kernel.is_empty() {
            println!("{distro}");
        } else {
            println!("{distro} (kernel {kernel})");
        }
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        println!("Unknown platform");
    }
}

fn format_matrix_highlight(matrix: &str) -> String {
    match matrix {
        "BT.709" => format!("{matrix} <-- expected for modern OS (Win 24H2+, Linux 720p+)"),
        "BT.601" => format!("{matrix} <-- legacy; may cause color shift on modern OS"),
        "Not specified" => format!("{matrix} <-- OS will assume a default (check OS docs)"),
        other => other.to_string(),
    }
}

fn print_legend() {
    println!("Legend");
    println!("------");
    println!("  YUV Matrix is the key diagnostic field:");
    println!("    BT.709  = HD standard. Required by Windows 24H2+, Linux 720p+, ChromeOS.");
    println!("    BT.601  = SD standard. Legacy; causes color shift if OS expects BT.709.");
    println!("    Not specified = OS will pick a default. May vary by OS version.");
    println!();
    println!("  If the matrix shows 'Not specified' for all formats, the camera driver");
    println!("  does not advertise colorspace info. The OS will apply its own default.");
}
