# track-time-tagger

Add GPS EXIF tags to timestamped photos by matching them to a FIT or GPX track.
Track Time Tagger works locally: the CLI supports JPEG/TIFF, and the optional
browser GUI produces geotagged JPEG copies without uploading photos or tracks.

The primary interface is a small interactive Rust CLI that:

1. parses a Garmin/ANT `.fit` file with [`fitparser`](https://github.com/stadelmanma/fitparse-rs) or a `.gpx` track with [`gpx`](https://docs.rs/gpx),
2. reads `DateTimeOriginal` from local JPEG/TIFF images,
3. interprets that timezone-less camera timestamp in a chosen IANA timezone,
4. finds the surrounding GPS samples and linearly interpolates the position,
5. prints OpenStreetMap and Google Maps links for verification,
6. prompts before writing GPS EXIF metadata through its built-in Rust writer.

The built-in writer is used by default, so no Perl or ExifTool installation is required. It retains an `<image>_original` backup unless `--no-backup` is given. Existing GPS tags are skipped unless `--overwrite-gps` is given.

## Requirements

The default writer has no external runtime requirements. Build with Rust:

```bash
cargo build --release
```

## Build

```bash
cargo build --release
```

The binary will be at:

```text
target/release/track-time-tagger
```

## Automation and releases

GitHub Actions validates formatting, tests, Clippy, and an optimized CLI build
on every push and pull request. Pushing a version tag such as `v0.1.0` creates
a GitHub release with native CLI archives for Linux x86_64, macOS ARM64, and
Windows x86_64.

## Optional local GUI (early preview)

The CLI remains the primary application. An optional Dioxus GUI lives in
[`gui/`](gui/) as the foundation for a browser-first, local-only workflow.
It does not send selected photos or tracks to a server.

The GUI currently supports local JPEG processing: choose or drag in one FIT/GPX
track and one or more JPEGs, review the local image previews, then download a
GPS-tagged copy of each matching image. It is deliberately separate from the
CLI, so normal CLI builds do not pull in GUI dependencies.

Install the Dioxus development tool and the WebAssembly target once:

```bash
cargo install dioxus-cli
rustup target add wasm32-unknown-unknown
```

Then serve the GUI locally:

```bash
cd gui
dx serve --web
```

For a desktop preview instead, use `dx serve --desktop`. Platform setup notes
and the privacy model are in [the GUI guide](gui/README.md).

## First run: dry-run and verify

```bash
track-time-tagger \
  --track activity.fit \
  --images ./photos \
  --timezone America/Toronto \
  --recursive \
  --dry-run
```

Each match prints optional map-provider links for verifying its proposed
location.

## Example: geotagging race-event photos

Suppose a professional photographer covers a road race with a camera whose clock is accurate, but whose photos contain no GPS metadata. A runner or other subject can carry a Garmin watch or another GPS device during the event, then use the original FIT file or a GPX export to retrofit GPS metadata after receiving the photographer's images.

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

## Example: hiking or birdwatching with a GPS-less SLR

A hiker or birdwatcher may carry a professional SLR camera with excellent image quality but no built-in GPS. As long as the camera clock is accurate, the Android [OpenTracks app on Google Play](https://play.google.com/store/apps/details?id=de.dennisguse.opentracks.playstore) or [F-Droid](https://f-droid.org/packages/de.dennisguse.opentracks/) can record the outing independently. Export that activity as GPX, then use Track Time Tagger to add the corresponding GPS position to each photo afterward.

This is useful when the camera and tracking device are separate: the camera records the best image, while the phone or watch records the route. Accurate timestamps provide the link between them.

## Interactive update

```bash
track-time-tagger \
  --track activity.fit \
  --images ./photos \
  --timezone America/Toronto \
  --recursive
```

The program asks separately for each photo before writing.

## Writing to a separate directory

By default, accepted matches update the input images in place. Use `--output` to leave the originals unchanged. When scanning a directory, the input directory layout is reproduced below the output directory:

```bash
track-time-tagger \
  --track hike.gpx \
  --images ./camera-originals \
  --output ./camera-geotagged \
  --recursive \
  --yes
```

For a single input image, `--output` is treated as the destination directory for that image. Backups, when enabled, are created beside the copied output image.

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
- `--output DIR` writes updated copies under `DIR` and leaves input images unchanged.
- `--no-backup` disables the `<image>_original` backup for either metadata writer. Leave it off initially.
- Supported input images are JPEG and TIFF.

## Important timezone note

Standard EXIF `DateTimeOriginal` usually has no timezone. FIT and GPX timestamps represent an absolute time. Therefore `--timezone` must describe the timezone in which the camera clock was set when the photos were taken.

During an ambiguous fall daylight-saving transition, the tool refuses the timestamp rather than silently choosing the wrong UTC instant.

## Advanced optional backend

The standard installation uses only the integrated Rust metadata writer. For the optional ExifTool backend, see [the advanced ExifTool guide](docs/exiftool-backend.md).
