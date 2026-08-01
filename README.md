# track-time-tagger

A small interactive Rust CLI that:

1. parses a Garmin/ANT `.fit` file with [`fitparser`](https://github.com/stadelmanma/fitparse-rs) or a `.gpx` track with [`gpx`](https://docs.rs/gpx),
2. reads `DateTimeOriginal` from local JPEG/TIFF images,
3. interprets that timezone-less camera timestamp in a chosen IANA timezone,
4. finds the surrounding GPS samples and linearly interpolates the position,
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
target/release/track-time-tagger
```

## First run: dry-run and verify

```bash
track-time-tagger \
  --track activity.fit \
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

## Example: geotagging race-event photos

Suppose a professional photographer covers a road race with a camera whose clock is accurately synchronized, but whose photos contain no GPS metadata. The photographer can record the course with a Garmin watch or another GPS device at the same time, then use either the original FIT file or a GPX export.

The camera provides the timestamp for each photo, while the track file provides the position for each moment. The tool combines those two sources, interpolating between nearby track points when necessary. This can place photos taken at aid stations, turns, finish lines, and along the course on a map without requiring a GPS-enabled camera.

First preview the proposed locations without changing any photos:

```bash
cargo run --release -- \
  --track race-day.fit \
  --images ./race-photos \
  --timezone America/New_York \
  --recursive \
  --dry-run
```

If the preview matches the event route, run the update interactively:

```bash
cargo run --release -- \
  --track race-day.fit \
  --images ./race-photos \
  --timezone America/New_York \
  --recursive
```

If the camera clock was consistently 12 seconds behind the GPS device, include `--camera-offset-seconds 12`. Existing GPS metadata is skipped by default, so the workflow is safe to repeat while reviewing the results.

GPX input works the same way:

```bash
cargo run --release -- \
  --track race-day.gpx \
  --images ./race-photos \
  --timezone America/New_York \
  --recursive \
  --dry-run
```

The track format is selected from the `.fit` or `.gpx` filename extension. GPX track points without timestamps are ignored.

## Interactive update

```bash
track-time-tagger \
  --track activity.fit \
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
- `--max-gap-seconds 300` requires both surrounding track points to be no more than five minutes away.
- `--dry-run` never prompts or writes.
- `--yes` accepts all valid matches.
- `--no-backup` passes `-overwrite_original` to ExifTool. Leave it off initially.
- Supported input images are JPEG and TIFF. HEIC/AVIF support would require a different EXIF-reading path, although ExifTool itself can handle many additional formats.

## Important timezone note

Standard EXIF `DateTimeOriginal` usually has no timezone. FIT and GPX timestamps represent an absolute time. Therefore `--timezone` must describe the timezone in which the camera clock was set when the photos were taken.

During an ambiguous fall daylight-saving transition, the tool refuses the timestamp rather than silently choosing the wrong UTC instant.
