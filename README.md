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

## What this tool shows

For each connected camera, the tool reports:
- Pixel format (NV12, YUY2, MJPG, etc.)
- Resolution and frame rate
- **YUV matrix** (BT.601 vs BT.709) — the key diagnostic field
- Color primaries, transfer function, and nominal range

If the camera driver doesn't advertise colorspace attributes (common for many webcams), the tool reports "Not specified", which is itself diagnostic — it means the OS will apply its own default.

## Build

```
cargo build --release
```

## Usage

```
cargo run --release
```

Or run the binary directly:

```
# Windows
target\release\webcam-colorspace.exe

# Linux
target/release/webcam-colorspace
```

### Sample output

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

## Platform notes

### Windows
Uses Media Foundation to enumerate video capture devices and read media type attributes (`MF_MT_YUV_MATRIX`, `MF_MT_VIDEO_PRIMARIES`, etc.).

### Linux
Uses V4L2 (via the `v4l` crate) to enumerate `/dev/video*` devices and read colorspace info from the current device format. You may need to be in the `video` group or run as root to access camera devices:

```
sudo usermod -aG video $USER
# Log out and back in
```
