# Optional ExifTool backend

Track Time Tagger uses its integrated Rust EXIF writer by default. No Perl or external metadata program is required for the normal JPEG/TIFF workflow.

ExifTool is an optional fallback for camera files that need its wider metadata and format support. It is never available in the planned web/WASM build because browsers cannot run external programs.

## Installation

Fedora:

```bash
sudo dnf install perl-Image-ExifTool
```

Debian/Ubuntu:

```bash
sudo apt install libimage-exiftool-perl
```

## Use

Pass `--exiftool` to opt in:

```bash
track-time-tagger \
  --track activity.gpx \
  --images ./photos \
  --recursive \
  --exiftool
```

The application checks that ExifTool is available before it starts. The usual backup behavior still applies; use `--no-backup` only when you explicitly do not want `<image>_original` copies.
