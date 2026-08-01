# fit-photo-geotag

A small interactive Rust CLI that:

1. parses a Garmin/ANT `.fit` file with [`fitparser`](https://github.com/stadelmanma/fitparse-rs),
2. reads `DateTimeOriginal` from local JPEG/TIFF images,
3. interprets that timezone-less camera timestamp in a chosen IANA timezone,
4. finds the surrounding FIT GPS samples and linearly interpolates the position,
5. prints OpenStreetMap and Google Maps links for verification,
6. prompts before writing GPS EXIF metadata through `exiftool`.

By default, ExifTool retains an `<image>_original` backup. Existing GPS tags are skipped unless `--overwrite-gps` is given.

## Requirements

Fedora:

```bash
sudo dnf install rust cargo perl-Image-ExifTool
```

Debian/Ubuntu:

```bash
sudo apt install cargo libimage-exiftool-perl
```

## Build

```bash
cargo build --release
```

The binary will be at:

```text
target/release/fit-photo-geotag
```

## First run: dry-run and verify

```bash
fit-photo-geotag \
  --fit activity.fit \
  --images ./photos \
  --timezone America/Toronto \
  --recursive \
  --dry-run
```

Each match prints links like:

```text
OpenStreetMap: https://www.openstreetmap.org/?mlat=42.98&mlon=-81.24#map=18/42.98/-81.24
Google Maps:  https://www.google.com/maps/search/?api=1&query=42.98,-81.24
```

## Interactive update

```bash
fit-photo-geotag \
  --fit activity.fit \
  --images ./photos \
  --timezone America/Toronto \
  --recursive
```

The program asks separately for each photo before writing.

## Camera clock offset

If the camera was 37 seconds slow, add 37 seconds before matching:

```bash
--camera-offset-seconds 37
```

If the camera was two minutes fast, subtract 120 seconds:

```bash
--camera-offset-seconds -120
```

## Safety and matching behavior

- Images already containing GPS tags are skipped by default.
- `--overwrite-gps` permits replacing them.
- `--max-gap-seconds 300` requires both surrounding FIT points to be no more than five minutes away.
- `--dry-run` never prompts or writes.
- `--yes` accepts all valid matches.
- `--no-backup` passes `-overwrite_original` to ExifTool. Leave it off initially.
- Supported input images are JPEG and TIFF. HEIC/AVIF support would require a different EXIF-reading path, although ExifTool itself can handle many additional formats.

## Important timezone note

Standard EXIF `DateTimeOriginal` usually has no timezone. FIT timestamps represent an absolute time. Therefore `--timezone` must describe the timezone in which the camera clock was set when the photos were taken.

During an ambiguous fall daylight-saving transition, the tool refuses the timestamp rather than silently choosing the wrong UTC instant.
