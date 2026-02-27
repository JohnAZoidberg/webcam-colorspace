use std::env;

pub enum Command {
    Enumerate,
    CaptureTest {
        device_index: usize,
        resolution: Option<(u32, u32)>,
        mirror: bool,
        save_raw: bool,
    },
    ForceMatrix {
        matrix: MatrixChoice,
        device_index: usize,
    },
}

#[derive(Clone, Copy)]
pub enum MatrixChoice {
    Bt601,
    Bt709,
}

pub fn parse_args() -> anyhow::Result<Command> {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        return Ok(Command::Enumerate);
    }

    match args[0].as_str() {
        "-h" | "--help" => {
            print_usage();
            std::process::exit(0);
        }
        "--capture-test" => {
            let mut device_index = 0usize;
            let mut resolution = None;
            let mut mirror = false;
            let mut save_raw = false;

            for arg in &args[1..] {
                if arg == "--mirror" {
                    mirror = true;
                } else if arg == "--save-raw" {
                    save_raw = true;
                } else if let Some(res) = parse_resolution(arg) {
                    resolution = Some(res);
                } else if let Ok(n) = arg.parse::<usize>() {
                    if n == 0 {
                        anyhow::bail!("Device number must be >= 1 (1-based index).");
                    }
                    device_index = n - 1;
                } else {
                    anyhow::bail!(
                        "Unknown argument '{}' for --capture-test. Expected a device number, WxH resolution, --mirror, or --save-raw.",
                        arg
                    );
                }
            }

            Ok(Command::CaptureTest {
                device_index,
                resolution,
                mirror,
                save_raw,
            })
        }
        "--force-matrix" => {
            if args.len() < 2 {
                anyhow::bail!("--force-matrix requires a value: bt601 or bt709");
            }
            let matrix = match args[1].to_lowercase().as_str() {
                "bt601" => MatrixChoice::Bt601,
                "bt709" => MatrixChoice::Bt709,
                other => anyhow::bail!("Unknown matrix '{}'. Expected: bt601 or bt709", other),
            };
            let device_index = parse_optional_device_index(&args, 2)?;
            Ok(Command::ForceMatrix {
                matrix,
                device_index,
            })
        }
        other => {
            anyhow::bail!("Unknown argument '{}'. Use --help for usage.", other);
        }
    }
}

/// Parse "WxH" or "WXH" (case-insensitive) into (width, height).
fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let s_lower = s.to_lowercase();
    let parts: Vec<&str> = s_lower.split('x').collect();
    if parts.len() == 2 {
        if let (Ok(w), Ok(h)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
            return Some((w, h));
        }
    }
    None
}

fn parse_optional_device_index(args: &[String], pos: usize) -> anyhow::Result<usize> {
    if pos < args.len() {
        let n: usize = args[pos].parse().map_err(|_| {
            anyhow::anyhow!(
                "Invalid device number '{}'. Expected a positive integer.",
                args[pos]
            )
        })?;
        if n == 0 {
            anyhow::bail!("Device number must be >= 1 (1-based index).");
        }
        Ok(n - 1)
    } else {
        Ok(0) // default: first device
    }
}

pub fn print_usage() {
    eprintln!("webcam-colorspace — Camera Colorspace Diagnostic Tool");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    webcam-colorspace");
    eprintln!("        Enumerate devices and show colorspace info");
    eprintln!();
    eprintln!("    webcam-colorspace --capture-test [N] [WxH] [--mirror] [--save-raw]");
    eprintln!("        Capture a frame and decode with BT.601 + BT.709");
    eprintln!();
    eprintln!("    webcam-colorspace --force-matrix bt601|bt709 [N]");
    eprintln!("        Override YUV matrix on the media type");
    eprintln!();
    eprintln!("    webcam-colorspace --help");
    eprintln!("        Show this help");
    eprintln!();
    eprintln!("ARGUMENTS:");
    eprintln!("    N      Device number (1-based, default: 1)");
    eprintln!("    WxH    Resolution to capture (e.g. 1280x720). Default: highest available.");
    eprintln!();
    eprintln!("EXAMPLES:");
    eprintln!("    webcam-colorspace                            # list all cameras");
    eprintln!("    webcam-colorspace --capture-test              # capture highest-res NV12");
    eprintln!("    webcam-colorspace --capture-test 1280x720     # capture at 1280x720");
    eprintln!(
        "    webcam-colorspace --capture-test --mirror      # capture mirrored (selfie view)"
    );
    eprintln!("    webcam-colorspace --capture-test --save-raw    # also save raw NV12 bytes");
    eprintln!("    webcam-colorspace --capture-test 2 640x480    # device 2, 640x480");
    eprintln!("    webcam-colorspace --force-matrix bt709        # force BT.709 on device 1");
    eprintln!("    webcam-colorspace --force-matrix bt601 2      # force BT.601 on device 2");
}
