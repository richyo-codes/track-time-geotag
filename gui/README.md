# Track Time Tagger GUI

This is an optional, early Dioxus GUI for Track Time Tagger. The Rust CLI at
the repository root remains the primary supported application.

## Privacy model

The intended browser workflow is local-first: selected FIT/GPX tracks and
photos are processed in WebAssembly in the browser, then geotagged copies are
downloaded. Local mode has no account, analytics, telemetry, advertising, or
third-party API calls. It does not upload selected files to an application
server.

Map links are optional; opening one shares that coordinate with the map
provider. Future Immich or repository integration will be an explicitly
separate networked mode.

Browsers can access only files the user selects, and they cannot overwrite
original local photos in place. The initial browser writer target is JPEG.

For the full design boundary, read [the GUI privacy plan](../docs/gui-privacy-plan.md).

## Run locally

Install the Dioxus CLI and WebAssembly target once:

```bash
cargo install dioxus-cli
rustup target add wasm32-unknown-unknown
```

Serve the web application locally:

```bash
dx serve --web
```

For the desktop renderer, run:

```bash
dx serve --desktop
```

The desktop build uses the system WebView and may require platform packages.
The optional ExifTool backend is intentionally excluded from web/WASM; see
[the advanced backend guide](../docs/exiftool-backend.md).

## Current status

The web GUI supports JPEG input: select or drag in one FIT/GPX track and one or
more JPEGs, set the camera timezone and matching tolerances, then download one
`geotagged-...` copy for each successful match. A separate tab shows previews
of the selected local photos. Existing GPS tags are skipped, and originals are
never modified.

The matching and browser-compatible JPEG writer live in the shared
`track-time-tagger-core` crate. TIFF output, in-place desktop writes, and the
optional ExifTool backend remain CLI/desktop work.
