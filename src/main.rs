mod platform;

use platform::FormatInfo;

fn main() -> anyhow::Result<()> {
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

fn print_header() {
    println!("webcam-colorspace — Camera Colorspace Diagnostic Tool");
    println!("======================================================");
}

fn print_os_info() {
    println!();
    print!("OS: ");

    #[cfg(windows)]
    {
        // Use ver command for Windows version
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
        // Read /etc/os-release for distro info
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
