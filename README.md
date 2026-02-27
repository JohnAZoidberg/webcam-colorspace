# webcam-colorspace

Camera colorspace debugging tool. Inspects what the OS reports it expects from each connected camera, helping diagnose BT.601 vs BT.709 YUV matrix mismatches.

## Background

Webcam firmware encodes YUV using a specific color matrix (BT.601 or BT.709). The OS decodes using whatever matrix it expects. A mismatch causes color shift:

| Scenario | Result |
|---|---|
| BT.709 firmware, BT.601 OS decode | Minor color shift |
| BT.601 firmware, BT.709 OS decode | Worse color shift |

### Current OS requirements

- **Windows 24H2+**: Requires BT.709 color matrix
- **Windows <23H2**: May decode as BT.601 (slight color shift with BT.709 firmware)
- **Linux**: Requires BT.709 for 720p+ camera output
- **ChromeOS**: Requires BT.709

Most vendors are moving to BT.709 as default; BT.601 support is being phased out.

## What this tool does

Three modes, used together to fully diagnose a colorspace problem:

1. **Enumerate** (default) — For each connected camera, reports pixel format, resolution, frame rate, and colorspace attributes (YUV matrix, primaries, transfer function, nominal range). The YUV matrix field is the key diagnostic: it shows what the *driver advertises* to the OS. "Not specified" means the driver doesn't set the attribute and the OS will fall back to its own default.

2. **Capture test** (`--capture-test`) — Captures a raw NV12 frame from the camera and decodes it twice: once assuming BT.601, once assuming BT.709. Saves two BMP files. By comparing the two images visually, you can determine which matrix the camera *firmware actually encodes* — which may differ from what the driver advertises.

3. **Force matrix** (`--force-matrix`) — Overrides the `MF_MT_YUV_MATRIX` attribute on the media type to tell the OS to decode with a specific matrix. This is a workaround for cases where the driver advertises the wrong matrix.

## Build

```
cargo build --release
```

## Usage

```
webcam-colorspace                               # enumerate devices (default)
webcam-colorspace --capture-test                 # capture highest-res NV12 from device 1
webcam-colorspace --capture-test 1280x720        # capture at 1280x720
webcam-colorspace --capture-test 2 640x480       # device 2 at 640x480
webcam-colorspace --force-matrix bt709           # override YUV matrix on device 1
webcam-colorspace --force-matrix bt601 2         # override YUV matrix on device 2
webcam-colorspace --help                         # show usage
```

Or via cargo:

```
cargo run --release
cargo run --release -- --capture-test 1280x720
cargo run --release -- --force-matrix bt709
```

### `--capture-test`

Captures a raw NV12 frame from the camera, decodes it with both BT.601 and BT.709 matrices, and saves two BMP images in the current directory:

- `capture_bt601.bmp` — decoded assuming BT.601
- `capture_bt709.bmp` — decoded assuming BT.709

Open both images side by side. One will have accurate colors and the other will have a visible color shift (skin tones skew orange/green, whites have a tint). The image with correct colors tells you which matrix the firmware actually encodes — this is the ground truth, regardless of what the driver advertises via `MF_MT_YUV_MATRIX`.

You can specify a resolution (e.g. `--capture-test 1280x720`) to match what your video app actually uses — different resolutions may behave differently. If omitted, the highest-resolution NV12 format is used. If the requested resolution isn't available, the tool lists the valid options.

The tool reads 5 frames and keeps the last one, giving the camera time to settle auto-exposure. It also reads the nominal range (full vs limited) from the media type and uses it for conversion — this matters because full-range (0-255) and limited-range (16-235) use different math.

### `--force-matrix bt601|bt709`

Overrides `MF_MT_YUV_MATRIX` on the source reader's media type. This tells the OS to decode the camera's YUV output using the specified matrix instead of whatever the driver advertises.

Use this when `--capture-test` reveals a mismatch — for example, if the driver says BT.709 but the firmware actually encodes BT.601. The override applies only to the source reader session created by this tool and does not persist after the program exits.

Some drivers may reject the override; the tool will report the failure.

### Device index

Both `--capture-test` and `--force-matrix` accept an optional device number (1-based). Default is 1. Run `webcam-colorspace` without arguments to see the device list with numbers.

## Debugging workflow

A typical debugging session when a camera shows wrong colors:

```
# Step 1: See what the driver advertises
webcam-colorspace

# Step 2: Find out what the firmware actually encodes
# Use the same resolution your video app uses (e.g. 1280x720 for a 720p call)
webcam-colorspace --capture-test 1280x720
# Open capture_bt601.bmp and capture_bt709.bmp side by side.
# The one with correct colors is the firmware's actual matrix.

# Step 3: If there's a mismatch, try overriding
webcam-colorspace --force-matrix bt709
```

**Interpreting results:**

| Driver says | Capture test shows | Diagnosis |
|---|---|---|
| BT.709 | BT.709 looks correct | No mismatch — problem is elsewhere |
| BT.709 | BT.601 looks correct | Driver is wrong; firmware uses BT.601. Use `--force-matrix bt601` as workaround. Firmware update needed. |
| BT.601 | BT.709 looks correct | Driver is wrong; firmware uses BT.709. Use `--force-matrix bt709`. On Win 24H2+ the OS already assumes BT.709. |
| Not specified | BT.709 looks correct | Driver doesn't advertise. On Win 24H2+ the OS defaults to BT.709, which matches — no issue. |
| Not specified | BT.601 looks correct | Driver doesn't advertise and firmware uses BT.601. Color shift on modern OS. Firmware update needed. |

### Sample output (enumerate)

```
webcam-colorspace — Camera Colorspace Diagnostic Tool
======================================================

OS: Microsoft Windows [Version 10.0.26100.3194]

Found 1 camera device(s):

━━━ Device 1: HD Webcam ━━━
    Path: \\?\usb#vid_0408&pid_...
    Formats (6 unique):
      YUY2 1920x1080 @ 5.00 fps
        Primaries: BT.709
        YUV Matrix: BT.709 <-- expected for modern OS (Win 24H2+, Linux 720p+)
        Transfer: BT.709
        Range: Full (0-255)
      NV12 1280x720 @ 30.00 fps
        Primaries: Not specified
        YUV Matrix: Not specified <-- OS will assume a default (check OS docs)
        Transfer: Not specified
        Range: Not specified
```

### Platform support

| Feature | Windows | Linux |
|---|---|---|
| Enumerate devices | Yes | Yes |
| `--capture-test` | Yes | Not yet |
| `--force-matrix` | Yes | Not yet |

## Platform notes

### Windows
Uses Media Foundation to enumerate video capture devices and read media type attributes (`MF_MT_YUV_MATRIX`, `MF_MT_VIDEO_PRIMARIES`, etc.).

### Linux
Uses V4L2 (via the `v4l` crate) to enumerate `/dev/video*` devices and read colorspace info from the current device format. You may need to be in the `video` group or run as root to access camera devices:

```
sudo usermod -aG video $USER
# Log out and back in
```
